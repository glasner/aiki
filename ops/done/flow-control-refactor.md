# Option 2: FlowStatement Design

## Critical Issues Fixed

This document has been updated to fix several critical implementation issues:

### 1. **OnFailure enum now uses Vec<FlowStatement>**
- **Problem**: Original spec had `OnFailure::Actions(Vec<Action>)` which prevented if/switch in failure handlers
- **Solution**: Changed to `OnFailure::Statements(Vec<FlowStatement>)` to allow control flow in on_failure blocks
- **Impact**: Enables YAML like:
  ```yaml
  on_failure:
    - if: $EXIT_CODE == 1
      then:
        - log: Recoverable error
      else:
        - stop: Fatal error
  ```

### 2. **Fixed temporary value borrowing in get_action_on_failure**
- **Problem**: Returned `&OnFailure` with temporary values caused compile errors
- **Solution**: Changed to return `OnFailure` by value (with Clone)
- **Impact**: No borrowing issues, cleaner API

### 3. **Proper nested timing capture implemented**
- **Problem**: Nested timings from if/switch branches were discarded
- **Solution**: 
  - `execute_if`/`execute_switch` return `(FlowResult, Option<FlowExecutionTimings>)`
  - `execute_statement` propagates nested timings
  - `StatementTiming::nested` properly captures branch timings
- **Impact**: Full timing visibility into control flow execution

### 4. **Added missing helper methods**
- Added `statement_type_name()` to identify statement types for timing
- Added `action_type_name()` to identify action types for timing
- Both methods support instrumentation and debugging

## Overview

Separate control flow constructs (if/switch) from actions by introducing a `FlowStatement` enum that wraps both.

## Core Type Changes

### New FlowStatement Enum

```rust
/// A statement in a flow - either an action or control flow
/// 
/// CRITICAL: Variant order matters for #[serde(untagged)] deserialization.
/// If/Switch MUST be tried before Action, otherwise serde will successfully
/// deserialize control-flow statements as actions and never reach the
/// control-flow variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlowStatement {
    /// Conditional execution - MUST be first to match before Action
    If(IfStatement),
    /// Switch/case statement - MUST be second to match before Action
    Switch(SwitchStatement),
    /// Regular action (shell, jj, log, etc.) - MUST be last as fallback
    Action(Action),
}
```

### Refactored Action Enum

```rust
/// An action to execute in a flow
/// Note: Control flow (if/switch) is now in FlowStatement, not here
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Action {
    // Control flow REMOVED - now in FlowStatement
    // If(IfAction),      ❌ REMOVED
    // Switch(SwitchAction), ❌ REMOVED
    
    /// Let binding (function call or variable aliasing)
    Let(LetAction),
    /// Self function call (call a function without storing result)
    Self_(SelfAction),
    /// Shell command
    Shell(ShellAction),
    /// JJ command
    Jj(JjAction),
    /// Log message
    Log(LogAction),
    /// Context injection (for PrePrompt events)
    Context(ContextAction),
    /// Autoreply (for PostResponse events)
    Autoreply(AutoreplyAction),
    /// Commit message (for PrepareCommitMessage events)
    CommitMessage(CommitMessageAction),
    /// Continue flow execution (generates Failure and continues)
    Continue(ContinueAction),
    /// Stop flow execution (emits warning and stops silently)
    Stop(StopAction),
    /// Block editor operation (emits error and blocks with exit 2)
    Block(BlockAction),
}

/// OnFailure enum updated to use FlowStatement instead of Action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OnFailure {
    /// Shortcut failure mode
    Shortcut(OnFailureShortcut),
    /// Execute statements on failure (can include if/switch)
    Statements(Vec<FlowStatement>),
}
```

### New Statement Types

```rust
/// Conditional statement (if/then/else)
/// 
/// Note: No on_failure field - evaluation errors propagate as flow errors.
/// This is intentional because condition evaluation failures indicate bugs
/// in the flow definition (e.g., malformed syntax, undefined variables).
/// Actions inside the branches handle their own failures via their on_failure fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfStatement {
    /// Condition to evaluate (supports variable access with $, JSON field access with .)
    #[serde(rename = "if")]
    pub condition: String,

    /// Statements to execute if condition is true
    pub then: Vec<FlowStatement>,

    /// Optional statements to execute if condition is false
    #[serde(default, rename = "else")]
    pub else_: Option<Vec<FlowStatement>>,
}

/// Switch/case statement
/// 
/// Note: No on_failure field - evaluation errors propagate as flow errors.
/// This is intentional because expression evaluation failures indicate bugs
/// in the flow definition (e.g., malformed syntax, undefined variables).
/// Actions inside the cases handle their own failures via their on_failure fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchStatement {
    /// Expression to evaluate and match against cases
    #[serde(rename = "switch")]
    pub expression: String,

    /// Map of case values to statements
    pub cases: std::collections::HashMap<String, Vec<FlowStatement>>,

    /// Optional default case if no cases match
    #[serde(default)]
    pub default: Option<Vec<FlowStatement>>,
}
```

### Updated Flow Type

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub name: String,
    
    #[serde(default)]
    pub description: Option<String>,
    
    #[serde(default = "default_version")]
    pub version: String,

    // All event handlers now use FlowStatement instead of Action
    #[serde(rename = "SessionStart", default)]
    pub session_start: Vec<FlowStatement>,

    #[serde(rename = "PrePrompt", default)]
    pub pre_prompt: Vec<FlowStatement>,

    #[serde(rename = "PreFileChange", default)]
    pub pre_file_change: Vec<FlowStatement>,

    #[serde(rename = "PostFileChange", default)]
    pub post_file_change: Vec<FlowStatement>,

    #[serde(rename = "PostResponse", default)]
    pub post_response: Vec<FlowStatement>,

    #[serde(rename = "PrepareCommitMessage", default)]
    pub prepare_commit_message: Vec<FlowStatement>,

    #[serde(rename = "Stop", default)]
    pub stop: Vec<FlowStatement>,
}
```

## Execution Engine Changes

### New Execute Method

```rust
impl FlowEngine {
    /// Execute a list of statements
    pub fn execute_statements(
        statements: &[FlowStatement],
        state: &mut AikiState,
    ) -> Result<(FlowResult, FlowExecutionTimings)> {
        use std::time::Instant;
        let total_start = Instant::now();
        let mut statement_timings = Vec::new();

        for statement in statements {
            let stmt_start = Instant::now();
            let (flow_result, nested_timing) = Self::execute_statement(statement, state)?;
            let duration = stmt_start.elapsed().as_secs_f64();
            
            statement_timings.push(StatementTiming {
                statement_type: Self::statement_type_name(statement),
                duration,
                nested: nested_timing.map(|t| t.statement_timings),
            });
            
            // Check for flow control
            match flow_result {
                FlowResult::Success | FlowResult::FailedContinue => {
                    // Continue to next statement
                }
                FlowResult::FailedStop => {
                    let total_duration = total_start.elapsed().as_secs_f64();
                    return Ok((
                        FlowResult::FailedStop, 
                        FlowExecutionTimings { total_duration, statement_timings }
                    ));
                }
                FlowResult::FailedBlock => {
                    let total_duration = total_start.elapsed().as_secs_f64();
                    return Ok((
                        FlowResult::FailedBlock, 
                        FlowExecutionTimings { total_duration, statement_timings }
                    ));
                }
            }
        }

        let total_duration = total_start.elapsed().as_secs_f64();
        Ok((
            FlowResult::Success,
            FlowExecutionTimings { total_duration, statement_timings }
        ))
    }

    /// Execute a single statement
    /// Returns tuple of (FlowResult, Option<FlowExecutionTimings>)
    fn execute_statement(
        statement: &FlowStatement,
        state: &mut AikiState,
    ) -> Result<(FlowResult, Option<FlowExecutionTimings>)> {
        match statement {
            FlowStatement::Action(action) => {
                let result = Self::execute_action(action, state)?;
                Ok((result, None))
            }
            FlowStatement::If(if_stmt) => {
                Self::execute_if(if_stmt, state)
            }
            FlowStatement::Switch(switch_stmt) => {
                Self::execute_switch(switch_stmt, state)
            }
        }
    }

    /// Execute an if statement
    /// 
    /// Evaluation errors (malformed conditions, undefined variables) propagate as Err,
    /// not as FlowResult failures. This is intentional - they indicate flow bugs.
    /// Returns tuple of (FlowResult, Option<FlowExecutionTimings>)
    fn execute_if(
        if_stmt: &IfStatement,
        state: &mut AikiState,
    ) -> Result<(FlowResult, Option<FlowExecutionTimings>)> {
        // Evaluate condition - propagate errors (flow bugs)
        let resolver = Self::create_variable_resolver(state);
        let condition_value = resolver.resolve(&if_stmt.condition)
            .context("If statement condition evaluation failed")?;
        let condition_result = Self::evaluate_condition(&condition_value)?;

        // Execute appropriate branch
        let branch = if condition_result {
            &if_stmt.then
        } else if let Some(else_branch) = &if_stmt.else_ {
            else_branch
        } else {
            // No else branch and condition is false - success (no-op)
            return Ok((FlowResult::Success, None));
        };

        // Execute the branch - return timing for nesting
        let (result, timing) = Self::execute_statements(branch, state)?;
        Ok((result, Some(timing)))
    }

    /// Execute a switch statement
    /// 
    /// Evaluation errors (undefined variables) propagate as Err,
    /// not as FlowResult failures. This is intentional - they indicate flow bugs.
    /// Returns tuple of (FlowResult, Option<FlowExecutionTimings>)
    fn execute_switch(
        switch_stmt: &SwitchStatement,
        state: &mut AikiState,
    ) -> Result<(FlowResult, Option<FlowExecutionTimings>)> {
        // Evaluate expression - propagate errors (flow bugs)
        let resolver = Self::create_variable_resolver(state);
        let expr_value = resolver.resolve(&switch_stmt.expression)
            .context("Switch statement expression evaluation failed")?;

        // Find matching case
        let branch = if let Some(case_actions) = switch_stmt.cases.get(&expr_value) {
            case_actions
        } else if let Some(default_actions) = &switch_stmt.default {
            default_actions
        } else {
            // No matching case and no default - success (no-op)
            return Ok((FlowResult::Success, None));
        };

        // Execute the branch - return timing for nesting
        let (result, timing) = Self::execute_statements(branch, state)?;
        Ok((result, Some(timing)))
    }

    /// Execute a regular action
    fn execute_action(
        action: &Action,
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        // Execute the action
        let result = match action {
            Action::Shell(shell_action) => Self::execute_shell(shell_action, state)?,
            Action::Jj(jj_action) => Self::execute_jj(jj_action, state)?,
            Action::Log(log_action) => Self::execute_log(log_action, state)?,
            Action::Let(let_action) => Self::execute_let(let_action, state)?,
            Action::Self_(self_action) => Self::execute_self(self_action, state)?,
            Action::Context(context_action) => Self::execute_context(context_action, state)?,
            Action::Autoreply(autoreply_action) => Self::execute_autoreply(autoreply_action, state)?,
            Action::CommitMessage(commit_msg_action) => Self::execute_commit_message(commit_msg_action, state)?,
            Action::Continue(continue_action) => {
                return Self::handle_action_failure(
                    &OnFailure::Shortcut(OnFailureShortcut::Continue),
                    &continue_action.failure,
                    state,
                );
            }
            Action::Stop(stop_action) => {
                return Self::handle_action_failure(
                    &OnFailure::Shortcut(OnFailureShortcut::Stop),
                    &stop_action.failure,
                    state,
                );
            }
            Action::Block(block_action) => {
                return Self::handle_action_failure(
                    &OnFailure::Shortcut(OnFailureShortcut::Block),
                    &block_action.failure,
                    state,
                );
            }
        };

        // Store action result
        Self::store_action_result(action, &result, state);

        // Handle action failure
        if !result.success {
            let on_failure = Self::get_action_on_failure(action);
            return Self::handle_action_failure(&on_failure, &result.stderr, state);
        }

        Ok(FlowResult::Success)
    }

    /// Handle action execution failure
    fn handle_action_failure(
        on_failure: &OnFailure,
        error_message: &str,
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        match on_failure {
            OnFailure::Shortcut(shortcut) => match shortcut {
                OnFailureShortcut::Continue => {
                    state.add_failure(error_message);
                    Ok(FlowResult::FailedContinue)
                }
                OnFailureShortcut::Stop => {
                    state.add_failure(error_message);
                    Ok(FlowResult::FailedStop)
                }
                OnFailureShortcut::Block => {
                    state.add_failure(error_message);
                    Ok(FlowResult::FailedBlock)
                }
            },
            OnFailure::Statements(statements) => {
                // Execute statements directly (they can include if/switch)
                let (result, _timing) = Self::execute_statements(statements, state)?;
                Ok(result)
            }
        }
    }

    fn get_action_on_failure(action: &Action) -> OnFailure {
        match action {
            Action::Shell(a) => a.on_failure.clone(),
            Action::Jj(a) => a.on_failure.clone(),
            Action::Let(a) => a.on_failure.clone(),
            Action::Self_(a) => a.on_failure.clone(),
            Action::Context(a) => a.on_failure.clone(),
            Action::Autoreply(a) => a.on_failure.clone(),
            Action::CommitMessage(a) => a.on_failure.clone(),
            // Flow control actions don't have on_failure
            Action::Log(_) | Action::Continue(_) | Action::Stop(_) | Action::Block(_) => {
                OnFailure::Shortcut(OnFailureShortcut::Continue)
            }
        }
    }

    fn statement_type_name(statement: &FlowStatement) -> String {
        match statement {
            FlowStatement::If(_) => "if".to_string(),
            FlowStatement::Switch(_) => "switch".to_string(),
            FlowStatement::Action(action) => Self::action_type_name(action),
        }
    }

    fn action_type_name(action: &Action) -> String {
        match action {
            Action::Shell(_) => "shell",
            Action::Jj(_) => "jj",
            Action::Log(_) => "log",
            Action::Let(_) => "let",
            Action::Self_(_) => "self",
            Action::Context(_) => "context",
            Action::Autoreply(_) => "autoreply",
            Action::CommitMessage(_) => "commit_message",
            Action::Continue(_) => "continue",
            Action::Stop(_) => "stop",
            Action::Block(_) => "block",
        }.to_string()
    }
}
```

## Benefits of This Approach

### 1. **Clearer Semantics**
```rust
// BEFORE: Everything is an "action"
let actions = vec![
    Action::If(...),      // Not really an action
    Action::Shell(...),   // Actual action
];

// AFTER: Clear distinction
let statements = vec![
    FlowStatement::If(...),      // Control flow
    FlowStatement::Action(Action::Shell(...)),  // Action
];
```

### 2. **No Result Mutation Needed**
```rust
// BEFORE: Mutate result to strip flow-control markers
let mut result = Self::execute_action(action, state)?;
result.stderr = result.stderr.strip_prefix("__FLOW_CONTROL__:").unwrap();

// AFTER: Direct flow control propagation
let flow_result = Self::execute_statement(statement, state)?;
// Returns FlowResult directly, no mutation needed
```

### 3. **No Flow Control Markers**
```rust
// BEFORE: Encode flow control in stderr strings
result.stderr = "__FLOW_CONTROL__:FailedStop:User cancelled".to_string();

// AFTER: Return typed flow control
return Ok(FlowResult::FailedStop);
```

### 4. **Type Safety**
```rust
// BEFORE: Special case handling for if/switch
match action {
    Action::If(_) | Action::Switch(_) => {
        // Check for flow control markers in stderr (fragile!)
        if result.stderr.starts_with("__FLOW_CONTROL__:") { ... }
    }
    _ => { ... }
}

// AFTER: Type-safe pattern matching
match statement {
    FlowStatement::If(if_stmt) => Self::execute_if(if_stmt, state),
    FlowStatement::Switch(switch_stmt) => Self::execute_switch(switch_stmt, state),
    FlowStatement::Action(action) => Self::execute_action(action, state),
}
```

### 5. **Better Error Messages**
```rust
// BEFORE: Generic action failure
"Action failed: ..."

// AFTER: Specific to statement type
"If statement condition evaluation failed: ..."
"Switch statement expression evaluation failed: ..."
"Shell action failed: ..."
```

## Timing Data Structures

```rust
/// Captures timing information for executed statements
pub struct FlowExecutionTimings {
    /// Total duration for entire statement list
    pub total_duration: f64,
    /// Per-statement timings (includes nested statements)
    pub statement_timings: Vec<StatementTiming>,
}

/// Timing information for a single statement
pub struct StatementTiming {
    /// Type of statement (action, if, switch)
    pub statement_type: String,
    /// Duration of this statement (includes nested execution)
    pub duration: f64,
    /// Nested statement timings (for if/switch branches)
    pub nested: Option<Vec<StatementTiming>>,
}

## Challenges

### 1. **YAML Deserialization**

The `#[serde(untagged)]` attribute will try each variant in order. This means:

```yaml
# This works - clear action
- shell: ls

# This works - clear if statement
- if: $x == 1
  then:
    - log: matched
```

**Solution**: Order matters in untagged enums. Put `If` and `Switch` before `Action` in the enum definition, and they'll be tried first. Since they have unique required fields (`if`/`then`, `switch`/`cases`), deserialization should work correctly.

**Note**: With `on_failure` removed from control statements, there's no ambiguity between `IfStatement` and actions anymore.

**Testing requirement**: Add regression tests to prove round-trip serialization works:

```rust
#[test]
fn test_if_statement_roundtrip() {
    let yaml = r#"
- if: $x == 1
  then:
    - log: matched
  else:
    - log: not matched
"#;
    
    // Deserialize
    let statements: Vec<FlowStatement> = serde_yaml::from_str(yaml).unwrap();
    
    // Verify deserialized as If, not Action
    assert!(matches!(statements[0], FlowStatement::If(_)));
    
    // Serialize back
    let serialized = serde_yaml::to_string(&statements).unwrap();
    
    // Deserialize again
    let roundtrip: Vec<FlowStatement> = serde_yaml::from_str(&serialized).unwrap();
    
    // Should still be If
    assert!(matches!(roundtrip[0], FlowStatement::If(_)));
}

#[test]
fn test_switch_statement_roundtrip() {
    let yaml = r#"
- switch: $EXIT_CODE
  cases:
    "0":
      - log: success
    "1":
      - log: error
  default:
    - log: unknown
"#;
    
    let statements: Vec<FlowStatement> = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(statements[0], FlowStatement::Switch(_)));
    
    let serialized = serde_yaml::to_string(&statements).unwrap();
    let roundtrip: Vec<FlowStatement> = serde_yaml::from_str(&serialized).unwrap();
    assert!(matches!(roundtrip[0], FlowStatement::Switch(_)));
}

#[test]
fn test_action_statement_roundtrip() {
    let yaml = r#"
- shell: ls
- log: message
- continue: failure
"#;
    
    let statements: Vec<FlowStatement> = serde_yaml::from_str(yaml).unwrap();
    
    // All should deserialize as Action variants
    assert!(matches!(statements[0], FlowStatement::Action(Action::Shell(_))));
    assert!(matches!(statements[1], FlowStatement::Action(Action::Log(_))));
    assert!(matches!(statements[2], FlowStatement::Action(Action::Continue(_))));
    
    let serialized = serde_yaml::to_string(&statements).unwrap();
    let roundtrip: Vec<FlowStatement> = serde_yaml::from_str(&serialized).unwrap();
    
    assert!(matches!(roundtrip[0], FlowStatement::Action(Action::Shell(_))));
    assert!(matches!(roundtrip[1], FlowStatement::Action(Action::Log(_))));
    assert!(matches!(roundtrip[2], FlowStatement::Action(Action::Continue(_))));
}
```

### 2. **OnFailure Actions** [FIXED]

The `OnFailure` enum has been updated to use `Vec<FlowStatement>` instead of `Vec<Action>`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OnFailure {
    Shortcut(OnFailureShortcut),
    // Changed from Vec<Action> to Vec<FlowStatement>
    Statements(Vec<FlowStatement>),
}
```

This allows if/switch in failure handlers:
```yaml
- shell: risky-command
  on_failure:
    - if: $EXIT_CODE == 1
      then:
        - log: Recoverable error
      else:
        - stop: Fatal error
```

**Implementation Notes:**
- `handle_action_failure` now executes statements directly without wrapping
- `get_action_on_failure` returns `OnFailure` by value to avoid temporary borrow issues

### 3. **Migration Path**

This is a **breaking change** only at the Rust API level. The YAML format remains the same:

```yaml
# This YAML works before and after the change
SessionStart:
  - if: $SESSION_ID != ""
    then:
      - log: Resuming session
    else:
      - shell: generate-new-session-id
```

**Breaking at Rust level**:
- Any code that constructs `Action::If(...)` manually breaks
- Any code that pattern matches on `Action::If(...)` breaks

**Not breaking**:
- YAML flow files continue to work
- Deserialization works the same way
- CLI behavior unchanged

### 4. **Testing**

Need to update tests that construct actions programmatically:

```rust
// BEFORE
let action = Action::If(IfAction {
    condition: "$x == 1".to_string(),
    then: vec![Action::Log(...)],
    else_: None,
    on_failure: OnFailure::default(),
});

// AFTER
let statement = FlowStatement::If(IfStatement {
    condition: "$x == 1".to_string(),
    then: vec![FlowStatement::Action(Action::Log(...))],
    else_: None,
    on_failure: OnFailure::default(),
});
```

## Preserving Instrumentation

### Current Timing/Trace Infrastructure

The existing `execute_actions` method provides:
1. **Action-level timing** - Each action tracks its execution duration
2. **Flow-level aggregation** - Total duration across all actions
3. **Stack traces** - `state.flow_trace` tracks execution path
4. **Failure attribution** - Which action failed and why
5. **FlowExecutionTimings** - Structured timing data for tests

### Strategy for Preserving Instrumentation

#### 1. Timing Hierarchy

```rust
pub struct FlowExecutionTimings {
    /// Total duration for entire statement list
    pub total_duration: f64,
    /// Per-statement timings (includes nested statements)
    pub statement_timings: Vec<StatementTiming>,
}

pub struct StatementTiming {
    /// Type of statement (action, if, switch)
    pub statement_type: String,
    /// Duration of this statement (includes nested execution)
    pub duration: f64,
    /// Nested statement timings (for if/switch branches)
    pub nested: Option<Vec<StatementTiming>>,
}
```

#### 2. Updated execute_statements with Instrumentation

```rust
impl FlowEngine {
    pub fn execute_statements(
        statements: &[FlowStatement],
        state: &mut AikiState,
    ) -> Result<(FlowResult, FlowExecutionTimings)> {
        use std::time::Instant;
        let total_start = Instant::now();
        let mut statement_timings = Vec::new();

        for (index, statement) in statements.iter().enumerate() {
            let stmt_start = Instant::now();
            
            // Add to trace
            state.flow_trace.push(format!("statement[{}]", index));
            
            let (flow_result, nested_timing) = Self::execute_statement(statement, state)?;
            let duration = stmt_start.elapsed().as_secs_f64();
            
            statement_timings.push(StatementTiming {
                statement_type: Self::statement_type_name(statement),
                duration,
                nested: nested_timing.map(|t| t.statement_timings),
            });
            
            // Check for flow control
            match flow_result {
                FlowResult::Success | FlowResult::FailedContinue => {
                    // Continue to next statement
                }
                FlowResult::FailedStop => {
                    state.flow_trace.push("stopped".to_string());
                    let total_duration = total_start.elapsed().as_secs_f64();
                    return Ok((
                        FlowResult::FailedStop, 
                        FlowExecutionTimings { total_duration, statement_timings }
                    ));
                }
                FlowResult::FailedBlock => {
                    state.flow_trace.push("blocked".to_string());
                    let total_duration = total_start.elapsed().as_secs_f64();
                    return Ok((
                        FlowResult::FailedBlock,
                        FlowExecutionTimings { total_duration, statement_timings }
                    ));
                }
            }
        }

        let total_duration = total_start.elapsed().as_secs_f64();
        Ok((
            FlowResult::Success, 
            FlowExecutionTimings { total_duration, statement_timings }
        ))
    }

    fn statement_type_name(statement: &FlowStatement) -> String {
        match statement {
            FlowStatement::If(_) => "if".to_string(),
            FlowStatement::Switch(_) => "switch".to_string(),
            FlowStatement::Action(action) => Self::action_type_name(action),
        }
    }

    fn action_type_name(action: &Action) -> String {
        match action {
            Action::Shell(_) => "shell",
            Action::Jj(_) => "jj",
            Action::Log(_) => "log",
            Action::Let(_) => "let",
            Action::Self_(_) => "self",
            Action::Context(_) => "context",
            Action::Autoreply(_) => "autoreply",
            Action::CommitMessage(_) => "commit_message",
            Action::Continue(_) => "continue",
            Action::Stop(_) => "stop",
            Action::Block(_) => "block",
        }.to_string()
    }
}
```

#### 3. Nested Timing Extraction

```rust
impl FlowEngine {
    fn execute_if(
        if_stmt: &IfStatement,
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        // Evaluate condition
        let resolver = Self::create_variable_resolver(state);
        let condition_value = resolver.resolve(&if_stmt.condition)
            .context("If statement condition evaluation failed")?;
        let condition_result = Self::evaluate_condition(&condition_value)?;

        // Determine branch
        let (branch, branch_name) = if condition_result {
            (&if_stmt.then, "then")
        } else if let Some(else_branch) = &if_stmt.else_ {
            (else_branch, "else")
        } else {
            return Ok(FlowResult::Success);
        };

        // Add to trace
        state.flow_trace.push(format!("if:{}", branch_name));

        // Execute the branch with full instrumentation
        let (result, _timing) = Self::execute_statements(branch, state)?;
        
        Ok(result)
    }

    fn execute_switch(
        switch_stmt: &SwitchStatement,
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        // Evaluate expression
        let resolver = Self::create_variable_resolver(state);
        let expr_value = resolver.resolve(&switch_stmt.expression)
            .context("Switch statement expression evaluation failed")?;

        // Find matching case
        let (branch, case_name) = if let Some(case_actions) = switch_stmt.cases.get(&expr_value) {
            (case_actions, expr_value.clone())
        } else if let Some(default_actions) = &switch_stmt.default {
            (default_actions, "default".to_string())
        } else {
            return Ok(FlowResult::Success);
        };

        // Add to trace
        state.flow_trace.push(format!("switch:case:{}", case_name));

        // Execute the branch with full instrumentation
        let (result, _timing) = Self::execute_statements(branch, state)?;
        
        Ok(result)
    }
}
```

#### 4. Failure Attribution

```rust
impl FlowEngine {
    fn handle_action_failure(
        on_failure: &OnFailure,
        error_message: &str,
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        // Add failure to trace
        state.flow_trace.push(format!("failure: {}", error_message));
        
        match on_failure {
            OnFailure::Shortcut(shortcut) => match shortcut {
                OnFailureShortcut::Continue => {
                    state.add_failure(error_message);
                    state.flow_trace.push("continuing".to_string());
                    Ok(FlowResult::FailedContinue)
                }
                OnFailureShortcut::Stop => {
                    state.add_failure(error_message);
                    state.flow_trace.push("stopping".to_string());
                    Ok(FlowResult::FailedStop)
                }
                OnFailureShortcut::Block => {
                    state.add_failure(error_message);
                    state.flow_trace.push("blocking".to_string());
                    Ok(FlowResult::FailedBlock)
                }
            },
            OnFailure::Actions(actions) => {
                state.flow_trace.push("executing on_failure actions".to_string());
                let statements: Vec<FlowStatement> = actions
                    .iter()
                    .map(|a| FlowStatement::Action(a.clone()))
                    .collect();
                let (result, _timing) = Self::execute_statements(&statements, state)?;
                Ok(result)
            }
        }
    }
}
```

### Impact on Tests

**No regression**: Tests that verify timing/trace data will continue to work:

```rust
#[test]
fn test_timing_preserved() {
    let flow = Flow {
        session_start: vec![
            FlowStatement::Action(Action::Shell(ShellAction { cmd: "sleep 0.1".to_string(), .. })),
            FlowStatement::If(IfStatement {
                condition: "true".to_string(),
                then: vec![
                    FlowStatement::Action(Action::Log(LogAction { message: "branch".to_string() })),
                ],
                else_: None,
            }),
        ],
        // ...
    };
    
    let (result, timings) = engine.execute_statements(&flow.session_start, &mut state)?;
    
    // Verify total timing exists
    assert!(timings.total_duration > 0.1);
    
    // Verify statement-level timing
    assert_eq!(timings.statement_timings.len(), 2);
    assert_eq!(timings.statement_timings[0].statement_type, "shell");
    assert_eq!(timings.statement_timings[1].statement_type, "if");
    
    // Verify trace
    assert_eq!(state.flow_trace, vec![
        "statement[0]",
        "statement[1]",
        "if:then",
        "statement[0]", // nested log action
    ]);
}
```

### Summary

**Preserved**:
- ✅ Action-level timing (now statement-level, includes actions)
- ✅ Flow-level aggregation (total_duration)
- ✅ Stack traces (state.flow_trace with more detail)
- ✅ Failure attribution (clearer with statement types)
- ✅ Structured timing data (FlowExecutionTimings)

**Enhanced**:
- ✅ Nested timing visibility (can see if/switch branch timing)
- ✅ Statement type attribution (know if it was if/switch/action)
- ✅ Better trace granularity (see which branch was taken)

**No regression risk**: All existing timing/trace tests will pass with minimal updates to expected trace strings.

## Implementation Steps

1. **Add FlowStatement enum** to `types.rs`
2. **Rename IfAction → IfStatement, SwitchAction → SwitchStatement**
3. **Remove If/Switch from Action enum**
4. **Update Flow struct** to use `Vec<FlowStatement>`
5. **Update OnFailure** to use `Vec<FlowStatement>`
6. **Add StatementTiming and FlowExecutionTimings** to support nested timing
7. **Refactor engine.rs**:
   - Update `execute_statements()` with full instrumentation
   - Add `execute_statement()` with trace tracking
   - Add `execute_if()` with branch tracking
   - Add `execute_switch()` with case tracking
   - Update `execute_action()` to preserve existing timing
   - Update `handle_action_failure()` to add trace entries
   - Add helper methods: `statement_type_name()`, `action_type_name()`
8. **Update tests** to expect enhanced trace data
9. **Add roundtrip serialization tests** for If/Switch/Action
10. **Verify existing timing tests** still pass

## Files to Change

- `cli/src/flows/types.rs` - Type definitions
- `cli/src/flows/engine.rs` - Execution logic
- `cli/tests/*.rs` - Test updates

## Estimated Impact

- **Lines added**: ~150 (new execute_statements/execute_statement/execute_if/execute_switch methods)
- **Lines removed**: ~150 (flow-control marker handling, on_failure for control statements, handle_statement_failure)
- **Net complexity**: Significantly reduced (clearer separation of concerns, simpler error handling)
- **Breaking changes**: Rust API only, not YAML format
- **Test updates**: ~10-15 tests need updating

## Key Simplifications from Removing on_failure

1. **No `handle_statement_failure()` method needed** - Control statements propagate errors directly
2. **No `mut result` mutation** - No flow-control marker stripping
3. **No flow-control markers** - Direct FlowResult propagation
4. **Clearer error semantics** - Evaluation errors are flow bugs, not recoverable failures
5. **Simpler type definitions** - Fewer fields on IfStatement/SwitchStatement

## Decision Point

This is a significant refactor. Key questions:

1. **Is the complexity worth it?** - The current system works, albeit with some awkwardness
2. **Do we have breaking changes we need to make anyway?** - If so, bundle them
3. **Are we planning to add more control flow constructs?** - If yes, this makes future additions cleaner

If we proceed, this should be done in a single PR with comprehensive testing.
