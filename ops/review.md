Findings

All issues have been addressed:

## Round 1 Fixes

- [FIXED] Critical: Flow composition wasn't invoked anywhere in runtime.
  - Fix: Added `execute_core_flow()` helper in `cli/src/events/prelude.rs` that uses FlowComposer
  - Updated all 16 event handlers to use `execute_core_flow()` instead of direct `FlowEngine::execute_statements()`
  - FlowComposer is now the entry point for flow execution, with fallback to bundled core flow

- [FIXED] High: Variable isolation was not implemented.
  - Fix: Added `clear_variables()` method to `AikiState` (`cli/src/flows/state.rs:173-176`)
  - FlowComposer now calls `state.clear_variables()` before executing each flow's actions (`cli/src/flows/composer.rs:249-250`)
  - Each flow gets a fresh variable context while sharing event state (ContextAssembler)

- [FIXED] High: self.* resolution used display name instead of flow path.
  - Fix: Added `extract_flow_identifier()` method to `FlowComposer` (`cli/src/flows/composer.rs:346-375`)
  - Extracts flow identifier from canonical path (e.g., `/project/.aiki/flows/aiki/quick-lint.yml` → `aiki/quick-lint`)
  - FlowComposer now uses `extract_flow_identifier(canonical_path)` instead of `flow.name`

## Round 2 Fixes

- [FIXED] High: execute_core_flow fell back to bundled core on any FlowNotFound, including missing dependencies.
  - Fix: Now only falls back if the error's `path` field equals "aiki/core" (`cli/src/events/prelude.rs:43-56`)
  - Missing dependencies (before/after flows) now correctly propagate errors to the user
  - Prevents partially mutated AikiState from being used with bundled core

- [FIXED] High: Variable isolation only applied before main statements, not before-flows.
  - Fix: Added `clear_variables()` at the start of `execute_composed_flow()` (`cli/src/flows/composer.rs:214-217`)
  - Before flows now start with a fresh variable context (can't see caller's variables)
  - Existing clear before main statements ensures they don't see before-flows' variables
  - Each flow in the composition chain now gets proper isolation
