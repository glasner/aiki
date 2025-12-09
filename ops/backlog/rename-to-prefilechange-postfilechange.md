# Plan: Rename PreChange/PostChange to PreFileChange/PostFileChange

## Goal

Rename `PreChange` and `PostChange` to `PreFileChange` and `PostFileChange` throughout the codebase to better reflect that these events are specifically for file modifications, not general changes.

## Motivation

**Current naming is ambiguous:**
- "Change" is too generic - could mean any kind of change (config, state, database, etc.)
- "PreChange/PostChange" doesn't clearly indicate these are **file-specific** events

**Better naming:**
- `PreFileChange` - Clearly indicates this fires before **file modifications**
- `PostFileChange` - Clearly indicates this fires after **file modifications**
- Consistent with the fact that we only fire these for file-modifying tools (Edit, Write, Delete, Move)

**Benefits:**
1. **Clearer semantics** - Developers immediately understand these are file-specific
2. **Room for future events** - We can add `PreDatabaseChange`, `PreConfigChange`, etc. without confusion
3. **Better documentation** - Self-documenting code
4. **Aligns with tool classification** - We already filter for "file-modifying tools"

## Scope

This is a **pure rename** with no functional changes. The behavior remains identical.

### Files to Update

#### 1. Core Event Types
- **File:** `cli/src/events.rs`
  - `AikiPreChangeEvent` → `AikiPreFileChangeEvent`
  - `AikiPostChangeEvent` → `AikiPostFileChangeEvent`
  - `AikiEvent::PreChange` → `AikiEvent::PreFileChange`
  - `AikiEvent::PostChange` → `AikiEvent::PostFileChange`
  - Update all trait implementations (`cwd()`, `agent_type()`, `From`)
  - Update documentation comments

#### 2. Flow System
- **File:** `cli/src/flows/types.rs`
  - `pub pre_change: Vec<Action>` → `pub pre_file_change: Vec<Action>`
  - `pub post_change: Vec<Action>` → `pub post_file_change: Vec<Action>`
  - `#[serde(rename = "PreChange")]` → `#[serde(rename = "PreFileChange")]`
  - `#[serde(rename = "PostChange")]` → `#[serde(rename = "PostFileChange")]`

- **File:** `cli/src/flows/core/flow.yaml`
  - `PreChange:` → `PreFileChange:`
  - `PostChange:` → `PostFileChange:`
  - Update comments explaining the events

- **File:** `cli/src/flows/executor.rs`
  - Update match arms for event types
  - Update test names: `test_prechange_flow_*` → `test_prefilechange_flow_*`
  - Update comments

- **File:** `cli/src/flows/bundled.rs`
  - Update test names and assertions
  - `test_core_flow_has_pre_change` → `test_core_flow_has_pre_file_change`
  - `test_core_flow_has_post_change` → `test_core_flow_has_post_file_change`

#### 3. Event Handling
- **File:** `cli/src/handlers.rs`
  - `handle_pre_change()` → `handle_pre_file_change()`
  - `handle_post_change()` → `handle_post_file_change()`
  - Update function documentation
  - Update event type references in match statements

- **File:** `cli/src/event_bus.rs`
  - Update dispatch match arms
  - `AikiEvent::PreChange` → `AikiEvent::PreFileChange`
  - `AikiEvent::PostChange` → `AikiEvent::PostFileChange`
  - Update debug logging strings

#### 4. Integration - ACP Protocol
- **File:** `cli/src/commands/acp.rs`
  - `fire_pre_change_event()` → `fire_pre_file_change_event()`
  - Update function documentation
  - Update event construction: `AikiEvent::PreChange` → `AikiEvent::PreFileChange`
  - Update event construction: `AikiEvent::PostChange` → `AikiEvent::PostFileChange`
  - Update comments and debug messages
  - Update references in `process_tool_call()` and `record_post_change_events()`

#### 5. Integration - Claude Code Hooks
- **File:** `cli/src/vendors/claude_code.rs`
  - Update import: `AikiPreChangeEvent` → `AikiPreFileChangeEvent`
  - Update import: `AikiPostChangeEvent` → `AikiPostFileChangeEvent`
  - Update event construction in `"PreToolUse"` handler
  - Update event construction in `"PostToolUse"` handler
  - Update comments explaining when events fire

#### 6. Integration - Cursor Hooks
- **File:** `cli/src/vendors/cursor.rs`
  - Update import: `AikiPreChangeEvent` → `AikiPreFileChangeEvent`
  - Update import: `AikiPostChangeEvent` → `AikiPostFileChangeEvent`
  - Update event construction in `"beforeMCPExecution"` handler
  - Update event construction in `"afterFileEdit"` handler
  - Update comments

#### 7. Tests
- **File:** `cli/src/flows/executor.rs` (test module)
  - `test_prechange_flow_with_jj_diff_output` → `test_prefilechange_flow_with_jj_diff_output`
  - `test_prechange_flow_with_empty_jj_diff` → `test_prefilechange_flow_with_empty_jj_diff`
  - Update comments in test functions

- **File:** `cli/src/flows/bundled.rs` (test module)
  - `test_core_flow_has_pre_change` → `test_core_flow_has_pre_file_change`
  - `test_core_flow_has_post_change` → `test_core_flow_has_post_file_change`
  - Update assertions checking field names

#### 8. Documentation
- **File:** `CLAUDE.md`
  - Update references to PreChange/PostChange events
  - Update event descriptions

- **File:** `ops/prechange-stashing-plan.md`
  - Add note at top: "Note: PreChange was renamed to PreFileChange after implementation"
  - Optionally update throughout (or leave as historical record)

## Implementation Strategy

### Step 1: Update Core Event Types (events.rs)
Start with the source of truth - the event definitions. This will cause compilation errors throughout the codebase, which is good - we want the compiler to find all references.

### Step 2: Update Flow System (types.rs, flow.yaml)
Update the flow type definitions and the core flow YAML. The YAML field names are critical since they're serialized.

### Step 3: Update Handlers (handlers.rs, event_bus.rs)
Fix the event routing and handler functions.

### Step 4: Update Integrations (acp.rs, claude_code.rs, cursor.rs)
Fix all three integration paths that fire these events.

### Step 5: Update Tests
Rename test functions and update assertions. Run full test suite.

### Step 6: Update Documentation
Update CLAUDE.md and add migration notes to the original plan.

## Testing Strategy

1. **Compilation check** - Ensure no references to old names remain
   ```bash
   cargo check
   ```

2. **Run full test suite** - All 140+ tests should pass
   ```bash
   cargo test --lib
   ```

3. **Grep for old names** - Verify no strings/comments were missed
   ```bash
   grep -r "PreChange" cli/src/
   grep -r "PostChange" cli/src/
   grep -r "pre_change" cli/src/
   grep -r "post_change" cli/src/
   ```

4. **Run benchmark** - Verify flow YAML is correctly updated
   ```bash
   ./target/release/aiki benchmark aiki/core
   ```

5. **Manual verification** - Check flow.yaml loads correctly
   ```bash
   ./target/release/aiki --help  # Should not error
   ```

## Risks and Mitigation

### Risk 1: Breaking YAML Compatibility
**Issue:** Existing flow.yaml files use `PreChange:` and `PostChange:`

**Mitigation:**
- The bundled core flow is embedded in the binary, so it will update automatically
- Document in CHANGELOG that custom flows need updating
- Consider adding backwards-compatible serde aliases (future enhancement)

### Risk 2: Missed String References
**Issue:** Comments, debug messages, error strings might not be caught by compiler

**Mitigation:**
- Grep for all variations of the old names
- Manual code review
- Run with `AIKI_DEBUG=1` to check log messages

### Risk 3: Documentation Drift
**Issue:** Documentation might reference old names

**Mitigation:**
- Update CLAUDE.md as part of this change
- Add note to historical plan documents
- Update any README files

## Backwards Compatibility

**Flow YAML files:**
- ❌ **Breaking change** - Custom flows using `PreChange:` or `PostChange:` will need updating
- ✅ Bundled core flow updates automatically (embedded in binary)

**JSON/API:**
- ✅ No breaking changes - this is internal naming only
- Event dispatching still works the same way

**Hooks:**
- ✅ No breaking changes - hook names (`PreToolUse`, `PostToolUse`, etc.) are unchanged
- Only internal event type names change

## Migration Guide for Users

If users have custom flow YAML files:

**Before:**
```yaml
PreChange:
  - jj: diff -r @ --name-only
    alias: changed_files

PostChange:
  - let: metadata = self.build_metadata
  - jj: metaedit --message "$metadata.message"
```

**After:**
```yaml
PreFileChange:
  - jj: diff -r @ --name-only
    alias: changed_files

PostFileChange:
  - let: metadata = self.build_metadata
  - jj: metaedit --message "$metadata.message"
```

## Rationale: Why This Rename is Worth It

1. **Clarity > Brevity** - "FileChange" is 4 extra characters but infinitely clearer
2. **Future-proof** - Makes room for non-file events (database, config, network)
3. **Self-documenting** - Code explains itself without comments
4. **Early in lifecycle** - Better to rename now before widespread adoption
5. **Low risk** - Pure rename, no logic changes, caught by compiler

## Alternative Considered: Keep Current Names

**Pros:**
- No migration needed
- No breaking changes

**Cons:**
- Technical debt accumulates
- Naming ambiguity persists
- Harder to add new event types later

**Decision:** Rename now while codebase is young and adoption is limited.

## Checklist

- [ ] Update core event types (events.rs)
- [ ] Update flow system (types.rs, flow.yaml)
- [ ] Update executor (executor.rs)
- [ ] Update handlers (handlers.rs, event_bus.rs)
- [ ] Update ACP integration (acp.rs)
- [ ] Update Claude Code hooks (claude_code.rs)
- [ ] Update Cursor hooks (cursor.rs)
- [ ] Update tests (executor.rs tests, bundled.rs tests)
- [ ] Grep for remaining old names
- [ ] Run full test suite (140+ tests)
- [ ] Run benchmark
- [ ] Update CLAUDE.md
- [ ] Add migration note to prechange-stashing-plan.md
- [ ] Verify with `AIKI_DEBUG=1`

## Expected Outcome

After this rename:
- ✅ All event names clearly indicate they're file-specific
- ✅ All tests pass
- ✅ Benchmark shows identical performance
- ✅ Documentation is updated
- ✅ No functional changes to behavior
- ✅ Easier to reason about the event system
- ✅ Room for future non-file events

**Lines of code changed:** ~50-80 files, ~200-300 lines (mostly mechanical renames)

**Estimated time:** 30-45 minutes for implementation + testing

**Complexity:** Low (pure rename, compiler-verified)

**Value:** High (long-term clarity and maintainability)
