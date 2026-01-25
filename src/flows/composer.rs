//! Flow composition with before/after orchestration and cycle detection.
//!
//! This module provides the [`FlowComposer`] struct which orchestrates:
//! - Before flow execution (runs before this flow's actions)
//! - This flow's action execution (delegated to FlowEngine)
//! - After flow execution (runs after this flow's actions)
//! - Cycle detection using a runtime call stack
//!
//! # Architecture
//!
//! ```text
//! User triggers event (e.g., change.completed)
//!     ↓
//! FlowComposer.compose_flow("my-workflow.yml", state)
//!     ↓
//!     Loads flow via FlowLoader
//!     Checks call stack for cycles
//!     ↓
//!     Executes before flows (each gets fresh variable context, shares event state)
//!     ↓
//!     Executes this flow's actions via FlowEngine (fresh variable context, shares event state)
//!     ↓
//!     Executes after flows (each gets fresh variable context, shares event state)
//!     ↓
//!     Returns FlowResult
//! ```
//!
//! # Isolation Model
//!
//! - **Variables are isolated**: Each flow gets a fresh variable context
//! - **Event state is shared**: All flows modify the same event object
//!   - Example: TurnStarted's ContextAssembler accumulates chunks from all flows
//!   - Example: TurnCompleted's autoreply builder accumulates from all flows

use std::path::{Path, PathBuf};

use super::engine::{FlowEngine, FlowResult};
use super::loader::FlowLoader;
use super::state::AikiState;
use super::types::{Flow, FlowStatement};
use crate::cache::debug_log;
use crate::error::{AikiError, Result};

/// Event type enum for routing to correct handler in composed flows.
///
/// Used to select the appropriate statement list from a Flow struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    SessionStarted,
    SessionResumed,
    SessionEnded,
    TurnStarted,
    TurnCompleted,
    ReadPermissionAsked,
    ReadCompleted,
    ChangePermissionAsked,
    ChangeCompleted,
    ShellPermissionAsked,
    ShellCompleted,
    WebPermissionAsked,
    WebCompleted,
    McpPermissionAsked,
    McpCompleted,
    CommitMessageStarted,
}

impl EventType {
    /// Get the statements for this event type from a Flow.
    #[must_use]
    pub fn get_statements<'a>(&self, flow: &'a Flow) -> &'a [FlowStatement] {
        match self {
            EventType::SessionStarted => &flow.session_started,
            EventType::SessionResumed => &flow.session_resumed,
            EventType::SessionEnded => &flow.session_ended,
            EventType::TurnStarted => &flow.turn_started,
            EventType::TurnCompleted => &flow.turn_completed,
            EventType::ReadPermissionAsked => &flow.read_permission_asked,
            EventType::ReadCompleted => &flow.read_completed,
            EventType::ChangePermissionAsked => &flow.change_permission_asked,
            EventType::ChangeCompleted => &flow.change_completed,
            EventType::ShellPermissionAsked => &flow.shell_permission_asked,
            EventType::ShellCompleted => &flow.shell_completed,
            EventType::WebPermissionAsked => &flow.web_permission_asked,
            EventType::WebCompleted => &flow.web_completed,
            EventType::McpPermissionAsked => &flow.mcp_permission_asked,
            EventType::McpCompleted => &flow.mcp_completed,
            EventType::CommitMessageStarted => &flow.commit_message_started,
        }
    }
}

/// Orchestrates flow composition and delegates action execution to FlowEngine.
///
/// FlowComposer handles:
/// - Flow loading via FlowLoader (with caching)
/// - Cycle detection via call stack (using canonical paths)
/// - Before/after flow orchestration
/// - Event type routing
///
/// # Example
///
/// ```rust,ignore
/// use aiki::flows::composer::{FlowComposer, EventType};
///
/// let mut loader = FlowLoader::new()?;
/// let mut composer = FlowComposer::new(&mut loader);
///
/// // Compose and execute a flow
/// let result = composer.compose_flow(
///     "aiki/my-workflow",
///     EventType::ChangeCompleted,
///     &mut state,
/// )?;
/// ```
pub struct FlowComposer<'a> {
    loader: &'a mut FlowLoader,
    call_stack: Vec<PathBuf>,
}

impl<'a> FlowComposer<'a> {
    /// Create a new FlowComposer with a FlowLoader.
    ///
    /// The loader is borrowed mutably because loading flows may update its cache.
    #[must_use]
    pub fn new(loader: &'a mut FlowLoader) -> Self {
        Self {
            loader,
            call_stack: Vec::new(),
        }
    }

    /// Compose and execute a flow atomically (before → this flow → after).
    ///
    /// This is the main entry point for flow composition. It:
    /// 1. Loads the flow via FlowLoader
    /// 2. Checks for circular dependencies
    /// 3. Executes before flows (each atomically)
    /// 4. Executes this flow's actions for the given event type
    /// 5. Executes after flows (each atomically)
    ///
    /// # Arguments
    ///
    /// * `flow_path` - Path to the flow (e.g., "aiki/quick-lint", "./helpers/lint.yml")
    /// * `event_type` - The event type being handled
    /// * `state` - Mutable state shared across all flows
    ///
    /// # Returns
    ///
    /// The combined [`FlowResult`] from all executed flows.
    ///
    /// # Errors
    ///
    /// Returns:
    /// - `AikiError::CircularFlowDependency` if a cycle is detected
    /// - `AikiError::FlowNotFound` if a flow file doesn't exist
    /// - Other errors from flow execution
    pub fn compose_flow(
        &mut self,
        flow_path: &str,
        event_type: EventType,
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        // Load the flow (FlowLoader uses FlowResolver which returns canonical paths)
        let (flow, canonical_path) = self.loader.load(flow_path)?;

        // Check for circular dependency
        if self.call_stack.contains(&canonical_path) {
            return Err(AikiError::CircularFlowDependency {
                path: flow_path.to_string(),
                canonical_path: canonical_path.display().to_string(),
                stack: self
                    .call_stack
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect(),
            });
        }

        debug_log(|| {
            format!(
                "Composing flow: {} (canonical: {})",
                flow_path,
                canonical_path.display()
            )
        });

        // Push canonical path onto call stack for cycle detection
        self.call_stack.push(canonical_path.clone());

        // Execute the flow composition
        let result = self.execute_composed_flow(&flow, &canonical_path, event_type, state);

        // Pop from call stack (even on error)
        self.call_stack.pop();

        result
    }

    /// Compose and execute a flow from an absolute file path.
    ///
    /// This is used for loading flows that aren't in the standard namespace structure,
    /// such as .aiki/flows/default.yml.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Absolute path to the flow file
    /// * `event_type` - The event type to execute
    /// * `state` - Mutable execution state
    ///
    /// # Errors
    ///
    /// Returns:
    /// - `AikiError::CircularFlowDependency` if a cycle is detected
    /// - `AikiError::FlowNotFound` if the file doesn't exist
    /// - Other errors from flow execution
    pub fn compose_flow_from_path(
        &mut self,
        file_path: &Path,
        event_type: EventType,
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        // Load the flow directly from the file path
        let (flow, canonical_path) = self.loader.load_from_file_path(file_path)?;

        // Check for circular dependency
        if self.call_stack.contains(&canonical_path) {
            return Err(AikiError::CircularFlowDependency {
                path: file_path.display().to_string(),
                canonical_path: canonical_path.display().to_string(),
                stack: self
                    .call_stack
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect(),
            });
        }

        debug_log(|| {
            format!(
                "Composing flow from path: {} (canonical: {})",
                file_path.display(),
                canonical_path.display()
            )
        });

        // Push canonical path onto call stack for cycle detection
        self.call_stack.push(canonical_path.clone());

        // Execute the flow composition
        let result = self.execute_composed_flow(&flow, &canonical_path, event_type, state);

        // Pop from call stack (even on error)
        self.call_stack.pop();

        result
    }

    /// Execute a composed flow (before → this flow → after).
    ///
    /// This is separated from compose_flow to ensure call_stack is always popped.
    fn execute_composed_flow(
        &mut self,
        flow: &Flow,
        canonical_path: &Path,
        event_type: EventType,
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        let mut overall_result = FlowResult::Success;

        // Variable isolation: each flow starts with a fresh variable context.
        // This ensures before flows don't see variables from the caller.
        // (We clear again before this flow's statements to isolate from before flows.)
        state.clear_variables();

        // 1. Execute before flows (each atomically, each clears their own variables on entry)
        for before_path in &flow.before {
            debug_log(|| format!("  Before: {}", before_path));
            let result = self.compose_flow(before_path, event_type, state)?;

            // Before flow failures abort the workflow
            match result {
                FlowResult::Success => {}
                FlowResult::FailedContinue => {
                    overall_result = FlowResult::FailedContinue;
                }
                FlowResult::FailedStop | FlowResult::FailedBlock => {
                    // Before flow failure - abort entire workflow
                    debug_log(|| {
                        format!(
                            "  Before flow '{}' failed with {:?}, aborting",
                            before_path, result
                        )
                    });
                    return Ok(result);
                }
            }
        }

        // 2. Execute this flow's actions (if any for this event)
        let statements = event_type.get_statements(flow);
        if !statements.is_empty() {
            debug_log(|| {
                format!(
                    "  Executing {} statements for {:?}",
                    statements.len(),
                    event_type
                )
            });

            // Variable isolation: clear variables before executing this flow's actions
            state.clear_variables();

            // Set flow name for self.* resolution using the canonical path
            // Extract flow identifier from path (e.g., "/project/.aiki/flows/aiki/quick-lint.yml" -> "aiki/quick-lint")
            state.flow_name = Some(Self::extract_flow_identifier(canonical_path));

            let result = FlowEngine::execute_statements(statements, state)?;

            match result {
                FlowResult::Success => {}
                FlowResult::FailedContinue => {
                    overall_result = FlowResult::FailedContinue;
                }
                FlowResult::FailedStop | FlowResult::FailedBlock => {
                    // Main flow failure - don't run after flows
                    debug_log(|| {
                        format!(
                            "  Flow '{}' failed with {:?}, skipping after flows",
                            flow.name, result
                        )
                    });
                    return Ok(result);
                }
            }
        }

        // 3. Execute after flows (each atomically)
        for after_path in &flow.after {
            debug_log(|| format!("  After: {}", after_path));
            let result = self.compose_flow(after_path, event_type, state)?;

            // After flow failures are honored - allows validation logic in after flows
            match result {
                FlowResult::Success => {}
                FlowResult::FailedContinue => {
                    overall_result = FlowResult::FailedContinue;
                }
                FlowResult::FailedStop | FlowResult::FailedBlock => {
                    // After flow wants to stop or block - honor it
                    debug_log(|| {
                        format!(
                            "  After flow '{}' failed with {:?}, aborting",
                            after_path, result
                        )
                    });
                    return Ok(result);
                }
            }
        }

        Ok(overall_result)
    }

    /// Get the current call stack depth.
    #[must_use]
    #[allow(dead_code)] // Part of FlowComposer API
    pub fn depth(&self) -> usize {
        self.call_stack.len()
    }

    /// Check if a path is already in the call stack.
    ///
    /// This is a helper for testing cycle detection.
    #[must_use]
    #[allow(dead_code)] // Part of FlowComposer API
    pub fn is_in_stack(&self, path: &Path) -> bool {
        self.call_stack.contains(&path.to_path_buf())
    }

    /// Extract flow identifier from canonical path for self.* resolution.
    ///
    /// Converts paths like:
    /// - `/project/.aiki/flows/aiki/quick-lint.yml` → `aiki/quick-lint`
    /// - `/project/.aiki/flows/eslint/check.yml` → `eslint/check`
    /// - `/project/.aiki/flows/helpers/lint.yml` → `helpers/lint`
    ///
    /// Falls back to filename without extension if pattern doesn't match.
    fn extract_flow_identifier(canonical_path: &Path) -> String {
        // Convert to string for pattern matching
        let path_str = canonical_path.to_string_lossy();

        // Look for ".aiki/flows/" pattern and extract everything after it
        if let Some(flows_idx) = path_str.find(".aiki/flows/") {
            let after_flows = &path_str[flows_idx + ".aiki/flows/".len()..];
            // Remove .yml or .yaml extension
            let without_ext = after_flows
                .strip_suffix(".yml")
                .or_else(|| after_flows.strip_suffix(".yaml"))
                .unwrap_or(after_flows);
            return without_ext.to_string();
        }

        // Fallback: use filename without extension
        canonical_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AikiChangeCompletedPayload, AikiEvent, ChangeOperation, WriteOperation};
    use crate::provenance::{AgentType, DetectionMethod};
    use crate::session::AikiSession;
    use std::fs;
    use tempfile::TempDir;

    /// Create a test project with .aiki/ directory structure
    fn create_test_project() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        // Create namespaces - aiki is just another namespace
        fs::create_dir_all(temp_dir.path().join(".aiki/flows/aiki")).unwrap();
        fs::create_dir_all(temp_dir.path().join(".aiki/flows/eslint")).unwrap();
        fs::create_dir_all(temp_dir.path().join(".aiki/flows/prettier")).unwrap();
        fs::create_dir_all(temp_dir.path().join(".aiki/flows/helpers")).unwrap();
        temp_dir
    }

    /// Create a flow file with specified before/after dependencies and optional statements
    fn create_flow_file(
        path: &Path,
        name: &str,
        before: &[&str],
        after: &[&str],
        has_change_completed: bool,
    ) {
        // Helper to quote paths that need it (start with @ or contain special chars)
        let quote_if_needed = |s: &str| -> String {
            if s.starts_with('@') || s.contains(':') || s.contains('#') {
                format!("\"{}\"", s)
            } else {
                s.to_string()
            }
        };

        let before_yaml = if before.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = before
                .iter()
                .map(|b| format!("  - {}", quote_if_needed(b)))
                .collect();
            format!("before:\n{}\n", items.join("\n"))
        };

        let after_yaml = if after.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = after
                .iter()
                .map(|a| format!("  - {}", quote_if_needed(a)))
                .collect();
            format!("after:\n{}\n", items.join("\n"))
        };

        let change_completed_yaml = if has_change_completed {
            "change.completed:\n  - log: \"Executed from flow\"\n".to_string()
        } else {
            String::new()
        };

        let content = format!(
            r#"name: {}
version: "1"
{}{}{}
"#,
            name, before_yaml, after_yaml, change_completed_yaml
        );
        fs::write(path, content).unwrap();
    }

    /// Create a test AikiState with a ChangeCompleted event
    fn create_test_state(temp_dir: &TempDir) -> AikiState {
        let session = AikiSession::new(
            AgentType::ClaudeCode,
            "test-session".to_string(),
            None::<&str>,
            DetectionMethod::Hook,
        );
        let event = AikiEvent::ChangeCompleted(AikiChangeCompletedPayload {
            session,
            cwd: temp_dir.path().to_path_buf(),
            timestamp: chrono::Utc::now(),
            tool_name: "Edit".to_string(),
            success: true,
            operation: ChangeOperation::Write(WriteOperation {
                file_paths: vec!["/test/file.rs".to_string()],
                edit_details: vec![],
            }),
        });
        AikiState::new(event)
    }

    #[test]
    fn test_compose_simple_flow() {
        let temp_dir = create_test_project();
        let flow_path = temp_dir.path().join(".aiki/flows/aiki/simple.yml");
        create_flow_file(&flow_path, "Simple Flow", &[], &[], true);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer
            .compose_flow("aiki/simple", EventType::ChangeCompleted, &mut state)
            .unwrap();

        assert!(matches!(result, FlowResult::Success));
    }

    #[test]
    fn test_compose_flow_with_before() {
        let temp_dir = create_test_project();

        // Create base flow (no dependencies)
        let base_path = temp_dir.path().join(".aiki/flows/aiki/base.yml");
        create_flow_file(&base_path, "Base Flow", &[], &[], true);

        // Create main flow (depends on base)
        let main_path = temp_dir.path().join(".aiki/flows/aiki/main.yml");
        create_flow_file(&main_path, "Main Flow", &["aiki/base"], &[], true);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer
            .compose_flow("aiki/main", EventType::ChangeCompleted, &mut state)
            .unwrap();

        assert!(matches!(result, FlowResult::Success));
    }

    #[test]
    fn test_compose_flow_with_after() {
        let temp_dir = create_test_project();

        // Create cleanup flow (no dependencies)
        let cleanup_path = temp_dir.path().join(".aiki/flows/aiki/cleanup.yml");
        create_flow_file(&cleanup_path, "Cleanup Flow", &[], &[], true);

        // Create main flow (has after)
        let main_path = temp_dir.path().join(".aiki/flows/aiki/main.yml");
        create_flow_file(&main_path, "Main Flow", &[], &["aiki/cleanup"], true);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer
            .compose_flow("aiki/main", EventType::ChangeCompleted, &mut state)
            .unwrap();

        assert!(matches!(result, FlowResult::Success));
    }

    #[test]
    fn test_compose_nested_flows() {
        let temp_dir = create_test_project();

        // Create level 0 flow (no dependencies)
        let level0_path = temp_dir.path().join(".aiki/flows/aiki/level0.yml");
        create_flow_file(&level0_path, "Level 0", &[], &[], true);

        // Create level 1 flow (depends on level 0)
        let level1_path = temp_dir.path().join(".aiki/flows/aiki/level1.yml");
        create_flow_file(&level1_path, "Level 1", &["aiki/level0"], &[], true);

        // Create level 2 flow (depends on level 1)
        let level2_path = temp_dir.path().join(".aiki/flows/aiki/level2.yml");
        create_flow_file(&level2_path, "Level 2", &["aiki/level1"], &[], true);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer
            .compose_flow("aiki/level2", EventType::ChangeCompleted, &mut state)
            .unwrap();

        assert!(matches!(result, FlowResult::Success));
    }

    #[test]
    fn test_circular_dependency_detected() {
        let temp_dir = create_test_project();

        // Create flow-a (depends on flow-b)
        let flow_a_path = temp_dir.path().join(".aiki/flows/aiki/flow-a.yml");
        create_flow_file(&flow_a_path, "Flow A", &["aiki/flow-b"], &[], true);

        // Create flow-b (depends on flow-a - circular!)
        let flow_b_path = temp_dir.path().join(".aiki/flows/aiki/flow-b.yml");
        create_flow_file(&flow_b_path, "Flow B", &["aiki/flow-a"], &[], true);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer.compose_flow("aiki/flow-a", EventType::ChangeCompleted, &mut state);

        assert!(matches!(
            result,
            Err(AikiError::CircularFlowDependency { .. })
        ));
    }

    #[test]
    fn test_circular_dependency_self_reference() {
        let temp_dir = create_test_project();

        // Create flow that references itself
        let flow_path = temp_dir.path().join(".aiki/flows/aiki/self-ref.yml");
        create_flow_file(&flow_path, "Self Reference", &["aiki/self-ref"], &[], true);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer.compose_flow("aiki/self-ref", EventType::ChangeCompleted, &mut state);

        assert!(matches!(
            result,
            Err(AikiError::CircularFlowDependency { .. })
        ));
    }

    #[test]
    fn test_flow_not_found() {
        let temp_dir = create_test_project();

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result =
            composer.compose_flow("aiki/nonexistent", EventType::ChangeCompleted, &mut state);

        assert!(matches!(result, Err(AikiError::FlowNotFound { .. })));
    }

    #[test]
    fn test_depth_tracking() {
        let temp_dir = create_test_project();

        // Create simple flow
        let flow_path = temp_dir.path().join(".aiki/flows/aiki/simple.yml");
        create_flow_file(&flow_path, "Simple Flow", &[], &[], true);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let composer = FlowComposer::new(&mut loader);

        // Initially depth should be 0
        assert_eq!(composer.depth(), 0);
    }

    #[test]
    fn test_event_type_get_statements() {
        let yaml = r#"
name: Multi Event Flow
version: "1"
session.started:
  - log: "Session started"
change.completed:
  - log: "Change completed"
commit.message_started:
  - log: "Commit message started"
"#;

        let flow: Flow = serde_yaml::from_str(yaml).unwrap();

        // Check each event type returns correct statements
        assert_eq!(EventType::SessionStarted.get_statements(&flow).len(), 1);
        assert_eq!(EventType::ChangeCompleted.get_statements(&flow).len(), 1);
        assert_eq!(
            EventType::CommitMessageStarted.get_statements(&flow).len(),
            1
        );
        assert!(EventType::TurnStarted.get_statements(&flow).is_empty());
    }

    #[test]
    fn test_before_and_after_both() {
        let temp_dir = create_test_project();

        // Create pre flow
        let pre_path = temp_dir.path().join(".aiki/flows/aiki/pre.yml");
        create_flow_file(&pre_path, "Pre Flow", &[], &[], true);

        // Create post flow
        let post_path = temp_dir.path().join(".aiki/flows/aiki/post.yml");
        create_flow_file(&post_path, "Post Flow", &[], &[], true);

        // Create main flow with both before and after
        let main_path = temp_dir.path().join(".aiki/flows/aiki/main.yml");
        create_flow_file(&main_path, "Main Flow", &["aiki/pre"], &["aiki/post"], true);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer
            .compose_flow("aiki/main", EventType::ChangeCompleted, &mut state)
            .unwrap();

        assert!(matches!(result, FlowResult::Success));
    }

    #[test]
    fn test_extract_flow_identifier() {
        // Test aiki namespace
        let path = PathBuf::from("/project/.aiki/flows/aiki/quick-lint.yml");
        assert_eq!(
            FlowComposer::extract_flow_identifier(&path),
            "aiki/quick-lint"
        );

        // Test other namespace
        let path = PathBuf::from("/home/user/code/.aiki/flows/eslint/check.yml");
        assert_eq!(FlowComposer::extract_flow_identifier(&path), "eslint/check");

        // Test nested paths
        let path = PathBuf::from("/project/.aiki/flows/helpers/lint/core.yml");
        assert_eq!(
            FlowComposer::extract_flow_identifier(&path),
            "helpers/lint/core"
        );

        // Test .yaml extension
        let path = PathBuf::from("/project/.aiki/flows/aiki/test.yaml");
        assert_eq!(FlowComposer::extract_flow_identifier(&path), "aiki/test");

        // Test fallback for non-standard paths
        let path = PathBuf::from("/some/random/path/flow.yml");
        assert_eq!(FlowComposer::extract_flow_identifier(&path), "flow");
    }

    // =========================================================================
    // Execution order and variable isolation tests
    // =========================================================================

    /// Helper to create a flow that appends to a log file (for order verification)
    fn create_logging_flow(
        path: &Path,
        name: &str,
        log_msg: &str,
        before: &[&str],
        after: &[&str],
    ) {
        let quote_if_needed = |s: &str| -> String {
            if s.starts_with('@') || s.contains(':') || s.contains('#') {
                format!("\"{}\"", s)
            } else {
                s.to_string()
            }
        };

        let before_yaml = if before.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = before
                .iter()
                .map(|b| format!("  - {}", quote_if_needed(b)))
                .collect();
            format!("before:\n{}\n", items.join("\n"))
        };

        let after_yaml = if after.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = after
                .iter()
                .map(|a| format!("  - {}", quote_if_needed(a)))
                .collect();
            format!("after:\n{}\n", items.join("\n"))
        };

        // Use shell to append to execution_log.txt
        let content = format!(
            r#"name: {name}
version: "1"
{before_yaml}{after_yaml}change.completed:
  - shell: echo "{log_msg}" >> execution_log.txt
"#
        );
        fs::write(path, content).unwrap();
    }

    /// Helper to create a flow that captures shell output to a variable
    fn create_shell_var_flow(
        path: &Path,
        name: &str,
        var_name: &str,
        echo_value: &str,
        before: &[&str],
    ) {
        let quote_if_needed = |s: &str| -> String {
            if s.starts_with('@') || s.contains(':') || s.contains('#') {
                format!("\"{}\"", s)
            } else {
                s.to_string()
            }
        };

        let before_yaml = if before.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = before
                .iter()
                .map(|b| format!("  - {}", quote_if_needed(b)))
                .collect();
            format!("before:\n{}\n", items.join("\n"))
        };

        // Use shell with 'alias:' to capture output to a variable, then echo it
        let content = format!(
            r#"name: {name}
version: "1"
{before_yaml}change.completed:
  - shell: echo {echo_value}
    alias: {var_name}
  - shell: echo "${var_name}" >> var_log.txt
"#
        );
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_execution_order_before_main_after() {
        let temp_dir = create_test_project();

        // Create before flow
        let before_path = temp_dir.path().join(".aiki/flows/aiki/before.yml");
        create_logging_flow(&before_path, "Before Flow", "BEFORE", &[], &[]);

        // Create after flow
        let after_path = temp_dir.path().join(".aiki/flows/aiki/after.yml");
        create_logging_flow(&after_path, "After Flow", "AFTER", &[], &[]);

        // Create main flow with before and after
        let main_path = temp_dir.path().join(".aiki/flows/aiki/main.yml");
        create_logging_flow(
            &main_path,
            "Main Flow",
            "MAIN",
            &["aiki/before"],
            &["aiki/after"],
        );

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer
            .compose_flow("aiki/main", EventType::ChangeCompleted, &mut state)
            .unwrap();

        assert!(matches!(result, FlowResult::Success));

        // Verify execution order
        let log_path = temp_dir.path().join("execution_log.txt");
        let log_content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = log_content.lines().collect();

        assert_eq!(lines.len(), 3, "Expected 3 log entries, got: {:?}", lines);
        assert_eq!(lines[0], "BEFORE", "Before flow should run first");
        assert_eq!(lines[1], "MAIN", "Main flow should run second");
        assert_eq!(lines[2], "AFTER", "After flow should run last");
    }

    #[test]
    fn test_execution_order_nested_before() {
        let temp_dir = create_test_project();

        // Create level 0 (innermost before)
        let level0_path = temp_dir.path().join(".aiki/flows/aiki/level0.yml");
        create_logging_flow(&level0_path, "Level 0", "L0", &[], &[]);

        // Create level 1 (has before: level0)
        let level1_path = temp_dir.path().join(".aiki/flows/aiki/level1.yml");
        create_logging_flow(&level1_path, "Level 1", "L1", &["aiki/level0"], &[]);

        // Create level 2 (has before: level1)
        let level2_path = temp_dir.path().join(".aiki/flows/aiki/level2.yml");
        create_logging_flow(&level2_path, "Level 2", "L2", &["aiki/level1"], &[]);

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer
            .compose_flow("aiki/level2", EventType::ChangeCompleted, &mut state)
            .unwrap();

        assert!(matches!(result, FlowResult::Success));

        // Verify execution order: L0 -> L1 -> L2
        let log_path = temp_dir.path().join("execution_log.txt");
        let log_content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = log_content.lines().collect();

        assert_eq!(lines.len(), 3, "Expected 3 log entries, got: {:?}", lines);
        assert_eq!(lines[0], "L0", "Level 0 should run first");
        assert_eq!(lines[1], "L1", "Level 1 should run second");
        assert_eq!(lines[2], "L2", "Level 2 should run last");
    }

    #[test]
    fn test_variable_isolation_between_flows() {
        let temp_dir = create_test_project();

        // Create before flow that sets $my_var via shell output capture
        let before_path = temp_dir.path().join(".aiki/flows/aiki/before.yml");
        create_shell_var_flow(&before_path, "Before Flow", "my_var", "from_before", &[]);

        // Create main flow that checks $my_var then sets its own
        // If isolation works, main should NOT see before's $my_var
        let main_path = temp_dir.path().join(".aiki/flows/aiki/main.yml");
        let main_content = r#"name: Main Flow
version: "1"
before:
  - aiki/before
change.completed:
  - shell: echo "main_sees:$my_var" >> var_log.txt
  - shell: echo from_main
    alias: my_var
  - shell: echo "main_set:$my_var" >> var_log.txt
"#;
        fs::write(&main_path, main_content).unwrap();

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        let result = composer
            .compose_flow("aiki/main", EventType::ChangeCompleted, &mut state)
            .unwrap();

        assert!(matches!(result, FlowResult::Success));

        // Verify variable isolation
        let log_path = temp_dir.path().join("var_log.txt");
        let log_content = fs::read_to_string(&log_path).unwrap();
        // Filter out empty lines (shell captures trailing newlines)
        let lines: Vec<&str> = log_content.lines().filter(|l| !l.is_empty()).collect();

        // Before flow sets and echoes its var
        assert!(
            lines[0] == "from_before",
            "Before should echo its var: got {:?}",
            lines[0]
        );

        // Main flow should NOT see before's variable (isolation)
        // The $my_var should be empty/unset when main starts
        assert!(
            lines[1] == "main_sees:" || lines[1] == "main_sees:$my_var",
            "Main should NOT see before's variable (isolation): got {:?}",
            lines[1]
        );

        // After setting, main should see its own value
        assert_eq!(
            lines[2], "main_set:from_main",
            "Main should see its own variable"
        );
    }

    #[test]
    fn test_variable_isolation_before_flows_dont_see_caller() {
        let temp_dir = create_test_project();

        // Create a before flow that tries to read $caller_var
        let before_path = temp_dir.path().join(".aiki/flows/aiki/before.yml");
        let before_content = r#"name: Before Flow
version: "1"
change.completed:
  - shell: echo "before_sees:$caller_var" >> var_log.txt
"#;
        fs::write(&before_path, before_content).unwrap();

        // Create main flow that sets $caller_var via shell
        let main_path = temp_dir.path().join(".aiki/flows/aiki/main.yml");
        let main_content = r#"name: Main Flow
version: "1"
before:
  - aiki/before
change.completed:
  - shell: echo should_not_leak
    alias: caller_var
  - shell: echo "main_set:$caller_var" >> var_log.txt
"#;
        fs::write(&main_path, main_content).unwrap();

        let mut loader = FlowLoader::with_start_dir(temp_dir.path()).unwrap();
        let mut composer = FlowComposer::new(&mut loader);
        let mut state = create_test_state(&temp_dir);

        // Pre-set a variable in state to simulate a "caller" having variables
        state.set_variable("caller_var".to_string(), "from_caller".to_string());

        let result = composer
            .compose_flow("aiki/main", EventType::ChangeCompleted, &mut state)
            .unwrap();

        assert!(matches!(result, FlowResult::Success));

        // Verify isolation: before flow should NOT see caller's variable
        let log_path = temp_dir.path().join("var_log.txt");
        let log_content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = log_content.lines().collect();

        // Before flow should not see the caller's variable (isolation clears on entry)
        assert!(
            lines[0] == "before_sees:" || lines[0] == "before_sees:$caller_var",
            "Before should NOT see caller's variable: got {:?}",
            lines[0]
        );
    }
}
