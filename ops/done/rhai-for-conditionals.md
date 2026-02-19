# Unified Expression Evaluation with Rhai

**Status**: Done
**Prerequisite for**: `loop-flags.md` (requires array indexing `subtasks[2].approved`)

## Problem

We have **three different conditional evaluation systems** in aiki:

1. **Hooks** (`cli/src/flows/engine.rs`): String-based parser, `$var` syntax, `&&`/`||` operators
2. **Task Templates** (`cli/src/tasks/templates/conditionals.rs`): Full AST parser, `{{var}}` syntax, `and`/`or` operators
3. **Loop frontmatter** (coming in `loop-flags.md`): Needs `subtasks[2].approved || data.loop.index1 >= 5`

This creates:
- **Inconsistent syntax** across systems (`&&` vs `and`, `$var` vs `{{var}}`)
- **Missing features** (no array indexing, limited field access)
- **Maintenance burden** (~1000 LOC of custom parser code)
- **Performance issues** (no compile-once-eval-many)

## Solution

Adopt **Rhai** (https://rhai.rs) for unified expression evaluation across all systems.

### Why Rhai?

| Feature | Rhai | Current Custom |
|---------|------|----------------|
| Dotted field access (`event.task.type`) | ✓ | Partial |
| Array indexing (`subtasks[2].approved`) | ✓ | ✗ (blocks loop-flags.md) |
| Map/object literals | ✓ | ✗ |
| Numeric comparison (`x > 7`) | ✓ | ✓ |
| String comparison | ✓ | ✓ |
| Logical operators (`&&`, `||`, `!`) | ✓ | ✓ (inconsistent syntax) |
| Compile once, eval many | ✓ | ✗ |
| Type coercion | Lenient (returns false) | Mixed (hooks strict, templates lenient) |

**Performance**: 1M iterations in 0.14s, ~2x slower than Python (acceptable for our use case)

**Dependencies**: 8 lightweight crates (ahash, smartstring, etc.), well-maintained (1.5K+ GitHub stars)

## Migration Strategy

**Principle**: Migrate existing functionality first, then enable new features (loop frontmatter).

### Phase 1: Add Rhai Infrastructure (✓ Non-breaking)

**Goal**: Add Rhai dependency and create abstraction layer without changing any behavior.

**Tasks**:
1. Add `rhai = "1.24"` to `cli/Cargo.toml`
2. Create `cli/src/expressions/mod.rs` with unified evaluator
3. Add tests for expression evaluation (existing test cases)
4. Document Rhai expression syntax

**Files**:
- `cli/Cargo.toml` (add dependency)
- `cli/src/expressions/mod.rs` (new module)
- `cli/src/expressions/tests.rs` (new tests)

**Acceptance**:
- `cargo test -p aiki` passes (full test suite, no regressions)
- New module has 100% test coverage for core operations
- Documentation exists for supported syntax

### Phase 2: Migrate Hooks (⚠️ Breaking - requires migration)

**Goal**: Replace string-based hook condition parser with Rhai.

**Current behavior** (hooks use):
```yaml
- if: $event.task.type == "review"
  then: [...]
```

**New behavior** (Rhai-based):
```yaml
- if: event.task.type == "review"  # No $ prefix needed
  then: [...]
```

**Migration**:
1. Update hook condition evaluator to use Rhai
2. Add backwards compatibility: strip `$` prefix if present (deprecated)
3. Update all `.aiki/hooks/**/*.yml` files to new syntax
4. Update hook documentation
5. Add deprecation warning for `$var` syntax

**Files**:
- `cli/src/flows/engine.rs` (replace `evaluate_condition`, `resolve_condition_value`, `compare_numeric`)
- `.aiki/hooks/**/*.yml` (update all hooks in repo)
- `HOOKS.md` (update documentation)

**Breaking changes**:
- Old syntax: `$event.task.type` → New syntax: `event.task.type`
- Old operators: Still work (`&&`, `||`, `!` are Rhai defaults)
- Type coercion: Changes from strict (error on type mismatch) to lenient (false on type mismatch)

**Acceptance**:
- `cargo test -p aiki` passes (full test suite, no regressions)
- `cargo test -p aiki -- hook` passes (hook-specific tests)
- User hooks show deprecation warning if using `$var` syntax
- All aiki-provided hooks use new syntax

### Phase 3: Migrate Task Templates (⚠️ Breaking - requires careful handling)

**Goal**: Replace custom AST parser in task templates with Rhai for `{% if %}` conditions.

**Current behavior**:
```markdown
{% if data.needs_review %}
Review required.
{% elif data.needs_tests %}
Tests required.
{% else %}
No action needed.
{% endif %}
```

**New behavior** (same surface syntax, Rhai backend):
```markdown
{% if data.needs_review %}
Review required.
{% elif data.needs_tests %}
Tests required.
{% else %}
No action needed.
{% endif %}
```

**Migration**:
1. Keep template parsing (`{% if %}` block detection)
2. Replace condition evaluation with Rhai
3. Keep variable substitution (`{{var}}`) separate from conditions
4. Update operators: `and`/`or`/`not` → `&&`/`||`/`!` (or support both)
5. If supporting both: rewrite `and`/`or`/`not` → `&&`/`||`/`!` as whole-word token replacement before Rhai evaluation (avoid substring matches like `"band"` → `"b&&"`)
6. Test all existing templates
7. Add mixed-syntax coexistence tests (see Testing Strategy)

**Files**:
- `cli/src/tasks/templates/conditionals.rs` (replace `evaluate_condition`, keep `parse`/`render`)
- `.aiki/templates/**/*.md` (audit and update if needed)

**Breaking changes**:
- Operators: `and`/`or`/`not` → `&&`/`||`/`!` (can support both with minimal overhead)
- Type coercion: Lenient (returns false) vs current lenient (same, no change)

**Acceptance**:
- `cargo test -p aiki` passes (full test suite, no regressions)
- `cargo test -p aiki -- template` passes (template-specific tests)
- All built-in templates work correctly
- Template documentation updated

### Phase 4: Enable New Features (✓ Non-breaking)

**Goal**: Use Rhai features that weren't possible before.

**New capabilities**:
1. Array indexing: `subtasks[2].approved` (unblocks loop-flags.md)
2. Complex field access: `event.task.data.scope.kind`
3. Inline maps: `#{key: value}.key`
4. Compile-once-eval-many for loop conditions

**Where used**:
- Loop frontmatter `until:` conditions (loop-flags.md Phase 2)
- Hook conditions with complex data structures
- Template conditions with array/map access

**Files**:
- Documentation: Add examples of new syntax
- Tests: Add coverage for array/map operations
- `cli/src/expressions/mod.rs` - Add `build_scope_with_typed_values` (or extend `build_scope_from_flat`) to accept typed data (arrays, nested objects) alongside flat string maps

**Acceptance**:
- Array indexing works: `subtasks[2].approved == true`
- Nested map access works: e.g., `data.config.threshold >= 5`
- Loop frontmatter can use new syntax (prerequisite satisfied)

**Variable plumbing gap**: The current scope builder (`build_scope_from_flat`) only accepts `BTreeMap<String, String>` — flat key-value string pairs. Array indexing (e.g., `subtasks[2].approved`) requires Rhai Array values in scope, which can't be represented as flat strings. Similarly, hook variable context (`collect_variables` in `flows/variables.rs`) and template variable context (`VariableContext` in `tasks/templates/variables.rs`) store only `HashMap<String, String>`. To support typed values:
1. Add a `build_scope_with_json` function (or similar) that converts `serde_json::Value` trees into Rhai `Dynamic` values (arrays → Rhai Array, objects → Rhai Map)
2. Extend the variable contexts to carry typed data alongside flat strings where needed (e.g., `subtasks` array from task state)
3. The calling code (loop-flags evaluation, hook conditions with structured data) must populate typed values before building the scope

This is the responsibility of loop-flags.md and any future feature that needs typed variable access — not of this spec. The Rhai engine already handles array/map operations natively once the values are in scope.

**Note on loop metadata namespaces**: Template loops (`<!-- AIKI_LOOP -->`) expose `loop.index` (1-based), `loop.index0`, `loop.first`, `loop.last`, `loop.length` in the template EvalContext. Task iteration loops (from `loop-flags.md`) will expose `data.loop.index` (0-based), `data.loop.index1` (1-based), `data.loop.first`, `data.loop.last`, `data.loop.length` via the task `data` map. These are separate systems — template loop metadata does not use the `data.` prefix.

## Implementation Plan

### Step 1: Phase 1 - Infrastructure (Week 1)

**Deliverables**:
- [ ] Add Rhai dependency
- [ ] Create `cli/src/expressions/mod.rs` with `ExpressionEvaluator`
- [ ] Add comprehensive tests (numeric, string, logical, field access)
- [ ] Document Rhai expression syntax in `EXPRESSIONS.md`

**Estimated effort**: 4-6 hours

### Step 2: Phase 2 - Migrate Hooks (Week 1-2)

**Deliverables**:
- [ ] Replace hook condition evaluator with Rhai
- [ ] Add `$var` → `var` backwards compat (with deprecation warning)
- [ ] Update all `.aiki/hooks/**/*.yml` in repo
- [ ] Update `HOOKS.md` documentation
- [ ] Test all hooks (unit + integration)

**Estimated effort**: 6-8 hours

**Migration guide for users**:
```diff
# Old syntax (deprecated)
- if: $event.task.type == "review"

# New syntax
- if: event.task.type == "review"
```

### Step 3: Phase 3 - Migrate Task Templates (Week 2)

**Deliverables**:
- [ ] Replace condition evaluator in `conditionals.rs`
- [ ] Support both `and`/`or` and `&&`/`||` (transition period)
- [ ] Test all built-in templates
- [ ] Update template documentation
- [ ] Add migration guide

**Estimated effort**: 6-8 hours

**Migration guide for users**:
```diff
# Old syntax (still works)
{% if data.needs_review and data.priority > 5 %}

# New syntax (preferred)
{% if data.needs_review && data.priority > 5 %}
```

### Step 4: Phase 4 - Enable New Features (Week 2)

**Deliverables**:
- [ ] Document array indexing syntax
- [ ] Document map access syntax
- [ ] Add examples to `EXPRESSIONS.md`
- [ ] Test complex expressions
- [ ] Mark loop-flags.md prerequisite as satisfied

**Estimated effort**: 2-4 hours

## Files Changed

### New Files
- `cli/src/expressions/mod.rs` - Rhai evaluator wrapper (~100 LOC)
- `cli/src/expressions/tests.rs` - Comprehensive tests (~200 LOC)
- `EXPRESSIONS.md` - Expression syntax documentation

### Modified Files
- `cli/Cargo.toml` - Add Rhai dependency
- `cli/src/flows/engine.rs` - Replace condition evaluator (~200 LOC deleted, ~50 LOC added)
- `cli/src/tasks/templates/conditionals.rs` - Replace evaluator (~100 LOC deleted, ~30 LOC added)
- `.aiki/hooks/**/*.yml` - Update syntax (remove `$` prefix)
- `.aiki/templates/**/*.md` - Update operators (optional, both work)
- `HOOKS.md` - Update documentation (deferred: file does not exist yet; hook syntax is documented in `EXPRESSIONS.md`)
- `TEMPLATES.md` - Update documentation (deferred: file does not exist yet; template syntax is documented in `EXPRESSIONS.md`)

### Deleted Code
- ~200 LOC in `cli/src/flows/engine.rs` (condition parser)
- ~100 LOC in `cli/src/tasks/templates/conditionals.rs` (evaluator only, keep parser/renderer)

**Net change**: ~1000 LOC deleted, ~400 LOC added (including tests and docs)

## Testing Strategy

### Unit Tests
- [x] Rhai numeric comparisons (`x > 7`)
- [x] Rhai string comparisons (`status == "done"`)
- [x] Rhai logical operators (`x > 5 && approved`)
- [x] Rhai field access (`event.task.type`)
- [x] Rhai array indexing (`subtasks[2].approved`) — tested with typed scope; flat-string plumbing deferred to loop-flags.md
- [x] Rhai compile-once-eval-many — tested via evaluator reuse; AST caching optimization deferred (current perf is acceptable)
- [ ] Hook condition migration (backwards compat)
- [ ] Template condition migration (both syntaxes)

### Integration Tests
- [ ] All existing hook tests pass
- [ ] All existing template tests pass
- [ ] Session hooks work correctly
- [ ] Task templates render correctly
- [ ] Loop frontmatter works (after loop-flags.md)

### Compatibility Tests
- [ ] Old hook syntax (`$var`) shows deprecation warning
- [ ] Old template syntax (`and`/`or`) still works
- [ ] Type mismatches handled gracefully (lenient)

### Mixed-Syntax Coexistence Tests

During the transition period, old and new syntaxes coexist. These tests ensure no regressions when both forms are present in the same repo or template:

- [ ] Template using `and`/`or` evaluates identically to one using `&&`/`||` (same inputs → same outputs)
- [ ] Template mixing `and` with `||` in the same expression is rejected or handled deterministically (define precedence rule)
- [ ] Hook using `$var` syntax alongside hooks using bare `var` syntax in the same `.yml` file both evaluate correctly
- [ ] Pre-migration templates (pure `and`/`or`) still render correctly after Rhai backend is active
- [ ] Pre-migration hooks (pure `$var`) still evaluate correctly after Rhai backend is active
- [ ] Deprecation warnings fire for old syntax but do not alter evaluation results

**Precedence rule for mixed operators**: If `and`/`or` are supported via pre-processing (rewriting to `&&`/`||` before Rhai evaluation), define and test the rewriting order to prevent ambiguity (e.g., `"band"` should not be rewritten to `"b&&"`, `"orange"` should not be rewritten to `"||ange"`). Recommended: only rewrite whole-word `and`/`or`/`not` tokens, not substrings.

## Breaking Changes & Migration

### For Hook Authors

**Breaking**: Variable syntax changes from `$var` to `var`

**Migration**:
```diff
# Before
- if: $event.task.type == "review"
  then:
    - log: "Review task"

# After
- if: event.task.type == "review"
  then:
    - log: "Review task"
```

**Timeline**: 
- Phase 2: `$var` syntax deprecated with warning
- 2 releases later: `$var` syntax removed

### For Template Authors

**Breaking**: Operators change from `and`/`or`/`not` to `&&`/`||`/`!`

**Migration** (both supported during transition):
```diff
# Before
{% if data.needs_review and data.priority > 5 %}

# After (preferred)
{% if data.needs_review && data.priority > 5 %}
```

**Timeline**: 
- Phase 3: Both syntaxes work
- Future release: Deprecate `and`/`or` with warning
- Later release: Remove `and`/`or` support

### Type Coercion Changes

**Hooks**: Changes from strict (error) to lenient (false)

**Before**:
```yaml
# This would ERROR if x is not a number
- if: x > 7
```

**After**:
```yaml
# This returns false if x is not a number (no error)
- if: x > 7
```

**Impact**: Reduces errors, but may hide bugs. Document recommended patterns.

**Diagnostic logging**: When a condition evaluation fails (Rhai parse/eval error), the evaluator emits a warning via `eprintln!("[aiki] Warning: condition evaluation failed (defaulting to false): ...")` with the expression text and error details. This preserves lenient semantics (conditions still default to false) while making failures visible to users for diagnosis.

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking user hooks | High | Deprecation period, clear migration guide, auto-migration tool |
| Breaking user templates | Medium | Support both syntaxes during transition |
| Type coercion bugs | Medium | Comprehensive testing, document edge cases |
| Performance regression | Low | Benchmark before/after, Rhai is fast enough |
| Dependency bloat | Low | Rhai is lightweight (8 deps, all small) |

## Success Metrics

- [ ] `cargo test -p aiki` passes (full test suite, no regressions)
- [ ] Zero regressions in hook/template behavior
- [ ] New features work: array indexing, complex field access
- [ ] Performance: <10% regression in condition evaluation
- [ ] Code reduction: ~600+ LOC net deletion
- [ ] Prerequisite satisfied: loop-flags.md can proceed

## Rollout Plan

### Release 1: Infrastructure + Hooks Migration
- Phase 1: Add Rhai infrastructure
- Phase 2: Migrate hooks (with deprecation warnings)
- Release notes: Announce `$var` deprecation, new syntax

### Release 2: Templates Migration + New Features
- Phase 3: Migrate templates (support both syntaxes)
- Phase 4: Enable new features (array indexing, etc.)
- Release notes: Announce new capabilities, loop frontmatter coming

### Release 3+: Cleanup
- Remove deprecated `$var` syntax
- Optionally remove `and`/`or` syntax (keep for longer if desired)
- Full documentation update

## Open Questions

1. **Deprecation timeline**: How many releases before removing `$var` syntax?
   - **Recommendation**: 2 releases (warn in R1, remove in R3)

2. **Template syntax**: Support both `and`/`or` and `&&`/`||` forever, or deprecate?
   - **Recommendation**: Support both indefinitely (low cost, high compatibility)

3. **Type coercion**: Strict mode option for hooks that want errors on type mismatch?
   - **Recommendation**: No, keep it simple. Document lenient behavior.

4. **Custom functions**: Allow users to define functions like `contains()`, `matches()`?
   - **Recommendation**: Future enhancement, not needed for MVP

## Dependencies

**Blocks**:
- `loop-flags.md` Phase 2 (requires array indexing: `subtasks[2].approved`)

**Blocked by**:
- None (can start immediately)

## References

- Rhai documentation: https://rhai.rs/book/
- Rhai expression-only mode: https://rhai.rs/book/engine/expressions.html
