# Event Module Refactor Plan

**Status:** Planning  
**Date:** 2025-12-10  
**Goal:** Reorganize events into individual files, co-locating handlers with their event types.

---

## Motivation

### Current Pain Points

1. **Poor navigability** - `events.rs` (213 lines) and `handlers.rs` (520 lines) are hard to navigate
2. **Artificial separation** - Event types and their handlers are tightly coupled but live in separate files
3. **Hard to trace event flow** - Must jump between 3+ files to understand one event's lifecycle
4. **Scales poorly** - Adding new events means editing massive files

### Current Structure

```
cli/src/
├── events.rs (213 lines) - All 7 event type definitions
├── handlers.rs (520 lines) - All 7 event handlers
├── event_bus.rs (104 lines) - Central dispatcher
└── vendors/
    ├── claude_code.rs - Constructs events
    └── cursor.rs - Constructs events
```

**Event flow today:**
```
Vendor → events.rs (type def) → event_bus.rs (dispatch) → handlers.rs (logic)
```

### Proposed Solution

Create an `events/` directory where each event gets its own file containing both type definition and handler logic.

```
cli/src/events/
├── mod.rs                  # Common infrastructure
├── session_start.rs        # AikiStartEvent + handle_start()
├── pre_prompt.rs           # AikiPrePromptEvent + handle_pre_prompt()
├── pre_file_change.rs      # AikiPreFileChangeEvent + handle_pre_file_change()
├── post_file_change.rs     # AikiPostFileChangeEvent + handle_post_file_change()
├── post_response.rs        # AikiPostResponseEvent + handle_post_response()
├── session_end.rs          # AikiSessionEndEvent + handle_session_end()
└── prepare_commit_msg.rs   # AikiPrepareCommitMessageEvent + handle_prepare_commit_message()
```

**Event flow after refactor:**
```
Vendor → events/pre_prompt.rs (type + logic) → event_bus.rs (dispatch)
```

---

## What Goes Where

### `events/mod.rs` - Common Infrastructure

**Purpose:** Shared types, main enum, trait implementations

```rust
// Response types (shared by all handlers)
pub struct Failure(pub String);
pub enum Decision { Allow, Block }
pub struct HookResponse {
    pub context: Option<String>,
    pub decision: Decision,
    pub failures: Vec<Failure>,
}

// Main event enum (dispatch target)
pub enum AikiEvent {
    SessionStart(AikiStartEvent),
    PrePrompt(AikiPrePromptEvent),
    PreFileChange(AikiPreFileChangeEvent),
    PostFileChange(AikiPostFileChangeEvent),
    PostResponse(AikiPostResponseEvent),
    SessionEnd(AikiSessionEndEvent),
    PrepareCommitMessage(AikiPrepareCommitMessageEvent),
    Unsupported,
}

// Common methods
impl AikiEvent {
    pub fn cwd(&self) -> &Path { ... }
    pub fn agent_type(&self) -> AgentType { ... }
}

// From trait implementations (enables vendor .into() pattern)
impl From<AikiStartEvent> for AikiEvent { ... }
impl From<AikiPrePromptEvent> for AikiEvent { ... }
// ... etc for all event types

// Module declarations
mod session_start;
mod pre_prompt;
mod pre_file_change;
mod post_file_change;
mod post_response;
mod session_end;
mod prepare_commit_msg;

// Re-exports (maintains existing import paths)
pub use session_start::*;
pub use pre_prompt::*;
pub use pre_file_change::*;
pub use post_file_change::*;
pub use post_response::*;
pub use session_end::*;
pub use prepare_commit_msg::*;
```

**What stays here:**
- `AikiEvent` enum and its common methods
- `HookResponse`, `Decision`, `Failure` (response types)
- All `From<XEvent> for AikiEvent` trait implementations
- Module declarations and re-exports

### Individual Event Files

**Pattern:** Each file contains the event struct, handler function, and event-specific helpers.

**Example: `events/pre_prompt.rs`**

```rust
use crate::error::Result;
use crate::flows::{self, ExecutionContext};
use super::{HookResponse, Decision, Failure};  // Import common types
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Event fired before the AI receives a user prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AikiPrePromptEvent {
    pub cwd: PathBuf,
    pub agent_type: AgentType,
    pub prompt: String,
    pub session_id: Option<String>,
}

/// Handle pre-prompt events
pub fn handle_pre_prompt(event: AikiPrePromptEvent) -> Result<HookResponse> {
    let ctx = ExecutionContext::new(event.cwd)
        .with_agent_type(event.agent_type)
        .with_session_id(event.session_id.unwrap_or_default());

    let result = flows::execute_all(&ctx, event.into())?;
    
    Ok(HookResponse::from(result))
}

// Any event-specific helper functions would go here
```

**File sizes (estimated):**
- `session_start.rs` - ~60 lines
- `pre_prompt.rs` - ~70 lines
- `pre_file_change.rs` - ~90 lines (includes `EditDetail` type)
- `post_file_change.rs` - ~80 lines
- `post_response.rs` - ~100 lines (includes auto-SessionEnd dispatch logic)
- `session_end.rs` - ~120 lines (includes JJ provenance logic)
- `prepare_commit_msg.rs` - ~70 lines

---

## Migration Steps

### 1. Create Events Directory

```bash
mkdir cli/src/events
```

### 2. Create `events/mod.rs`

**Move from `handlers.rs`:**
- `pub struct Failure(pub String);`
- `pub enum Decision { Allow, Block }`
- `pub struct HookResponse { ... }`
- All `impl HookResponse` methods

**Move from `events.rs`:**
- `pub enum AikiEvent { ... }`
- `impl AikiEvent` methods (`cwd()`, `agent_type()`)
- All `impl From<XEvent> for AikiEvent`

**Add:**
- Module declarations for all 7 event files
- Re-exports: `pub use session_start::*;` etc.

### 3. Create Individual Event Files

For each event type, create a new file:

**`events/session_start.rs`:**
- Move `AikiStartEvent` struct from `events.rs`
- Move `handle_start()` function from `handlers.rs`

**`events/pre_prompt.rs`:**
- Move `AikiPrePromptEvent` struct from `events.rs`
- Move `handle_pre_prompt()` function from `handlers.rs`

**`events/pre_file_change.rs`:**
- Move `AikiPreFileChangeEvent` struct from `events.rs`
- Move `EditDetail` struct from `events.rs`
- Move `handle_pre_file_change()` function from `handlers.rs`

**`events/post_file_change.rs`:**
- Move `AikiPostFileChangeEvent` struct from `events.rs`
- Move `handle_post_file_change()` function from `handlers.rs`

**`events/post_response.rs`:**
- Move `AikiPostResponseEvent` struct from `events.rs`
- Move `handle_post_response()` function from `handlers.rs`
- Note: Contains special auto-dispatch logic for SessionEnd

**`events/session_end.rs`:**
- Move `AikiSessionEndEvent` struct from `events.rs`
- Move `handle_session_end()` function from `handlers.rs`
- Note: Contains JJ provenance recording logic

**`events/prepare_commit_msg.rs`:**
- Move `AikiPrepareCommitMessageEvent` struct from `events.rs`
- Move `handle_prepare_commit_message()` function from `handlers.rs`

### 4. Update `event_bus.rs`

Change handler calls from `handlers::handle_X` to `events::handle_X`:

```rust
// Before
use crate::handlers;

match event {
    AikiEvent::PrePrompt(e) => handlers::handle_pre_prompt(e),
    // ...
}

// After
use crate::events;

match event {
    AikiEvent::PrePrompt(e) => events::handle_pre_prompt(e),
    // ...
}
```

### 5. Update `cli/src/lib.rs`

```rust
// Before
mod events;
mod handlers;
pub use events::*;
pub use handlers::*;

// After
mod events;
pub use events::*;  // Now re-exports everything (types + handlers)
```

### 6. Delete Old Files

```bash
rm cli/src/events.rs
rm cli/src/handlers.rs
```

### 7. Verify Compilation

```bash
cargo check
```

### 8. Run Tests

```bash
cargo test
```

**Critical tests to verify:**
- `cli/tests/test_session_end.rs` - SessionEnd event handling
- Any flow tests that use events
- Vendor integration tests (if any)

---

## No Changes Needed To

✅ **Vendor code** (`vendors/claude_code.rs`, `vendors/cursor.rs`)
- Still construct event types the same way
- `.into()` conversion still works via `From` impls in `mod.rs`

✅ **ACP code** (`commands/acp.rs`)
- Still imports `AikiEvent` from `events`
- Re-exports maintain compatibility

✅ **Flow engine** (`flows/state.rs`, `flows/engine.rs`)
- Still receives `AikiEvent` enum
- Pattern matching unchanged

✅ **Event bus** (`event_bus.rs`)
- Dispatch logic stays the same
- Only import path changes (`handlers::` → `events::`)

---

## Benefits

### Developer Experience

1. **Single-file ownership** - Everything for one event in one place
2. **Easy navigation** - Find `pre_prompt.rs` instead of searching 500-line files
3. **Clear boundaries** - ~70-100 lines per file vs 500+ line monoliths
4. **Easier review** - Changes to one event don't touch other events

### Code Organization

1. **Follows proven pattern** - Matches existing `commands/` module structure
2. **Better encapsulation** - Event-specific helpers stay with their event
3. **Scalable** - Adding new events doesn't bloat existing files
4. **Self-documenting** - File structure reflects event architecture

### Maintenance

1. **Easier refactoring** - Changes isolated to single files
2. **Better git history** - Commits touch fewer files
3. **Clearer dependencies** - Import statements show what each event needs
4. **Testability** - Can add event-specific tests in same module

---

## Risks and Mitigations

### Risk: Breaking existing imports

**Mitigation:** Re-exports from `mod.rs` maintain all existing import paths.

```rust
// This still works after refactor
use aiki::events::{AikiEvent, AikiPrePromptEvent, HookResponse};
```

### Risk: Circular dependencies

**Mitigation:** Event files only import from `mod.rs` (response types), not each other.

### Risk: PostResponse auto-dispatch complexity

**Mitigation:** Keep auto-SessionEnd dispatch logic in `post_response.rs`, well-documented.

---

## Success Criteria

- [ ] `cargo check` passes
- [ ] `cargo test` passes
- [ ] All existing imports work unchanged
- [ ] Each event is in its own file (~70-100 lines)
- [ ] `events/mod.rs` contains only shared infrastructure
- [ ] No changes to vendor or ACP code
- [ ] Git diff shows clean file moves (not rewrites)

---

## Future Enhancements

Once this refactor is complete, consider:

1. **Event-specific tests** - Add test modules in each event file
2. **Event documentation** - Add module-level docs to each event file
3. **Handler trait** - Consider `trait EventHandler` for consistency
4. **Event builder pattern** - Add `.with_X()` methods to event structs

---

## References

- Current code: `cli/src/events.rs`, `cli/src/handlers.rs`, `cli/src/event_bus.rs`
- Similar pattern: `cli/src/commands/` (proven module structure)
- Related docs: `CLAUDE.md` (Module Organization section)
