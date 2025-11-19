# Milestone 5.2: Codebase Optimization & Performance Improvements

**Date**: 2025-01-18  
**Status**: 📋 Planned  
**Type**: Optimization & Refactoring

## Overview

Comprehensive optimization pass across the codebase to address performance bottlenecks, simplify code patterns, improve API consistency, and refactor architecture for better maintainability. Based on detailed code review identifying 37 specific issues across performance, simplification, architecture, and API design.

## Motivation

The codebase is well-structured and follows Rust best practices, but there are opportunities to:
- **Improve performance** in hot code paths (blame, flow execution)
- **Reduce allocations** through better string handling and caching
- **Simplify code** by extracting common patterns
- **Standardize APIs** for better ergonomics and consistency
- **Improve architecture** through better separation of concerns

## Expected Impact

- **15-25% faster** blame operations
- **10-15% faster** flow execution  
- **Better UX** during hook installation (fewer process scans)
- **More maintainable** vendor integration code
- **Consistent APIs** throughout codebase
- **Reduced allocations** in hot paths

---

## Phase 1: Critical Performance Fixes

**Impact**: Highest - These run in hot code paths  
**Estimated Effort**: 2-3 hours

### 1.1 Fix HashMap Lookup in blame.rs (CRITICAL)

**File**: `cli/src/blame.rs:115-138`  
**Issue**: Multiple pattern matches on same HashMap in hot loop (runs for every line)

**Current (3 lookups per signature check):**
```rust
match signature_cache.get(&attr.change_id) {
    Some(verify::SignatureStatus::Good) => "✓ ",
    Some(verify::SignatureStatus::Bad) => "✗ ",
    Some(verify::SignatureStatus::Unknown) => "? ",
    Some(verify::SignatureStatus::Unsigned) => "⚠ ",
    _ => "⚠ ",
}
```

**Fix (1 lookup):**
```rust
let sig_indicator = if verify {
    signature_cache.get(&attr.change_id)
        .map(|status| match status {
            verify::SignatureStatus::Good => "✓ ",
            verify::SignatureStatus::Bad => "✗ ",
            verify::SignatureStatus::Unknown => "? ",
            verify::SignatureStatus::Unsigned => "⚠ ",
        })
        .unwrap_or("⚠ ")
} else {
    ""
};
```

**Impact**: Very High - runs for every line in blame output

---

### 1.2 Reuse VariableResolver in flows/executor.rs (CRITICAL)

**File**: `cli/src/flows/executor.rs:289-330`  
**Issue**: Creating new `VariableResolver` for every action when variables rarely change

**Current:**
```rust
fn execute_shell(action: &ShellAction, context: &AikiState) -> Result<ActionResult> {
    let mut resolver = Self::create_resolver(context); // New allocation every time
    let command = resolver.resolve(&action.shell);
    // ...
}
```

**Fix:**
```rust
pub fn execute_actions(actions: &[Action], context: &mut AikiState) -> Result<FlowResult> {
    let mut resolver = Self::create_resolver(context);
    
    for action in actions {
        let result = Self::execute_action(action, context, &mut resolver)?;
        
        // Only recreate resolver when variables change
        if matches!(action, Action::Let(_)) {
            resolver = Self::create_resolver(context);
        }
    }
}

// Update signatures to accept resolver
fn execute_action(
    action: &Action,
    context: &mut AikiState,
    resolver: &mut VariableResolver,
) -> Result<ActionResult>

fn execute_shell(
    action: &ShellAction,
    context: &AikiState,
    resolver: &mut VariableResolver,
) -> Result<ActionResult>
```

**Impact**: High - executes for every action in a flow  
**Estimated improvement**: 10-15% faster flow execution

---

### 1.3 Fix String Concatenation in flows/executor.rs (CRITICAL)

**File**: `cli/src/flows/executor.rs:364-369`  
**Issue**: Using `String::push_str()` in loop without pre-allocation causes multiple reallocations

**Current (inefficient):**
```rust
if !continue_failure_msg.is_empty() {
    continue_failure_msg.push_str("; ");
}
continue_failure_msg.push_str(&error_msg);
```

**Fix:**
```rust
// Collect all errors, then join once
let mut errors = Vec::new();
// ... in loop:
errors.push(error_msg);
// ... after loop:
let continue_failure_msg = errors.join("; ");
```

**Impact**: High - executes on every action failure in flows

---

## Phase 2: High-Priority Performance Improvements

**Impact**: High - User-facing performance  
**Estimated Effort**: 3-4 hours

### 2.1 Extract to_repo_path() Helper

**File**: `cli/src/authors.rs:133,145`  
**Issue**: Converting `PathBuf` → `&str` → `String` → `RepoPath` for every file

**Fix:**
```rust
fn to_repo_path(file_path: &Path) -> Result<RepoPath> {
    let path_str = file_path.to_str()
        .context("File path contains invalid UTF-8")?;
    RepoPath::from_internal_string(path_str)
        .context("Invalid repository path")
}
```

**Impact**: Medium-High - runs for every changed file

---

### 2.2 Single Process Scan for Editor Detection

**File**: `cli/src/commands/hooks.rs:51-80`  
**Issue**: `is_claude_code_running()` and `is_cursor_running()` each do full process scans

**Current (2 full scans):**
```rust
let claude_running = is_claude_code_running(); // Full scan
let cursor_running = is_cursor_running();      // Full scan
```

**Fix (1 scan):**
```rust
fn get_running_editors() -> (bool, bool) {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    
    let (claude, cursor) = sys.processes().values().fold((false, false), |(c, cu), p| {
        let name = p.name().to_string_lossy().to_lowercase();
        (
            c || (name.contains("claude") && (name.contains("code") || name == "claude")),
            cu || name.contains("cursor")
        )
    });
    (claude, cursor)
}
```

**Impact**: Medium - only runs during install, but significantly better UX

---

### 2.3 Remove Intermediate Vec in authors.rs

**File**: `cli/src/authors.rs:283-288`  
**Issue**: Creating intermediate `Vec` just to join strings

**Current:**
```rust
authors.iter()
    .map(|author| format!("{} <{}>", author.name, author.email))
    .collect::<Vec<_>>()  // Unnecessary Vec
    .join("\n")
```

**Fix:**
```rust
use std::fmt::Write;

let mut output = String::with_capacity(authors.len() * 50); // Estimate
for (i, author) in authors.iter().enumerate() {
    if i > 0 {
        output.push('\n');
    }
    write!(output, "{} <{}>", author.name, author.email).unwrap();
}
output
```

**Impact**: Low-Medium - only for display

---

### 2.4 Pre-allocate Output Buffer in blame.rs

**File**: `cli/src/blame.rs:115-148`  
**Issue**: Multiple allocations per line when building output

**Fix:**
```rust
use std::fmt::Write;

let mut output = String::with_capacity(attributions.len() * 100); // Pre-allocate

for attr in attributions {
    // ... filters ...
    write!(
        output,
        "{}{} ({:12} {:12} {:6}) {:4}| {}\n",
        sig_indicator,
        &attr.commit_id[..8.min(attr.commit_id.len())],
        attr.agent_type,
        attr.session_id.as_deref().unwrap_or("-"),
        // ...
    ).unwrap();
}
```

**Impact**: Medium-High - runs for every line in blame output  
**Estimated improvement**: 15-25% faster blame formatting

---

### 2.5 Batch git config Calls

**File**: `cli/src/commands/init.rs:24-37`  
**Issue**: Multiple `git config` subprocess calls that could be batched

**Fix:**
```rust
fn get_git_config_batch(keys: &[&str]) -> Result<HashMap<String, Option<String>>> {
    let output = Command::new("git")
        .args(&["config", "--list"])
        .output()?;
    
    let config_str = String::from_utf8_lossy(&output.stdout);
    let mut config = HashMap::new();
    
    for line in config_str.lines() {
        if let Some((key, value)) = line.split_once('=') {
            if keys.contains(&key) {
                config.insert(key.to_string(), Some(value.to_string()));
            }
        }
    }
    
    // Ensure all requested keys are in map (even if None)
    for key in keys {
        config.entry(key.to_string()).or_insert(None);
    }
    
    Ok(config)
}
```

**Impact**: Low-Medium - only during init, but cleaner code

---

## Phase 3: API Consistency & Design

**Impact**: High - Better developer experience  
**Estimated Effort**: 4-5 hours

### 3.1 Standardize Path Parameters

**Files**: Multiple (blame.rs, authors.rs, etc.)  
**Issue**: Inconsistent use of `&Path`, `impl AsRef<Path>`, `PathBuf`

**Standard Pattern (per CLAUDE.md):**
```rust
// For functions that store paths
pub fn new(repo_path: impl AsRef<Path>) -> Self {
    Self {
        repo_path: repo_path.as_ref().to_path_buf(),
    }
}

// For functions that only read paths
pub fn process_file(&self, file_path: &Path) -> Result<()> {
    // ...
}
```

**Files to update:**
- `cli/src/blame.rs:83` - `blame_file(file_path: &Path)` → `blame_file(file_path: impl AsRef<Path>)`
- `cli/src/authors.rs` - Various functions
- Any other path-taking functions

**Impact**: Better API ergonomics

---

### 3.2 Add Missing #[must_use] Attributes

**File**: `cli/src/handlers.rs:38-78`  
**Issue**: Builder pattern methods missing `#[must_use]` attribute

**Fix:**
```rust
impl HookResponse {
    #[must_use]
    pub fn success() -> Self { ... }
    
    #[must_use]
    pub fn success_with_message(user_msg: impl Into<String>) -> Self { ... }
    
    #[must_use]
    pub fn with_metadata(mut self, metadata: Vec<(String, String)>) -> Self { ... }
    
    #[must_use]
    pub fn with_agent_message(mut self, msg: impl Into<String>) -> Self { ... }
}
```

**Also check:**
- All other builder methods in the codebase
- Constructor functions that return values

**Impact**: Better API safety (compiler warnings on unused values)

---

### 3.3 Fix Error Type Inconsistencies

**Files**: `cli/src/blame.rs`, `cli/src/authors.rs`  
**Issue**: Using `anyhow::Result` instead of `crate::error::Result`

**Current:**
```rust
// blame.rs:9
type Result<T> = anyhow::Result<T>;
```

**Per CLAUDE.md:** Should use `AikiError` for Aiki-specific errors, `anyhow::Result` only for heavy jj-lib interop

**Fix:**
1. Audit functions in blame.rs and authors.rs
2. Convert appropriate functions to use `crate::error::Result`
3. Add new `AikiError` variants as needed:
   ```rust
   #[error("Blame failed for file: {0}")]
   BlameFailed(PathBuf),
   
   #[error("Author extraction failed: {0}")]
   AuthorExtractionFailed(String),
   ```

**Impact**: Consistent error handling throughout codebase

---

### 3.4 Add Builder Pattern for BlameFormatter

**File**: `cli/src/blame.rs`  
**Issue**: `format_blame()` could benefit from builder for optional parameters

**Current:**
```rust
pub fn format_blame(
    &self,
    attributions: &[LineAttribution],
    agent_filter: Option<AgentType>,
    verify: bool,
) -> String
```

**Fix:**
```rust
pub struct BlameFormatter<'a> {
    attributions: &'a [LineAttribution],
    agent_filter: Option<AgentType>,
    verify: bool,
}

impl<'a> BlameFormatter<'a> {
    #[must_use]
    pub fn new(attributions: &'a [LineAttribution]) -> Self {
        Self {
            attributions,
            agent_filter: None,
            verify: false,
        }
    }
    
    #[must_use]
    pub fn with_agent_filter(mut self, agent: AgentType) -> Self {
        self.agent_filter = Some(agent);
        self
    }
    
    #[must_use]
    pub fn with_verification(mut self, verify: bool) -> Self {
        self.verify = verify;
        self
    }
    
    pub fn format(&self) -> String {
        // Current format_blame logic
    }
}

// Usage:
let output = BlameFormatter::new(&attributions)
    .with_agent_filter(AgentType::ClaudeCode)
    .with_verification(true)
    .format();
```

**Impact**: Better API ergonomics for optional parameters

---

## Phase 4: Architecture Refactoring

**Impact**: High - Better maintainability  
**Estimated Effort**: 5-6 hours

### 4.1 Separate Vendor Handler Concerns

**Files**: `cli/src/vendors/claude_code.rs`, `cli/src/vendors/cursor.rs`  
**Issue**: Vendor handlers do parsing, translation, dispatch, AND output formatting

**Current Flow:**
```
handle() → parse JSON → translate → dispatch → translate response → output JSON
```

**Recommended Separation:**
```rust
// vendors/claude_code.rs
pub fn parse_event(json: &str) -> Result<AikiEvent> {
    // JSON parsing only
}

pub fn format_response(response: HookResponse, event_type: &str) -> String {
    // Response formatting only
}

// Keep handle() thin
pub fn handle(event_name: &str) -> Result<()> {
    let json = super::read_stdin();
    let event = parse_event(&json)?;
    let response = event_bus::dispatch(event)?;
    let output = format_response(response, event_type);
    println!("{}", output);
    Ok(())
}
```

**Benefits:**
- Easier to test each component
- Clear separation of concerns
- Simpler to add new vendors

**Impact**: Better testability and maintainability

---

### 4.2 Split AikiState into EventContext + ExecutionState

**File**: `cli/src/flows/state.rs`  
**Issue**: `AikiState` mixes immutable event data with mutable execution state

**Current:**
```rust
pub struct AikiState {
    pub event: AikiEvent,  // Immutable trigger
    let_vars: HashMap<String, String>,  // Mutable state
    variable_metadata: HashMap<String, ActionResult>,  // Mutable state
    pub flow_name: Option<String>,  // Execution context
}
```

**Recommended:**
```rust
/// Immutable event context
pub struct EventContext {
    pub event: AikiEvent,
}

/// Mutable execution state
pub struct ExecutionState {
    context: EventContext,
    let_vars: HashMap<String, String>,
    variable_metadata: HashMap<String, ActionResult>,
    pub flow_name: Option<String>,
}

impl ExecutionState {
    pub fn new(event: AikiEvent) -> Self {
        Self {
            context: EventContext { event },
            let_vars: HashMap::new(),
            variable_metadata: HashMap::new(),
            flow_name: None,
        }
    }
    
    pub fn event(&self) -> &AikiEvent {
        &self.context.event
    }
}
```

**Benefits:**
- Clear ownership of immutable vs mutable data
- Prevents accidental event modification
- Better encapsulation

**Impact**: API clarity, immutability guarantees

---

### 4.3 Create CommandExecutor Abstraction

**File**: `cli/src/flows/executor.rs`  
**Issue**: Command execution logic (shell, jj) has duplicated timeout/error handling

**Fix:**
```rust
trait CommandExecutor {
    fn execute(&self, context: &ExecutionContext, timeout: Option<Duration>) -> Result<ActionResult>;
}

struct ShellCommand {
    command: String,
}

struct JjCommand {
    command: String,
}

impl CommandExecutor for ShellCommand {
    fn execute(&self, context: &ExecutionContext, timeout: Option<Duration>) -> Result<ActionResult> {
        // Shared timeout/error handling
        execute_with_timeout(
            || run_shell(&self.command, &context.cwd),
            timeout,
        )
    }
}

impl CommandExecutor for JjCommand {
    fn execute(&self, context: &ExecutionContext, timeout: Option<Duration>) -> Result<ActionResult> {
        execute_with_timeout(
            || run_jj(&self.command, &context.cwd),
            timeout,
        )
    }
}

// Shared timeout logic
fn execute_with_timeout<F>(f: F, timeout: Option<Duration>) -> Result<ActionResult>
where
    F: FnOnce() -> Result<ActionResult>,
{
    // Common timeout and error handling
}
```

**Benefits:**
- DRY: timeout/error logic in one place
- Easier to add new command types
- More testable

**Impact**: Better code organization

---

### 4.4 Extract Command Runner Utility

**Files**: `cli/src/config.rs`, `cli/src/signing.rs`, `cli/src/commands/init.rs`  
**Issue**: Repeated pattern for running git/jj commands with error handling

**Fix:**
```rust
// cli/src/utils/command.rs
pub struct CommandRunner;

impl CommandRunner {
    pub fn git(args: &[&str]) -> Result<Output> {
        Command::new("git")
            .args(args)
            .output()
            .context("Failed to run git command")
    }
    
    pub fn git_in_dir(args: &[&str], dir: &Path) -> Result<Output> {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .context("Failed to run git command")
    }
    
    pub fn jj(args: &[&str]) -> Result<Output> {
        Command::new("jj")
            .args(args)
            .output()
            .context("Failed to run jj command")
    }
    
    pub fn jj_in_dir(args: &[&str], dir: &Path) -> Result<Output> {
        Command::new("jj")
            .args(args)
            .current_dir(dir)
            .output()
            .context("Failed to run jj command")
    }
}
```

**Benefits:**
- Consistent command execution
- Centralized error handling
- Easier to add logging/tracing

**Impact**: Better code reuse

---

## Phase 5: Code Simplification

**Impact**: Medium - Better readability  
**Estimated Effort**: 3-4 hours

### 5.1 Centralize Agent Formatting

**Files**: `cli/src/authors.rs:326-342`, `cli/src/provenance.rs`  
**Issue**: Agent formatting logic duplicated across files

**Fix:**
```rust
// In cli/src/provenance.rs
impl AgentType {
    pub fn email(&self) -> &'static str {
        match self {
            AgentType::ClaudeCode => "claude-code@anthropic.ai",
            AgentType::Cursor => "cursor@cursor.sh",
            AgentType::Unknown => "unknown@aiki.dev",
        }
    }
    
    pub fn git_author(&self) -> String {
        format!("{} <{}>", self, self.email())
    }
}
```

**Usage:**
```rust
// Instead of duplicating formatting logic
let author = agent_type.git_author();
```

**Impact**: DRY, consistent formatting

---

### 5.2 Extract Common Event Construction

**Files**: `cli/src/vendors/claude_code.rs`, `cli/src/vendors/cursor.rs`  
**Issue**: Similar event construction logic in each vendor

**Fix:**
```rust
// In cli/src/vendors/mod.rs
pub fn create_session_start(
    agent_type: AgentType,
    session_id: Option<String>,
    cwd: PathBuf,
) -> AikiEvent {
    AikiEvent::SessionStart(AikiStartEvent {
        agent_type,
        session_id,
        cwd,
        timestamp: chrono::Utc::now(),
    })
}

pub fn create_post_change(
    agent_type: AgentType,
    session_id: String,
    tool_name: String,
    file_path: String,
    cwd: PathBuf,
) -> AikiEvent {
    AikiEvent::PostChange(AikiPostChangeEvent {
        agent_type,
        session_id,
        tool_name,
        file_path,
        cwd,
        timestamp: chrono::Utc::now(),
    })
}
```

**Impact**: Less duplication across vendor handlers

---

### 5.3 Simplify Nested Matches

**File**: `cli/src/handlers.rs:58-102`  
**Issue**: Nested pattern matching makes logic hard to follow

**Current:**
```rust
match &context.event {
    AikiEvent::PostChange(event) => {
        // nested logic
    }
    _ => {
        return Err(AikiError::Other(anyhow::anyhow!(
            "build_description can only be called for PostChange events"
        )));
    }
}
```

**Fix (using let-else):**
```rust
let AikiEvent::PostChange(event) = &context.event else {
    return Err(AikiError::Other(anyhow::anyhow!(
        "build_description can only be called for PostChange events"
    )));
};

// Direct access to event now
```

**Impact**: Better readability

---

### 5.4 Standardize Error Context Messages

**File**: `cli/src/config.rs`  
**Issue**: Repeated `.context()` calls with similar messages

**Fix:**
```rust
fn read_config_file(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .with_context(|| format!("Failed to read config: {}", path.display()))
}

fn parse_config_toml(content: &str, path: &Path) -> Result<Config> {
    toml::from_str(content)
        .with_context(|| format!("Failed to parse config: {}", path.display()))
}
```

**Impact**: Consistent error messages

---

## Phase 6: Medium & Low Priority Optimizations

**Impact**: Low-Medium - Final polish  
**Estimated Effort**: 2-3 hours

### 6.1 Pre-allocate Strings Where Size is Known

**Files**: Multiple  
**Issue**: Building strings without capacity hints

**Pattern:**
```rust
// Before
let mut output = String::new();

// After
let mut output = String::with_capacity(estimated_size);
```

**Locations:**
- `cli/src/flows/executor.rs:606-692` - commit_message building
- Various formatting functions

---

### 6.2 Cache Environment Variables

**Files**: `cli/src/flows/executor.rs`, `cli/src/event_bus.rs`  
**Issue**: `std::env::var("AIKI_DEBUG")` called repeatedly

**Fix:**
```rust
use lazy_static::lazy_static;

lazy_static! {
    static ref DEBUG_MODE: bool = std::env::var("AIKI_DEBUG").is_ok();
}

// Usage:
if *DEBUG_MODE {
    eprintln!("[flows] ...");
}
```

**Impact**: Very Low - env::var is fast, but cleaner code

---

### 6.3 Optimize Diff Parsing

**File**: `cli/src/authors.rs:250-302`  
**Issue**: Parsing diff line-by-line with repeated string operations

**Consider:**
- State machine for faster parsing
- Regex for pattern matching
- Or leave as-is if performance is acceptable

**Impact**: Medium - only during `aiki authors --changes=staged`

---

### 6.4 Add Convenience Methods to HookResponse

**File**: `cli/src/handlers.rs`  
**Issue**: Could have more ergonomic constructors

**Add:**
```rust
impl HookResponse {
    #[must_use]
    pub fn warn(user_msg: impl Into<String>) -> Self {
        Self::success_with_message(format!("⚠️ {}", user_msg.into()))
    }
    
    #[must_use]
    pub fn info(user_msg: impl Into<String>) -> Self {
        Self::success_with_message(format!("ℹ️ {}", user_msg.into()))
    }
}
```

---

### 6.5 Module Reorganization (Optional)

**File**: `cli/src/flows/executor.rs` (850+ lines)  
**Issue**: Large module handling multiple concerns

**Recommended:**
```
flows/
  ├── executor.rs (orchestration only)
  ├── actions/
  │   ├── mod.rs
  │   ├── shell.rs
  │   ├── jj.rs
  │   ├── let.rs
  │   └── commit_message.rs
  ├── resolver.rs (variable resolution)
  └── timeout.rs (timeout utilities)
```

**Impact**: Better organization for future maintenance

---

### 6.6 Minor Improvements

- Add `const fn` where applicable
- Remove unnecessary intermediate variables
- Combine similar match arms
- Add `Default` implementations where appropriate
- Use `impl Display` instead of `Into<String>` where appropriate
- Add `From` implementations for common conversions

---

## Testing Strategy

### After Each Phase:

1. **Run existing test suite**: `cargo test`
2. **Run clippy**: `cargo clippy -- -D warnings`
3. **Run fmt**: `cargo fmt --check`
4. **Manual testing**:
   - Blame operations with various filters
   - Flow execution with multiple actions
   - Hook installation on different systems
   - Vendor event handling

### New Tests to Add:

**Performance Tests:**
- Benchmark blame with 1000+ lines
- Benchmark flow execution with 50+ actions
- Benchmark variable resolution

**Integration Tests:**
- Vendor handler parsing/formatting
- Error handling across boundaries
- Command execution with timeouts

**Unit Tests:**
- BlameFormatter builder pattern
- CommandExecutor implementations
- Error type conversions

---

## Implementation Checklist

### Phase 1: Critical Performance (2-3 hours)
- [ ] Fix HashMap lookup in blame.rs
- [ ] Reuse VariableResolver in executor
- [ ] Fix string concatenation in error handling
- [ ] Run tests and benchmarks
- [ ] Commit: "perf: Optimize hot paths in blame and flow execution"

### Phase 2: High-Priority Performance (3-4 hours)
- [ ] Extract to_repo_path() helper
- [ ] Single process scan for editors
- [ ] Remove intermediate Vec in authors
- [ ] Pre-allocate blame output buffer
- [ ] Batch git config calls
- [ ] Run tests
- [ ] Commit: "perf: Reduce allocations and subprocess calls"

### Phase 3: API Consistency (4-5 hours)
- [ ] Standardize path parameters
- [ ] Add #[must_use] attributes
- [ ] Fix error type inconsistencies
- [ ] Add BlameFormatter builder
- [ ] Update documentation
- [ ] Run tests
- [ ] Commit: "refactor: Standardize APIs for better ergonomics"

### Phase 4: Architecture (5-6 hours)
- [ ] Separate vendor handler concerns
- [ ] Split AikiState into EventContext + ExecutionState
- [ ] Create CommandExecutor abstraction
- [ ] Extract command runner utility
- [ ] Add tests for new abstractions
- [ ] Run tests
- [ ] Commit: "refactor: Improve architecture and separation of concerns"

### Phase 5: Simplification (3-4 hours)
- [ ] Centralize agent formatting
- [ ] Extract common event construction
- [ ] Simplify nested matches
- [ ] Standardize error context messages
- [ ] Run tests
- [ ] Commit: "refactor: Simplify code and reduce duplication"

### Phase 6: Final Polish (2-3 hours)
- [ ] Pre-allocate strings
- [ ] Cache environment variables
- [ ] Optimize diff parsing (if needed)
- [ ] Add convenience methods
- [ ] Module reorganization (optional)
- [ ] Minor improvements
- [ ] Run full test suite
- [ ] Commit: "chore: Final optimizations and polish"

---

## Success Criteria

- ✅ All existing tests pass
- ✅ No clippy warnings
- ✅ Code formatted consistently
- ✅ Measurable performance improvements:
  - Blame operations 15-25% faster
  - Flow execution 10-15% faster
  - Hook installation faster (single process scan)
- ✅ Consistent API patterns throughout codebase
- ✅ Better separation of concerns
- ✅ Reduced code duplication
- ✅ Documentation updated

---

## Estimated Total Effort

- **Phase 1**: 2-3 hours (Critical)
- **Phase 2**: 3-4 hours (High Priority)
- **Phase 3**: 4-5 hours (API)
- **Phase 4**: 5-6 hours (Architecture)
- **Phase 5**: 3-4 hours (Simplification)
- **Phase 6**: 2-3 hours (Polish)

**Total: 19-25 hours** (2.5-3 full days)

Can be split into 6 separate PRs for easier review.

---

## Breaking Changes

**None.** All changes are internal optimizations and refactoring. Public APIs will maintain backward compatibility (with additions for better ergonomics).

---

## Future Considerations

After this optimization pass, consider:

1. **Performance monitoring**: Add instrumentation to track hot paths
2. **Benchmarking suite**: Continuous performance regression testing
3. **Profile-guided optimization**: Use profiling data for further improvements
4. **Caching layer**: Add caching for expensive operations (if needed)

---

## Related Documentation

- `CLAUDE.md` - Rust idioms and best practices
- `ops/phase-5.md` - Flow engine architecture
- `ops/ROADMAP.md` - Overall project roadmap

---

## Notes

- This milestone focuses on **internal quality** rather than new features
- Performance improvements are **measured** not guessed
- All optimizations maintain **code clarity** - no premature optimization
- Architecture changes improve **maintainability** for future development
- Changes align with **CLAUDE.md** guidelines
