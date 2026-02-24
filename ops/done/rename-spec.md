# Rename "Spec" to "Plan"

**Date**: 2026-02-21
**Status**: Draft
**Purpose**: Rename `aiki spec` → `aiki plan`, and add `aiki spec` as an alias. After this rename, the primary command is `aiki plan` with `spec` as a deprecated alias.

**Compatibility**: Breaking migration with backward compatibility alias.

---

## Executive Summary

The current `aiki spec` creates what users think of as a "plan". This rename changes the command to match user expectations: `aiki spec` becomes `aiki plan`, and we add `aiki spec` as a deprecated alias for backward compatibility.

---

## Terminology Mapping

| Current | New | What It Is |
|---------|-----|------------|
| `aiki spec` (command) | `aiki plan` | Interactive plan authoring with AI |
| `spec` (task type) | `plan` | Task type for plan authoring sessions |
| `aiki/spec` (template) | `aiki/plan` | Template for plan authoring |
| `aiki/review/spec` (template) | `aiki/review/plan` | Template for reviewing a plan document |
| `data.spec` / `data.spec_path` | `data.plan` / `data.plan_path` | Data attributes referencing the plan file |
| `ReviewScopeKind::Spec` | `ReviewScopeKind::Plan` | Review scope for plan documents |

The `aiki spec` command remains as a deprecated alias that maps to `aiki plan`.

---

## User Experience

### Commands After Rename

```bash
# Author a plan (interactive session with AI) - PRIMARY COMMAND
aiki plan feature.md
aiki plan feature.md "add JWT auth"
aiki plan "Add user authentication"

# Backward compatibility alias (deprecated)
aiki spec feature.md

# Review a plan document
aiki review --plan ops/now/feature.md
```

### Help Text

```
aiki plan     Interactive plan authoring with AI agent
aiki spec     (deprecated alias for 'aiki plan')
```

---

## How It Works

### Rename Order (to avoid conflicts)

**Templates:**
1. `.aiki/templates/aiki/spec.md` → `.aiki/templates/aiki/plan.md`
2. `.aiki/templates/aiki/review/spec.md` → `.aiki/templates/aiki/review/plan.md`

**Source files:**
1. `cli/src/commands/spec.rs` → `cli/src/commands/plan.rs`

### Template Changes

**`plan.md` (was `spec.md`):**
- Front matter: `type: plan` (was `type: spec`)
- Heading: `# Plan: {{data.plan_path}}` (was `# Spec: {{data.spec_path}}`)
- All `{{data.spec_path}}` → `{{data.plan_path}}`
- "spec" → "plan" in all prose
- "spec document" → "plan document", "spec file" → "plan file"
- Subtask names: keep as-is (still applicable)

**`review/plan.md` (was `review/spec.md`):**
- "spec" → "plan" throughout

**`review/implementation.md`:**
- `# Understand the implementation of spec` → `# Understand the implementation of plan`
- `{% subtask aiki/review/spec %}` → `{% subtask aiki/review/plan %}`
- "spec" → "plan" in review criteria

---

### Source Code Changes

**`plan.rs` (was `spec.rs`):**
- Rename internal references from "spec" to "plan"
- `SpecMode` → `PlanMode` (or equivalent enum)
- `create_spec_task` → `create_plan_task`
- `run_spec` → `run_plan`
- `data.spec_path` → `data.plan_path`
- Default template: `"aiki/spec"` → `"aiki/plan"`
- Help text: "Interactive spec authoring" → "Interactive plan authoring"

**`main.rs`:**
- Keep both `Spec { ... }` and `Plan { ... }` variants
- `Spec { args, .. }` dispatches to `commands::plan::run(args, ..)` (alias)
- `Plan { args, .. }` dispatches to `commands::plan::run(args, ..)` (primary)
- Add deprecation notice to `Spec` variant

**`commands/mod.rs`:**
- `pub mod spec;` → `pub mod plan;`

**`review.rs`:**
- References to "spec" scope → "plan" scope
- `ReviewScopeKind::Spec` → `ReviewScopeKind::Plan`
- Default review template for plan files: `"aiki/review/plan"`

**`fix.rs`:**
- `ReviewScopeKind::Spec` → `ReviewScopeKind::Plan` in match arms

**`output.rs`:**
- Any references to "spec" in output formatting → "plan"

**`error.rs`:**
- Valid review scope list: `'spec'` → `'plan'`

**`session/mod.rs`:**
- Doc comments: `aiki spec` → `aiki plan`

### Complete File Inventory

Every file that needs changes, organized by category.

---

#### Templates (3 files)

**`.aiki/templates/aiki/spec.md` → rename to `plan.md`:**
- Front matter: `type: spec` → `type: plan`
- Heading: `# Spec: {{data.spec_path}}` → `# Plan: {{data.plan_path}}`
- `{{data.spec_path}}` → `{{data.plan_path}}`
- "spec" → "plan" in all prose (~30 occurrences)
- "spec document" → "plan document", "spec file" → "plan file"

**`.aiki/templates/aiki/review/spec.md` → rename to `review/plan.md`:**
- "spec" → "plan" throughout

**`.aiki/templates/aiki/review/implementation.md`:**
- `# Understand the implementation of spec` → `# Understand the implementation of plan`
- `{% subtask aiki/review/spec %}` → `{% subtask aiki/review/plan %}`
- "spec" → "plan" in review criteria (multiple occurrences)

---

#### CLI Source — Commands (6 files)

**`cli/src/commands/spec.rs` → rename to `plan.rs` (~96 spec references):**
- Module doc: "Spec command" → "Plan command"
- `SpecMode` → `PlanMode`
- `SpecMode::Edit` → `PlanMode::Edit`
- `SpecMode::CreateAtPath` → `PlanMode::CreateAtPath`
- `SpecMode::Autogen` → `PlanMode::Autogen`
- `determine_mode()` error: "No spec path" → "No plan path"
- `run_spec()` → `run_plan()`
- `create_spec_task()` → `create_plan_task()`
- `output_spec_started()` → `output_plan_started()`
- `output_spec_completed()` → `output_plan_completed()`
- `output_spec_error()` → `output_plan_error()`
- `"Spec: {}"` format strings → `"Plan: {}"`
- Task type filter: `Some("spec")` → `Some("plan")`
- Default template: `"aiki/spec"` → `"aiki/plan"`
- `task_type: Some("spec".to_string())` → `Some("plan".to_string())`
- `variables.set_data("spec_path", ...)` → `variables.set_data("plan_path", ...)`
- `MdBuilder::new("spec")` → `MdBuilder::new("plan")`
- `"## Spec Started"` → `"## Plan Started"`
- `"## Spec Completed"` → `"## Plan Completed"`
- `"Spec task {}: {}"` → `"Plan task {}: {}"`
- **Tests:** All test references to `SpecMode`, spec paths, etc. (~10 tests)

**`cli/src/main.rs`:**
- Add new `Plan { ... }` variant alongside existing `Spec { ... }` variant
- Both variants have identical structure and arguments
- `Spec { args, .. }` → dispatch to `commands::plan::run(args, ..)` with deprecation warning
- `Plan { args, .. }` → dispatch to `commands::plan::run(args, ..)`
- Help text for `Plan`: "Interactive plan authoring with AI agent"
- Help text for `Spec`: "(deprecated alias for 'aiki plan')"

**`cli/src/commands/mod.rs`:**
- `pub mod spec;` → `pub mod plan;`

**`cli/src/commands/review.rs`:**
- `ReviewScopeKind::Spec` → `ReviewScopeKind::Plan`
- `"spec"` string → `"plan"` in `as_str()`
- `from_str("spec")` → `from_str("plan")`
- `ReviewScopeKind::Spec` display: `"Spec ({})"` → `"Plan ({})"`
- Default review template: `"aiki/review/spec"` → `"aiki/review/plan"`
- **Tests:** Update test names and expected strings

**`cli/src/commands/fix.rs`:**
- `ReviewScopeKind::Spec` → `ReviewScopeKind::Plan` in match arms
- **Tests:** Update test expected strings

**`cli/src/commands/output.rs`:**
- **Tests:** Update test expected strings for scope type

**`cli/src/commands/task.rs`:**
- Help text: `"Spec file this task implements"` → `"Plan file this task implements"`

**`cli/src/commands/agents_template.rs`:**
- Link example: `--implements ops/now/spec.md` → `ops/now/plan.md`
- Link description: `implements | plan → spec` → update

---

#### CLI Source — Other (2 files)

**`cli/src/error.rs`:**
- `"Unknown review scope type: '{0}'. Valid values: 'task', 'spec', 'implementation', 'session'"` → `'plan'` instead of `'spec'`

**`cli/src/session/mod.rs`:**
- Doc comments: `aiki spec` → `aiki plan` (6 occurrences)

---

#### CLI Source — Tasks (1 file)

**`cli/src/tasks/graph.rs`:**
- `data.get("spec")` → `data.get("plan")`
- Variables: `spec` → `plan`
- `Some("spec") | Some("implementation")` → `Some("plan") | Some("implementation")`
- **Tests:** Update test data keys and task types

---

#### Integration Tests (1 file)

**`cli/tests/test_task_events.rs`:**
- Review test fixture paths (may be fine as-is if they're just examples)

---

#### Documentation (8 files)

**`ops/now/polish-workflow-commands-ux.md`:**
- `aiki spec` → `aiki plan` (~50 occurrences)
- `spec.rs` filename → `plan.rs` (~10 occurrences)
- "Spec:" prefixes in output → "Plan:"
- "spec session", "spec task", "spec prompt" → "plan" equivalents

**`ops/now/tui.md`:**
- `aiki spec` → `aiki plan`
- `type: spec` → `type: plan`
- "Spec sessions", "spec files", "Specs column"

**`ops/now/workflow-hook-commands.md`:**
- `spec:` flow action → `plan:`
- `spec.completed` event → `plan.completed`
- All example YAML configs

**`ops/now/spec-file-frontmatter.md`:**
- Title: "spec file" → "plan file"
- `aiki spec` → `aiki plan`
- `spec.rs` → `plan.rs`
- All `data.spec` references → `data.plan`

**`ops/now/instructions-cli.md`:**
- References to spec command and concepts

**`ops/now/the-aiki-way.md`:**
- Command references

**`AGENTS.md`:**
- `--implements ops/now/spec.md` → `ops/now/plan.md`
- Link descriptions

**`cli/src/CLAUDE.md`:**
- Same as AGENTS.md

---

## Implementation Plan

### Phase 1: Templates

1. Rename `spec.md` → `plan.md`, update content
2. Rename `review/spec.md` → `review/plan.md`, update content
3. Update `review/implementation.md` subtask ref and prose

### Phase 2: CLI Source — Commands

4. Rename `spec.rs` → `plan.rs`, update all internal references + tests
5. Update `mod.rs` exports
6. Update `main.rs`: add `Plan` command, keep `Spec` as alias with deprecation
7. Update `review.rs` — `ReviewScopeKind::Spec` → `Plan` + tests
8. Update `fix.rs` — scope kind references + tests
9. Update `output.rs` — scope kind references + tests
10. Update `task.rs` — help text for `--implements`
11. Update `agents_template.rs` — link examples

### Phase 3: CLI Source — Other

12. Update `error.rs` — valid scope type list
13. Update `session/mod.rs` — doc comments
14. Update `tasks/graph.rs` — data key reads + tests

### Phase 4: Documentation

15. Update `ops/now/polish-workflow-commands-ux.md`
16. Update `ops/now/tui.md`
17. Update `ops/now/workflow-hook-commands.md`
18. Update `ops/now/spec-file-frontmatter.md`
19. Update `ops/now/instructions-cli.md`
20. Update `ops/now/the-aiki-way.md`
21. Update `AGENTS.md` and `cli/src/CLAUDE.md`

### Phase 5: Validation

22. Grep for stale `"spec"` references in CLI source (excluding ops/done/, false positives like "specific", "specify", "inspect")
23. `cargo test` — ensure all tests pass
24. `cargo build` — ensure clean compile
25. Manual smoke test: `aiki plan`, `aiki spec` (alias)

---

## Notes

- The `aiki spec` command remains available as a deprecated alias
- Over time, we can add a deprecation warning when users invoke `aiki spec`
- Documentation should primarily reference `aiki plan`, with notes about the `spec` alias
- Historical documents in `ops/done/` should NOT be changed

## Open Questions

1. **Shell completions** — Do we generate shell completions that need updating?
2. **`spec-file-frontmatter.md` `plan_task` field** — The frontmatter field name `plan_task` (which stores the epic task ID) should be renamed to `epic_task` for consistency. But this spec hasn't been implemented yet — if it's still a draft, just rename in the spec.
