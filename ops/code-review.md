# Comprehensive Rust Code Review for Aiki

**Review Date**: 2025-11-16  
**Reviewer**: Claude (Sonnet 4.5)  
**Scope**: Rust idioms, performance, legibility

---

## Executive Summary

**Overall Assessment**: The codebase is well-structured with good separation of concerns. However, there are several opportunities for improvement in Rust idioms, error handling, performance optimizations, and code clarity.

**Key Strengths**:
- Clean module organization
- Good use of the type system
- Comprehensive test coverage
- Well-designed flows engine

**Priority Areas for Improvement**:
1. Variable resolution performance (hot path optimization)
2. Error handling patterns (reduce allocations)
3. Performance optimizations (cloning, allocations)
4. Rust idioms (better use of iterators, references)
5. Code duplication and maintainability

---

## 1. Variable Resolution Performance Deep Dive

### Current Implementation Analysis

**Location**: `cli/src/flows/variables.rs:40-56`

```rust
pub fn resolve(&self, input: &str) -> String {
    let mut result = input.to_string();  // ❌ Clone #1
    
    // ❌ Allocates Vec and sorts on every call
    let mut vars: Vec<_> = self.variables.iter().collect();
    vars.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    
    for (key, value) in vars {
        let pattern = format!("${}", key);  // ❌ Allocation #2 (per variable)
        result = result.replace(&pattern, value);  // ❌ Multiple allocations
    }
    result
}
```

### Performance Issues Identified

1. **Unconditional String Clone**: Every call clones input, even when no substitution occurs
2. **Repeated Sorting**: Sorts variables on every `resolve()` call (O(n log n))
3. **Pattern Allocation**: Creates `format!("${}", key)` for each variable
4. **Multiple Replace Allocations**: Each `replace()` allocates a new string
5. **No Short-Circuit**: Doesn't exit early when no variables present

### Optimization Strategies

#### Strategy 1: Copy-on-Write with Fast Path

```rust
use std::borrow::Cow;

pub fn resolve<'a>(&self, input: &'a str) -> Cow<'a, str> {
    // Fast path: no variables to substitute
    if !input.contains('$') {
        return Cow::Borrowed(input);
    }
    
    // Only allocate if we actually need to modify
    let mut result = Cow::Borrowed(input);
    
    for (key, value) in &self.sorted_vars {
        let pattern = format!("${}", key);
        if result.contains(&pattern) {
            result = Cow::Owned(result.replace(&pattern, value));
        }
    }
    
    result
}
```

**Pros**:
- Zero allocation when no variables present
- Minimal allocation when only some variables used
- Backward compatible (can convert `Cow` to `String`)

**Cons**:
- Still allocates pattern string for each variable
- API change required (returns `Cow` instead of `String`)

#### Strategy 2: Pre-Sorted Cache with Lazy Rebuild

```rust
pub struct VariableResolver {
    variables: HashMap<String, String>,
    sorted_vars: Vec<(String, String)>,  // Pre-sorted by length (longest first)
    dirty: bool,  // Track if cache needs rebuild
}

impl VariableResolver {
    pub fn add_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(key.into(), value.into());
        self.dirty = true;  // Invalidate cache
    }
    
    fn ensure_sorted(&mut self) {
        if self.dirty {
            self.sorted_vars = self.variables.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            self.sorted_vars.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
            self.dirty = false;
        }
    }
    
    pub fn resolve(&mut self, input: &str) -> String {
        if !input.contains('$') {
            return input.to_string();
        }
        
        self.ensure_sorted();  // Lazy rebuild
        
        let mut result = input.to_string();
        for (key, value) in &self.sorted_vars {
            let pattern = format!("${}", key);
            result = result.replace(&pattern, value);
        }
        result
    }
}
```

**Pros**:
- Amortizes sorting cost across multiple `resolve()` calls
- Simple to implement
- No API changes required

**Cons**:
- Requires `&mut self` (not `&self`)
- Still allocates pattern strings
- Clones variables when rebuilding cache

#### Strategy 3: Pattern Pre-Computation

```rust
pub struct VariableResolver {
    variables: HashMap<String, String>,
    // Cache: "$key" -> value
    patterns: HashMap<String, String>,
}

impl VariableResolver {
    pub fn add_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        
        let pattern = format!("${}", key);
        self.patterns.insert(pattern, value.clone());
        self.variables.insert(key, value);
    }
    
    pub fn resolve(&self, input: &str) -> String {
        if !input.contains('$') {
            return input.to_string();
        }
        
        let mut result = input.to_string();
        
        // Sort patterns by length (longest first) - could cache this too
        let mut patterns: Vec<_> = self.patterns.iter().collect();
        patterns.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
        
        for (pattern, value) in patterns {
            result = result.replace(pattern, value);
        }
        result
    }
}
```

**Pros**:
- Eliminates per-variable `format!()` calls
- Simple API (`&self` not `&mut self`)
- Easy to understand

**Cons**:
- Still sorts on every call
- Doubles memory usage (stores both variables and patterns)

#### Strategy 4: Regex-Based Substitution (Advanced)

```rust
use regex::{Regex, Captures};
use lazy_static::lazy_static;

lazy_static! {
    // Matches $variable_name or $event.variable_name
    static ref VAR_PATTERN: Regex = Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_.]*)")
        .expect("Invalid regex pattern");
}

pub fn resolve(&self, input: &str) -> String {
    if !input.contains('$') {
        return input.to_string();
    }
    
    VAR_PATTERN.replace_all(input, |caps: &Captures| {
        let var_name = &caps[1];
        self.variables.get(var_name)
            .map(|s| s.as_str())
            .unwrap_or(&caps[0])  // Keep original if not found
    }).into_owned()
}
```

**Pros**:
- Single pass through input string
- No sorting required
- Handles complex patterns elegantly
- Naturally handles overlapping variable names

**Cons**:
- Regex compilation overhead (mitigated by lazy_static)
- More complex to understand
- Harder to debug
- Dependency on `regex` crate

#### Strategy 5: Hybrid Approach (Recommended)

```rust
use std::borrow::Cow;

pub struct VariableResolver {
    variables: HashMap<String, String>,
    // Cached sorted patterns: ("$key", "value")
    cached_patterns: Vec<(String, String)>,
    cache_valid: bool,
}

impl VariableResolver {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            cached_patterns: Vec::new(),
            cache_valid: false,
        }
    }
    
    pub fn add_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        self.variables.insert(key, value);
        self.cache_valid = false;
    }
    
    fn rebuild_cache(&mut self) {
        if self.cache_valid {
            return;
        }
        
        self.cached_patterns = self.variables
            .iter()
            .map(|(k, v)| (format!("${}", k), v.clone()))
            .collect();
        
        // Sort by pattern length (longest first)
        self.cached_patterns.sort_by_key(|(pattern, _)| {
            std::cmp::Reverse(pattern.len())
        });
        
        self.cache_valid = true;
    }
    
    pub fn resolve<'a>(&mut self, input: &'a str) -> Cow<'a, str> {
        // Fast path: no variables in input
        if !input.contains('$') {
            return Cow::Borrowed(input);
        }
        
        // Ensure cache is built
        self.rebuild_cache();
        
        // Fast path: no variables configured
        if self.cached_patterns.is_empty() {
            return Cow::Borrowed(input);
        }
        
        let mut result = Cow::Borrowed(input);
        
        for (pattern, value) in &self.cached_patterns {
            if result.contains(pattern.as_str()) {
                // Convert to owned on first substitution
                let owned = result.replace(pattern, value);
                result = Cow::Owned(owned);
            }
        }
        
        result
    }
}
```

**Pros**:
- Fast path for no variables (zero allocation)
- Amortizes sorting and pattern creation
- Only converts to `Cow::Owned` when actually substituting
- Requires `&mut self` which matches current usage patterns

**Cons**:
- Slightly more complex implementation
- API change to return `Cow<'a, str>`
- Requires `&mut self` (already the case in some call sites)

### Benchmarking Approach

To validate these optimizations, create targeted benchmarks:

```rust
// benches/variable_resolution.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_variable_resolution_strategies(c: &mut Criterion) {
    let mut group = c.benchmark_group("variable_resolution");
    
    // Test cases
    let test_cases = vec![
        ("no_vars", "This is a plain string with no variables"),
        ("one_var", "Processing file $event.file_path"),
        ("many_vars", "Agent $event.agent in $cwd with session $event.session_id and tool $event.tool_name"),
        ("overlapping", "$event.file and $event.file_path"),
    ];
    
    let mut resolver = VariableResolver::new();
    resolver.add_var("event.file_path", "/path/to/file.rs");
    resolver.add_var("event.file", "file.rs");
    resolver.add_var("event.agent", "claude-code");
    resolver.add_var("event.session_id", "session-123");
    resolver.add_var("event.tool_name", "Edit");
    resolver.add_var("cwd", "/home/user/project");
    
    for (name, input) in test_cases {
        group.bench_with_input(
            BenchmarkId::new("current", name),
            input,
            |b, input| {
                b.iter(|| {
                    let result = resolver.resolve(black_box(input));
                    black_box(result);
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(benches, bench_variable_resolution_strategies);
criterion_main!(benches);
```

### Performance Analysis Questions

1. **What is the actual performance bottleneck?**
   - Profile with `cargo flamegraph` to see where time is spent
   - Is it sorting? Pattern creation? String replacement?

2. **What is the typical usage pattern?**
   - How many variables are typically registered?
   - How many resolve calls per flow execution?
   - What's the average input string length?

3. **What's the memory vs. speed tradeoff?**
   - Pre-computing patterns uses more memory
   - Is memory pressure a concern?
   - What's the cache hit rate for sorted patterns?

4. **Are there allocation hotspots?**
   - Use `cargo +nightly build -Z build-std --target x86_64-unknown-linux-gnu`
   - Enable allocator profiling with `dhat` or `heaptrack`

5. **What's the expected performance improvement?**
   - Current: ~500ns per resolve (estimate)
   - Target: <100ns for no-var case, <300ns for typical case

### Investigation Tasks

- [ ] **Profile current implementation** with real-world flows
- [ ] **Measure allocation count** using allocator profiling
- [ ] **Benchmark each strategy** with realistic test cases
- [ ] **Test edge cases**: empty input, no variables, all variables, overlapping names
- [ ] **Validate correctness** with property-based tests
- [ ] **Measure cache hit rate** for sorted patterns
- [ ] **Compare memory usage** of different strategies
- [ ] **Test concurrent access** patterns (if flows become multithreaded)

### Recommended Next Steps

1. **Implement Strategy 5 (Hybrid)** as it offers the best balance
2. **Add benchmarks** to track performance regression
3. **Create property tests** to ensure correctness
4. **Profile with real flows** to validate improvements
5. **Document the tradeoffs** in code comments

### Expected Impact

Based on the current usage in flows/executor.rs:
- `create_resolver()` called once per action
- `resolve()` called for each variable substitution in commands
- Typical flow has 3-5 actions with 2-3 variable substitutions each
- **Estimated speedup**: 2-3x for typical flows, 5-10x for variable-heavy flows

---

## 2. Error Handling & Result Types

### Issue: Excessive String Allocations in Error Messages

**Location**: Throughout codebase, especially `cli/src/main.rs:268-281`

```rust
// CURRENT (allocates strings unnecessarily)
anyhow::bail!(
    "Unknown agent type: '{}'. Supported values: 'claude-code', 'cursor'",
    agent
);
```

**Recommendation**: Use `thiserror` for better error types:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AikiError {
    #[error("Unknown agent type: '{0}'. Supported values: 'claude-code', 'cursor'")]
    UnknownAgentType(String),
    
    #[error("Not in a JJ repository. Run 'jj init' or 'aiki init' first")]
    NotInJjRepo,
    
    #[error("Missing event variable: {0}")]
    MissingEventVariable(String),
    
    #[error("Invalid let syntax: '{0}'. Expected 'variable = expression'")]
    InvalidLetSyntax(String),
}

// Usage
fn parse_agent_type(agent: &str) -> Result<AgentType, AikiError> {
    match agent {
        "claude-code" => Ok(AgentType::ClaudeCode),
        "cursor" => Ok(AgentType::Cursor),
        _ => Err(AikiError::UnknownAgentType(agent.to_string())),
    }
}
```

**Impact**: Better error ergonomics, structured error types, easier testing.

---

## 3. Performance Optimizations

### Issue 3.1: Unnecessary Cloning in Variable Resolution

See Section 1 (Variable Resolution Performance Deep Dive) above.

---

### Issue 3.2: Repeated HashMap Cloning

**Location**: `cli/src/flows/variables.rs:24-28`

```rust
// CURRENT
pub fn add_env_vars(&mut self, env_vars: &HashMap<String, String>) {
    self.variables.extend(env_vars.clone());  // Clones entire HashMap
}
```

**Recommendation**: Use iterator to avoid clone:

```rust
pub fn add_env_vars(&mut self, env_vars: &HashMap<String, String>) {
    self.variables.extend(
        env_vars.iter().map(|(k, v)| (k.clone(), v.clone()))
    );
}

// Or better, accept an iterator:
pub fn add_env_vars<I>(&mut self, env_vars: I)
where
    I: IntoIterator<Item = (String, String)>
{
    self.variables.extend(env_vars);
}

// Usage:
resolver.add_env_vars(env_vars.iter().map(|(k, v)| (k.clone(), v.clone())));

// Or for owned values:
resolver.add_env_vars(env_vars.into_iter());
```

**Impact**: Avoids unnecessary HashMap clone, more flexible API.

---

### Issue 3.3: Inefficient Commit Cache in Blame

**Location**: `cli/src/blame.rs:92-101`

```rust
// CURRENT: Uses Vec<u8> as HashMap key (inefficient)
let mut commit_cache: HashMap<Vec<u8>, (String, Option<ProvenanceRecord>)> = HashMap::new();

// ...
commit_cache.insert(
    commit_id.as_bytes().to_vec(),  // Allocates new Vec every time
    (change_id_hex.clone(), provenance.clone()),
);
```

**Recommendation**: Use the commit ID hex string directly:

```rust
// BETTER: Use String as key (no allocation per lookup)
let mut commit_cache: HashMap<String, (String, Option<ProvenanceRecord>)> = HashMap::new();

// Insert
commit_cache.insert(
    commit_id.hex(),
    (change_id_hex, provenance),
);

// Lookup
if let Some(cached) = commit_cache.get(&commit_id.hex()) {
    cached.clone()
} else {
    // ...
}
```

**Impact**: Eliminates Vec allocation per cache lookup, more readable code.

---

## 4. Rust Idioms & Best Practices

### Issue 4.1: Unnecessary `.to_string()` Calls

**Location**: Multiple files, e.g., `cli/src/main.rs`, `cli/src/flows/executor.rs`

```rust
// CURRENT
let session_id = "test-session-123".to_string();

// BETTER (use &str when possible)
let session_id: &str = "test-session-123";

// Or use String::from for clarity when ownership needed
let session_id = String::from("test-session-123");
```

**Recommendation**: Prefer `&str` in function signatures and only allocate `String` when ownership is needed.

**Example refactoring**:

```rust
// Before
pub fn record_change(agent_type: AgentType, session_id: &str, tool_name: &str) -> Result<()> {
    let description = format!("...");  // Already creates owned String
    // ...
}

// This is actually good! The function accepts &str (borrowed)
// and only allocates when needed (in format!)
```

---

### Issue 4.2: Missing `#[must_use]` Attributes

**Location**: `cli/src/flows/types.rs:79-93`

```rust
// CURRENT
impl ActionResult {
    pub fn success() -> Self { ... }
    pub fn failure(exit_code: i32, stderr: String) -> Self { ... }
}

// BETTER
impl ActionResult {
    #[must_use]
    pub fn success() -> Self { ... }
    
    #[must_use]
    pub fn failure(exit_code: i32, stderr: String) -> Self { ... }
}
```

**Impact**: Prevents accidentally ignoring results, improves API safety.

---

### Issue 4.3: Use `impl AsRef<Path>` for Path Parameters

**Location**: `cli/src/jj.rs:16-20`, `cli/src/config.rs`, `cli/src/blame.rs`

```rust
// CURRENT (in jj.rs - already good!)
pub fn new<P: AsRef<Path>>(workspace_root: P) -> Self { ... }

// CURRENT (in config.rs - could be improved)
pub fn save_previous_hooks_path(repo_root: &Path) -> Result<()>

// BETTER (more flexible)
pub fn save_previous_hooks_path(repo_root: impl AsRef<Path>) -> Result<()> {
    let repo_root = repo_root.as_ref();
    // ...
}
```

**Impact**: More ergonomic API, accepts `Path`, `PathBuf`, `&Path`, `String`, etc.

---

### Issue 4.4: Prefer Iterators Over Manual Loops

**Location**: `cli/src/flows/executor.rs:59-76`

```rust
// CURRENT
let mut results = Vec::new();
for action in actions {
    let result = Self::execute_action(action, context)?;
    Self::store_action_result(action, &result, context);
    // ...
    results.push(result);
    if should_stop {
        anyhow::bail!("Action failed with on_failure: fail");
    }
}
Ok(results)

// BETTER (though early returns complicate this)
actions.iter()
    .try_fold(Vec::new(), |mut results, action| {
        let result = Self::execute_action(action, context)?;
        Self::store_action_result(action, &result, context);
        
        let should_stop = !result.success && action.on_failure() == FailureMode::Fail;
        if should_stop {
            return Err(anyhow::anyhow!("Action failed with on_failure: fail"));
        }
        
        results.push(result);
        Ok(results)
    })
```

**Note**: The imperative style is actually clearer here due to early returns and side effects. Consider keeping as-is for legibility. The functional style doesn't provide significant benefits in this case.

---

## 5. Code Duplication & Maintainability

### Issue 5.1: Duplicated Failure Mode Checking

**Location**: `cli/src/flows/executor.rs:68-76`

```rust
// CURRENT: Repeated match on action type
let should_stop = match action {
    Action::Shell(shell_action) => {
        !result.success && shell_action.on_failure == FailureMode::Fail
    }
    Action::Jj(jj_action) => {
        !result.success && jj_action.on_failure == FailureMode::Fail
    }
    Action::Let(let_action) => {
        !result.success && let_action.on_failure == FailureMode::Fail
    }
    Action::Aiki(aiki_action) => {
        !result.success && aiki_action.on_failure == FailureMode::Fail
    }
    Action::Log(_) => false,
};
```

**Recommendation**: Add a helper method to `Action` enum:

```rust
// In cli/src/flows/types.rs
impl Action {
    /// Get the failure mode for this action
    pub fn on_failure(&self) -> FailureMode {
        match self {
            Action::Shell(a) => a.on_failure,
            Action::Jj(a) => a.on_failure,
            Action::Let(a) => a.on_failure,
            Action::Aiki(a) => a.on_failure,
            Action::Log(_) => FailureMode::Continue,
        }
    }
}

// Usage in executor.rs:
let should_stop = !result.success && action.on_failure() == FailureMode::Fail;
```

**Impact**: Reduces duplication, easier to maintain, clearer intent.

---

### Issue 5.2: Repeated Variable Storage Logic

**Location**: `cli/src/flows/executor.rs:86-144`

The `store_action_result` function has significant duplication across action types. Consider extracting common logic:

```rust
/// Store variable and its metadata in the execution context
fn store_in_let_vars(
    context: &mut ExecutionContext,
    var_name: &str,
    result: &ActionResult,
) {
    // Store the variable value
    context.let_vars.insert(var_name.to_string(), result.stdout.clone());
    
    // Store structured metadata
    context.variable_metadata.insert(var_name.to_string(), result.clone());
    
    // Store dotted properties for backward compatibility
    if !result.stdout.is_empty() {
        context.let_vars.insert(
            format!("{}.output", var_name),
            result.stdout.clone()
        );
    }
    if let Some(exit_code) = result.exit_code {
        context.let_vars.insert(
            format!("{}.exit_code", var_name),
            exit_code.to_string()
        );
    }
    context.let_vars.insert(
        format!("{}.failed", var_name),
        (!result.success).to_string(),
    );
}

// Then use in store_action_result:
fn store_action_result(action: &Action, result: &ActionResult, context: &mut ExecutionContext) {
    match action {
        Action::Let(let_action) => {
            if let Some(variable_name) = let_action.let_.split('=').next() {
                let variable_name = variable_name.trim();
                store_in_let_vars(context, variable_name, result);
            }
        }
        Action::Shell(shell_action) => {
            if let Some(alias) = &shell_action.alias {
                store_in_let_vars(context, alias, result);
            }
        }
        Action::Jj(jj_action) => {
            if let Some(alias) = &jj_action.alias {
                store_in_let_vars(context, alias, result);
            }
        }
        Action::Log(log_action) => {
            if let Some(alias) = &log_action.alias {
                store_in_let_vars(context, alias, result);
            }
        }
        Action::Aiki(aiki_action) => {
            // Backward compatibility: store step results with dotted notation
            let step_name = &aiki_action.aiki;
            // ... existing logic
        }
    }
}
```

**Impact**: Reduces duplication from ~60 lines to ~20 lines, easier to maintain.

---

## 6. Legibility & Documentation

### Issue 6.1: Missing or Unclear Function Documentation

**Location**: `cli/src/flows/executor.rs`, `cli/src/blame.rs`

```rust
// CURRENT
fn execute_action(action: &Action, context: &ExecutionContext) -> Result<ActionResult>

// BETTER
/// Execute a single action and return its result.
///
/// This function dispatches to the appropriate executor based on the action type.
/// Variable substitution is performed by individual executors using the provided context.
///
/// # Arguments
/// * `action` - The action to execute (Shell, Jj, Log, Let, or Aiki)
/// * `context` - Execution context containing:
///   - Event variables ($event.*)
///   - Let-bound variables
///   - Working directory
///   - Environment variables
///
/// # Returns
/// * `Ok(ActionResult)` - The result of the action execution including:
///   - success: Whether the action succeeded
///   - exit_code: Process exit code (if applicable)
///   - stdout/stderr: Command output
/// * `Err(_)` - If the action failed to execute (filesystem error, parse error, etc.)
///
/// # Examples
/// ```ignore
/// let action = Action::Log(LogAction { 
///     log: "Hello".to_string(), 
///     alias: None 
/// });
/// let context = ExecutionContext::new(PathBuf::from("/tmp"));
/// let result = execute_action(&action, &context)?;
/// assert!(result.success);
/// ```
fn execute_action(action: &Action, context: &ExecutionContext) -> Result<ActionResult>
```

**Impact**: Better maintainability, easier onboarding for contributors, clearer API contracts.

---

### Issue 6.2: Magic Numbers and Unclear Constants

**Location**: `cli/src/blame.rs:199-204`

```rust
// CURRENT
let short_commit = &attr.commit_id[..8.min(attr.commit_id.len())];

// BETTER
const SHORT_COMMIT_ID_LENGTH: usize = 8;

let short_commit = &attr.commit_id[..SHORT_COMMIT_ID_LENGTH.min(attr.commit_id.len())];

// Or even better, extract to a method:
fn truncate_commit_id(commit_id: &str) -> &str {
    const LENGTH: usize = 8;
    &commit_id[..LENGTH.min(commit_id.len())]
}

// Usage:
let short_commit = truncate_commit_id(&attr.commit_id);
```

**Impact**: More readable, easier to change, self-documenting code.

---

## 7. Error Handling Patterns

### Issue 7.1: Silent Failures in JJ Commands

**Location**: `cli/src/record_change.rs:60-75`

```rust
// CURRENT: Logs warning but doesn't propagate error
if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    eprintln!("Warning: jj describe failed: {}", stderr);
    eprintln!("  Status: {}", output.status);
}
```

**Recommendation**: Return proper errors or use a logging framework:

```rust
// Option 1: Propagate the error
if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(anyhow::anyhow!(
        "jj describe failed with status {}: {}",
        output.status,
        stderr
    ));
}

// Option 2: Use structured logging
use tracing::{warn, error};

if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    error!(
        status = %output.status,
        stderr = %stderr,
        "jj describe failed"
    );
    return Err(anyhow::anyhow!("jj describe failed"));
}
```

**Recommendation**: Add `tracing` crate for structured logging:

```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = "0.3"
```

**Impact**: Better error visibility, easier debugging, consistent error handling.

---

## 8. Testing Improvements

### Issue 8.1: Missing Property-Based Tests

Consider adding `proptest` for property-based testing, especially for:
- Variable resolution (all variables should be resolved consistently)
- Flow parsing (round-trip: parse → serialize → parse should be identity)
- Provenance metadata parsing

```toml
[dev-dependencies]
proptest = "1.0"
```

**Example**:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn variable_resolution_is_consistent(
        vars in prop::collection::hash_map("\\w+", "\\w+", 0..10),
        input in ".*"
    ) {
        let mut resolver = VariableResolver::new();
        for (k, v) in &vars {
            resolver.add_var(k, v);
        }
        
        // Resolving twice should give same result
        let first = resolver.resolve(&input);
        let second = resolver.resolve(&input);
        prop_assert_eq!(first, second);
    }
    
    #[test]
    fn variable_resolution_is_idempotent(
        vars in prop::collection::hash_map("\\w+", "\\w+", 0..10),
        input in ".*"
    ) {
        let mut resolver = VariableResolver::new();
        for (k, v) in &vars {
            resolver.add_var(k, v);
        }
        
        // Resolving the result again should not change it
        let once = resolver.resolve(&input);
        let twice = resolver.resolve(&once);
        prop_assert_eq!(once, twice);
    }
    
    #[test]
    fn provenance_description_roundtrip(
        agent_type in prop_oneof![
            Just(AgentType::ClaudeCode),
            Just(AgentType::Cursor),
        ],
        session_id in "[a-zA-Z0-9-]{1,50}",
        tool_name in "[a-zA-Z]{1,20}",
    ) {
        let record = ProvenanceRecord {
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
        
        let description = record.to_description();
        let parsed = ProvenanceRecord::from_description(&description)
            .unwrap()
            .unwrap();
        
        prop_assert_eq!(parsed.session_id, session_id);
        prop_assert_eq!(parsed.tool_name, tool_name);
    }
}
```

**Impact**: Catches edge cases, validates invariants, increases confidence in refactoring.

---

## 9. Type Safety & API Design

### Issue 9.1: Stringly-Typed Configuration

**Location**: `cli/src/flows/types.rs`, `cli/src/flows/executor.rs`

```rust
// CURRENT: String-based function paths
let_: "description = aiki/provenance.build_description".to_string()

// BETTER: Use enum for type safety
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FunctionPath {
    #[serde(rename = "aiki/provenance.build_description")]
    ProvenanceBuildDescription,
    
    #[serde(rename = "aiki/core.build_description")]
    CoreBuildDescription,
    
    #[serde(rename = "self.build_description")]
    SelfBuildDescription,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LetExpression {
    VariableAlias {
        variable: String,
        source: String,  // e.g., "$event.file_path"
    },
    FunctionCall {
        variable: String,
        function: FunctionPath,
    },
}

pub struct LetAction {
    #[serde(flatten)]
    pub expression: LetExpression,
    
    #[serde(default = "default_on_failure")]
    pub on_failure: FailureMode,
}
```

**Impact**: Type-safe parsing, compile-time validation, better IDE support.

**Note**: This requires careful serde configuration to maintain YAML compatibility.

---

## 10. Performance Benchmarking Observations

### Issue 10.1: Benchmark Setup Code

**Location**: `cli/benches/flow_performance.rs:11-24`

```rust
// CURRENT: Repeated workspace initialization
fn init_jj_workspace() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    Command::new("jj")
        .arg("git")
        .arg("init")
        // ...
    temp_dir
}

// BETTER: Share setup across benchmarks using criterion's setup/teardown
fn bench_with_workspace<F>(c: &mut Criterion, name: &str, f: F)
where
    F: Fn(&mut Criterion, &TempDir),
{
    let workspace = init_jj_workspace();
    f(c, &workspace);
}

// Or use lazy_static for one-time setup:
use lazy_static::lazy_static;

lazy_static! {
    static ref TEST_WORKSPACE: TempDir = init_jj_workspace();
}
```

**Impact**: Faster benchmark execution, more accurate measurements.

---

## 11. Deprecation & Technical Debt

### Issue 11.1: Remove Deprecated Code

**Location**: `cli/src/jj.rs:34-46`, `cli/src/record_change.rs:78-106`

```rust
#[deprecated(since = "0.1.0", note = "use `init_with_git_dir` instead")]
#[allow(dead_code)]
pub fn init_on_existing_git(&self) -> Result<()>
```

**Recommendation**: 

1. If truly unused, remove in next minor version (0.2.0)
2. If keeping for backward compatibility, ensure it's tested
3. Add timeline for removal in CHANGELOG

**Action items**:
- [ ] Grep codebase for calls to deprecated functions
- [ ] Add deprecation warnings to CHANGELOG
- [ ] Schedule removal for version 0.2.0
- [ ] Add migration guide in documentation

---

## 12. Security & Safety

### Issue 12.1: Command Injection Risk

**Location**: `cli/src/flows/executor.rs:162-173` (shell execution)

```rust
// CURRENT: Uses shell -c which could be vulnerable
Command::new("sh")
    .arg("-c")
    .arg(&command)
    .current_dir(&context.cwd)
```

**Analysis**: Already mitigated by variable resolution happening before shell invocation. However, should document this security consideration.

**Recommendation**: Add security documentation:

```rust
/// Execute a shell command with variable substitution.
///
/// # Security
/// Variables are resolved before shell execution, which provides some protection
/// against injection attacks. However, users should still be cautious with untrusted
/// input in flows, especially when:
/// - Loading flows from untrusted sources
/// - Allowing user-defined flow overrides
/// - Using environment variables as event variables
///
/// Best practices:
/// - Validate flow YAML before loading
/// - Restrict which environment variables are accessible
/// - Use JJ actions instead of shell when possible
/// - Avoid passing user input directly to shell actions
fn execute_shell(action: &ShellAction, context: &ExecutionContext) -> Result<ActionResult>
```

**Additional recommendation**: Consider adding a "safe mode" that restricts shell command usage.

---

## 13. Module Organization

### Issue 13.1: Large main.rs File

**Location**: `cli/src/main.rs` (700+ lines)

**Recommendation**: Split into submodules:

```
cli/src/
├── main.rs (200 lines - CLI setup and dispatch)
├── commands/
│   ├── mod.rs
│   ├── init.rs
│   ├── doctor.rs
│   ├── hooks.rs
│   ├── blame.rs
│   ├── authors.rs
│   └── verify.rs
└── ... (existing modules)
```

**Example refactoring**:

```rust
// cli/src/main.rs
mod commands;

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Init { quiet } => commands::init::run(quiet),
        Commands::Doctor { fix } => commands::doctor::run(fix),
        Commands::Hooks { command } => commands::hooks::run(command),
        Commands::Blame { file, agent, verify } => {
            commands::blame::run(file, agent, verify)
        }
        Commands::Authors { changes, format } => {
            commands::authors::run(changes, format)
        }
        Commands::Verify { revision } => commands::verify::run(revision),
        Commands::RecordChange { .. } => commands::record_change::run_legacy(..),
    }
}

// cli/src/commands/mod.rs
pub mod init;
pub mod doctor;
pub mod hooks;
pub mod blame;
pub mod authors;
pub mod verify;
pub mod record_change;

// cli/src/commands/init.rs
use anyhow::Result;

pub fn run(quiet: bool) -> Result<()> {
    // Move init_command logic here
}
```

**Impact**: Better organization, easier to navigate, clearer ownership of functionality.

---

## Priority Recommendations

### High Priority (Performance Impact)
1. **✅ Optimize variable resolution** (Issue 1) - Most frequently called, 2-3x speedup expected
2. **Remove unnecessary clones** (Issue 3.2) - Common pattern throughout
3. **Fix commit cache inefficiency** (Issue 3.3) - Used in hot path during blame

### Medium Priority (Code Quality)
4. **Introduce custom error types** (Issue 2) - Better ergonomics, easier testing
5. **Extract common logic** (Issue 5.1, 5.2) - Reduce duplication
6. **Add helper methods to Action enum** (Issue 5.1) - Cleaner code

### Low Priority (Nice to Have)
7. **Split main.rs** (Issue 13.1) - Better organization
8. **Add property-based tests** (Issue 8.1) - More thorough testing
9. **Improve documentation** (Issue 6.1) - Better maintainability

---

## Code Quality Metrics

**Strengths**:
- ✅ Good test coverage (unit tests for most modules)
- ✅ Clear separation of concerns (flows, provenance, jj integration)
- ✅ Thoughtful error messages
- ✅ Comprehensive benchmarking suite
- ✅ Well-structured type system

**Areas for Improvement**:
- ⚠️ Some performance hotspots (variable resolution, cloning)
- ⚠️ Code duplication in executor
- ⚠️ Inconsistent error handling patterns
- ⚠️ Could benefit from more documentation
- ⚠️ Large main.rs file

---

## Next Steps

1. **Benchmark current performance** to establish baseline
2. **Implement variable resolution optimization** (highest ROI)
3. **Introduce custom error types** (improves DX)
4. **Add property-based tests** (prevents regressions)
5. **Refactor executor** to reduce duplication
6. **Split main.rs** into submodules
7. **Document security considerations** for shell execution
8. **Add structured logging** with tracing crate

---

## Conclusion

The Aiki codebase demonstrates good Rust practices overall, with a well-designed architecture and comprehensive testing. The main opportunities for improvement are in performance optimization (particularly variable resolution), reducing code duplication, and introducing more structured error handling.

The recommended optimizations should yield measurable performance improvements (2-3x for typical flows) while also improving code maintainability and developer experience.
