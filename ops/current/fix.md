# Hook System Fix Plan — 2025-12-12

This plan addresses all issues from `review.md`, grouped by scope and dependency order.

## ⚠️ Design Constraints (from Codex review)

1. **Do not cache env vars globally** — Runtime `set_var`/`remove_var` mutations (e.g., `AIKI_COMMIT_MSG_FILE`, `CLAUDE_SESSION_ID`) would be invisible after first access. Use lazy per-key lookup instead.

2. **Do not freeze resolver across statements** — The resolver must see fresh `let_vars` after each action. Only cache immutable event data (`$event.*`, `$cwd`), not user-defined variables.

---

## Fix 1: Add Caching Infrastructure Module

**Addresses:** Perf #1, #8, #9 (note: Perf #7 moved to Fix 2b)

Create `cli/src/cache.rs` with all static caches in one place:

```rust
use std::sync::LazyLock;
use std::collections::HashMap;

/// Debug mode flag - checked once per process
pub static DEBUG_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    std::env::var("AIKI_DEBUG").is_ok()
});

// NOTE: Do NOT cache ENV_VARS in a LazyLock. Runtime mutations via
// std::env::set_var (e.g., AIKI_COMMIT_MSG_FILE, CLAUDE_SESSION_ID)
// would be invisible after first access. Use lazy per-key lookup instead.

/// Aiki binary path - resolved once per process
pub static AIKI_BINARY_PATH: LazyLock<Result<String, String>> = LazyLock::new(|| {
    resolve_aiki_path()
});

fn resolve_aiki_path() -> Result<String, String> { ... }

/// Debug logging helper
#[inline]
pub fn debug_log(msg: impl std::fmt::Display) {
    if *DEBUG_ENABLED {
        eprintln!("[aiki] {}", msg);
    }
}
```

**Files to modify:**
- Create `cli/src/cache.rs`
- `cli/src/lib.rs` — add `pub mod cache;`
- `cli/src/event_bus.rs` — replace `std::env::var("AIKI_DEBUG")` with `cache::DEBUG_ENABLED`
- `cli/src/vendors/claude_code.rs` — use `cache::debug_log()`
- `cli/src/vendors/cursor.rs` — use `cache::debug_log()`
- `cli/src/events/*.rs` — use `cache::debug_log()`
- `cli/src/flows/engine.rs:190` — use lazy per-key lookup (see below)
- `cli/src/config.rs:50-81` — use `cache::AIKI_BINARY_PATH`
- `cli/src/flows/bundled.rs` — rename `load_core_flow` → `load_core_flow_uncached`, add cached wrapper

**Also addresses:** Perf #1 (core flow caching) — add to same module:

```rust
use std::sync::OnceLock;
use crate::flows::types::Flow;

static CORE_FLOW: OnceLock<Flow> = OnceLock::new();

/// Get cached core flow (parsed once per process)
pub fn get_core_flow() -> &'static Flow {
    CORE_FLOW.get_or_init(|| {
        // Call the existing load function, unwrap since bundled YAML is known-good
        crate::flows::bundled::load_core_flow_uncached()
            .expect("Failed to parse bundled core flow")
    })
}
```

Then update `cli/src/flows/bundled.rs`:
```rust
// Rename existing function to make caching explicit
pub fn load_core_flow_uncached() -> Result<Flow> {
    // ... existing YAML parsing logic ...
}

// New cached entrypoint
pub fn load_core_flow() -> Result<&'static Flow> {
    Ok(crate::cache::get_core_flow())
}
```

---

## Fix 2: Optimize Flow Engine

**Addresses:** Perf #2, #5, #7, CodeQuality #6

### 2a: Make timing collection opt-in (Perf #2 + CodeQuality #6)

**⚠️ Constraint:** Cannot remove `FlowTiming`/`StatementTiming` — existing tests depend on them:
- `cli/tests/test_timing_infrastructure.rs:27-102`
- `cli/tests/test_end_to_end_flow.rs:86-249`
- `cli/tests/test_flow_statements.rs:23-156`
- `cli/tests/test_session_end.rs:106`
- `cli/src/flows/engine.rs:3940-3941` (helper routines)

**Approach:** Two public methods with internal helper (prevents accidentally enabling timing in production):

```rust
impl FlowEngine {
    /// Execute statements (production hot path, no timing overhead)
    pub fn execute_statements(
        statements: &[FlowStatement],
        state: &mut AikiState,
    ) -> Result<FlowResult> {
        Self::execute_statements_with_options(statements, state, false)
            .map(|(r, _)| r)
    }

    /// Execute statements with timing collection (for benchmarks only)
    pub fn execute_statements_with_timing(
        statements: &[FlowStatement],
        state: &mut AikiState,
    ) -> Result<(FlowResult, FlowTiming)> {
        Self::execute_statements_with_options(statements, state, true)
    }

    fn execute_statements_with_options(
        statements: &[FlowStatement],
        state: &mut AikiState,
        collect_timing: bool,
    ) -> Result<(FlowResult, FlowTiming)> {
        // When !collect_timing:
        // - Skip Instant::now() calls
        // - Return empty FlowTiming { duration_secs: 0.0, statement_timings: vec![] }
    }
}
```

**Files:**
- `cli/src/flows/engine.rs` — add `execute_statements_with_timing()`, refactor `execute_statements()` to skip timing
- `cli/src/events/*.rs` — update to use new signature (returns `Result<FlowResult>`)
- `cli/tests/*.rs` — update to use new signature (tests don't need timing data, only benchmarks do)

---

## Fix 3: Remove Unnecessary Payload Cloning

**Addresses:** Perf #3

Remove unnecessary `.clone()` calls in 3 event handlers:

```rust
// Before
let mut state = AikiState::new(payload.clone());

// After
let mut state = AikiState::new(payload);
```

The clones were only there for error messages that no longer exist. The payload is consumed by `AikiState::new()` which takes ownership — no clone needed.

**Files:**
- `cli/src/events/post_file_change.rs:70` — remove `.clone()`
- `cli/src/events/pre_file_change.rs:35` — remove `.clone()`
- `cli/src/events/session_end.rs:34` — remove `.clone()`

---

## Fix 4: Consolidate Vendor Code

**Addresses:** CodeQuality #1, #2, #4

### 4a: Extract shared `HookCommandResponse` struct

**Current:** Both vendors have identical 100% duplicate code:
- `struct CursorResponse` vs `struct ClaudeCodeResponse` - identical fields
- `print_json()` method - identical 10 lines of code
- Exit code pattern - identical

Add to `cli/src/commands/hooks.rs`:

```rust
/// Response for vendor hook commands (JSON output + exit code)
///
/// This is the vendor protocol format, distinct from our internal `HookResult`.
/// - `HookResult`: Aiki's internal result (Decision, context, failures)
/// - `HookCommandResponse`: Vendor protocol (JSON value, exit code)
pub struct HookCommandResponse {
    pub json_value: Option<serde_json::Value>,
    pub exit_code: i32,
}

impl HookCommandResponse {
    #[must_use]
    pub fn new(json_value: Option<serde_json::Value>, exit_code: i32) -> Self {
        Self { json_value, exit_code }
    }

    pub fn print_and_exit(self) -> ! {
        if let Some(value) = &self.json_value {
            if let Ok(json) = serde_json::to_string(value) {
                println!("{}", json);
            }
        }
        std::process::exit(self.exit_code);
    }
}
```

Then in both vendors:
```rust
use crate::commands::hooks::HookCommandResponse;

// Before: CursorResponse { json_value: Some(json), exit_code: 2 }
// After:  HookCommandResponse::new(Some(json), 2)

// Before: cursor_response.print_json(); std::process::exit(cursor_response.exit_code);
// After:  hook_response.print_and_exit();
```

**Saves:** ~30 lines of duplicate code, clearer intent, struct lives with hook command logic


### 4b: Remove unnecessary `expect()` calls

**Current issue:** Both vendors use `.expect("Failed to create AikiSession")` but `AikiSession::new()` is infallible (always returns `Ok`).

**Root cause:** `AikiSession::new()` returns `Result<Self>` but never actually fails - it always returns `Ok(...)`.

**Better fix:** Make `AikiSession::new()` return `Self` instead of `Result<Self>`:

```rust
// In cli/src/session.rs
impl AikiSession {
    pub fn new(
        agent_type: AgentType,
        external_id: impl Into<String>,
        agent_version: Option<impl Into<String>>,
        detection_method: DetectionMethod,
    ) -> Self {  // Remove Result wrapper
        let external_id = external_id.into();
        let uuid = Self::generate_uuid(agent_type, &external_id);

        Self {
            uuid,
            agent_type,
            external_id,
            client_name: None,
            client_version: None,
            agent_version: agent_version.map(|v| v.into()),
            detection_method,
        }
    }
}
```

Then in both vendors, remove `.expect()`:

```rust
// Before
fn create_session(payload: &CursorPayload) -> AikiSession {
    AikiSession::new(...)
        .expect("Failed to create AikiSession for Cursor")
}

// After
fn create_session(payload: &CursorPayload) -> AikiSession {
    AikiSession::new(...)  // No Result to unwrap
}
```

**Files:**
- `cli/src/session.rs` — make `AikiSession::new()` return `Self` instead of `Result<Self>`
- `cli/src/commands/hooks.rs` — add `HookCommandResponse` struct
- `cli/src/vendors/mod.rs` — add `is_file_modifying_tool`
- `cli/src/vendors/claude_code.rs` — replace `ClaudeCodeResponse` with `HookCommandResponse`, remove `.expect()`
- `cli/src/vendors/cursor.rs` — replace `CursorResponse` with `HookCommandResponse`, remove `.expect()`
- Any other callers of `AikiSession::new()` — remove `?` or `.expect()` since it's no longer a Result

---

## Fix 5: Small Optimizations

**Addresses:** Perf #10

### 5a: Early exit in `get_running_editors()`

```rust
fn get_running_editors() -> (bool, bool) {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let (mut claude, mut cursor) = (false, false);
    for process in sys.processes().values() {
        if claude && cursor {
            break;  // Early exit
        }
        let name = process.name().to_string_lossy().to_lowercase();
        if !claude && name == "claude" {
            claude = true;
        }
        if !cursor && name.contains("cursor") {
            cursor = true;
        }
    }
    (claude, cursor)
}
```

**Files:**
- `cli/src/commands/hooks.rs:86-104`

---

## Fix 6: Add `#[must_use]` Attributes

**Addresses:** CodeQuality #5

Audit and add `#[must_use]` to:

```rust
// cli/src/events/post_file_change.rs
impl EditDetail {
    #[must_use]
    pub fn new(...) -> Self { ... }
}
```

**Files to audit:**
- `cli/src/events/post_file_change.rs` — `EditDetail::new`
- `cli/src/events/result.rs` — verify existing coverage
- `cli/src/session.rs` — `AikiSessionFile::new`

---

## Fix 7: Add Missing Tests

**Addresses:** Test #1, #2

### 7a: Vendor translator tests

Create `cli/tests/vendor_tests.rs`:

```rust
#[test]
fn test_claude_code_post_tool_use_translation() { ... }

#[test]
fn test_cursor_after_file_edit_translation() { ... }

#[test]
fn test_translate_response_unknown_event() { ... }

#[test]
fn test_event_bus_error_recovery() { ... }
```

### 7b: Session lifecycle tests

Create `cli/tests/session_lifecycle_tests.rs`:

```rust
#[test]
fn test_full_claude_session_lifecycle() {
    // SessionStart → PostFileChange × 3 → PostResponse → SessionEnd
}

#[test]
fn test_full_cursor_session_lifecycle() { ... }
```

**Files:**
- Create `cli/tests/vendor_tests.rs`
- Create `cli/tests/session_lifecycle_tests.rs`

---



---

## Implementation Order

| Phase | Fixes | Dependencies | Est. LOC |
|-------|-------|--------------|----------|
| 1 | Fix 1 (caching: debug, binary path, core flow) | None | ~80 |
| 2 | Fix 4 (vendors) | None | ~80 |
| 3 | Fix 2 (flow engine: timing, resolver) | Fix 1 | ~120 |
| 4 | Fix 3, 5, 6 (remove clone, small opts, must_use) | None | ~10 |
| 5 | Fix 7 (tests) | Fixes 1-4 | ~200 |


---

## Cross-Reference: Review → Fix

| Review Item | Fix |
|-------------|-----|
| Perf #1 (cache flow) | Fix 1 |
| Perf #2 (disable timing) | Fix 2a |
| Perf #3 (avoid cloning) | Fix 3 |
| Perf #5 (reuse resolvers) | Fix 2b |
| Perf #7 (cache env vars) | Fix 2b (lazy lookup, not bulk cache) |
| Perf #8 (cache debug flag) | Fix 1 |
| Perf #9 (cache binary path) | Fix 1 |
| Perf #10 (process scan) | Fix 5a |
| CodeQuality #1 (duplicate fn) | Fix 4b |
| CodeQuality #2 (duplicate structs) | Fix 4a |
| CodeQuality #3 (boilerplate) | ~~Skipped~~ (code is clear as-is) |
| CodeQuality #4 (expect→Result) | Fix 4c |
| CodeQuality #5 (must_use) | Fix 6 |
| CodeQuality #6 (unused timing) | Fix 2a |
| Test #1 (vendor tests) | Fix 7a |
| Test #2 (lifecycle tests) | Fix 7b |
