use anyhow::Context;
use std::process::Command;
use std::time::Duration;

use super::state::{ActionResult, AikiState};
use super::types::{Action, FailureMode, JjAction, LetAction, LogAction, ShellAction};
use super::variables::VariableResolver;
use crate::error::{AikiError, Result};

/// Executes flow actions
pub struct FlowExecutor;

impl FlowExecutor {
    /// Create a variable resolver with consistent variable availability
    ///
    /// Makes variables available both with and without `event.` prefix:
    /// - $event.file_path (for event variables)
    /// - $file_path (for event variables, let-bound variables)
    /// - $description (for let-bound variables)
    /// Create a variable resolver with proper variable scoping
    ///
    /// Variable scopes:
    /// - Event variables (from actual events): $event.file_path, $event.agent_type
    /// - Let variables (user-defined): $description, $my_var (no event. prefix)
    /// - System variables: $cwd
    /// - Environment variables: $HOME, $PATH
    fn create_resolver(context: &AikiState) -> VariableResolver {
        let mut resolver = VariableResolver::new();

        // Add event-specific variables based on event type
        match &context.event {
            crate::events::AikiEvent::PostChange(e) => {
                resolver.add_var("event.tool_name".to_string(), e.tool_name.clone());
                resolver.add_var("event.file_path".to_string(), e.file_path.clone());
                resolver.add_var("event.session_id".to_string(), e.session_id.clone());
            }
            crate::events::AikiEvent::Start(e) => {
                if let Some(ref session_id) = e.session_id {
                    resolver.add_var("event.session_id".to_string(), session_id.clone());
                }
            }
            crate::events::AikiEvent::PreCommit(_) => {
                // No event-specific variables for PreCommit
            }
        }

        // Add agent type as event.agent_type
        let agent_str = match context.event.agent_type() {
            crate::provenance::AgentType::ClaudeCode => "claude-code",
            crate::provenance::AgentType::Cursor => "cursor",
            crate::provenance::AgentType::Unknown => "unknown",
        };
        resolver.add_var("event.agent_type".to_string(), agent_str.to_string());

        // Add let variables (accessible via $key without event. prefix)
        for (key, value) in context.iter_variables() {
            resolver.add_var(key.clone(), value.clone());
        }

        // Add cwd using helper method
        resolver.add_var("cwd", context.cwd().to_string_lossy().to_string());

        // Fetch environment variables on-demand
        let env_vars: std::collections::HashMap<String, String> = std::env::vars().collect();
        resolver.add_env_vars(&env_vars);

        resolver
    }
    /// Execute a list of actions sequentially
    pub fn execute_actions(
        actions: &[Action],
        context: &mut AikiState,
    ) -> Result<Vec<ActionResult>> {
        let mut results = Vec::new();

        for action in actions {
            let result = Self::execute_action(action, context)?;

            // Store action results for reference by subsequent actions
            Self::store_action_result(action, &result, context);

            // Check failure mode
            let should_stop = match action {
                Action::Shell(shell_action) => {
                    !result.success && shell_action.on_failure == FailureMode::Stop
                }
                Action::Jj(jj_action) => {
                    !result.success && jj_action.on_failure == FailureMode::Stop
                }
                Action::Let(let_action) => {
                    !result.success && let_action.on_failure == FailureMode::Stop
                }
                Action::Log(_) => false, // Log actions never fail
            };

            results.push(result);

            if should_stop {
                return Err(AikiError::ActionFailed);
            }
        }

        Ok(results)
    }

    /// Execute a single action
    fn execute_action(action: &Action, context: &AikiState) -> Result<ActionResult> {
        match action {
            Action::Shell(shell_action) => Self::execute_shell(shell_action, context),
            Action::Jj(jj_action) => Self::execute_jj(jj_action, context),
            Action::Log(log_action) => Self::execute_log(log_action, context),
            Action::Let(let_action) => Self::execute_let(let_action, context),
        }
    }

    /// Store action result as variables for subsequent actions
    ///
    /// For Let actions: stores the variable and its structured metadata
    /// For Shell/Jj/Log with alias: stores the variable with its result
    fn store_action_result(action: &Action, result: &ActionResult, context: &mut AikiState) {
        match action {
            Action::Let(let_action) => {
                // Parse the variable name from "variable = expression"
                if let Some(variable_name) = let_action.let_.split('=').next() {
                    let variable_name = variable_name.trim();
                    context.store_action_result(variable_name.to_string(), result.clone());
                }
            }
            Action::Shell(shell_action) => {
                if let Some(alias) = &shell_action.alias {
                    context.store_action_result(alias.clone(), result.clone());
                }
            }
            Action::Jj(jj_action) => {
                if let Some(alias) = &jj_action.alias {
                    context.store_action_result(alias.clone(), result.clone());
                }
            }
            Action::Log(log_action) => {
                if let Some(alias) = &log_action.alias {
                    context.store_action_result(alias.clone(), result.clone());
                }
            }
        }
    }

    /// Execute a shell command
    fn execute_shell(action: &ShellAction, context: &AikiState) -> Result<ActionResult> {
        // Create variable resolver with consistent variable availability
        let mut resolver = Self::create_resolver(context);

        // Resolve variables in command
        let command = resolver.resolve(&action.shell);

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Executing shell: {}", command);
        }

        // Execute command
        let output = if let Some(timeout_str) = &action.timeout {
            // Parse timeout (e.g., "30s", "1m")
            let timeout = parse_timeout(timeout_str)?;
            execute_with_timeout(&command, context.cwd(), timeout)?
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(&command)
                .current_dir(context.cwd())
                .output()
                .context("Failed to execute shell command")?
        };

        Ok(ActionResult {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Execute a JJ command
    fn execute_jj(action: &JjAction, context: &AikiState) -> Result<ActionResult> {
        // Create variable resolver with consistent variable availability
        let mut resolver = Self::create_resolver(context);

        // Resolve variables in command
        let jj_args = resolver.resolve(&action.jj);

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Executing jj: {}", jj_args);
        }

        // Parse arguments using proper shell word splitting (handles quoted args)
        let args = shell_words::split(&jj_args)
            .with_context(|| format!("Failed to parse jj arguments: {}", jj_args))?;

        // Execute JJ command (using direct argv, no shell invocation)
        let output = if let Some(timeout_str) = &action.timeout {
            let timeout = parse_timeout(timeout_str)?;
            execute_with_timeout_argv("jj", &args, context.cwd(), timeout)?
        } else {
            Command::new("jj")
                .args(&args)
                .current_dir(context.cwd())
                .output()
                .context("Failed to execute jj command")?
        };

        Ok(ActionResult {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Execute a log action
    fn execute_log(action: &LogAction, context: &AikiState) -> Result<ActionResult> {
        // Create variable resolver with consistent variable availability
        let mut resolver = Self::create_resolver(context);

        // Resolve variables in message
        let message = resolver.resolve(&action.log);

        // Print to stderr (so it appears in hook output)
        eprintln!("[aiki] {}", message);

        Ok(ActionResult::success())
    }

    /// Execute a let binding action
    ///
    /// Supports two modes:
    /// 1. Function call: `let description = aiki/core.build_description`
    /// 2. Variable aliasing: `let desc = $description`
    fn execute_let(action: &LetAction, context: &AikiState) -> Result<ActionResult> {
        // Parse the let binding: "variable = expression"
        let parts: Vec<&str> = action.let_.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(AikiError::InvalidLetSyntax(action.let_.to_string()));
        }

        let variable_name = parts[0].trim();
        let expression = parts[1].trim();

        // Validate variable name
        if !Self::is_valid_variable_name(variable_name) {
            return Err(AikiError::InvalidVariableName(variable_name.to_string()));
        }

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Let binding: {} = {}", variable_name, expression);
        }

        // Check if this is variable aliasing (starts with $) or a function call
        if expression.starts_with('$') {
            // Mode 2: Variable aliasing
            Self::execute_let_alias(variable_name, expression, context)
        } else {
            // Mode 1: Function call
            Self::execute_let_function(variable_name, expression, context)
        }
    }

    /// Validate variable name (must start with letter/underscore, contain only alphanumeric/underscore)
    fn is_valid_variable_name(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        let mut chars = name.chars();
        let first = chars.next().unwrap();

        // First character must be letter or underscore
        if !first.is_alphabetic() && first != '_' {
            return false;
        }

        // Remaining characters must be alphanumeric or underscore
        chars.all(|c| c.is_alphanumeric() || c == '_')
    }

    /// Execute a let binding for variable aliasing: `let desc = $description`
    fn execute_let_alias(
        variable_name: &str,
        expression: &str,
        context: &AikiState,
    ) -> Result<ActionResult> {
        // Create variable resolver with consistent variable availability
        let mut resolver = Self::create_resolver(context);

        // Resolve the variable reference
        let value = resolver.resolve(expression);

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Variable alias: {} = {}", variable_name, value);
        }

        // Return the value as stdout
        Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: value,
            stderr: String::new(),
        })
    }

    /// Execute a let binding for function call: `let description = aiki/core.build_description`
    /// Supports `self.function` syntax to reference functions in the current flow
    fn execute_let_function(
        variable_name: &str,
        function_path: &str,
        context: &AikiState,
    ) -> Result<ActionResult> {
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!(
                "[flows] Function call: {} = {}",
                variable_name, function_path
            );
        }

        // Handle self.function syntax
        let resolved_path = if function_path.starts_with("self.") {
            // Extract function name from self.function
            let function_name = function_path.strip_prefix("self.").unwrap();

            // Get current flow name from context
            let flow_name = context.flow_name.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Cannot use 'self.{}' - no flow context available",
                    function_name
                )
            })?;

            // Convert flow name (e.g., "aiki/core") to module.function
            // Extract module from flow name: aiki/core -> core
            let module = flow_name.split('/').last().unwrap_or(flow_name);
            format!("aiki/{}.{}", module, function_name)
        } else {
            function_path.to_string()
        };

        // Parse function path: namespace/module.function
        // For now, we only support aiki/* namespace
        if !resolved_path.starts_with("aiki/") {
            return Err(AikiError::UnsupportedFunctionNamespace(
                resolved_path.to_string(),
            ));
        }

        // Extract module.function part
        let module_function = resolved_path.strip_prefix("aiki/").unwrap();

        // Split into module and function
        let parts: Vec<&str> = module_function.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(AikiError::MissingFunction(function_path.to_string()));
        }

        let module = parts[0];
        let function = parts[1];

        // Route to appropriate function
        match (module, function) {
            ("core", "build_description") => {
                // build_description requires PostChange event
                match &context.event {
                    crate::events::AikiEvent::PostChange(event) => {
                        crate::flows::core::build_description(event)
                    }
                    _ => Err(AikiError::Other(anyhow::anyhow!(
                        "build_description can only be called for PostChange events"
                    ))),
                }
            }
            ("core", "generate_coauthors") => {
                // generate_coauthors requires PreCommit event
                match &context.event {
                    crate::events::AikiEvent::PreCommit(event) => {
                        crate::flows::core::generate_coauthors(event)
                    }
                    _ => Err(AikiError::Other(anyhow::anyhow!(
                        "generate_coauthors can only be called for PreCommit events"
                    ))),
                }
            }
            _ => Err(AikiError::FunctionNotFoundInNamespace(
                function.to_string(),
                module.to_string(),
            )),
        }
    }
}

/// Parse timeout string (e.g., "30s", "1m", "2h")
fn parse_timeout(timeout_str: &str) -> Result<Duration> {
    let timeout_str = timeout_str.trim();

    if let Some(seconds_str) = timeout_str.strip_suffix('s') {
        let seconds: u64 = seconds_str.parse().context("Invalid timeout value")?;
        Ok(Duration::from_secs(seconds))
    } else if let Some(minutes_str) = timeout_str.strip_suffix('m') {
        let minutes: u64 = minutes_str.parse().context("Invalid timeout value")?;
        Ok(Duration::from_secs(minutes * 60))
    } else if let Some(hours_str) = timeout_str.strip_suffix('h') {
        let hours: u64 = hours_str.parse().context("Invalid timeout value")?;
        Ok(Duration::from_secs(hours * 3600))
    } else {
        Err(AikiError::InvalidTimeoutFormat(timeout_str.to_string()))
    }
}

/// Execute command with timeout using direct argv (no shell invocation)
fn execute_with_timeout_argv(
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
    timeout: Duration,
) -> Result<std::process::Output> {
    use std::sync::mpsc;
    use std::thread;

    let cwd = cwd.to_path_buf();
    let program = program.to_string();
    let args = args.to_vec();

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let output = Command::new(&program)
            .args(&args)
            .current_dir(&cwd)
            .output();
        let _ = tx.send(output);
    });

    Ok(rx
        .recv_timeout(timeout)
        .context("Command timed out")?
        .context("Failed to execute command")?)
}

/// Execute command with timeout (legacy shell-based version)
fn execute_with_timeout(
    command: &str,
    cwd: &std::path::Path,
    timeout: Duration,
) -> Result<std::process::Output> {
    use std::sync::mpsc;
    use std::thread;

    let cwd = cwd.to_path_buf();
    let command = command.to_string();

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let output = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(&cwd)
            .output();
        let _ = tx.send(output);
    });

    Ok(rx
        .recv_timeout(timeout)
        .context("Command timed out")?
        .context("Failed to execute command")?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AikiEvent, AikiPostChangeEvent};
    use crate::provenance::AgentType;

    // Helper to create a simple test event
    fn create_test_event() -> AikiEvent {
        AikiEvent::PostChange(AikiPostChangeEvent {
            agent_type: AgentType::ClaudeCode,
            session_id: "test-session".to_string(),
            tool_name: "Edit".to_string(),
            file_path: "/tmp/file.rs".to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
        })
    }

    // Helper to create a test event with custom file_path
    fn create_test_event_with_file(file_path: &str) -> AikiEvent {
        AikiEvent::PostChange(AikiPostChangeEvent {
            agent_type: AgentType::ClaudeCode,
            session_id: "test-session".to_string(),
            tool_name: "Edit".to_string(),
            file_path: file_path.to_string(),
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
        })
    }

    #[test]
    fn test_parse_timeout_seconds() {
        assert_eq!(parse_timeout("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_timeout("1s").unwrap(), Duration::from_secs(1));
    }

    #[test]
    fn test_parse_timeout_minutes() {
        assert_eq!(parse_timeout("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_timeout("1m").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn test_parse_timeout_hours() {
        assert_eq!(parse_timeout("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_timeout("2h").unwrap(), Duration::from_secs(7200));
    }

    #[test]
    fn test_parse_timeout_invalid() {
        assert!(parse_timeout("30").is_err());
        assert!(parse_timeout("abc").is_err());
        assert!(parse_timeout("30x").is_err());
    }

    #[test]
    fn test_execute_log_action() {
        let action = LogAction {
            log: "Test message".to_string(),
            alias: None,
        };

        let context = AikiState::new(create_test_event());

        let result = FlowExecutor::execute_log(&action, &context).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_execute_log_with_variables() {
        let action = LogAction {
            log: "File: $event.file_path".to_string(),
            alias: None,
        };

        let context = AikiState::new(create_test_event_with_file("test.rs"));

        let result = FlowExecutor::execute_log(&action, &context).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_execute_shell_echo() {
        let action = ShellAction {
            shell: "echo 'test'".to_string(),
            timeout: None,
            on_failure: FailureMode::Continue,
            alias: None,
        };

        let context = AikiState::new(create_test_event());

        let result = FlowExecutor::execute_shell(&action, &context).unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("test"));
    }

    #[test]
    fn test_execute_shell_with_variables() {
        let action = ShellAction {
            shell: "echo $event.file_path".to_string(),
            timeout: None,
            on_failure: FailureMode::Continue,
            alias: None,
        };

        let context = AikiState::new(create_test_event_with_file("test.rs"));

        let result = FlowExecutor::execute_shell(&action, &context).unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("test.rs"));
    }

    #[test]
    fn test_execute_actions_sequential() {
        let actions = vec![
            Action::Log(LogAction {
                log: "Step 1".to_string(),
                alias: None,
            }),
            Action::Shell(ShellAction {
                shell: "echo 'Step 2'".to_string(),
                timeout: None,
                on_failure: FailureMode::Continue,
                alias: None,
            }),
            Action::Log(LogAction {
                log: "Step 3".to_string(),
                alias: None,
            }),
        ];

        let mut context = AikiState::new(create_test_event());

        let results = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.success));
    }

    #[test]
    fn test_execute_actions_fail_mode_continue() {
        let actions = vec![
            Action::Shell(ShellAction {
                shell: "false".to_string(), // This command fails
                timeout: None,
                on_failure: FailureMode::Continue, // But we continue
                alias: None,
            }),
            Action::Log(LogAction {
                log: "This should still run".to_string(),
                alias: None,
            }),
        ];

        let mut context = AikiState::new(create_test_event());

        let results = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert_eq!(results.len(), 2);
        assert!(!results[0].success); // First action failed
        assert!(results[1].success); // Second action succeeded
    }

    #[test]
    fn test_execute_actions_fail_mode_stop() {
        let actions = vec![
            Action::Shell(ShellAction {
                shell: "false".to_string(), // This command fails
                timeout: None,
                on_failure: FailureMode::Stop, // Stop on failure
                alias: None,
            }),
            Action::Log(LogAction {
                log: "This should NOT run".to_string(),
                alias: None,
            }),
        ];

        let event = create_test_event();
        let mut context = AikiState::new(event);

        let result = FlowExecutor::execute_actions(&actions, &mut context);
        assert!(result.is_err()); // Should fail
    }

    #[test]
    fn test_is_valid_variable_name() {
        // Valid names
        assert!(FlowExecutor::is_valid_variable_name("description"));
        assert!(FlowExecutor::is_valid_variable_name("desc"));
        assert!(FlowExecutor::is_valid_variable_name("_private"));
        assert!(FlowExecutor::is_valid_variable_name("var123"));
        assert!(FlowExecutor::is_valid_variable_name("my_var"));
        assert!(FlowExecutor::is_valid_variable_name("CamelCase"));

        // Invalid names
        assert!(!FlowExecutor::is_valid_variable_name(""));
        assert!(!FlowExecutor::is_valid_variable_name("123var")); // starts with number
        assert!(!FlowExecutor::is_valid_variable_name("my-var")); // contains hyphen
        assert!(!FlowExecutor::is_valid_variable_name("my.var")); // contains dot
        assert!(!FlowExecutor::is_valid_variable_name("my var")); // contains space
        assert!(!FlowExecutor::is_valid_variable_name("$var")); // starts with $
    }

    #[test]
    fn test_execute_let_variable_aliasing() {
        let action = LetAction {
            let_: "desc = $event.file_path".to_string(),
            on_failure: FailureMode::Continue,
        };

        let context = AikiState::new(create_test_event_with_file("test.rs"));

        let result = FlowExecutor::execute_let(&action, &context).unwrap();
        assert!(result.success);
        assert_eq!(result.stdout, "test.rs");
    }

    #[test]
    fn test_execute_let_invalid_syntax() {
        let action = LetAction {
            let_: "invalid_syntax".to_string(), // Missing '='
            on_failure: FailureMode::Continue,
        };

        let event = create_test_event();
        let context = AikiState::new(event);

        let result = FlowExecutor::execute_let(&action, &context);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid let syntax"));
    }

    #[test]
    fn test_execute_let_invalid_variable_names() {
        let invalid_names = vec![
            "123var = value", // starts with number
            "my-var = value", // contains hyphen
            "my.var = value", // contains dot
            "my var = value", // contains space
            "$var = value",   // starts with $
            " = value",       // empty name
        ];

        for let_str in invalid_names {
            let action = LetAction {
                let_: let_str.to_string(),
                on_failure: FailureMode::Continue,
            };

            let event = create_test_event();
            let context = AikiState::new(event);

            let result = FlowExecutor::execute_let(&action, &context);
            assert!(result.is_err(), "Should reject: {}", let_str);
            assert!(
                result.unwrap_err().to_string().contains("Invalid variable"),
                "Should mention invalid variable for: {}",
                let_str
            );
        }
    }

    #[test]
    fn test_execute_let_whitespace_trimming() {
        let action = LetAction {
            let_: "  description  =  $event.file_path  ".to_string(),
            on_failure: FailureMode::Continue,
        };

        let context = AikiState::new(create_test_event_with_file("test.rs"));

        let result = FlowExecutor::execute_let(&action, &context).unwrap();
        assert!(result.success);
        assert_eq!(result.stdout, "test.rs");
    }

    #[test]
    fn test_let_variable_storage() {
        let actions = vec![
            Action::Let(LetAction {
                let_: "desc = $event.file_path".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Log(LogAction {
                log: "Variable: $desc".to_string(),
                alias: None,
            }),
        ];

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let results = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));

        // Check that the variable was stored
        assert_eq!(context.get_variable("desc"), Some(&"test.rs".to_string()));
    }

    #[test]
    fn test_shell_alias_stores_structured_metadata() {
        let actions = vec![Action::Shell(ShellAction {
            shell: "echo 'test output'".to_string(),
            timeout: None,
            on_failure: FailureMode::Continue,
            alias: Some("result".to_string()),
        })];

        let event = create_test_event();
        let mut context = AikiState::new(event);

        let results = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        // Check that the variable was stored
        assert!(context.get_variable("result").is_some());
        assert!(context
            .get_variable("result")
            .unwrap()
            .contains("test output"));

        // Check that structured metadata was stored
        assert!(context.get_metadata("result").is_some());
        assert!(context.get_metadata("result").unwrap().success);
    }

    #[test]
    fn test_let_creates_structured_metadata() {
        let actions = vec![Action::Let(LetAction {
            let_: "desc = $event.file_path".to_string(),
            on_failure: FailureMode::Continue,
        })];

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // Check that structured metadata was stored
        assert!(context.get_metadata("desc").is_some());
        let metadata = context.get_metadata("desc").unwrap();
        assert!(metadata.success);
        assert_eq!(metadata.stdout, "test.rs");
    }

    #[test]
    fn test_actions_without_alias_dont_store_variables() {
        let actions = vec![Action::Shell(ShellAction {
            shell: "echo 'test'".to_string(),
            timeout: None,
            on_failure: FailureMode::Continue,
            alias: None, // No alias
        })];

        let event = create_test_event();
        let mut context = AikiState::new(event);

        FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // Check that no extra variables were stored (except for any built-ins)
        // The metadata should be empty since no alias was provided
        #[cfg(test)]
        {
            assert!(context.get_metadata("result").is_none());
        }
    }

    #[test]
    fn test_let_with_context_vars() {
        // This test verifies that build_description works with typed events.
        // The type system now guarantees that PostChange events have all required fields.
        let action = LetAction {
            let_: "description = aiki/core.build_description".to_string(),
            on_failure: FailureMode::Stop,
        };

        let event = create_test_event();
        let mut context = AikiState::new(event);
        context.flow_name = Some("aiki/core".to_string());

        // This should succeed because PostChangeEvent has session_id and tool_name
        let result = FlowExecutor::execute_let(&action, &context).unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("[aiki]"));
    }

    #[test]
    fn test_let_creates_copy_not_reference() {
        // Verify aliasing behavior creates copies
        let actions = vec![
            Action::Let(LetAction {
                let_: "original = $event.file_path".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Let(LetAction {
                let_: "copy = $original".to_string(),
                on_failure: FailureMode::Continue,
            }),
        ];

        let mut context = AikiState::new(create_test_event());

        FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // Both should have the same value
        assert_eq!(
            context.get_variable("original"),
            Some(&"/tmp/file.rs".to_string())
        );
        assert_eq!(
            context.get_variable("copy"),
            Some(&"/tmp/file.rs".to_string())
        );

        // Modify original
        context.set_variable("original".to_string(), "modified".to_string());

        // Copy should still have original value (it's a copy, not a reference)
        assert_eq!(
            context.get_variable("copy"),
            Some(&"/tmp/file.rs".to_string())
        );
        assert_eq!(
            context.get_variable("original"),
            Some(&"modified".to_string())
        );
    }

    #[test]
    fn test_let_variable_shadowing() {
        // Verify that reassigning variables works correctly
        let actions = vec![
            Action::Let(LetAction {
                let_: "x = $event.tool_name".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Let(LetAction {
                let_: "x = $event.session_id".to_string(),
                on_failure: FailureMode::Continue,
            }),
        ];

        // PostChange event has tool_name and session_id fields
        let mut context = AikiState::new(create_test_event());

        FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // Second assignment should overwrite first
        assert_eq!(context.get_variable("x"), Some(&"test-session".to_string()));
    }

    #[test]
    fn test_let_aliasing_copies_all_structured_metadata() {
        let actions = vec![
            Action::Let(LetAction {
                let_: "file = $event.file_path".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Let(LetAction {
                let_: "copy = $file".to_string(),
                on_failure: FailureMode::Continue,
            }),
        ];

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // Both should have the value
        assert_eq!(context.get_variable("file"), Some(&"test.rs".to_string()));
        assert_eq!(context.get_variable("copy"), Some(&"test.rs".to_string()));

        // Both should have structured metadata
        assert!(context.get_metadata("file").is_some());
        assert!(context.get_metadata("copy").is_some());
    }

    #[test]
    fn test_let_self_reference() {
        let action = LetAction {
            let_: "description = self.build_description".to_string(),
            on_failure: FailureMode::Stop,
        };

        let event = create_test_event();
        let mut context = AikiState::new(event);
        context.flow_name = Some("aiki/core".to_string());

        let result = FlowExecutor::execute_let(&action, &context).unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("[aiki]"));
    }

    #[test]
    fn test_let_self_reference_without_flow_context() {
        let action = LetAction {
            let_: "description = self.build_description".to_string(),
            on_failure: FailureMode::Stop,
        };

        // No flow_name set
        let event = create_test_event();
        let context = AikiState::new(event);

        let result = FlowExecutor::execute_let(&action, &context);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no flow context available"));
    }

    #[test]
    fn test_let_variables_work_in_shell_actions() {
        let actions = vec![
            Action::Let(LetAction {
                let_: "my_var = $event.file_path".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Shell(ShellAction {
                shell: "echo $my_var".to_string(),
                timeout: None,
                on_failure: FailureMode::Continue,
                alias: None,
            }),
        ];

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let results = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));

        // Shell should have received the resolved variable
        assert!(results[1].stdout.contains("test.rs"));
    }

    #[test]
    fn test_let_variables_work_in_jj_actions() {
        let actions = vec![
            Action::Let(LetAction {
                let_: "msg = $event.message".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Jj(JjAction {
                jj: "log -r $msg".to_string(),
                timeout: None,
                on_failure: FailureMode::Continue,
                alias: None,
            }),
        ];

        let event = create_test_event();
        let mut context = AikiState::new(event);

        let results = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert_eq!(results.len(), 2);

        // First action (let) should succeed
        assert!(results[0].success);
    }

    #[test]
    fn test_let_variables_work_in_log_actions() {
        let actions = vec![
            Action::Let(LetAction {
                let_: "file = $event.file_path".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Log(LogAction {
                log: "Processing $file".to_string(),
                alias: None,
            }),
        ];

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let results = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
    }
}
