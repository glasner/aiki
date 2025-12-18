# ACP Refactor Plan

## Goal

Clean up `commands/acp.rs` by extracting editor-specific logic into a dedicated `editors/acp` module, and rename `vendors` to `editors` for better clarity.

## Current Status

**Not started.** All phases pending.

## Background

Currently `commands/acp.rs` is a **3005-line** file that mixes:
- Command-line interface logic (CLI args, process spawning)
- ACP protocol handling (JSON-RPC parsing, message forwarding)
- Editor-specific event building (Claude Code vs Cursor differences)
- Thread coordination (3-thread architecture with channels)

The `vendors/` directory contains editor-specific event builders but should be renamed to `editors/` since "vendor" implies third-party code, when really these are first-party integrations for different editors.

## Current Structure

```
cli/src/
├── acp/
│   ├── mod.rs
│   └── protocol.rs         # Protocol wrappers and types
├── commands/
│   └── acp.rs (3005 lines)
├── ide_config.rs           # Zed editor configuration
├── npm.rs                  # npm package manager detection
└── vendors/
    ├── mod.rs
    ├── claude_code/
    │   ├── mod.rs
    │   ├── events.rs
    │   ├── output.rs
    │   ├── session.rs
    │   └── tools.rs
    └── cursor/
        ├── mod.rs
        ├── events.rs
        ├── output.rs
        ├── session.rs
        └── tools.rs
```

## Proposed Structure

```
cli/src/
├── commands/
│   └── acp.rs (slim CLI entrypoint <300 lines)
└── editors/
    ├── mod.rs
    ├── zed.rs                # Zed configuration (moved from src/ide_config.rs)
    ├── npm.rs                # npm detection (moved from src/npm.rs)
    ├── acp/
    │   ├── mod.rs
    │   ├── proxy.rs          # Core proxy logic (3-thread architecture)
    │   ├── protocol.rs       # ACP protocol types and parsing (moved from src/acp/)
    │   ├── state.rs          # State management (channels, accumulators)
    │   └── handlers.rs       # Event firing logic
    ├── claude_code/
    │   ├── mod.rs
    │   ├── events.rs
    │   ├── output.rs
    │   ├── session.rs
    │   └── tools.rs
    └── cursor/
        ├── mod.rs
        ├── events.rs
        ├── output.rs
        ├── session.rs
        └── tools.rs
```

**Note:** The existing `src/acp/` directory will be deleted since all ACP-related code will live in `editors/acp/`.

## Refactoring Steps

### Phase 0: Move editor utilities into editors/

**Goal:** Move standalone editor-related files into the new `editors/` directory structure before the main refactor.

1. Rename directory: `cli/src/vendors/` → `cli/src/editors/`
2. Move files into editors/:
   - `cli/src/ide_config.rs` → `cli/src/editors/zed.rs`
   - `cli/src/npm.rs` → `cli/src/editors/npm.rs`
3. Update module declarations in `main.rs`:
   - `mod vendors;` → `mod editors;`
   - Remove `mod ide_config;` and `mod npm;` (now under editors)
4. Update `cli/src/editors/mod.rs` to include:
   ```rust
   pub mod zed;
   pub mod npm;
   pub mod claude_code;
   pub mod cursor;
   ```
   Also preserve the `read_stdin_json()` utility function from the old `vendors/mod.rs`.
5. Update all imports throughout codebase:
   - `use crate::vendors::` → `use crate::editors::`
   - `use crate::ide_config::` → `use crate::editors::zed::`
   - `use crate::npm::` → `use crate::editors::npm::`
6. Run tests to ensure no breakage

**Rationale:** 
- "vendors" implies external code, but these are first-party editor integrations we maintain
- `ide_config.rs` is entirely Zed-specific, so `editors/zed.rs` is more accurate
- `npm.rs` is used for editor binary detection, so belongs in editors/
- Doing this first establishes the `editors/` namespace cleanly

### Phase 1: Move existing ACP module and create structure

**Goal:** Move `src/acp/` to `editors/acp/` and set up the new module hierarchy.

1. Move `cli/src/acp/protocol.rs` → `cli/src/editors/acp/protocol.rs`

2. Delete `cli/src/acp/` directory (now empty)

3. Create `cli/src/editors/acp/mod.rs` with module declarations:
   ```rust
   pub mod proxy;
   pub mod protocol;
   pub mod state;
   pub mod handlers;
   ```

4. Create empty files:
   - `cli/src/editors/acp/proxy.rs`
   - `cli/src/editors/acp/state.rs`
   - `cli/src/editors/acp/handlers.rs`

5. Update `cli/src/editors/mod.rs` to include:
   ```rust
   pub mod acp;
   pub mod zed;
   pub mod npm;
   pub mod claude_code;
   pub mod cursor;
   ```

6. Update import in `commands/acp.rs`:
   - `use crate::acp::protocol::` → `use crate::editors::acp::protocol::`

7. Remove `mod acp;` from `main.rs` (no longer needed)

### Phase 2: Extract protocol types and state enums

**Goal:** Add proxy-specific types to their appropriate modules.

The existing `protocol.rs` already has:
- `JsonRpcMessage` - JSON-RPC wrapper
- `ClientInfo`, `AgentInfo` - initialization types
- `InitializeRequest`, `InitializeResponse` - handshake types

Move from `commands/acp.rs` to `editors/acp/protocol.rs`:
- `JsonRpcId` - normalized JSON-RPC ID type
- `SessionId` type alias
- `session_id()` helper

Move from `commands/acp.rs` to `editors/acp/state.rs`:
- `StateMessage` enum (for thread communication)
- `AutoreplyMessage` enum (for autoreply channel)

Move from `commands/acp.rs` to `editors/acp/handlers.rs`:
- `Autoreply` struct (autoreply request builder)

Keep in `commands/acp.rs` for now:
- Thread spawning logic
- Main `run()` function
- CLI argument handling

**Validation:** Run tests, ensure compilation succeeds.

### Phase 3: Extract state management

**Goal:** Move state tracking to `editors/acp/state.rs`.

Extract state-related logic:
- `MAX_AUTOREPLIES` constant
- Autoreply counter functions:
  - `check_autoreply_limit()`
  - `increment_autoreply_counter()`
  - `reset_autoreply_counter()`
- Response accumulator helpers (if we create them)
- Tool call context tracking (might stay in proxy for now)

**Design decision:** Should state be encapsulated in a struct, or remain as function parameters?
- **Option A:** Keep as pure functions with `HashMap` parameters (current style)
- **Option B:** Create `ProxyState` struct with methods (more OO)
- **Recommendation:** Start with Option A (pure functions) since the existing code already uses this pattern and it's easier to test

### Phase 4: Extract event handlers

**Goal:** Move event firing logic to `editors/acp/handlers.rs`.

Extract event handling functions:
- `fire_session_start_event()`
- `fire_pre_file_change_event()`
- `handle_session_end()`
- `handle_session_prompt()`
- `handle_session_update()`
- `create_session()` helper
- Prompt manipulation helpers:
  - `extract_text_from_prompt_array()`
  - `concatenate_text_chunks()`
  - `build_modified_prompt()`
  - `extract_autoreply()`

These are all relatively pure functions that take parameters and fire events via `event_bus::dispatch()`.

### Phase 5: Extract proxy core logic

**Goal:** Move the 3-thread proxy architecture to `editors/acp/proxy.rs`.

This is the most complex step. Given the file size (~2400 lines in thread closures), break into sub-phases:

#### Phase 5a: Extract IDE→Agent thread logic

Extract the IDE→Agent thread closure into a helper function in `commands/acp.rs`:
```rust
fn run_ide_to_agent_thread(...) -> Result<()> {
    // All IDE→Agent forwarding logic
}
```

Verify compilation and tests pass.

#### Phase 5b: Extract Agent→IDE thread logic

Extract the Agent→IDE thread closure into a helper function:
```rust
fn run_agent_to_ide_thread(...) -> Result<()> {
    // All Agent→IDE forwarding logic (this is the largest piece)
}
```

Verify compilation and tests pass.

#### Phase 5c: Move to proxy.rs

Move both helper functions to `editors/acp/proxy.rs` and create the public entry point:
```rust
// editors/acp/proxy.rs
pub fn run_proxy(mut agent: Child, agent_type: AgentType) -> Result<()> {
    // Channel setup, thread spawning, coordination
}

fn run_ide_to_agent_thread(...) -> Result<()> { ... }
fn run_agent_to_ide_thread(...) -> Result<()> { ... }
```

Keep in `commands/acp.rs`:
- CLI argument parsing
- Binary resolution (using `editors::zed`, `editors::npm`)
- Agent process spawning
- Call to `editors::acp::proxy::run_proxy()`

**Final signature:**
```rust
// commands/acp.rs
pub fn run(agent_type: String, bin: Option<String>, agent_args: Vec<String>) -> Result<()> {
    let validated_agent_type = parse_agent_type(&agent_type)?;
    let (command, command_args) = resolve_binary(&agent_type, bin, agent_args)?;

    // Spawn agent
    let mut agent = Command::new(&command)
        .args(&command_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;

    // Run proxy
    editors::acp::proxy::run_proxy(agent, validated_agent_type)
}
```

### Phase 6: Final cleanup

**Goal:** Ensure `commands/acp.rs` is a slim CLI entrypoint.

1. Review what's left in `commands/acp.rs`:
   - Should be <300 lines (down from 3005)
   - Just CLI parsing and delegation to `editors::acp`
   - No protocol logic, no thread management

2. Add module-level docs to new files explaining their purpose

3. Run full test suite

4. Update any documentation references to old structure

## Testing Strategy

After each phase:
1. Run `cargo check` to ensure compilation
2. Run `cargo test` to ensure no behavior changes
3. Manual testing with `aiki acp` command if possible

## Parallel Execution Strategy

Some phases can be executed in parallel by subagents to speed up the refactor.

### Dependency Graph

```
Phase 0 (vendors→editors)
    ↓
Phase 1 (create structure)
    ↓
Phase 2 (extract types) ←── Must complete before 3, 4, 5
    ↓
    ├── Phase 3 (state.rs)     ─┐
    │                           ├── Can run in parallel
    └── Phase 4 (handlers.rs)  ─┘
                ↓
        Merge & verify
                ↓
    Phase 5a (IDE→Agent extraction)
                ↓
    Phase 5b (Agent→IDE extraction)
                ↓
    Phase 5c (move to proxy.rs)
                ↓
        Phase 6 (cleanup)
```

### Parallel Window: Phase 3 + Phase 4

After Phase 2 completes, two agents can work simultaneously:

| Agent | Target | Work |
|-------|--------|------|
| Agent A | `state.rs` | Extract 3 autoreply counter functions |
| Agent B | `handlers.rs` | Extract 17 handler/utility functions + `Autoreply` struct |

**Coordination required:** Both agents remove code from `commands/acp.rs`. To avoid conflicts:
- Agent A edits lines 1013-1035 (autoreply counter functions)
- Agent B edits all other function definitions
- Merge sequentially: Agent A commits first, Agent B rebases

### Optional: Split Phase 4 Further

If Phase 4 is bottlenecking, split `handlers.rs` work by function category:

| Agent | Category | Functions |
|-------|----------|-----------|
| Agent B1 | Session lifecycle | `create_session`, `handle_session_*`, `fire_session_start_event` (5 functions) |
| Agent B2 | Tool processing | `process_tool_*`, `record_post_change_events`, `parse_permission_request`, `fire_pre_file_change_event` (5 functions) |
| Agent B3 | Utilities | `extract_*`, `concatenate_*`, `build_*`, `paths_from_*`, `tool_kind_to_name` (7 functions) |

**Note:** Verify call graph before splitting - these functions may have internal dependencies.

### Phase 5: Sequential Recommended

Phase 5 sub-phases (5a, 5b, 5c) share channels and state variables in `run()`. Run sequentially to avoid complexity.

## Benefits

1. **Clarity:** `commands/acp.rs` becomes a clear entrypoint, not a 3005-line monolith
2. **Testability:** Protocol types, state management, and handlers can be unit tested independently
3. **Modularity:** ACP proxy logic lives in `editors/acp/`, clearly separated from CLI concerns
4. **Naming:** `editors/` is more accurate than `vendors/` for what these modules do
5. **Maintainability:** Easier to find and modify specific pieces of functionality

## Decisions Made

1. **Documentation location:**
   - Keep high-level architecture docs in `commands/acp.rs` for CLI users
   - Add implementation details to `editors/acp/mod.rs`

2. **Tool call context tracking:**
   - Start in `proxy.rs`, extract to `state.rs` if it becomes complex

3. **Session creation:**
   - `create_session()` → `handlers.rs` (specific to event firing)

4. **Type placement:**
   - `JsonRpcId` → `protocol.rs`
   - `StateMessage`, `AutoreplyMessage` → `state.rs`
   - `Autoreply` struct → `handlers.rs`

5. **Pure utility functions:**
   - Move based on what they operate on (state vs. events)
   - See Function Inventory table above

## Non-Goals

- Not changing any behavior or logic
- Not optimizing performance
- Not adding new features
- Not refactoring `claude_code/` or `cursor/` modules (already well-organized)
- Not changing the 3-thread architecture (it works well)

## Success Criteria

- [ ] All tests pass
- [ ] `commands/acp.rs` is < 300 lines (down from 3005)
- [ ] New `editors/acp/` module has clear responsibilities
- [ ] No behavior changes (same events, same timing)
- [ ] Code is easier to navigate and understand

## Function Inventory

For reference, here are the 23 functions currently in `commands/acp.rs` and their destinations:

| Function | Destination |
|----------|-------------|
| `run()` | stays in `commands/acp.rs` |
| `session_id()` | `protocol.rs` |
| `parse_agent_type()` | `protocol.rs` |
| `check_autoreply_limit()` | `state.rs` |
| `increment_autoreply_counter()` | `state.rs` |
| `reset_autoreply_counter()` | `state.rs` |
| `create_session()` | `handlers.rs` |
| `handle_session_update()` | `handlers.rs` |
| `handle_session_end()` | `handlers.rs` |
| `handle_session_prompt()` | `handlers.rs` |
| `fire_session_start_event()` | `handlers.rs` |
| `fire_pre_file_change_event()` | `handlers.rs` |
| `extract_text_from_prompt_array()` | `handlers.rs` |
| `concatenate_text_chunks()` | `handlers.rs` |
| `build_modified_prompt()` | `handlers.rs` |
| `extract_autoreply()` | `handlers.rs` |
| `process_tool_call()` | `handlers.rs` |
| `process_tool_call_update()` | `handlers.rs` |
| `record_post_change_events()` | `handlers.rs` |
| `paths_from_locations()` | `handlers.rs` |
| `tool_kind_to_name()` | `handlers.rs` |
| `extract_edit_details()` | `handlers.rs` |
| `parse_permission_request()` | `handlers.rs` |
