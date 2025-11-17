# Refactor Plan: ExecutionContext → AikiState with Embedded AikiEvent

## Overview
Refactor the flow engine to eliminate data duplication by:
1. Embedding `AikiEvent` directly into `ExecutionContext`
2. Renaming `ExecutionContext` → `AikiState`
3. Updating variable naming for clarity

## Goals
- Single source of truth for event data
- Better naming consistency (AikiEvent, AikiState, AikiError, AikiAction)
- Maintain or improve performance
- All tests passing

## Changes Required

### Phase 1: Embed AikiEvent (STARTED)
✅ Updated `src/flows/types.rs`:
- Changed ExecutionContext to contain `event: AikiEvent`
- Removed: `cwd`, `event_vars`, `env_vars`
- Added helpers: `cwd()`, `agent_type()`
- Updated test: `test_execution_context_with_event`

### Phase 2: Update Executor
**File**: `src/flows/executor.rs`

**Changes needed**:
1. Update `create_resolver()` to stringify from `context.event`:
   - Map AgentType enum → string for variable interpolation
   - Extract session_id from `context.event.session_id`
   - Extract metadata as `event.{key}` variables
   - Remove env_vars storage, fetch on-demand with `std::env::vars()`

2. Fix field accesses:
   - `context.cwd` → `context.cwd()` (method call)
   - Remove `.envs(&context.env_vars)` from Command builders
   
3. Update action result storage:
   - Store computed values in `let_vars` (not event_vars)

### Phase 3: Update Handlers
**File**: `src/handlers.rs`

**Changes needed**:
1. `handle_start()`:
   - Change from `ExecutionContext::new(event.cwd.clone())` 
   - To: `ExecutionContext::new(event)`
   - Remove manual agent_type setting
   - Remove manual event_vars population

2. `handle_post_change()`:
   - Change validation to not consume event
   - Use `event.session_id.is_none()` instead of `ok_or_else`
   - Pass entire event to ExecutionContext::new()
   - Remove manual field population

### Phase 4: Update build_description
**File**: `src/flows/core/build_description.rs`

**Changes needed**:
1. Update to access event fields:
   - `context.agent_type()` instead of field access
   - `context.event.session_id` instead of event_vars
   - `context.event.metadata.get("tool_name")`

2. Update tests to create AikiEvent first

3. Remove AgentType from imports (use via context)

### Phase 5: Update All Tests
**Files**: All test files with ExecutionContext

**Pattern**:
```rust
// Old:
let mut context = ExecutionContext::new(PathBuf::from("/tmp"));
context.event_vars.insert("key", "value");

// New:
let event = AikiEvent::new(AikiEventType::PostChange, AgentType::ClaudeCode, "/tmp")
    .with_metadata("key", "value");
let mut context = ExecutionContext::new(event);
```

**Files to update**:
- `src/flows/executor.rs` - ~24 test functions
- `src/test_must_use.rs` - update to test AikiEvent path ergonomics
- Any other test files

### Phase 6: Update Benchmarks
**File**: `benches/flow_performance.rs`

**Changes needed**:
1. Add imports: `AikiEvent`, `AikiEventType`, `AgentType`
2. Update `bench_let_action_execution()`
3. Update `bench_provenance_flow_with_let()`

### Phase 7: Rename to AikiState
**Global search/replace**:
- `ExecutionContext` → `AikiState`
- Update module exports in `src/flows/mod.rs`
- Add better documentation

### Phase 8: Variable Naming
**At call sites (handlers, tests)**:
- `let mut context =` → `let mut state =`
- Update usages: `context.` → `state.`

**In build_description**:
- Parameter: `context: &AikiState` → `aiki: &AikiState`
- Usages: `context.` → `aiki.`

**In executor (keep as context)**:
- Internal implementation can keep "context" name

## Verification Steps
1. `cargo build` - must compile without errors
2. `cargo test --lib` - all 91 tests must pass
3. `cargo bench --no-run` - benchmarks must compile
4. Run benchmarks and compare to baseline

## Baseline Performance
```
current_provenance_recording: 92.4ms
let_action_execution:         22.3µs
provenance_flow_with_let:     8.33ms
```

## Success Criteria
- All compilation errors resolved
- All 91 tests passing
- No performance regression (within 5%)
- Ideally: slight performance improvement

## Files to Change (Summary)
1. ✅ `src/flows/types.rs` - Core struct definition
2. `src/flows/mod.rs` - Exports
3. `src/flows/executor.rs` - Variable resolution, field accesses, tests
4. `src/flows/core/build_description.rs` - Event access, tests
5. `src/handlers.rs` - Event passing, validation
6. `src/test_must_use.rs` - Test updates
7. `benches/flow_performance.rs` - Benchmark updates
8. `benches/provenance_comparison.rs` - If uses ExecutionContext

Since we're doing this incrementally with testing between phases.
