# Milestone 5.1: Migrate to `let:` Syntax for Function Calls

**Date**: 2025-01-16  
**Status**: 📋 Planned  
**Type**: Breaking change (syntax migration)

## Overview

Migrate from the current `aiki:` action syntax to the more intuitive `let: var = function(args)` syntax for calling built-in and external functions in flows. This improves readability and makes variable binding explicit.

## Motivation

### Current Syntax (Implicit Variable Naming)
```yaml
PostChange:
  - aiki: build_provenance_description
    args:
      agent: "$event.agent"
      session_id: "$event.session_id"
  
  # Variable name is auto-generated and awkward
  - jj: describe -m "$build_provenance_description.output"
```

**Problems:**
- Variable names are auto-generated from function names
- Unclear what variable is created
- Verbose references (`.output` suffix)
- Not intuitive for users

### New Syntax (Explicit Variable Binding, No Args)
```yaml
PostChange:
  # Functions automatically receive $event context
  - let: description = aiki/provenance.build_description
  
  # Clean, explicit variable name
  - jj: describe -m "$description"
```

**Benefits:**
- ✅ Explicit variable naming (you choose the name)
- ✅ Reads like code: "let description equal..."
- ✅ Clean variable references (no `.output`)
- ✅ Familiar pattern (every language has `let`/`var`)
- ✅ Scannability: variable names are left-aligned
- ✅ **Simple**: No args needed, functions receive full $event context
- ✅ **Consistent**: All functions work the same way

## The `let:` Action

### Syntax Overview

```yaml
- let: variable_name = expression
  on_failure: continue  # or: fail (optional)
```

**No `args:` needed!** Functions automatically receive the full `$event` context.

### Two Modes of Operation

#### Mode 1: Function Call
Call a function and bind the result to a variable. The function automatically receives all event context.

```yaml
- let: description = aiki/provenance.build_description

- jj: describe -m "$description"
```

**How it works:**
1. Parse `let:` to extract variable name and function path
2. Pass full `ExecutionContext` to the function (includes all `$event.*` variables)
3. Function accesses what it needs from context
4. Store result as `$variable_name`
5. Also store structured outputs: `$variable_name.exit_code`, `$variable_name.failed`

**Example function implementation:**
```rust
fn fn_build_provenance_description(context: &ExecutionContext) -> Result<ActionResult> {
    // Function reads what it needs from context
    let agent = context.event_vars.get("agent")
        .ok_or_else(|| anyhow::anyhow!("Missing event.agent"))?;
    
    let session_id = context.event_vars.get("session_id")
        .ok_or_else(|| anyhow::anyhow!("Missing event.session_id"))?;
    
    // Build provenance...
    let description = format!("[aiki]\nagent={}\nsession={}\n[/aiki]", agent, session_id);
    
    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: description,
        stderr: String::new(),
    })
}
```

#### Mode 2: Variable Aliasing
Create a new name for an existing variable.

```yaml
- let: description = aiki/provenance.build_description

# Create a shorter alias (copies the value)
- let: desc = $description

- jj: describe -m "$desc"
```

**How it works:**
1. Detect `$` prefix on right-hand side
2. Resolve the source variable
3. Copy value and store under new variable name

**Note:** This creates a **copy**, not a reference. Variables are stored as copies, so modifying the original later won't affect the alias.

### Complete Examples

#### Example 1: Simple Function Call
```yaml
PostChange:
  # Function receives full $event context automatically
  - let: description = aiki/provenance.build_description
  
  # Function can access $event.agent, $event.session_id, $event.tool_name, etc.
  - jj: describe -m "$description"
  - jj: new
```

#### Example 2: Variable Aliasing for Readability
```yaml
PreCommit:
  # Capture event data with cleaner names
  - let: file = $event.file_path
  - let: agent = $event.agent
  
  # Build provenance
  - let: description = aiki/provenance.build_description
  
  - log: "$agent edited $file"
  - jj: describe -m "$description"
```

## Implementation Plan

### 1. Update Type Definitions (`cli/src/flows/types.rs`)

**Add new `LetAction` struct:**
```rust
/// Let binding action - call a function or alias a variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetAction {
    /// The let binding: "var_name = expression"
    #[serde(rename = "let")]
    pub let_: String,
    
    /// Failure handling mode
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}
```

**Note:** No `args` field! Functions receive the full `ExecutionContext` which contains all `$event.*` variables.

**Update `Action` enum:**
```rust
pub enum Action {
    Shell(ShellAction),
    Jj(JjAction),
    Log(LogAction),
    Let(LetAction),  // Replace Aiki with Let
}
```

**Note:** The `alias` field already exists on `ShellAction`, `JjAction`, and `LogAction` (added in Phase 5.0). This milestone will wire those existing aliases through the new `store_action_result()` helper to implement the hybrid pattern (`$var`, `$var.output`, `$var.exit_code`, `$var.failed`).

**Remove `AikiAction` struct** (clean break approach).

### 2. Update Executor (`cli/src/flows/executor.rs`)

**Add `execute_let()` function:**

```rust
/// Validate variable name: must match [A-Za-z_][A-Za-z0-9_]*
fn is_valid_variable_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    
    let mut chars = name.chars();
    
    // First character must be letter or underscore
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {},
        _ => return false,
    }
    
    // Remaining characters must be alphanumeric or underscore
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn execute_let(action: &LetAction, context: &mut ExecutionContext) -> Result<ActionResult> {
    // Parse: "var_name = something"
    // Use splitn(2, '=') to split only at first '=' and preserve any '=' in the expression
    let parts: Vec<&str> = action.let_.splitn(2, '=').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid let syntax: expected 'var = value', got '{}'", action.let_);
    }
    
    // Trim whitespace (handles YAML editors that auto-format)
    let var_name = parts[0].trim();
    let value_expr = parts[1].trim();
    
    // Validate variable name
    if !is_valid_variable_name(var_name) {
        anyhow::bail!(
            "Invalid variable name '{}': must match [A-Za-z_][A-Za-z0-9_]*",
            var_name
        );
    }
    
    // Check if it's a variable reference (starts with $)
    if value_expr.starts_with('$') {
        // Variable aliasing: let: new_name = $old_name
        let mut resolver = VariableResolver::new();
        resolver.add_event_vars(&context.event_vars);
        resolver.add_env_vars(&context.env_vars);
        
        let resolved_value = resolver.resolve(value_expr);
        
        // Store as new variable name (direct access)
        context.event_vars.insert(var_name.to_string(), resolved_value.clone());
        
        // IMPORTANT: Also copy structured metadata siblings if they exist
        // This ensures aliasing preserves .output, .exit_code, .failed, etc.
        // Extract the source variable name (strip leading $)
        let source_var = value_expr.trim_start_matches('$');
        
        // Copy .output sibling if it exists
        if let Some(output) = context.event_vars.get(&format!("{}.output", source_var)) {
            context.event_vars.insert(
                format!("{}.output", var_name),
                output.clone(),
            );
        }
        
        // Copy .exit_code sibling if it exists
        if let Some(exit_code) = context.event_vars.get(&format!("{}.exit_code", source_var)) {
            context.event_vars.insert(
                format!("{}.exit_code", var_name),
                exit_code.clone(),
            );
        }
        
        // Copy .failed sibling if it exists
        if let Some(failed) = context.event_vars.get(&format!("{}.failed", source_var)) {
            context.event_vars.insert(
                format!("{}.failed", var_name),
                failed.clone(),
            );
        }
        
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] let: {} = {} (alias)", var_name, value_expr);
        }
        
        Ok(ActionResult {
            success: true,
            exit_code: Some(0),
            stdout: resolved_value,
            stderr: String::new(),
        })
    } else {
        // Function call: let: var_name = function_name
        let function_path = value_expr;
        
        if std::env::var("AIKI_DEBUG").is_ok() {
            eprintln!("[flows] let: {} = {}()", var_name, function_path);
        }
        
        // Route to appropriate function based on namespace
        let result = if function_path.starts_with("aiki/") {
            // Built-in Aiki functions
            match function_path {
                "aiki/provenance.build_description" => {
                    Self::fn_build_provenance_description(context)?
                }
                _ => anyhow::bail!("Unknown aiki function: {}", function_path),
            }
        } else if function_path.starts_with("vendor/") || function_path.starts_with("my/") {
            // Future: External WASM/native functions (Phase 8)
            anyhow::bail!("External functions not yet implemented: {}", function_path)
        } else {
            anyhow::bail!(
                "Invalid function path '{}': must start with aiki/, vendor/, or my/",
                function_path
            )
        };
        
        // Store result with user-specified variable name (direct access)
        context.event_vars.insert(var_name.to_string(), result.stdout.clone());
        
        // Also store structured output for advanced use cases
        context.event_vars.insert(
            format!("{}.output", var_name),
            result.stdout.clone(),
        );
        
        if let Some(exit_code) = result.exit_code {
            context.event_vars.insert(
                format!("{}.exit_code", var_name),
                exit_code.to_string(),
            );
        }
        
        context.event_vars.insert(
            format!("{}.failed", var_name),
            (!result.success).to_string(),
        );
        
        Ok(result)
    }
}
```

**Update function signature:**
- Old: `fn aiki_build_provenance_description(args: &HashMap<String, String>, context: &ExecutionContext)`
- New: `fn fn_build_provenance_description(context: &ExecutionContext)`
- **No args parameter!** Functions read what they need from `context.event_vars`

**Example function implementation:**
```rust
fn fn_build_provenance_description(context: &ExecutionContext) -> Result<ActionResult> {
    use crate::provenance::{
        AgentInfo, AgentType, AttributionConfidence, DetectionMethod, ProvenanceRecord,
    };

    // Extract required data from context with helpful error messages
    let agent_str = context.event_vars.get("agent")
        .ok_or_else(|| anyhow::anyhow!(
            "Function 'aiki/provenance.build_description' requires event.agent, but it's not set. \
             This event may not provide agent information."
        ))?;

    let session_id = context.event_vars.get("session_id")
        .ok_or_else(|| anyhow::anyhow!(
            "Function 'aiki/provenance.build_description' requires event.session_id, but it's not set. \
             Ensure the session_id is available in the event context."
        ))?;

    let tool_name = context.event_vars.get("tool_name")
        .ok_or_else(|| anyhow::anyhow!(
            "Function 'aiki/provenance.build_description' requires event.tool_name, but it's not set. \
             This may indicate the tool information is unavailable."
        ))?;

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

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: description,
        stderr: String::new(),
    })
}
```

**Future functions follow this pattern:**
- All functions: `fn fn_<name>(context: &ExecutionContext) -> Result<ActionResult>`
- Read from `context.event_vars` as needed
- Return result in `ActionResult.stdout`

**Update `execute_log()` to populate stdout:**

The current implementation returns `ActionResult::success()` with empty stdout. To support aliasing (storing the resolved message in `$alias`), update it to:

```rust
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

    // Return the resolved message in stdout so aliases work
    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: message,  // Changed from ActionResult::success() which has empty stdout
        stderr: String::new(),
    })
}
```

This allows:
```yaml
- log: "Completed: $event.agent"
  alias: log_msg
- shell: echo "$log_msg"  # Will contain the resolved message
```

**Update `execute_action()` signature and match:**

**IMPORTANT:** `execute_action` must accept `&mut ExecutionContext` because `execute_let` needs to mutate the context to store variables.

```rust
fn execute_action(action: &Action, context: &mut ExecutionContext) -> Result<ActionResult> {
    match action {
        Action::Shell(shell_action) => Self::execute_shell(shell_action, context),
        Action::Jj(jj_action) => Self::execute_jj(jj_action, context),
        Action::Log(log_action) => Self::execute_log(log_action, context),
        Action::Let(let_action) => Self::execute_let(let_action, context),
    }
}
```

**Update `execute_actions()` for variable storage:**
```rust
/// Store an action result with the hybrid pattern (direct + structured access)
fn store_action_result(
    context: &mut ExecutionContext,
    var_name: &str,
    result: &ActionResult,
) {
    // Store direct access: $var_name = stdout
    context.event_vars.insert(var_name.to_string(), result.stdout.clone());
    
    // Store explicit output: $var_name.output = stdout (same as direct)
    context.event_vars.insert(
        format!("{}.output", var_name),
        result.stdout.clone(),
    );
    
    // Store exit code metadata
    if let Some(exit_code) = result.exit_code {
        context.event_vars.insert(
            format!("{}.exit_code", var_name),
            exit_code.to_string(),
        );
    }
    
    // Store failure status metadata
    context.event_vars.insert(
        format!("{}.failed", var_name),
        (!result.success).to_string(),
    );
}

for action in actions {
    let result = Self::execute_action(action, context)?;

    // Store step results for reference by subsequent actions (if alias provided)
    // IMPORTANT: Let actions handle their own storage inside execute_let()
    // because they need to parse the variable name and store during execution.
    // All other action types store results here after execution completes.
    match action {
        Action::Let(let_action) => {
            // Variable already stored in execute_let() with structured metadata
            // ($var_name, $var_name.output, $var_name.exit_code, $var_name.failed)
            // No additional storage needed here - just check for failure mode below
        }
        Action::Shell(shell_action) => {
            if let Some(ref alias) = shell_action.alias {
                Self::store_action_result(context, alias, &result);
            }
        }
        Action::Jj(jj_action) => {
            if let Some(ref alias) = jj_action.alias {
                Self::store_action_result(context, alias, &result);
            }
        }
        Action::Log(log_action) => {
            if let Some(ref alias) = log_action.alias {
                Self::store_action_result(context, alias, &result);
            }
        }
    }

    // Check failure mode
    let should_stop = match action {
        Action::Shell(a) => !result.success && a.on_failure == FailureMode::Fail,
        Action::Jj(a) => !result.success && a.on_failure == FailureMode::Fail,
        Action::Let(a) => !result.success && a.on_failure == FailureMode::Fail,
        Action::Log(_) => false,
    };

    results.push(result);

    if should_stop {
        anyhow::bail!("Action failed with on_failure: fail");
    }
}
```

**Remove `execute_aiki()` function.**

### 3. Update System Flows (`cli/flows/provenance.yaml`)

**Before:**
```yaml
name: "Aiki Provenance Recording"
description: "System flow that records AI change metadata in JJ change descriptions"
version: "1"

PostChange:
  - aiki: build_provenance_description
    args:
      agent: "$event.agent"
      session_id: "$event.session_id"
      tool_name: "$event.tool_name"
    on_failure: fail

  - jj: describe -m "$build_provenance_description.output"
  - jj: new
  - log: "Recorded change by $event.agent (session: $event.session_id)"
```

**After:**
```yaml
name: "Aiki Provenance Recording"
description: "System flow that records AI change metadata in JJ change descriptions"
version: "1"

PostChange:
  # Build provenance metadata (function reads $event.agent, $event.session_id, $event.tool_name from context)
  - let: description = aiki/provenance.build_description
    on_failure: fail

  # Set change description
  - jj: describe -m "$description"
  
  # Create new change for next edit
  - jj: new
  
  # Log success
  - log: "Recorded change by $event.agent (session: $event.session_id)"
```

### 4. Update Documentation

#### `ops/phase-5.md`

**Add new section under "Action Types":**

```markdown
### 8. Let Binding (Function Calls)

Call built-in Aiki functions or external WASM/Rust functions and bind the result to a variable.

**IMPORTANT:** Functions automatically receive the full `$event` context. No `args:` block needed!

**Syntax:**
```yaml
- let: variable_name = function_path
  on_failure: continue  # or: fail (optional)
```

**Two modes:**

1. **Function call** - Right-hand side is a function path:
```yaml
# Function automatically receives all $event.* variables
- let: complexity = vendor/analyzer.analyze_complexity

- log: "Complexity: $complexity"
```

The function accesses what it needs from the event context:
```rust
fn analyze_complexity(context: &ExecutionContext) -> Result<ActionResult> {
    // Function reads directly from context
    let file_path = context.event_vars.get("file_path")?;
    // ... perform analysis
}
```

2. **Variable aliasing** - Right-hand side starts with `$`:
```yaml
- let: desc = $description
- let: file = $event.file_path
```

**Variable storage (hybrid pattern):**
- Result stored as `$variable_name` (direct access to output string)
- Also available: `$variable_name.output` (explicit, same as `$variable_name`)
- Metadata: `$variable_name.exit_code`, `$variable_name.failed`

**This hybrid pattern applies to ALL action types** (shell, jj, log, let) for consistency. Each action type can optionally specify an `alias:` field to name the result variable.

**Important:** Variables are stored as **copies** in the execution context (internally as strings). There are no references - each variable assignment creates a new copy of the value. Values are converted to appropriate types when used (strings, numbers, booleans).

**Examples:**
```yaml
- let: description = aiki/provenance.build_description

# Simple access (most common)
- jj: describe -m "$description"

# Error checking (when needed)
- if: $description.failed
  then:
    - log: "Failed: exit code $description.exit_code"

# Variable aliasing also preserves structured metadata:
- let: desc = $description
- log: "$desc"                # Direct access
- log: "$desc.output"         # Explicit (same thing)
- log: "$desc.failed"         # Metadata is copied

# Shell/Jj/Log actions can also store results with alias:
- shell: git status
  alias: git_status
- log: "$git_status"          # Direct access
- log: "$git_status.output"   # Explicit
- log: "$git_status.failed"   # Metadata available
```

**Available built-in functions (Phase 5.1):**
- `aiki/provenance.build_description` - Generate `[aiki]` metadata block

**Namespace requirements:**
- Built-in functions: Must use `aiki/` prefix
- External functions: Reserved for Phase 8 (`vendor/` or `my/` namespaces)

**Future external functions (Phase 8):**
- WASM functions: `vendor/analyzer.analyze_complexity`
- Native compiled: `my/perf-analyzer.scan`

See [`ops/ROADMAP.md` Phase 8](ROADMAP.md#phase-8-external-flow-ecosystem) for external function implementation.
```

#### `ops/PHASE_5.1_NATIVE_FUNCTIONS.md`

**Update all examples to use `let:` syntax with namespaced function paths.**

**Before:**
```yaml
- aiki: build_provenance_description
  args:
    agent: "$event.agent"
```

**After:**
```yaml
# Function automatically receives full $event context - no args needed
- let: description = aiki/provenance.build_description
```

**Update "Available Aiki Functions" → "Available Built-in Functions"**

#### `ops/PHASE_5.1_COMPLETE.md`

**Update implementation notes:**

```markdown
## What Changed

### Before: Implicit Variable Naming
```yaml
- aiki: build_provenance_description
  args:
    agent: "$event.agent"

# Auto-generated variable
- jj: describe -m "$build_provenance_description.output"
```

### After: Explicit Variable Binding (No Args Needed!)
```yaml
# Function automatically receives full $event context
- let: description = aiki/provenance.build_description

# Clean, chosen variable name (direct access - no .output suffix)
- jj: describe -m "$description"
```

## Benefits

1. **Explicit variable naming** - You control the variable name
2. **Familiar syntax** - `let` is universal across languages
3. **Better readability** - "Let X equal Y" reads naturally
4. **Variable aliasing** - Create shorter names: `let: desc = $description`
5. **Future-proof** - Works with WASM/native functions in Phase 8
```

### 5. Add Tests

**Parser tests:**

Add to `cli/src/flows/parser.rs` or create new test file:

```rust
#[test]
fn test_parse_let_action() {
    let yaml = r#"
- let: result = some_function
  on_failure: fail
"#;
    
    let actions: Vec<Action> = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actions.len(), 1);
    
    match &actions[0] {
        Action::Let(let_action) => {
            assert_eq!(let_action.let_, "result = some_function");
            assert_eq!(let_action.on_failure, FailureMode::Fail);
        }
        _ => panic!("Expected Let action"),
    }
}

#[test]
fn test_parse_let_aliasing() {
    let yaml = r#"
- let: new_name = $old_name
"#;
    
    let actions: Vec<Action> = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actions.len(), 1);
    
    match &actions[0] {
        Action::Let(let_action) => {
            assert_eq!(let_action.let_, "new_name = $old_name");
            // No args field anymore
        }
        _ => panic!("Expected Let action"),
    }
}

#[test]
fn test_parse_let_minimal() {
    let yaml = r#"
- let: x = func
"#;
    
    let actions: Vec<Action> = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actions.len(), 1);
    
    match &actions[0] {
        Action::Let(let_action) => {
            assert_eq!(let_action.let_, "x = func");
            assert_eq!(let_action.on_failure, FailureMode::Continue); // Default
        }
        _ => panic!("Expected Let action"),
    }
}

#[test]
fn test_parse_shell_with_alias() {
    let yaml = r#"
- shell: git status
  alias: status
"#;
    
    let actions: Vec<Action> = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actions.len(), 1);
    
    match &actions[0] {
        Action::Shell(shell_action) => {
            assert_eq!(shell_action.shell, "git status");
            assert_eq!(shell_action.alias, Some("status".to_string()));
        }
        _ => panic!("Expected Shell action"),
    }
}

#[test]
fn test_parse_jj_with_alias() {
    let yaml = r#"
- jj: log -r @
  alias: current_log
"#;
    
    let actions: Vec<Action> = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actions.len(), 1);
    
    match &actions[0] {
        Action::Jj(jj_action) => {
            assert_eq!(jj_action.jj, "log -r @");
            assert_eq!(jj_action.alias, Some("current_log".to_string()));
        }
        _ => panic!("Expected Jj action"),
    }
}

#[test]
fn test_parse_log_with_alias() {
    let yaml = r#"
- log: "Test message"
  alias: log_result
"#;
    
    let actions: Vec<Action> = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actions.len(), 1);
    
    match &actions[0] {
        Action::Log(log_action) => {
            assert_eq!(log_action.log, "Test message");
            assert_eq!(log_action.alias, Some("log_result".to_string()));
        }
        _ => panic!("Expected Log action"),
    }
}

#[test]
fn test_parse_actions_without_alias() {
    let yaml = r#"
- shell: echo "test"
- jj: status
- log: "message"
"#;
    
    let actions: Vec<Action> = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actions.len(), 3);
    
    // Verify alias is None when not specified
    match &actions[0] {
        Action::Shell(shell_action) => {
            assert_eq!(shell_action.alias, None);
        }
        _ => panic!("Expected Shell action"),
    }
    
    match &actions[1] {
        Action::Jj(jj_action) => {
            assert_eq!(jj_action.alias, None);
        }
        _ => panic!("Expected Jj action"),
    }
    
    match &actions[2] {
        Action::Log(log_action) => {
            assert_eq!(log_action.alias, None);
        }
        _ => panic!("Expected Log action"),
    }
}
```

**Executor tests:**

Add to `cli/src/flows/executor.rs`:

```rust
#[test]
fn test_execute_let_function_call() {
    let action = LetAction {
        let_: "description = aiki/provenance.build_description".to_string(),
        on_failure: FailureMode::Fail,
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    // Functions read from context.event_vars automatically
    context.event_vars.insert("agent".to_string(), "ClaudeCode".to_string());
    context.event_vars.insert("session_id".to_string(), "test-session".to_string());
    context.event_vars.insert("tool_name".to_string(), "Edit".to_string());
    
    let result = FlowExecutor::execute_let(&action, &mut context).unwrap();
    
    assert!(result.success);
    assert!(context.event_vars.contains_key("description"));
    
    let description = context.event_vars.get("description").unwrap();
    assert!(description.contains("[aiki]"));
    assert!(description.contains("agent=claude-code"));
}

#[test]
fn test_execute_let_variable_aliasing() {
    let action = LetAction {
        let_: "new_name = $old_name".to_string(),
        on_failure: FailureMode::Continue,
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    context.event_vars.insert("old_name".to_string(), "test_value".to_string());
    
    let result = FlowExecutor::execute_let(&action, &mut context).unwrap();
    
    assert!(result.success);
    assert_eq!(context.event_vars.get("new_name"), Some(&"test_value".to_string()));
}

#[test]
fn test_execute_let_invalid_syntax() {
    let action = LetAction {
        let_: "invalid syntax without equals".to_string(),
        on_failure: FailureMode::Continue,
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    let result = FlowExecutor::execute_let(&action, &mut context);
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid let syntax"));
}

#[test]
fn test_execute_let_invalid_variable_names() {
    let test_cases = vec![
        ("123invalid = $foo", "starts with number"),
        ("kebab-case = $foo", "contains hyphen"),
        ("dot.name = $foo", "contains dot"),
        ("space name = $foo", "contains space"),
        ("$dollar = $foo", "starts with dollar"),
        ("= $foo", "empty variable name"),
    ];

    for (invalid_let, description) in test_cases {
        let action = LetAction {
            let_: invalid_let.to_string(),
            on_failure: FailureMode::Continue,
        };

        let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
        context.event_vars.insert("foo".to_string(), "value".to_string());
        
        let result = FlowExecutor::execute_let(&action, &mut context);
        
        assert!(
            result.is_err(),
            "Should reject variable name that {}: '{}'",
            description,
            invalid_let
        );
        assert!(
            result.unwrap_err().to_string().contains("Invalid variable name"),
            "Error should mention invalid variable name for case: {}",
            description
        );
    }
}

#[test]
fn test_execute_let_valid_variable_names() {
    let test_cases = vec![
        "simple",
        "camelCase",
        "snake_case",
        "PascalCase",
        "with123numbers",
        "_leading_underscore",
        "__double_underscore",
        "a",  // single char
        "_",  // single underscore
    ];

    for valid_name in test_cases {
        let action = LetAction {
            let_: format!("{} = $event.test", valid_name),
            on_failure: FailureMode::Continue,
        };

        let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
        context.event_vars.insert("test".to_string(), "value".to_string());
        
        let result = FlowExecutor::execute_let(&action, &mut context);
        
        assert!(
            result.is_ok(),
            "Should accept valid variable name '{}': {:?}",
            valid_name,
            result.err()
        );
        assert_eq!(
            context.event_vars.get(valid_name),
            Some(&"value".to_string()),
            "Variable '{}' should be set",
            valid_name
        );
    }
}

#[test]
fn test_execute_let_whitespace_trimming() {
    // Test that whitespace around = is handled correctly (YAML auto-formatting)
    let test_cases = vec![
        "result = $foo",           // normal
        "result=$foo",             // no spaces
        "result  =  $foo",         // multiple spaces
        "  result = $foo  ",       // leading/trailing (YAML would trim this)
        "result\t=\t$foo",         // tabs
    ];

    for let_expr in test_cases {
        let action = LetAction {
            let_: let_expr.to_string(),
            on_failure: FailureMode::Continue,
        };

        let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
        context.event_vars.insert("foo".to_string(), "test_value".to_string());
        
        let result = FlowExecutor::execute_let(&action, &mut context);
        
        assert!(
            result.is_ok(),
            "Should handle whitespace in '{}': {:?}",
            let_expr,
            result.err()
        );
        assert_eq!(
            context.event_vars.get("result"),
            Some(&"test_value".to_string()),
            "Should set variable correctly despite whitespace: '{}'",
            let_expr
        );
    }
}

#[test]
fn test_let_with_missing_context_vars() {
    // Function expects event.agent but it's not set
    let action = LetAction {
        let_: "description = aiki/provenance.build_description".to_string(),
        on_failure: FailureMode::Fail,
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    // Deliberately don't set required context vars
    
    let result = FlowExecutor::execute_let(&action, &mut context);
    
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    // Should get clear error message with function name
    assert!(err_msg.contains("aiki/provenance.build_description"));
    assert!(err_msg.contains("event.agent"));
}

#[test]
fn test_let_creates_copy_not_reference() {
    // Verify aliasing behavior creates copies
    let action1 = LetAction {
        let_: "original = $event.value".to_string(),
        on_failure: FailureMode::Continue,
    };
    
    let action2 = LetAction {
        let_: "copy = $original".to_string(),
        on_failure: FailureMode::Continue,
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    context.event_vars.insert("value".to_string(), "initial".to_string());
    
    // Create original variable
    FlowExecutor::execute_let(&action1, &mut context).unwrap();
    assert_eq!(context.event_vars.get("original"), Some(&"initial".to_string()));
    
    // Create copy
    FlowExecutor::execute_let(&action2, &mut context).unwrap();
    assert_eq!(context.event_vars.get("copy"), Some(&"initial".to_string()));
    
    // Modify original (simulate by directly updating context)
    context.event_vars.insert("original".to_string(), "modified".to_string());
    
    // Copy should still have original value (it's a copy, not a reference)
    assert_eq!(context.event_vars.get("copy"), Some(&"initial".to_string()));
    assert_eq!(context.event_vars.get("original"), Some(&"modified".to_string()));
}

#[test]
fn test_let_variable_shadowing() {
    // Verify that reassigning variables works correctly
    let action1 = LetAction {
        let_: "x = $event.first".to_string(),
        on_failure: FailureMode::Continue,
    };
    
    let action2 = LetAction {
        let_: "x = $event.second".to_string(),
        on_failure: FailureMode::Continue,
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    context.event_vars.insert("first".to_string(), "foo".to_string());
    context.event_vars.insert("second".to_string(), "bar".to_string());
    
    // First assignment
    FlowExecutor::execute_let(&action1, &mut context).unwrap();
    assert_eq!(context.event_vars.get("x"), Some(&"foo".to_string()));
    
    // Second assignment (overwrites)
    FlowExecutor::execute_let(&action2, &mut context).unwrap();
    assert_eq!(context.event_vars.get("x"), Some(&"bar".to_string()));
    
    // Should allow shadowing/overwriting
}

#[test]
fn test_shell_alias_stores_structured_metadata() {
    let action = ShellAction {
        shell: "echo 'test output'".to_string(),
        timeout: None,
        on_failure: FailureMode::Continue,
        alias: Some("result".to_string()),
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    let result = FlowExecutor::execute_shell(&action, &context).unwrap();
    
    // Manually call store_action_result (in actual executor this is automatic)
    FlowExecutor::store_action_result(&mut context, "result", &result);
    
    // Verify hybrid pattern: direct access
    assert!(context.event_vars.contains_key("result"));
    assert_eq!(context.event_vars.get("result").unwrap(), "test output");
    
    // Verify explicit .output
    assert_eq!(
        context.event_vars.get("result.output").unwrap(),
        "test output"
    );
    
    // Verify .exit_code
    assert_eq!(
        context.event_vars.get("result.exit_code").unwrap(),
        "0"
    );
    
    // Verify .failed
    assert_eq!(
        context.event_vars.get("result.failed").unwrap(),
        "false"
    );
}

#[test]
fn test_jj_alias_stores_structured_metadata() {
    let action = JjAction {
        jj: "log -r @ --no-graph".to_string(),
        timeout: None,
        on_failure: FailureMode::Continue,
        alias: Some("log_output".to_string()),
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    let result = FlowExecutor::execute_jj(&action, &context).unwrap();
    
    FlowExecutor::store_action_result(&mut context, "log_output", &result);
    
    // Verify all metadata is stored
    assert!(context.event_vars.contains_key("log_output"));
    assert!(context.event_vars.contains_key("log_output.output"));
    assert!(context.event_vars.contains_key("log_output.exit_code"));
    assert!(context.event_vars.contains_key("log_output.failed"));
}

#[test]
fn test_log_alias_stores_structured_metadata() {
    let action = LogAction {
        log: "Test message".to_string(),
        alias: Some("log_result".to_string()),
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    let result = FlowExecutor::execute_log(&action, &context).unwrap();
    
    FlowExecutor::store_action_result(&mut context, "log_result", &result);
    
    // Log action always succeeds
    assert_eq!(context.event_vars.get("log_result.failed").unwrap(), "false");
    assert_eq!(context.event_vars.get("log_result.exit_code").unwrap(), "0");
}

#[test]
fn test_actions_without_alias_dont_store_variables() {
    let shell_action = ShellAction {
        shell: "echo 'test'".to_string(),
        timeout: None,
        on_failure: FailureMode::Continue,
        alias: None, // No alias
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    let _result = FlowExecutor::execute_shell(&shell_action, &context).unwrap();
    
    // Variables should not be stored when alias is None
    assert!(!context.event_vars.contains_key("shell"));
    assert!(!context.event_vars.contains_key("result"));
}

#[test]
fn test_let_aliasing_copies_all_structured_metadata() {
    // Create a variable with structured metadata
    let create_action = LetAction {
        let_: "original = aiki/provenance.build_description".to_string(),
        on_failure: FailureMode::Continue,
    };

    let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
    context.event_vars.insert("agent".to_string(), "ClaudeCode".to_string());
    context.event_vars.insert("session_id".to_string(), "test-session".to_string());
    context.event_vars.insert("tool_name".to_string(), "Edit".to_string());
    
    FlowExecutor::execute_let(&create_action, &mut context).unwrap();
    
    // Verify original has structured metadata
    assert!(context.event_vars.contains_key("original"));
    assert!(context.event_vars.contains_key("original.output"));
    assert!(context.event_vars.contains_key("original.exit_code"));
    assert!(context.event_vars.contains_key("original.failed"));
    
    // Now alias it
    let alias_action = LetAction {
        let_: "copy = $original".to_string(),
        on_failure: FailureMode::Continue,
    };
    
    FlowExecutor::execute_let(&alias_action, &mut context).unwrap();
    
    // Verify copy also has all structured metadata
    assert!(context.event_vars.contains_key("copy"));
    assert!(context.event_vars.contains_key("copy.output"));
    assert!(context.event_vars.contains_key("copy.exit_code"));
    assert!(context.event_vars.contains_key("copy.failed"));
    
    // Verify values match
    assert_eq!(
        context.event_vars.get("copy"),
        context.event_vars.get("original")
    );
    assert_eq!(
        context.event_vars.get("copy.output"),
        context.event_vars.get("original.output")
    );
}
```

**Integration tests:**

Update existing tests in `cli/src/flows/bundled.rs`:

```rust
#[test]
fn test_provenance_flow_uses_let_syntax() {
    let flows = load_system_flows().unwrap();
    let provenance = &flows["aiki/provenance"];

    // Should have PostChange handler with let action
    assert!(!provenance.post_change.is_empty());
    
    // First action should be a Let binding with the correct namespaced function
    match &provenance.post_change[0] {
        Action::Let(let_action) => {
            // Verify exact syntax to prevent migration errors
            assert_eq!(
                let_action.let_,
                "description = aiki/provenance.build_description",
                "Flow must use full namespaced path 'aiki/provenance.build_description'"
            );
            assert_eq!(let_action.on_failure, FailureMode::Fail);
        }
        _ => panic!("Expected Let action as first step"),
    }
}
```

### 6. Migrating the Current `build_provenance_description` Function

The current implementation needs these specific changes:

**Current function signature:**
```rust
fn aiki_build_provenance_description(
    args: &HashMap<String, String>,
    context: &ExecutionContext,
) -> Result<ActionResult>
```

**Changes needed:**

1. **Rename function:** `aiki_build_provenance_description` → `fn_build_provenance_description`

2. **Remove args parameter:** Function now receives only `context`
   ```rust
   fn fn_build_provenance_description(
       context: &ExecutionContext,
   ) -> Result<ActionResult>
   ```

3. **Read from context instead of args:**
   ```rust
   // OLD: Read from args parameter
   let agent_str = args.get("agent")
       .ok_or_else(|| anyhow::anyhow!("Missing 'agent' argument"))?;
   
   // NEW: Read from context.event_vars
   let agent_str = context.event_vars.get("agent")
       .ok_or_else(|| anyhow::anyhow!("Missing event.agent"))?;
   ```

4. **Update all arg reads:**
   - `args.get("agent")` → `context.event_vars.get("agent")`
   - `args.get("session_id")` → `context.event_vars.get("session_id")`
   - `args.get("tool_name")` → `context.event_vars.get("tool_name")`

5. **Update function routing:** Change from bare function name to namespaced path
   ```rust
   // OLD: in execute_aiki()
   match action.aiki.as_str() {
       "build_provenance_description" => {
           Self::aiki_build_provenance_description(&resolved_args, context)
       }
       _ => anyhow::bail!("Unknown aiki function: {}", action.aiki),
   }
   
   // NEW: in execute_let()
   if function_path.starts_with("aiki/") {
       match function_path {
           "aiki/provenance.build_description" => {
               Self::fn_build_provenance_description(context)?
           }
           _ => anyhow::bail!("Unknown aiki function: {}", function_path),
       }
   }
   ```

### 7. Implementation Checklist

- [ ] Update `cli/src/flows/types.rs`
  - [ ] Add `LetAction` struct (no `args` field)
  - [ ] Update `Action` enum to include `Let(LetAction)`
  - [ ] Remove `AikiAction` struct
  - [ ] ~~Add `alias: Option<String>` field to `ShellAction`~~ (already exists)
  - [ ] ~~Add `alias: Option<String>` field to `JjAction`~~ (already exists)
  - [ ] ~~Add `alias: Option<String>` field to `LogAction`~~ (already exists)
- [ ] Update `cli/src/flows/executor.rs`
  - [ ] Add `execute_let()` function with namespace routing
  - [ ] Rename `aiki_build_provenance_description()` → `fn_build_provenance_description()`
  - [ ] Change signature: remove `args` parameter, keep only `context`
  - [ ] Update function body to read from `context.event_vars` instead of `args`
  - [ ] Update `execute_action()` match to handle `Let` instead of `Aiki`
  - [ ] Remove `execute_aiki()` function
  - [ ] **Update `execute_log()` to populate stdout:** Change from `ActionResult::success()` to return resolved message in stdout
  - [ ] **Add `store_action_result()` helper to implement hybrid pattern:**
    - [ ] Store direct access: `$var_name = stdout`
    - [ ] Store explicit: `$var_name.output = stdout`
    - [ ] Store metadata: `$var_name.exit_code`, `$var_name.failed`
  - [ ] **Update `execute_actions()` to store results for all action types:**
    - [ ] Call `store_action_result()` for Shell actions with `alias`
    - [ ] Call `store_action_result()` for Jj actions with `alias`
    - [ ] Call `store_action_result()` for Log actions with `alias`
    - [ ] Let actions already store in `execute_let()`
  - [ ] **Update `execute_let()` aliasing branch to copy structured metadata:**
    - [ ] Copy `.output` sibling when aliasing
    - [ ] Copy `.exit_code` sibling when aliasing
    - [ ] Copy `.failed` sibling when aliasing
- [ ] Update `cli/flows/provenance.yaml`
  - [ ] Change `aiki: build_provenance_description` to `let: description = aiki/provenance.build_description`
  - [ ] Remove `args:` block
  - [ ] Update variable reference from `$build_provenance_description.output` to `$description`
- [ ] Add tests
  - [ ] Parser tests for `let:` syntax
  - [ ] Parser tests for `alias` field on Shell/Jj/Log actions
  - [ ] Executor tests for function calls with context
  - [ ] Executor tests for variable aliasing (including metadata copy)
  - [ ] Executor tests for namespace validation
  - [ ] Executor tests for Shell/Jj/Log with `alias` field
  - [ ] Update integration tests
- [ ] Update documentation
  - [ ] `ops/phase-5.md` - Add `let:` action section with namespace requirements
  - [ ] `ops/PHASE_5.1_NATIVE_FUNCTIONS.md` - Update all examples to use `let:` and namespaces
  - [ ] `ops/PHASE_5.1_COMPLETE.md` - Update implementation notes
- [ ] Run all tests: `cargo test`
- [ ] Manual testing with provenance flow
- [ ] Commit: "Migrate from aiki: to let: syntax with namespaced functions"

## Breaking Changes

### What Breaks

- **Old `aiki:` syntax no longer works**
  ```yaml
  # ❌ This will fail
  - aiki: build_provenance_description
  ```

- **Variable references change**
  ```yaml
  # ❌ Old
  - jj: describe -m "$build_provenance_description.output"
  
  # ✅ New
  - jj: describe -m "$description"
  ```

### Migration Guide for Users

**Before (Phase 5.0):**
```yaml
PostChange:
  - aiki: some_function
    args:
      key: value
  - shell: echo "$some_function.output"
```

**After (Phase 5.1):**
```yaml
PostChange:
  - let: result = aiki/namespace.some_function
  - shell: echo "$result"
```

**Migration steps:**
1. Replace `aiki:` with `let: var_name =`
2. Add namespace prefix (`aiki/` for built-in functions)
3. Remove `args:` block (functions read from context)
4. Choose meaningful variable names
5. Update variable references (remove `.output` suffix)

### Justification for Breaking Change

This is acceptable because:
- ✅ Project is pre-1.0 (Phase 5.1)
- ✅ Only affects system flows (bundled in binary, no external users yet)
- ✅ Significantly improves UX and readability
- ✅ Future-proof for WASM/native functions (Phase 8)
- ✅ Aligns with common programming patterns (`let` bindings)

## Future Compatibility

This `let:` syntax is designed to work seamlessly with external functions in Phase 8:

### Phase 8: External WASM Functions

The same `let:` syntax will work for external WASM functions:

```yaml
# Built-in Aiki function (Phase 5.1)
- let: description = aiki/provenance.build_description

# External WASM function (Phase 8)
- let: complexity = vendor/analyzer.analyze_complexity
```

**Key points:**
- Same syntax for both built-in and external functions
- All functions receive full `$event` context automatically
- Namespace determines implementation (`aiki/` = built-in, `vendor/` = external)
- Aiki handles routing transparently

See [`ops/ROADMAP.md` Phase 8](ROADMAP.md#phase-8-external-flow-ecosystem) for complete WASM implementation details including:
- WASM function authoring
- Optional native compilation
- Performance characteristics
- Distribution and installation

## Success Criteria

- ✅ All existing tests pass
- ✅ Provenance flow works with new `let:` syntax
- ✅ Variable aliasing works (`let: x = $y`)
- ✅ Function calls work (`let: x = function`)
- ✅ Error handling works (`on_failure: fail`)
- ✅ Documentation updated with clear examples
- ✅ No references to old `aiki:` syntax remain in codebase

## Estimated Effort

- **Code changes:** 2-3 hours
  - Update types and executor: 1 hour
  - Update provenance.yaml: 15 minutes
  - Add tests: 1 hour
- **Testing:** 1 hour
  - Run test suite
  - Manual testing
  - Edge case validation
- **Documentation:** 1 hour
  - Update phase-5.md
  - Update PHASE_5.1 docs
  - Add migration guide
  
**Total: 4-5 hours**

## Related Documentation

- [`ops/phase-5.md`](phase-5.md) - Flow engine design
- [`ops/PHASE_5.1_NATIVE_FUNCTIONS.md`](PHASE_5.1_NATIVE_FUNCTIONS.md) - Built-in functions architecture
- [`ops/PHASE_5.1_COMPLETE.md`](PHASE_5.1_COMPLETE.md) - Implementation completion notes
- [`ops/ROADMAP.md`](ROADMAP.md) - Phase 8 WASM functions

## Notes

- This migration sets up the foundation for Phase 8 (WASM/native functions)
- The `let:` syntax is intentionally simple to keep parsing straightforward
- Future enhancement: Consider supporting inline function calls without `args:` block
  ```yaml
  # Future syntax idea
  - let: desc = build_provenance(agent=$event.agent, session=$event.session_id)
  ```
- Variable aliasing enables powerful flow composition patterns
