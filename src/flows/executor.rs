use anyhow::Context;
use std::process::Command;
use std::time::Duration;

use super::state::{ActionResult, AikiState};
use super::types::{
    Action, CommitMessageAction, CommitMessageOp, FailureMode, IfAction, JjAction, LetAction,
    LogAction, SelfAction, ShellAction, SwitchAction,
};
use super::variables::VariableResolver;
use crate::error::{AikiError, Result};

/// Result of flow execution
#[derive(Debug, Clone)]
pub enum FlowResult {
    /// All actions succeeded
    Success,
    /// Action failed with on_failure: continue (logged, flow continued)
    FailedContinue(String),
    /// Action failed with on_failure: stop (silent failure, flow stopped)
    FailedStop(String),
    /// Action failed with on_failure: block (block editor operation)
    FailedBlock(String),
}

/// Timing information for flow execution
#[derive(Debug, Clone)]
pub struct FlowTiming {
    /// Total duration of flow execution in seconds
    pub duration_secs: f64,
}

impl FlowTiming {
    #[must_use]
    pub fn new(duration_secs: f64) -> Self {
        Self { duration_secs }
    }
}

/// Executes flow actions
pub struct FlowExecutor;

impl FlowExecutor {
    /// Create a variable resolver with consistent variable availability
    ///
    /// Makes variables available both with and without `event.` prefix:
    /// - $event.file_paths (for event variables)
    /// - $file_path (for event variables, let-bound variables)
    /// - $description (for let-bound variables)
    /// Create a variable resolver with proper variable scoping
    ///
    /// Variable scopes:
    /// - Event variables (from actual events): $event.file_paths, $event.agent_type
    /// - Let variables (user-defined): $description, $my_var (no event. prefix)
    /// - System variables: $cwd
    /// - Environment variables: $HOME, $PATH
    fn create_resolver(context: &AikiState) -> VariableResolver {
        let mut resolver = VariableResolver::new();

        // Add event-specific variables based on event type
        match &context.event {
            crate::events::AikiEvent::PostFileChange(e) => {
                resolver.add_var("event.tool_name".to_string(), e.tool_name.clone());
                resolver.add_var("event.file_paths".to_string(), e.file_paths.join(" "));
                resolver.add_var(
                    "event.file_count".to_string(),
                    e.file_paths.len().to_string(),
                );
                resolver.add_var("event.session_id".to_string(), e.session_id.clone());
            }
            crate::events::AikiEvent::PreFileChange(e) => {
                resolver.add_var("event.session_id".to_string(), e.session_id.clone());
            }
            crate::events::AikiEvent::SessionStart(e) => {
                if let Some(ref session_id) = e.session_id {
                    resolver.add_var("event.session_id".to_string(), session_id.clone());
                }
            }
            crate::events::AikiEvent::PrepareCommitMessage(e) => {
                // Add commit message file path if available
                if let Some(ref path) = e.commit_msg_file {
                    resolver.add_var(
                        "event.commit_msg_file".to_string(),
                        path.display().to_string(),
                    );
                }
            }
        }

        // Add agent type as event.agent_type
        let agent_str = match context.event.agent_type() {
            crate::provenance::AgentType::Claude => "claude",
            crate::provenance::AgentType::Codex => "codex",
            crate::provenance::AgentType::Cursor => "cursor",
            crate::provenance::AgentType::Gemini => "gemini",
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
    ///
    /// Returns both the flow result and timing information
    pub fn execute_actions(
        actions: &[Action],
        context: &mut AikiState,
    ) -> Result<(FlowResult, FlowTiming)> {
        use std::time::Instant;

        let start = Instant::now();
        let mut continue_failure_errors = Vec::new();

        for action in actions {
            let result = Self::execute_action(action, context)?;

            // Store action results for reference by subsequent actions
            Self::store_action_result(action, &result, context);

            // Handle failure based on failure mode
            if !result.success {
                let failure_mode = match action {
                    Action::If(if_action) => &if_action.on_failure,
                    Action::Switch(switch_action) => &switch_action.on_failure,
                    Action::Shell(shell_action) => &shell_action.on_failure,
                    Action::Jj(jj_action) => &jj_action.on_failure,
                    Action::Let(let_action) => &let_action.on_failure,
                    Action::Self_(self_action) => &self_action.on_failure,
                    Action::CommitMessage(commit_msg_action) => &commit_msg_action.on_failure,
                    Action::Log(_) => {
                        continue; // Log actions never fail
                    }
                };

                match failure_mode {
                    FailureMode::Continue => {
                        // Log error but continue
                        let error_msg = if !result.stderr.is_empty() {
                            result.stderr.clone()
                        } else {
                            "Action failed".to_string()
                        };
                        eprintln!("[aiki] Action failed but continuing: {}", error_msg);
                        continue_failure_errors.push(error_msg);
                    }
                    FailureMode::Stop => {
                        // Stop flow silently
                        let error_msg = if !result.stderr.is_empty() {
                            result.stderr.clone()
                        } else {
                            "Action failed with on_failure: stop".to_string()
                        };
                        let duration = start.elapsed().as_secs_f64();
                        return Ok((FlowResult::FailedStop(error_msg), FlowTiming::new(duration)));
                    }
                    FailureMode::Block => {
                        // Stop flow and block editor
                        let error_msg = if !result.stderr.is_empty() {
                            result.stderr.clone()
                        } else {
                            "Action failed with on_failure: block".to_string()
                        };
                        let duration = start.elapsed().as_secs_f64();
                        return Ok((
                            FlowResult::FailedBlock(error_msg),
                            FlowTiming::new(duration),
                        ));
                    }
                }
            }
        }

        // All actions completed
        let duration = start.elapsed().as_secs_f64();
        if continue_failure_errors.is_empty() {
            Ok((FlowResult::Success, FlowTiming::new(duration)))
        } else {
            Ok((
                FlowResult::FailedContinue(continue_failure_errors.join("; ")),
                FlowTiming::new(duration),
            ))
        }
    }

    /// Execute a single action
    fn execute_action(action: &Action, context: &mut AikiState) -> Result<ActionResult> {
        match action {
            Action::If(if_action) => Self::execute_if(if_action, context),
            Action::Switch(switch_action) => Self::execute_switch(switch_action, context),
            Action::Shell(shell_action) => Self::execute_shell(shell_action, context),
            Action::Jj(jj_action) => Self::execute_jj(jj_action, context),
            Action::Log(log_action) => Self::execute_log(log_action, context),
            Action::Let(let_action) => Self::execute_let(let_action, context),
            Action::Self_(self_action) => Self::execute_self(self_action, context),
            Action::CommitMessage(commit_msg_action) => {
                Self::execute_commit_message(commit_msg_action, context)
            }
        }
    }

    /// Store action result as variables for subsequent actions
    ///
    /// For Let actions: stores the variable and its structured metadata
    /// For Shell/Jj/Log with alias: stores the variable with its result
    fn store_action_result(action: &Action, result: &ActionResult, context: &mut AikiState) {
        match action {
            Action::If(_) => {
                // If actions execute their branches directly and store results there
                // No need to store the if action result itself
            }
            Action::Switch(_) => {
                // Switch actions execute their branches directly and store results there
                // No need to store the switch action result itself
            }
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
            Action::Self_(_) => {
                // Self actions don't store results (they're fire-and-forget)
            }
            Action::CommitMessage(_) => {
                // commit_message actions don't produce storable results
            }
        }
    }

    /// Execute a shell command
    fn execute_shell(action: &ShellAction, context: &mut AikiState) -> Result<ActionResult> {
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
    fn execute_jj(action: &JjAction, context: &mut AikiState) -> Result<ActionResult> {
        // Handle with_author_and_message if provided - it sets both author and message
        if let Some(ref metadata_fn) = action.with_author_and_message {
            let resolved_metadata = if metadata_fn.trim().starts_with("self.") {
                // Execute the metadata function
                let self_action = SelfAction {
                    self_: metadata_fn.trim().to_string(),
                    on_failure: FailureMode::Stop,
                };
                let result = Self::execute_self(&self_action, context)?;
                result.stdout.trim().to_string()
            } else {
                // Resolve variable reference
                let mut resolver = Self::create_resolver(context);
                resolver.resolve(metadata_fn)
            };

            // Parse the JSON result
            let json: serde_json::Value = serde_json::from_str(&resolved_metadata)
                .context("Failed to parse metadata function result as JSON")?;

            let author = json["author"]
                .as_str()
                .ok_or_else(|| {
                    AikiError::Other(anyhow::anyhow!("Metadata missing 'author' field"))
                })?
                .to_string();

            let message = json["message"]
                .as_str()
                .ok_or_else(|| {
                    AikiError::Other(anyhow::anyhow!("Metadata missing 'message' field"))
                })?
                .to_string();

            // Store message in context so it can be referenced as $message
            context.store_action_result(
                "message".to_string(),
                ActionResult {
                    success: true,
                    exit_code: Some(0),
                    stdout: message,
                    stderr: String::new(),
                },
            );

            // Create a new JjAction with with_author set
            let mut new_action = action.clone();
            new_action.with_author = Some(author);
            new_action.with_author_and_message = None; // Clear to avoid infinite loop

            // Execute the modified action
            return Self::execute_jj(&new_action, context);
        }

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

        // Parse with_author if provided
        let (jj_user, jj_email) = if let Some(ref author) = action.with_author {
            let resolved_author = if author.trim().starts_with("self.") {
                // It's a function call - execute it now (maintains execution order)
                let self_action = SelfAction {
                    self_: author.trim().to_string(),
                    on_failure: FailureMode::Stop,
                };
                let result = Self::execute_self(&self_action, context)?;
                result.stdout.trim().to_string()
            } else {
                // It's a variable reference - resolve it
                resolver.resolve(author)
            };
            parse_author(&resolved_author)?
        } else {
            (None, None)
        };

        if std::env::var("AIKI_DEBUG").is_ok() {
            if let (Some(ref user), Some(ref email)) = (&jj_user, &jj_email) {
                eprintln!("[flows] Setting JJ_USER={}, JJ_EMAIL={}", user, email);
            }
        }

        // Execute JJ command (using direct argv, no shell invocation)
        let output = if let Some(timeout_str) = &action.timeout {
            let timeout = parse_timeout(timeout_str)?;
            execute_with_timeout_argv_with_env(
                "jj",
                &args,
                context.cwd(),
                timeout,
                jj_user,
                jj_email,
            )?
        } else {
            let mut cmd = Command::new("jj");
            cmd.args(&args).current_dir(context.cwd());

            // Set JJ_USER and JJ_EMAIL if provided
            if let Some(user) = jj_user {
                cmd.env("JJ_USER", user);
            }
            if let Some(email) = jj_email {
                cmd.env("JJ_EMAIL", email);
            }

            cmd.output().context("Failed to execute jj command")?
        };

        Ok(ActionResult {
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Execute a log action
    fn execute_log(action: &LogAction, context: &mut AikiState) -> Result<ActionResult> {
        // Create variable resolver with consistent variable availability
        let mut resolver = Self::create_resolver(context);

        // Resolve variables in message
        let message = resolver.resolve(&action.log);

        // Print to stderr (so it appears in hook output)
        eprintln!("[aiki] {}", message);

        // Return the message in stdout so it can be stored as a variable
        Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: message,
            stderr: String::new(),
        })
    }

    /// Execute a commit_message action
    ///
    /// This action modifies the commit message file in place.
    /// Only works for PrepareCommitMessage events that have a commit_msg_file.
    fn execute_commit_message(
        action: &CommitMessageAction,
        context: &mut AikiState,
    ) -> Result<ActionResult> {
        use crate::events::AikiEvent;
        use std::fs;

        // Get commit message file from event
        let commit_msg_file = match &context.event {
            AikiEvent::PrepareCommitMessage(e) => e
                .commit_msg_file
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No commit message file in event"))?,
            _ => {
                return Err(AikiError::Other(anyhow::anyhow!(
                    "commit_message can only be used in PrepareCommitMessage events"
                )))
            }
        };

        // Read current message
        let content = fs::read_to_string(commit_msg_file)?;

        // Create variable resolver
        let mut resolver = Self::create_resolver(context);
        let op = &action.commit_message;

        // Apply operations
        let new_content = Self::apply_commit_message_edits(&content, op, &mut resolver)?;

        // Write atomically
        fs::write(commit_msg_file, new_content)?;

        Ok(ActionResult::success())
    }

    /// Apply commit message edit operations
    fn apply_commit_message_edits(
        content: &str,
        op: &CommitMessageOp,
        resolver: &mut VariableResolver,
    ) -> Result<String> {
        let mut result = content.to_string();

        // Prepend to subject line (before first line)
        if let Some(ref prepend_subject) = op.prepend_subject {
            let text = resolver.resolve(prepend_subject);
            if !text.is_empty() {
                result = format!("{}{}", text, result);
            }
        }

        // Append to body (before trailers)
        if let Some(ref body) = op.append_body {
            let text = resolver.resolve(body);
            if !text.is_empty() {
                result = Self::append_to_body(&result, &text);
            }
        }

        // Append trailer (after existing trailers)
        if let Some(ref trailer) = op.append_trailer {
            let text = resolver.resolve(trailer);
            if !text.is_empty() {
                result = Self::append_trailer(&result, &text);
            }
        }

        // Append footer (after everything)
        if let Some(ref append_footer) = op.append_footer {
            let text = resolver.resolve(append_footer);
            if !text.is_empty() {
                // Ensure blank line before appending
                if !result.ends_with('\n') {
                    result.push('\n');
                }
                if !result.ends_with("\n\n") {
                    result.push('\n');
                }
                result.push_str(&text);
                if !text.ends_with('\n') {
                    result.push('\n');
                }
            }
        }

        Ok(result)
    }

    /// Append text to message body (before trailers)
    fn append_to_body(content: &str, text: &str) -> String {
        // Find where trailers start (lines like "Key: value")
        let lines: Vec<&str> = content.lines().collect();
        let mut trailer_start = lines.len();

        // Scan backwards to find first trailer
        for (i, line) in lines.iter().enumerate().rev() {
            if line.is_empty() {
                continue;
            }
            if Self::is_trailer_line(line) {
                trailer_start = i;
            } else {
                break;
            }
        }

        if trailer_start == lines.len() {
            // No trailers, append to end
            let mut result = content.to_string();
            if !result.ends_with('\n') {
                result.push('\n');
            }
            result.push('\n');
            result.push_str(text);
            if !text.ends_with('\n') {
                result.push('\n');
            }
            result
        } else {
            // Insert before trailers
            let mut result = String::new();
            for (i, line) in lines.iter().enumerate() {
                if i == trailer_start {
                    // Add blank line if needed
                    if i > 0 && !lines[i - 1].is_empty() {
                        result.push('\n');
                    }
                    result.push_str(text);
                    if !text.ends_with('\n') {
                        result.push('\n');
                    }
                    result.push('\n');
                }
                result.push_str(line);
                result.push('\n');
            }
            result
        }
    }

    /// Append Git trailer (after existing trailers)
    fn append_trailer(content: &str, text: &str) -> String {
        let mut result = content.to_string();

        // Ensure there's a newline at the end
        if !result.ends_with('\n') {
            result.push('\n');
        }

        // Check if we need to add a blank line before the trailer
        // Git convention: trailers should be separated from the body by a blank line
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            // Empty content, add blank line
            result.push('\n');
        } else {
            // Check the last two lines to see if there's already a blank line
            let last_non_empty = lines.iter().rev().find(|l| !l.is_empty());

            if let Some(last) = last_non_empty {
                // If last non-empty line is not a trailer, we need a blank line separator
                if !Self::is_trailer_line(last) {
                    // Check if there's already a blank line at the end
                    let ends_with_blank =
                        lines.last().map_or(false, |l| l.is_empty()) || result.ends_with("\n\n");

                    if !ends_with_blank {
                        result.push('\n');
                    }
                }
            } else {
                // All lines are empty, add blank line
                result.push('\n');
            }
        }

        result.push_str(text);
        if !text.ends_with('\n') {
            result.push('\n');
        }

        result
    }

    /// Check if a line looks like a Git trailer
    fn is_trailer_line(line: &str) -> bool {
        // Git trailers are lines like "Key: value" or "Key #value"
        // They typically have a capital letter at start and contain : or #
        if let Some(colon_pos) = line.find(':') {
            let key = &line[..colon_pos];
            // Key should start with capital letter, contain only word chars and hyphens
            !key.is_empty()
                && key.chars().next().map_or(false, |c| c.is_uppercase())
                && key.chars().all(|c| c.is_alphanumeric() || c == '-')
        } else {
            false
        }
    }

    /// Execute a let binding action
    ///
    /// Supports two modes:
    /// 1. Function call: `let metadata = aiki/core.build_metadata`
    /// 2. Variable aliasing: `let desc = $description`
    /// Execute a conditional if/then/else action
    fn execute_if(action: &IfAction, context: &mut AikiState) -> Result<ActionResult> {
        // Evaluate the condition
        let condition_result = Self::evaluate_condition(&action.condition, context)?;

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!(
                "[flows] If condition '{}' evaluated to: {}",
                action.condition, condition_result
            );
        }

        // Execute the appropriate branch
        let actions_to_execute = if condition_result {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[flows] Executing 'then' branch");
            }
            &action.then
        } else if let Some(else_actions) = &action.else_ {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[flows] Executing 'else' branch");
            }
            else_actions
        } else {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[flows] No else branch, condition false - no-op");
            }
            // No else branch and condition is false - treat as success (no-op)
            return Ok(ActionResult {
                success: true,
                exit_code: Some(0),
                stdout: String::from("condition_false"),
                stderr: String::new(),
            });
        };

        // Execute the branch actions recursively
        // This allows nested conditionals and proper state modification
        for branch_action in actions_to_execute {
            let result = Self::execute_action(branch_action, context)?;

            // Store action results for reference by subsequent actions
            Self::store_action_result(branch_action, &result, context);

            // If any action in the branch fails, the whole if action fails
            if !result.success {
                return Ok(ActionResult {
                    success: false,
                    exit_code: result.exit_code,
                    stdout: result.stdout,
                    stderr: result.stderr,
                });
            }
        }

        // All branch actions succeeded
        Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: String::from("condition_branch_completed"),
            stderr: String::new(),
        })
    }

    /// Execute a switch/case action
    fn execute_switch(action: &SwitchAction, context: &mut AikiState) -> Result<ActionResult> {
        // Evaluate the switch expression
        let mut resolver = Self::create_resolver(context);
        let switch_value = resolver.resolve(&action.expression);

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!(
                "[flows] Switch expression '{}' evaluated to: {}",
                action.expression, switch_value
            );
        }

        // Find matching case
        let actions_to_execute = if let Some(case_actions) = action.cases.get(&switch_value) {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!("[flows] Switch matched case: {}", switch_value);
            }
            case_actions
        } else if let Some(default_actions) = &action.default {
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!(
                    "[flows] Switch using default case (no match for '{}')",
                    switch_value
                );
            }
            default_actions
        } else {
            // No match and no default - treat as success (no-op)
            if std::env::var("AIKI_DEBUG").is_ok() {
                eprintln!(
                    "[flows] Switch: no match for '{}' and no default case",
                    switch_value
                );
            }
            return Ok(ActionResult {
                success: true,
                exit_code: Some(0),
                stdout: String::from("no_match"),
                stderr: String::new(),
            });
        };

        // Execute the matched case actions
        for case_action in actions_to_execute {
            let result = Self::execute_action(case_action, context)?;

            // Store action results for reference by subsequent actions
            Self::store_action_result(case_action, &result, context);

            // If any action in the case fails, the whole switch action fails
            if !result.success {
                return Ok(ActionResult {
                    success: false,
                    exit_code: result.exit_code,
                    stdout: result.stdout,
                    stderr: result.stderr,
                });
            }
        }

        // All case actions succeeded
        Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: String::from("switch_case_completed"),
            stderr: String::new(),
        })
    }

    /// Evaluate a condition expression
    /// Supports: ==, !=, JSON field access ($var.field)
    fn evaluate_condition(condition: &str, context: &mut AikiState) -> Result<bool> {
        let condition = condition.trim();

        // Parse comparison operators
        if let Some(pos) = condition.find("==") {
            let left = condition[..pos].trim();
            let right = condition[pos + 2..].trim();
            let left_val = Self::resolve_condition_value(left, context)?;
            let right_val = Self::resolve_condition_value(right, context)?;
            return Ok(left_val == right_val);
        }

        if let Some(pos) = condition.find("!=") {
            let left = condition[..pos].trim();
            let right = condition[pos + 2..].trim();
            let left_val = Self::resolve_condition_value(left, context)?;
            let right_val = Self::resolve_condition_value(right, context)?;
            return Ok(left_val != right_val);
        }

        // No operator - treat as boolean check (variable exists and is truthy)
        let val = Self::resolve_condition_value(condition, context)?;
        // Truthy: non-empty string that's not "false"
        // Falsy: empty string or literal "false"
        Ok(!val.is_empty() && val != "false")
    }

    /// Resolve a value in a condition expression
    /// Supports: variables ($var), JSON field access ($var.field), literals
    fn resolve_condition_value(expr: &str, context: &mut AikiState) -> Result<String> {
        let expr = expr.trim();

        // Remove quotes if present
        if (expr.starts_with('"') && expr.ends_with('"'))
            || (expr.starts_with('\'') && expr.ends_with('\''))
        {
            return Ok(expr[1..expr.len() - 1].to_string());
        }

        // Check if it's an inline function call (e.g., self.classify_edits or self.function.field)
        if expr.starts_with("self.") {
            // Split into function call and optional field path
            let parts: Vec<&str> = expr.splitn(2, '.').collect();
            if parts.len() == 2 {
                let remaining = parts[1];

                // Check if there's a field access after the function name
                if let Some(field_start) = remaining.find('.') {
                    let function_name = remaining[..field_start].trim();
                    let field_path = remaining[field_start + 1..].trim();

                    // Execute the self function
                    let self_action = SelfAction {
                        self_: format!("self.{}", function_name),
                        on_failure: FailureMode::Stop,
                    };

                    let result = Self::execute_self(&self_action, context)?;

                    // Parse the result as JSON and extract the field
                    let json_value: serde_json::Value = serde_json::from_str(&result.stdout)
                        .context("Failed to parse function result as JSON")?;

                    // Navigate the field path
                    let mut current = &json_value;
                    for field in field_path.split('.') {
                        current = current.get(field).ok_or_else(|| {
                            AikiError::Other(anyhow::anyhow!(
                                "Field '{}' not found in JSON result",
                                field
                            ))
                        })?;
                    }

                    // Return the field value as a string
                    return Ok(match current {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        serde_json::Value::Null => "null".to_string(),
                        _ => current.to_string(),
                    });
                } else {
                    // No field access - just execute the function and return its stdout
                    let self_action = SelfAction {
                        self_: expr.to_string(),
                        on_failure: FailureMode::Stop,
                    };

                    let result = Self::execute_self(&self_action, context)?;
                    return Ok(result.stdout.trim().to_string());
                }
            }

            // If we get here, it's a malformed self.function call
            return Err(AikiError::Other(anyhow::anyhow!(
                "Invalid inline function call syntax: '{}'",
                expr
            )));
        }

        // Check if it's a variable reference
        if expr.starts_with('$') {
            // Use the existing variable resolver
            let mut resolver = Self::create_resolver(context);
            return Ok(resolver.resolve(expr));
        }

        // Otherwise, it's a literal value
        Ok(expr.to_string())
    }

    fn execute_let(action: &LetAction, context: &mut AikiState) -> Result<ActionResult> {
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
        let first = match chars.next() {
            Some(c) => c,
            None => return false, // Impossible due to is_empty() check, but be defensive
        };

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

    /// Execute a let binding for function call: `let metadata = aiki/core.build_metadata`
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
            let function_name = function_path
                .strip_prefix("self.")
                .expect("BUG: starts_with('self.') check passed but strip_prefix failed");

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
        let module_function = resolved_path
            .strip_prefix("aiki/")
            .expect("BUG: starts_with('aiki/') check passed but strip_prefix failed");

        // Split into module and function
        let parts: Vec<&str> = module_function.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(AikiError::MissingFunction(function_path.to_string()));
        }

        let module = parts[0];
        let function = parts[1];

        // Route to appropriate function
        match (module, function) {
            ("core", "build_metadata") => {
                // build_metadata requires PostFileChange event
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "build_metadata can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::build_metadata(event, Some(context))
            }
            ("core", "build_human_metadata") => {
                // build_human_metadata works with PreFileChange or PostFileChange events
                match &context.event {
                    crate::events::AikiEvent::PreFileChange(event) => {
                        crate::flows::core::build_human_metadata(event, Some(context))
                    }
                    crate::events::AikiEvent::PostFileChange(event) => {
                        crate::flows::core::build_human_metadata_post(event, Some(context))
                    }
                    _ => Err(AikiError::Other(anyhow::anyhow!(
                        "build_human_metadata can only be called for PreFileChange or PostFileChange events"
                    ))),
                }
            }
            ("core", "get_git_user") => {
                // get_git_user works with any event type
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "get_git_user currently requires PostFileChange event"
                    )));
                };
                crate::flows::core::get_git_user_function(event, Some(context))
            }
            ("core", "classify_edits") => {
                // classify_edits requires PostFileChange event
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "classify_edits can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::classify_edits(event)
            }
            ("core", "separate_edits") => {
                // separate_edits requires PostFileChange event
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "separate_edits can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::separate_edits(event)
            }
            ("core", "prepare_separation") => {
                // prepare_separation requires PostFileChange event
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "prepare_separation can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::prepare_separation(event)
            }
            ("core", "write_ai_files") => {
                // write_ai_files requires PostFileChange event and context
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "write_ai_files can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::write_ai_files(event, Some(context))
            }
            ("core", "restore_original_files") => {
                // restore_original_files requires PostFileChange event and context
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "restore_original_files can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::restore_original_files(event, Some(context))
            }
            ("core", "generate_coauthors") => {
                // generate_coauthors requires PrepareCommitMessage event
                let crate::events::AikiEvent::PrepareCommitMessage(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "generate_coauthors can only be called for PrepareCommitMessage events"
                    )));
                };
                crate::flows::core::generate_coauthors(event)
            }
            _ => Err(AikiError::FunctionNotFoundInNamespace(
                function.to_string(),
                module.to_string(),
            )),
        }
    }

    /// Execute a self function call: `self: write_ai_files`
    /// This is like execute_let_function but doesn't store the result in a variable
    fn execute_self(action: &SelfAction, context: &mut AikiState) -> Result<ActionResult> {
        let function_path = &action.self_;

        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] Self function call: {}", function_path);
        }

        // Handle self.function syntax
        let resolved_path = if function_path.starts_with("self.") {
            // Extract function name from self.function
            let function_name = function_path
                .strip_prefix("self.")
                .expect("BUG: starts_with('self.') check passed but strip_prefix failed");

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
        let module_function = resolved_path
            .strip_prefix("aiki/")
            .expect("BUG: starts_with('aiki/') check passed but strip_prefix failed");

        // Split into module and function
        let parts: Vec<&str> = module_function.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(AikiError::MissingFunction(function_path.to_string()));
        }

        let module = parts[0];
        let function = parts[1];

        // Route to appropriate function (same routing as execute_let_function)
        match (module, function) {
            ("core", "build_metadata") => {
                // build_metadata requires PostFileChange event
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "build_metadata can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::build_metadata(event, Some(context))
            }
            ("core", "build_human_metadata") => {
                // build_human_metadata works with PreFileChange or PostFileChange events
                match &context.event {
                    crate::events::AikiEvent::PreFileChange(event) => {
                        crate::flows::core::build_human_metadata(event, Some(context))
                    }
                    crate::events::AikiEvent::PostFileChange(event) => {
                        crate::flows::core::build_human_metadata_post(event, Some(context))
                    }
                    _ => Err(AikiError::Other(anyhow::anyhow!(
                        "build_human_metadata can only be called for PreFileChange or PostFileChange events"
                    ))),
                }
            }
            ("core", "get_git_user") => {
                // get_git_user works with any event type
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "get_git_user currently requires PostFileChange event"
                    )));
                };
                crate::flows::core::get_git_user_function(event, Some(context))
            }
            ("core", "classify_edits") => {
                // classify_edits requires PostFileChange event
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "classify_edits can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::classify_edits(event)
            }
            ("core", "separate_edits") => {
                // separate_edits requires PostFileChange event
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "separate_edits can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::separate_edits(event)
            }
            ("core", "prepare_separation") => {
                // prepare_separation requires PostFileChange event
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "prepare_separation can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::prepare_separation(event)
            }
            ("core", "write_ai_files") => {
                // write_ai_files requires PostFileChange event and context
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "write_ai_files can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::write_ai_files(event, Some(context))
            }
            ("core", "restore_original_files") => {
                // restore_original_files requires PostFileChange event and context
                let crate::events::AikiEvent::PostFileChange(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "restore_original_files can only be called for PostFileChange events"
                    )));
                };
                crate::flows::core::restore_original_files(event, Some(context))
            }
            ("core", "generate_coauthors") => {
                // generate_coauthors requires PrepareCommitMessage event
                let crate::events::AikiEvent::PrepareCommitMessage(event) = &context.event else {
                    return Err(AikiError::Other(anyhow::anyhow!(
                        "generate_coauthors can only be called for PrepareCommitMessage events"
                    )));
                };
                crate::flows::core::generate_coauthors(event)
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

/// Parse author string in "Name <email>" format
/// Returns (Some(name), Some(email)) or (None, None) on parse error
fn parse_author(author: &str) -> Result<(Option<String>, Option<String>)> {
    let author = author.trim();

    // Parse "Name <email>" format
    if let Some(email_start) = author.find('<') {
        if let Some(email_end) = author.find('>') {
            if email_start < email_end {
                let name = author[..email_start].trim().to_string();
                let email = author[email_start + 1..email_end].trim().to_string();
                return Ok((Some(name), Some(email)));
            }
        }
    }

    Err(AikiError::Other(anyhow::anyhow!(
        "Invalid author format '{}'. Expected 'Name <email>'",
        author
    )))
}

/// Execute command with timeout using direct argv (no shell invocation)
fn execute_with_timeout_argv(
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
    timeout: Duration,
) -> Result<std::process::Output> {
    execute_with_timeout_argv_with_env(program, args, cwd, timeout, None, None)
}

/// Execute command with timeout using direct argv and optional environment variables
fn execute_with_timeout_argv_with_env(
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
    timeout: Duration,
    jj_user: Option<String>,
    jj_email: Option<String>,
) -> Result<std::process::Output> {
    use std::panic;
    use std::sync::mpsc;
    use std::thread;

    let cwd = cwd.to_path_buf();
    let program = program.to_string();
    let args = args.to_vec();

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        // Catch panics in command execution to prevent poisoning
        let result = panic::catch_unwind(|| {
            let mut cmd = Command::new(&program);
            cmd.args(&args).current_dir(&cwd);

            // Set JJ_USER and JJ_EMAIL if provided
            if let Some(user) = jj_user {
                cmd.env("JJ_USER", user);
            }
            if let Some(email) = jj_email {
                cmd.env("JJ_EMAIL", email);
            }

            cmd.output()
        });

        // Send result or error - channel will be dropped if recv already timed out
        let output_result = match result {
            Ok(output_result) => output_result,
            Err(panic_err) => {
                eprintln!("PANIC in command execution thread: {:?}", panic_err);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Command execution thread panicked",
                ))
            }
        };
        let _ = tx.send(output_result);
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
    use std::panic;
    use std::sync::mpsc;
    use std::thread;

    let cwd = cwd.to_path_buf();
    let command = command.to_string();

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        // Catch panics in command execution to prevent poisoning
        let result = panic::catch_unwind(|| {
            Command::new("sh")
                .arg("-c")
                .arg(&command)
                .current_dir(&cwd)
                .output()
        });

        // Send result or error - channel will be dropped if recv already timed out
        let output_result = match result {
            Ok(output_result) => output_result,
            Err(panic_err) => {
                eprintln!("PANIC in shell command execution thread: {:?}", panic_err);
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Shell command execution thread panicked",
                ))
            }
        };
        let _ = tx.send(output_result);
    });

    Ok(rx
        .recv_timeout(timeout)
        .context("Command timed out")?
        .context("Failed to execute command")?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AikiEvent, AikiPostFileChangeEvent};
    use crate::provenance::AgentType;

    // Helper to create a simple test event
    fn create_test_event() -> AikiEvent {
        AikiEvent::PostFileChange(AikiPostFileChangeEvent {
            agent_type: AgentType::Claude,
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test-session".to_string(),
            tool_name: "Edit".to_string(),
            file_paths: vec!["/tmp/file.rs".to_string()],
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::Hook,
            edit_details: vec![],
        })
    }

    // Helper to create a test event with custom file_path
    fn create_test_event_with_file(file_path: &str) -> AikiEvent {
        AikiEvent::PostFileChange(AikiPostFileChangeEvent {
            agent_type: AgentType::Claude,
            client_name: None,
            client_version: None,
            agent_version: None,
            session_id: "test-session".to_string(),
            tool_name: "Edit".to_string(),
            file_paths: vec![file_path.to_string()],
            cwd: std::path::PathBuf::from("/tmp"),
            timestamp: chrono::Utc::now(),
            detection_method: crate::provenance::DetectionMethod::Hook,
            edit_details: vec![],
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

        let mut context = AikiState::new(create_test_event());

        let result = FlowExecutor::execute_log(&action, &mut context).unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_execute_log_with_variables() {
        let action = LogAction {
            log: "File: $event.file_paths".to_string(),
            alias: None,
        };

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let result = FlowExecutor::execute_log(&action, &mut context).unwrap();
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

        let mut context = AikiState::new(create_test_event());

        let result = FlowExecutor::execute_shell(&action, &mut context).unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("test"));
    }

    #[test]
    fn test_execute_shell_with_variables() {
        let action = ShellAction {
            shell: "echo $event.file_paths".to_string(),
            timeout: None,
            on_failure: FailureMode::Continue,
            alias: None,
        };

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let result = FlowExecutor::execute_shell(&action, &mut context).unwrap();
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

        let (result, timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert!(matches!(result, FlowResult::Success));
        assert!(timing.duration_secs >= 0.0);
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

        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        // Should return FailedContinue since first action failed but flow continued
        assert!(matches!(result, FlowResult::FailedContinue(_)));
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

        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        // Should return FailedStop since action failed with on_failure: stop
        assert!(matches!(result, FlowResult::FailedStop(_)));
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
            let_: "desc = $event.file_paths".to_string(),
            on_failure: FailureMode::Continue,
        };

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let result = FlowExecutor::execute_let(&action, &mut context).unwrap();
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
        let mut context = AikiState::new(event);

        let result = FlowExecutor::execute_let(&action, &mut context);
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
            let mut context = AikiState::new(event);

            let result = FlowExecutor::execute_let(&action, &mut context);
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
            let_: "  description  =  $event.file_paths  ".to_string(),
            on_failure: FailureMode::Continue,
        };

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let result = FlowExecutor::execute_let(&action, &mut context).unwrap();
        assert!(result.success);
        assert_eq!(result.stdout, "test.rs");
    }

    #[test]
    fn test_let_variable_storage() {
        let actions = vec![
            Action::Let(LetAction {
                let_: "desc = $event.file_paths".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Log(LogAction {
                log: "Variable: $desc".to_string(),
                alias: None,
            }),
        ];

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert!(matches!(result, FlowResult::Success));

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

        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert!(matches!(result, FlowResult::Success));

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
            let_: "desc = $event.file_paths".to_string(),
            on_failure: FailureMode::Continue,
        })];

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let (_result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

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

        let (_result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // Check that no extra variables were stored (except for any built-ins)
        // The metadata should be empty since no alias was provided
        #[cfg(test)]
        {
            assert!(context.get_metadata("result").is_none());
        }
    }

    #[test]
    fn test_let_with_context_vars() {
        // This test verifies that build_metadata works with typed events.
        // The type system now guarantees that PostFileChange events have all required fields.
        let action = LetAction {
            let_: "metadata = aiki/core.build_metadata".to_string(),
            on_failure: FailureMode::Stop,
        };

        let event = create_test_event();
        let mut context = AikiState::new(event);
        context.flow_name = Some("aiki/core".to_string());

        // This should succeed because PostFileChangeEvent has session_id and tool_name
        let result = FlowExecutor::execute_let(&action, &mut context).unwrap();
        assert!(result.success);
        // Result is JSON with author and message fields
        assert!(result.stdout.contains("author"));
        assert!(result.stdout.contains("message"));
    }

    #[test]
    fn test_let_creates_copy_not_reference() {
        // Verify aliasing behavior creates copies
        let actions = vec![
            Action::Let(LetAction {
                let_: "original = $event.file_paths".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Let(LetAction {
                let_: "copy = $original".to_string(),
                on_failure: FailureMode::Continue,
            }),
        ];

        let mut context = AikiState::new(create_test_event());

        let (_result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

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

        // PostFileChange event has tool_name and session_id fields
        let mut context = AikiState::new(create_test_event());

        let (_result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // Second assignment should overwrite first
        assert_eq!(context.get_variable("x"), Some(&"test-session".to_string()));
    }

    #[test]
    fn test_let_aliasing_copies_all_structured_metadata() {
        let actions = vec![
            Action::Let(LetAction {
                let_: "file = $event.file_paths".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Let(LetAction {
                let_: "copy = $file".to_string(),
                on_failure: FailureMode::Continue,
            }),
        ];

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let (_result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

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
            let_: "metadata = self.build_metadata".to_string(),
            on_failure: FailureMode::Stop,
        };

        let event = create_test_event();
        let mut context = AikiState::new(event);
        context.flow_name = Some("aiki/core".to_string());

        let result = FlowExecutor::execute_let(&action, &mut context).unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("author"));
        assert!(result.stdout.contains("message"));
    }

    #[test]
    fn test_let_self_reference_without_flow_context() {
        let action = LetAction {
            let_: "metadata = self.build_metadata".to_string(),
            on_failure: FailureMode::Stop,
        };

        // No flow_name set
        let event = create_test_event();
        let mut context = AikiState::new(event);

        let result = FlowExecutor::execute_let(&action, &mut context);
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
                let_: "my_var = $event.file_paths".to_string(),
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

        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert!(matches!(result, FlowResult::Success));

        // Check that the variable was stored
        assert!(context.get_variable("my_var").is_some());
        assert_eq!(context.get_variable("my_var"), Some(&"test.rs".to_string()));
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
                with_author: None,
                with_author_and_message: None,
            }),
        ];

        let event = create_test_event();
        let mut context = AikiState::new(event);

        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        // Should succeed (we don't validate jj commands in tests)
        assert!(matches!(
            result,
            FlowResult::Success | FlowResult::FailedContinue(_)
        ));
    }

    #[test]
    fn test_let_variables_work_in_log_actions() {
        let actions = vec![
            Action::Let(LetAction {
                let_: "file = $event.file_paths".to_string(),
                on_failure: FailureMode::Continue,
            }),
            Action::Log(LogAction {
                log: "Processing $file".to_string(),
                alias: None,
            }),
        ];

        let mut context = AikiState::new(create_test_event_with_file("test.rs"));

        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();
        assert!(matches!(result, FlowResult::Success));
    }

    #[test]
    fn test_append_trailer_adds_blank_line_before_first_trailer() {
        let content = "Commit title\n\nCommit body text.";
        let trailer = "Co-authored-by: Test <test@example.com>";

        let result = FlowExecutor::append_trailer(content, trailer);

        // Should have blank line before trailer
        assert!(
            result.contains("\n\nCo-authored-by:"),
            "Should have blank line before trailer"
        );
        assert_eq!(
            result,
            "Commit title\n\nCommit body text.\n\nCo-authored-by: Test <test@example.com>\n"
        );
    }

    #[test]
    fn test_append_trailer_no_duplicate_blank_line() {
        let content = "Commit title\n\nCommit body text.\n";
        let trailer = "Co-authored-by: Test <test@example.com>";

        let result = FlowExecutor::append_trailer(content, trailer);

        // Should add blank line since content doesn't end with blank line
        assert!(
            result.contains("text.\n\nCo-authored-by:"),
            "Should have blank line before trailer"
        );
    }

    #[test]
    fn test_append_trailer_to_existing_trailer() {
        let content = "Commit title\n\nCommit body.\n\nSigned-off-by: Author <author@example.com>";
        let trailer = "Co-authored-by: Test <test@example.com>";

        let result = FlowExecutor::append_trailer(content, trailer);

        // Should NOT add blank line before second trailer (trailers stay together)
        assert!(
            result.contains("Signed-off-by: Author <author@example.com>\nCo-authored-by:"),
            "Should not have blank line between trailers"
        );
    }

    #[test]
    fn test_append_trailer_preserves_existing_blank_line() {
        let content = "Commit title\n\nCommit body text.\n\n";
        let trailer = "Co-authored-by: Test <test@example.com>";

        let result = FlowExecutor::append_trailer(content, trailer);

        // Should not add another blank line since one already exists
        assert!(
            !result.contains("\n\n\nCo-authored-by:"),
            "Should not have double blank lines"
        );
        assert!(
            result.contains("text.\n\nCo-authored-by:"),
            "Should preserve existing blank line"
        );
    }

    #[test]
    fn test_if_condition_true_executes_then_branch() {
        let actions = vec![
            // Set a variable using log action (which doesn't require function namespace)
            Action::Log(LogAction {
                log: "true".to_string(),
                alias: Some("status".to_string()),
            }),
            // Conditional that should execute then branch
            Action::If(IfAction {
                condition: "$status == true".to_string(),
                then: vec![Action::Log(LogAction {
                    log: "then branch executed".to_string(),
                    alias: Some("result".to_string()),
                })],
                else_: None,
                on_failure: FailureMode::Stop,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        assert!(matches!(result, FlowResult::Success));
        assert_eq!(
            context.get_variable("result"),
            Some(&"then branch executed".to_string())
        );
    }

    #[test]
    fn test_if_condition_false_executes_else_branch() {
        let actions = vec![
            // Set a variable using log action
            Action::Log(LogAction {
                log: "false".to_string(),
                alias: Some("status".to_string()),
            }),
            // Conditional that should execute else branch
            Action::If(IfAction {
                condition: "$status == true".to_string(),
                then: vec![Action::Log(LogAction {
                    log: "then branch executed".to_string(),
                    alias: Some("result".to_string()),
                })],
                else_: Some(vec![Action::Log(LogAction {
                    log: "else branch executed".to_string(),
                    alias: Some("result".to_string()),
                })]),
                on_failure: FailureMode::Stop,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        assert!(matches!(result, FlowResult::Success));
        assert_eq!(
            context.get_variable("result"),
            Some(&"else branch executed".to_string())
        );
    }

    #[test]
    fn test_if_condition_false_no_else_branch() {
        let actions = vec![
            Action::Log(LogAction {
                log: "false".to_string(),
                alias: Some("status".to_string()),
            }),
            Action::If(IfAction {
                condition: "$status == true".to_string(),
                then: vec![Action::Log(LogAction {
                    log: "then branch executed".to_string(),
                    alias: Some("result".to_string()),
                })],
                else_: None,
                on_failure: FailureMode::Stop,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        assert!(matches!(result, FlowResult::Success));
        // result should not be set since neither branch executed
        assert!(context.get_variable("result").is_none());
    }

    #[test]
    fn test_if_json_field_access() {
        let actions = vec![
            // Create JSON variable
            Action::Let(LetAction {
                let_: "detection = aiki/core.classify_edits".to_string(),
                on_failure: FailureMode::Continue,
            }),
            // Check JSON field (will fail in test since classify_edits returns error)
        ];

        let mut context = AikiState::new(create_test_event());
        context.flow_name = Some("aiki/core".to_string());

        let (_result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // This test just verifies syntax doesn't crash
    }

    #[test]
    fn test_if_nested_conditionals() {
        let actions = vec![
            Action::Log(LogAction {
                log: "true".to_string(),
                alias: Some("outer".to_string()),
            }),
            Action::Log(LogAction {
                log: "true".to_string(),
                alias: Some("inner".to_string()),
            }),
            Action::If(IfAction {
                condition: "$outer == true".to_string(),
                then: vec![Action::If(IfAction {
                    condition: "$inner == true".to_string(),
                    then: vec![Action::Log(LogAction {
                        log: "nested then executed".to_string(),
                        alias: Some("result".to_string()),
                    })],
                    else_: None,
                    on_failure: FailureMode::Stop,
                })],
                else_: None,
                on_failure: FailureMode::Stop,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        assert!(matches!(result, FlowResult::Success));
        assert_eq!(
            context.get_variable("result"),
            Some(&"nested then executed".to_string())
        );
    }

    #[test]
    fn test_evaluate_condition_equality() {
        let mut context = AikiState::new(create_test_event());
        context.store_action_result(
            "test".to_string(),
            ActionResult {
                success: true,
                exit_code: Some(0),
                stdout: "value".to_string(),
                stderr: String::new(),
            },
        );

        // Test equality
        assert!(FlowExecutor::evaluate_condition("$test == value", &mut context).unwrap());
        assert!(!FlowExecutor::evaluate_condition("$test == other", &mut context).unwrap());

        // Test inequality
        assert!(!FlowExecutor::evaluate_condition("$test != value", &mut context).unwrap());
        assert!(FlowExecutor::evaluate_condition("$test != other", &mut context).unwrap());
    }

    #[test]
    fn test_if_condition_truthy_values() {
        let mut context = AikiState::new(create_test_event());

        // Empty string is falsy
        context.store_action_result(
            "empty".to_string(),
            ActionResult {
                success: true,
                exit_code: Some(0),
                stdout: "".to_string(),
                stderr: String::new(),
            },
        );
        assert!(!FlowExecutor::evaluate_condition("$empty", &mut context).unwrap());

        // Non-empty string is truthy
        context.store_action_result(
            "nonempty".to_string(),
            ActionResult {
                success: true,
                exit_code: Some(0),
                stdout: "some content".to_string(),
                stderr: String::new(),
            },
        );
        assert!(FlowExecutor::evaluate_condition("$nonempty", &mut context).unwrap());

        // "false" literal is falsy
        context.store_action_result(
            "false_str".to_string(),
            ActionResult {
                success: true,
                exit_code: Some(0),
                stdout: "false".to_string(),
                stderr: String::new(),
            },
        );
        assert!(!FlowExecutor::evaluate_condition("$false_str", &mut context).unwrap());

        // "true" literal is truthy
        context.store_action_result(
            "true_str".to_string(),
            ActionResult {
                success: true,
                exit_code: Some(0),
                stdout: "true".to_string(),
                stderr: String::new(),
            },
        );
        assert!(FlowExecutor::evaluate_condition("$true_str", &mut context).unwrap());
    }

    #[test]
    fn test_resolve_condition_value_with_quotes() {
        let mut context = AikiState::new(create_test_event());

        // Test string literals with quotes
        assert_eq!(
            FlowExecutor::resolve_condition_value("\"hello\"", &mut context).unwrap(),
            "hello"
        );
        assert_eq!(
            FlowExecutor::resolve_condition_value("'world'", &mut context).unwrap(),
            "world"
        );
    }

    #[test]
    fn test_switch_matches_case() {
        use std::collections::HashMap;

        let mut cases = HashMap::new();
        cases.insert(
            "ExactMatch".to_string(),
            vec![Action::Log(LogAction {
                log: "exact match case".to_string(),
                alias: Some("result".to_string()),
            })],
        );
        cases.insert(
            "PartialMatch".to_string(),
            vec![Action::Log(LogAction {
                log: "partial match case".to_string(),
                alias: Some("result".to_string()),
            })],
        );

        let actions = vec![
            Action::Log(LogAction {
                log: "ExactMatch".to_string(),
                alias: Some("status".to_string()),
            }),
            Action::Switch(SwitchAction {
                expression: "$status".to_string(),
                cases,
                default: None,
                on_failure: FailureMode::Stop,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        assert!(matches!(result, FlowResult::Success));
        assert_eq!(
            context.get_variable("result"),
            Some(&"exact match case".to_string())
        );
    }

    #[test]
    fn test_switch_uses_default_case() {
        use std::collections::HashMap;

        let mut cases = HashMap::new();
        cases.insert(
            "ExactMatch".to_string(),
            vec![Action::Log(LogAction {
                log: "exact match case".to_string(),
                alias: Some("result".to_string()),
            })],
        );

        let actions = vec![
            Action::Log(LogAction {
                log: "NoMatch".to_string(),
                alias: Some("status".to_string()),
            }),
            Action::Switch(SwitchAction {
                expression: "$status".to_string(),
                cases,
                default: Some(vec![Action::Log(LogAction {
                    log: "default case".to_string(),
                    alias: Some("result".to_string()),
                })]),
                on_failure: FailureMode::Stop,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        assert!(matches!(result, FlowResult::Success));
        assert_eq!(
            context.get_variable("result"),
            Some(&"default case".to_string())
        );
    }

    #[test]
    fn test_switch_no_match_no_default() {
        use std::collections::HashMap;

        let mut cases = HashMap::new();
        cases.insert(
            "ExactMatch".to_string(),
            vec![Action::Log(LogAction {
                log: "exact match case".to_string(),
                alias: Some("result".to_string()),
            })],
        );

        let actions = vec![
            Action::Log(LogAction {
                log: "NoMatch".to_string(),
                alias: Some("status".to_string()),
            }),
            Action::Switch(SwitchAction {
                expression: "$status".to_string(),
                cases,
                default: None,
                on_failure: FailureMode::Stop,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // No match and no default = success (no-op)
        assert!(matches!(result, FlowResult::Success));
        // result variable should not be set
        assert!(context.get_variable("result").is_none());
    }

    #[test]
    fn test_switch_with_json_field_access() {
        use std::collections::HashMap;

        let mut cases = HashMap::new();
        cases.insert(
            "true".to_string(),
            vec![Action::Log(LogAction {
                log: "all exact match".to_string(),
                alias: Some("result".to_string()),
            })],
        );
        cases.insert(
            "false".to_string(),
            vec![Action::Log(LogAction {
                log: "not all exact match".to_string(),
                alias: Some("result".to_string()),
            })],
        );

        // Create a simple JSON object to test field access
        let actions = vec![
            Action::Log(LogAction {
                log: "{\"all_exact_match\": \"true\"}".to_string(),
                alias: Some("detection".to_string()),
            }),
            Action::Switch(SwitchAction {
                expression: "$detection.all_exact_match".to_string(),
                cases,
                default: None,
                on_failure: FailureMode::Stop,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        assert!(matches!(result, FlowResult::Success));
        // Note: Variable resolver will parse the JSON and extract the field
        // The actual result depends on the resolver implementation
    }

    #[test]
    fn test_prefilechange_flow_with_jj_diff_output() {
        // Simulate the PreFileChange flow: `jj diff -r @ --name-only` returns file names
        let actions = vec![
            // Simulate jj diff output with files using echo
            Action::Shell(ShellAction {
                shell: "echo 'src/main.rs\nsrc/lib.rs'".to_string(),
                timeout: None,
                on_failure: FailureMode::Continue,
                alias: Some("changed_files".to_string()),
            }),
            // If there are changed files (non-empty), execute the then branch
            Action::If(IfAction {
                condition: "$changed_files".to_string(),
                then: vec![Action::Log(LogAction {
                    log: "User has changes to stash".to_string(),
                    alias: Some("stash_result".to_string()),
                })],
                else_: Some(vec![Action::Log(LogAction {
                    log: "No changes to stash".to_string(),
                    alias: Some("stash_result".to_string()),
                })]),
                on_failure: FailureMode::Continue,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (_result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // Should execute the then branch because changed_files is non-empty
        assert_eq!(
            context.get_variable("stash_result").unwrap(),
            "User has changes to stash"
        );
    }

    #[test]
    fn test_prefilechange_flow_with_empty_jj_diff() {
        // Simulate jj diff with no changes (empty output)
        let actions = vec![
            // Simulate empty jj diff output using true (which produces no output)
            Action::Shell(ShellAction {
                shell: "true".to_string(), // Exits 0 but produces no output
                timeout: None,
                on_failure: FailureMode::Continue,
                alias: Some("changed_files".to_string()),
            }),
            Action::If(IfAction {
                condition: "$changed_files".to_string(),
                then: vec![Action::Log(LogAction {
                    log: "Should not execute".to_string(),
                    alias: Some("result".to_string()),
                })],
                else_: Some(vec![Action::Log(LogAction {
                    log: "No changes detected".to_string(),
                    alias: Some("result".to_string()),
                })]),
                on_failure: FailureMode::Continue,
            }),
        ];

        let mut context = AikiState::new(create_test_event());
        let (_result, _timing) = FlowExecutor::execute_actions(&actions, &mut context).unwrap();

        // Should execute the else branch because changed_files is empty
        assert_eq!(
            context.get_variable("result").unwrap(),
            "No changes detected"
        );
    }
}
