use anyhow::{Context, Result};
use std::process::Command;
use std::time::Duration;

use super::types::{
    Action, ActionResult, AikiAction, ExecutionContext, FailureMode, JjAction, LogAction,
    ShellAction,
};
use super::variables::VariableResolver;

/// Executes flow actions
pub struct FlowExecutor;

impl FlowExecutor {
    /// Execute a list of actions sequentially
    pub fn execute_actions(
        actions: &[Action],
        context: &mut ExecutionContext,
    ) -> Result<Vec<ActionResult>> {
        let mut results = Vec::new();

        for action in actions {
            let result = Self::execute_action(action, context)?;

            // Store step results for reference by subsequent actions
            match action {
                Action::Aiki(aiki_action) => {
                    let step_name = &aiki_action.aiki;
                    Self::store_step_result(context, step_name, &result);
                }
                Action::Shell(_) | Action::Jj(_) | Action::Log(_) => {
                    // For now, only aiki actions are referenceable
                    // Phase 5.2 will add support for all action types
                }
            }

            // Check failure mode
            let should_stop = match action {
                Action::Shell(shell_action) => {
                    !result.success && shell_action.on_failure == FailureMode::Fail
                }
                Action::Jj(jj_action) => {
                    !result.success && jj_action.on_failure == FailureMode::Fail
                }
                Action::Aiki(aiki_action) => {
                    !result.success && aiki_action.on_failure == FailureMode::Fail
                }
                Action::Log(_) => false, // Log actions never fail
            };

            results.push(result);

            if should_stop {
                anyhow::bail!("Action failed with on_failure: fail");
            }
        }

        Ok(results)
    }

    /// Execute a single action
    fn execute_action(action: &Action, context: &ExecutionContext) -> Result<ActionResult> {
        match action {
            Action::Shell(shell_action) => Self::execute_shell(shell_action, context),
            Action::Jj(jj_action) => Self::execute_jj(jj_action, context),
            Action::Log(log_action) => Self::execute_log(log_action, context),
            Action::Aiki(aiki_action) => Self::execute_aiki(aiki_action, context),
        }
    }

    /// Store step result as variables for subsequent actions
    ///
    /// Creates variables like:
    /// - $step_name.output
    /// - $step_name.exit_code
    /// - $step_name.failed
    fn store_step_result(context: &mut ExecutionContext, step_name: &str, result: &ActionResult) {
        // Store output
        if !result.stdout.is_empty() {
            context
                .event_vars
                .insert(format!("{}.output", step_name), result.stdout.clone());
        }

        // Store exit code
        if let Some(exit_code) = result.exit_code {
            context
                .event_vars
                .insert(format!("{}.exit_code", step_name), exit_code.to_string());
        }

        // Store failed status
        context.event_vars.insert(
            format!("{}.failed", step_name),
            (!result.success).to_string(),
        );

        // Store result status
        context.event_vars.insert(
            format!("{}.result", step_name),
            if result.success { "success" } else { "failed" }.to_string(),
        );
    }

    /// Execute a shell command
    fn execute_shell(action: &ShellAction, context: &ExecutionContext) -> Result<ActionResult> {
        // Create variable resolver
        let mut resolver = VariableResolver::new();
        resolver.add_event_vars(&context.event_vars);
        resolver.add_var("cwd", context.cwd.to_string_lossy().to_string());
        resolver.add_env_vars(&context.env_vars);

        // Resolve variables in command
        let command = resolver.resolve(&action.shell);

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Executing shell: {}", command);
        }

        // Execute command
        let output = if let Some(timeout_str) = &action.timeout {
            // Parse timeout (e.g., "30s", "1m")
            let timeout = parse_timeout(timeout_str)?;
            execute_with_timeout(&command, &context.cwd, timeout)?
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(&command)
                .current_dir(&context.cwd)
                .envs(&context.env_vars)
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
    fn execute_jj(action: &JjAction, context: &ExecutionContext) -> Result<ActionResult> {
        // Create variable resolver
        let mut resolver = VariableResolver::new();
        resolver.add_event_vars(&context.event_vars);
        resolver.add_var("cwd", context.cwd.to_string_lossy().to_string());
        resolver.add_env_vars(&context.env_vars);

        // Resolve variables in command
        let jj_args = resolver.resolve(&action.jj);

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Executing jj: {}", jj_args);
        }

        // Parse arguments
        let args: Vec<&str> = jj_args.split_whitespace().collect();

        // Execute JJ command
        let output = if let Some(timeout_str) = &action.timeout {
            let timeout = parse_timeout(timeout_str)?;
            let full_command = format!("jj {}", jj_args);
            execute_with_timeout(&full_command, &context.cwd, timeout)?
        } else {
            Command::new("jj")
                .args(&args)
                .current_dir(&context.cwd)
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
    fn execute_log(action: &LogAction, context: &ExecutionContext) -> Result<ActionResult> {
        // Create variable resolver
        let mut resolver = VariableResolver::new();
        resolver.add_event_vars(&context.event_vars);
        resolver.add_var("cwd", context.cwd.to_string_lossy().to_string());
        resolver.add_env_vars(&context.env_vars);

        // Resolve variables in message
        let message = resolver.resolve(&action.log);

        // Print to stderr (so it appears in hook output)
        eprintln!("[aiki] {}", message);

        Ok(ActionResult::success())
    }

    /// Execute a built-in Aiki function
    fn execute_aiki(action: &AikiAction, context: &ExecutionContext) -> Result<ActionResult> {
        // Create variable resolver
        let mut resolver = VariableResolver::new();
        resolver.add_event_vars(&context.event_vars);
        resolver.add_var("cwd", context.cwd.to_string_lossy().to_string());
        resolver.add_env_vars(&context.env_vars);

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Executing aiki: {}", action.aiki);
        }

        // Resolve variables in arguments
        let mut resolved_args = std::collections::HashMap::new();
        for (key, value) in &action.args {
            resolved_args.insert(key.clone(), resolver.resolve(value));
        }

        // Route to appropriate aiki function
        match action.aiki.as_str() {
            "build_provenance_description" => {
                Self::aiki_build_provenance_description(&resolved_args, context)
            }
            _ => anyhow::bail!("Unknown aiki function: {}", action.aiki),
        }
    }

    /// Aiki function: Build provenance description
    fn aiki_build_provenance_description(
        args: &std::collections::HashMap<String, String>,
        context: &ExecutionContext,
    ) -> Result<ActionResult> {
        use crate::provenance::{
            AgentInfo, AgentType, AttributionConfidence, DetectionMethod, ProvenanceRecord,
        };

        // Extract required arguments
        let agent_str = args
            .get("agent")
            .ok_or_else(|| anyhow::anyhow!("Missing 'agent' argument"))?;

        let session_id = args
            .get("session_id")
            .ok_or_else(|| anyhow::anyhow!("Missing 'session_id' argument"))?;

        let tool_name = args
            .get("tool_name")
            .ok_or_else(|| anyhow::anyhow!("Missing 'tool_name' argument"))?;

        // Parse agent type
        let agent_type = match agent_str.as_str() {
            "ClaudeCode" => AgentType::ClaudeCode,
            "Cursor" => AgentType::Cursor,
            _ => AgentType::Unknown,
        };

        // Build provenance record
        let provenance = ProvenanceRecord {
            agent: AgentInfo {
                agent_type,
                version: None,
                detected_at: chrono::Utc::now(),
                confidence: AttributionConfidence::High,
                detection_method: DetectionMethod::Hook,
            },
            session_id: session_id.clone(),
            tool_name: tool_name.clone(),
        };

        // Generate description
        let description = provenance.to_description();

        // Store result in context as a variable for use by subsequent actions
        // We'll return it as stdout so it can be captured
        Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: description,
            stderr: String::new(),
        })
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
        anyhow::bail!(
            "Invalid timeout format: {}. Use 's', 'm', or 'h' suffix",
            timeout_str
        );
    }
}

/// Execute command with timeout
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

    rx.recv_timeout(timeout)
        .context("Command timed out")?
        .context("Failed to execute command")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        };

        let context = ExecutionContext::new(PathBuf::from("/tmp"));

        let result = FlowExecutor::execute_log(&action, &context).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_execute_log_with_variables() {
        let action = LogAction {
            log: "File: $event.file_path".to_string(),
        };

        let context =
            ExecutionContext::new(PathBuf::from("/tmp")).with_event_var("file_path", "test.rs");

        let result = FlowExecutor::execute_log(&action, &context).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_execute_shell_echo() {
        let action = ShellAction {
            shell: "echo 'test'".to_string(),
            timeout: None,
            on_failure: FailureMode::Continue,
        };

        let context = ExecutionContext::new(PathBuf::from("/tmp"));

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
        };

        let context =
            ExecutionContext::new(PathBuf::from("/tmp")).with_event_var("file_path", "test.rs");

        let result = FlowExecutor::execute_shell(&action, &context).unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("test.rs"));
    }

    #[test]
    fn test_execute_actions_sequential() {
        let actions = vec![
            Action::Log(LogAction {
                log: "Step 1".to_string(),
            }),
            Action::Shell(ShellAction {
                shell: "echo 'Step 2'".to_string(),
                timeout: None,
                on_failure: FailureMode::Continue,
            }),
            Action::Log(LogAction {
                log: "Step 3".to_string(),
            }),
        ];

        let mut context = ExecutionContext::new(PathBuf::from("/tmp"));

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
            }),
            Action::Log(LogAction {
                log: "This should still run".to_string(),
            }),
        ];

        let mut context = ExecutionContext::new(PathBuf::from("/tmp"));

        let results = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert_eq!(results.len(), 2);
        assert!(!results[0].success); // First action failed
        assert!(results[1].success); // Second action succeeded
    }

    #[test]
    fn test_execute_actions_fail_mode_fail() {
        let actions = vec![
            Action::Shell(ShellAction {
                shell: "false".to_string(), // This command fails
                timeout: None,
                on_failure: FailureMode::Fail, // Stop on failure
            }),
            Action::Log(LogAction {
                log: "This should NOT run".to_string(),
            }),
        ];

        let mut context = ExecutionContext::new(PathBuf::from("/tmp"));

        let result = FlowExecutor::execute_actions(&actions, &mut context);
        assert!(result.is_err()); // Should fail
    }
}
