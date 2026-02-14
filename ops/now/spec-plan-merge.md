# Rename Spec/Plan/Epic Terminology

**Date**: 2026-02-14
**Status**: Draft
**Purpose**: Rename `aiki spec` → `aiki plan`, `aiki plan` → `aiki decompose`, and the task created by decompose → "epic" for clearer nomenclature.

**Compatibility**: Breaking migration. There is no backward compatibility in this plan.

---

## Executive Summary

The current naming is confusing: `aiki spec` creates what users think of as a "plan", and `aiki plan` breaks that plan into subtasks — which is really "decomposition". The task that `aiki plan` outputs is called a "plan task", but it's really an epic (a container of implementation subtasks). This spec renames everything to match what it actually does.

---

## Terminology Mapping

| Current | New | What It Is |
|---------|-----|------------|
| `aiki spec` (command) | `aiki plan` | Interactive plan authoring with AI |
| `aiki plan` (command) | `aiki decompose` | Break a plan into an epic with subtasks |
| `spec` (task type) | `plan` | Task type for plan authoring sessions |
| `plan` (task type, ephemeral) | `decompose` | Task type for the decomposition process |
| "plan task" (output of decompose) | "epic" | Container task with implementation subtasks |
| `aiki/spec` (template) | `aiki/plan` | Template for plan authoring |
| `aiki/plan` (template) | `aiki/decompose` | Template for decomposition |
| `aiki/review/spec` (template) | `aiki/review/plan` | Template for reviewing a plan document |
| `data.spec` / `data.spec_path` | `data.plan` / `data.plan_path` | Data attributes referencing the plan file |

After the rename, `aiki spec` is removed.

---

## User Experience

### Commands After Rename

```bash
# Author a plan (interactive session with AI)
aiki plan feature.md
aiki plan feature.md "add JWT auth"
aiki plan "Add user authentication"

# Decompose a plan into an epic with subtasks
aiki decompose ops/now/feature.md

# Build (unchanged command name, but calls decompose internally)
aiki build ops/now/feature.md

# Review a plan document
aiki review --plan ops/now/feature.md
```

### Help Text

```
aiki plan    Interactive plan authoring with AI agent
aiki decompose    Create an epic from a plan file (implementation subtasks)
aiki build   Build from a plan file (decompose + execute all subtasks)
```

---

## How It Works

### Rename Order (to avoid conflicts)

Source files and templates have naming collisions (both `plan.rs` and `plan.md` exist today for different things). Renames must happen in this order:

**Templates:**
1. `.aiki/templates/aiki/plan.md` → `.aiki/templates/aiki/decompose.md`
2. `.aiki/templates/aiki/spec.md` → `.aiki/templates/aiki/plan.md`
3. `.aiki/templates/aiki/review/spec.md` → `.aiki/templates/aiki/review/plan.md`

**Source files:**
1. `cli/src/commands/plan.rs` → `cli/src/commands/decompose.rs`
2. `cli/src/commands/spec.rs` → `cli/src/commands/plan.rs`

### Template Changes

**`decompose.md` (was `plan.md`):**
- Front matter: `type: decompose` (was `type: plan`)
- Heading: `# Decompose: {{data.plan}}` (was `# Plan: {{data.spec}}`)
- All `{{data.spec}}` → `{{data.plan}}`
- "plan task" → "epic" in instructions
- `aiki task add "Plan: <title>"` → `aiki task add "Epic: <title>"`
- "Plan created with N subtasks" → "Epic created with N subtasks"

**`plan.md` (was `spec.md`):**
- Front matter: `type: plan` (was `type: spec`)
- Heading: `# Plan: {{data.plan_path}}` (was `# Spec: {{data.spec_path}}`)
- All `{{data.spec_path}}` → `{{data.plan_path}}`
- "spec" → "plan" in instructions and subtask descriptions
- Suggested structure section: keep as-is (it describes plan document format)

**`review/plan.md` (was `review/spec.md`):**
- "spec" → "plan" throughout

**`build.md`:**
- `{% subtask aiki/plan if not data.plan %}` → `{% subtask aiki/decompose if not data.epic %}`
- `{{data.spec}}` → `{{data.plan}}`
- `data.plan` → `data.epic` (the epic task ID)
- "plan subtasks" → "epic subtasks"

### Source Code Changes

**`decompose.rs` (was `plan.rs`):**
- Rename struct `PlanArgs` → `DecomposeArgs`
- Default template: `"aiki/plan"` → `"aiki/decompose"`
- `data.spec` → `data.plan` in task creation
- "plan" → "epic" when referring to the output task
- "planning task" → "decompose task" for the ephemeral task
- Help text: "Create an implementation plan" → "Create an epic from a plan file"

**`plan.rs` (was `spec.rs`):**
- Rename internal references from "spec" to "plan"
- `SpecMode` → `PlanMode` (or equivalent enum)
- `create_spec_task` → `create_plan_task`
- `run_spec` → `run_plan`
- `data.spec_path` → `data.plan_path`
- `data.is_new` stays (still tracks new vs existing)
- `data.initial_idea` stays (still the user's input)
- Default template: `"aiki/spec"` → `"aiki/plan"`
- Help text: "Interactive spec authoring" → "Interactive plan authoring"

**`build.rs`:**
- References to `plan` (the old command) → `decompose`
- `data.spec` → `data.plan`
- `data.plan` (old, meaning plan task ID) → `data.epic`
- Calls to `commands::plan::run()` → `commands::decompose::run()`
- Help text: "Build from a spec file" → "Build from a plan file"

**`main.rs`:**
- `Spec { ... }` variant is removed
- `Plan { ... }` remains the public plan command
- `Plan(PlanArgs)` → `Decompose(DecomposeArgs)`
- Dispatch: `Plan { args, .. } => commands::plan::run(args, ..)`
- Dispatch: `Decompose(args) => commands::decompose::run(args)`

**`commands/mod.rs`:**
- `pub mod spec;` → `pub mod plan;`
- `pub mod plan;` → `pub mod decompose;`

**`review.rs`:**
- References to "spec" scope → "plan" scope
- `ReviewScope::Spec` → `ReviewScope::Plan` (if such enum exists)
- Default review template for plan files: `"aiki/review/plan"`

**`output.rs`:**
- Any references to "spec" in output formatting → "plan"

### Complete File Inventory

Every file that needs changes, organized by category. Each entry lists exact lines/symbols affected.

---

#### Templates (5 files)

**`.aiki/templates/aiki/plan.md` → rename to `decompose.md`:**
- Front matter: `type: plan` → `type: decompose`
- Heading: `# Plan: {{data.spec}}` → `# Decompose: {{data.plan}}`
- All `{{data.spec}}` → `{{data.plan}}` (lines 6, 8, 12, 25, 28)
- `"Plan: <spec title>"` → `"Epic: <plan title>"` (line 25)
- `--data spec={{data.spec}}` → `--data plan={{data.plan}}` (line 28)
- `--source file:{{data.spec}}` → `--source file:{{data.plan}}` (line 27)
- "plan task" → "epic" in prose (lines 8, 22, 27, 29, 31)
- "spec file" / "spec title" → "plan file" / "plan title" (lines 12, 33)
- "Plan created with N subtasks. Plan ID:" → "Epic created with N subtasks. Epic ID:" (line 55)
- "planning task" → "decompose task" (line 55)

**`.aiki/templates/aiki/spec.md` → rename to `plan.md`:**
- Front matter: `type: spec` → `type: plan`
- Heading: `# Spec: {{data.spec_path}}` → `# Plan: {{data.plan_path}}`
- `{{data.spec_path}}` → `{{data.plan_path}}` (lines 8, 14)
- `{{data.user_context}}` stays (still relevant)
- "spec" → "plan" in all prose (~30 occurrences across subtask descriptions)
- "spec document" → "plan document", "spec file" → "plan file"
- Subtask names: "Clarify user intent" etc. — keep as-is (still applicable)

**`.aiki/templates/aiki/review/spec.md` → rename to `review/plan.md`:**
- "spec" → "plan" throughout (lines 1, 3, 7, 8)
- `{{data.scope.id}}` stays (no change)

**`.aiki/templates/aiki/review/implementation.md`:**
- Line 1: `# Understand the implementation of spec` → `# Understand the implementation of plan`
- Line 3: `implementation of plan in {{data.scope.id}}` — "plan" here refers to the plan document, OK as-is
- Line 13: `{% subtask aiki/review/spec %}` → `{% subtask aiki/review/plan %}`
- Lines 24-41: "spec" → "plan" in review criteria ("requirements from the spec", "spec design", "spec's prescribed approach", "spec are met")

**`.aiki/templates/aiki/build.md`:**
- Line 8: `{{data.spec}}` → `{{data.plan}}`
- Line 18: `{% subtask aiki/plan if not data.plan %}` → `{% subtask aiki/decompose if not data.epic %}`
- Line 22: "plan task" → "epic" in prose
- Line 22: `{{parent.id}}.1` stays (subtask numbering unchanged)

---

#### CLI Source — Commands (9 files)

**`cli/src/commands/plan.rs` → rename to `decompose.rs` (~77 spec/plan references):**
- Module doc: "Plan command" → "Decompose command" (lines 1-7)
- `PlanSubcommands` → `DecomposeSubcommands` (line 32)
- `PlanArgs` → `DecomposeArgs` (line 42)
- `spec_path` field → `plan_path` (line 44)
- `"aiki/plan"` default template → `"aiki/decompose"` (line 159)
- `PlanChoice` enum → `DecomposeChoice` (line 87)
- `run_plan()` → `run_decompose()` (line 93)
- `validate_spec_path()` → `validate_plan_path()` (line 236)
- `find_plan_for_spec()` → `find_epic_for_plan()` (line 275)
- `find_created_plan()` → `find_created_epic()` (line 308)
- `close_plan()` → `close_epic()` (line 358)
- `create_planning_task()` → `create_decompose_task()` (line 385)
- `output_plan_created()` → `output_epic_created()` (line 511)
- `output_plan_resumed()` → `output_epic_resumed()` (line 527)
- `output_plan_show()` → `output_epic_show()` (line 557)
- `is_spec_path()` → `is_plan_path()` (line 631)
- All `data.get("spec")` → `data.get("plan")` (~10 occurrences)
- All `"spec"` string literals → `"plan"` in task data
- Task type filter: `Some("plan")` → `Some("decompose")` (lines 284, 298)
- Error messages: "Spec file must be markdown" → "Plan file must be markdown" etc.
- `MdBuilder::new("plan")` → `MdBuilder::new("decompose")` (lines 521, 551)
- `MdBuilder::new("plan-show")` → `MdBuilder::new("decompose-show")` (line 623)
- `<aiki_plan plan_id=...>` XML output → `<aiki_epic epic_id=...>` (lines 138, 193, 204)
- `variables.set_data("spec", ...)` → `variables.set_data("plan", ...)` (line 398)
- `sources.push(format!("file:{}", spec_path))` stays (still tracks source file)
- Link event: `"scoped-to"` predicate stays
- **Tests (lines 692-911):** All test function names and assertions referencing `spec`/`plan` need updating (~20 tests)

**`cli/src/commands/spec.rs` → rename to `plan.rs` (~96 spec references):**
- Module doc: "Spec command" → "Plan command" (lines 1-4)
- `SpecMode` → `PlanMode` (line 33)
- `SpecMode::Edit` → `PlanMode::Edit` (line 34)
- `SpecMode::CreateAtPath` → `PlanMode::CreateAtPath` (line 36)
- `SpecMode::Autogen` → `PlanMode::Autogen` (line 38)
- `determine_mode()` error: "No spec path" → "No plan path" (line 65)
- `run_spec()` → `run_plan()` (line 344)
- `create_spec_task()` → `create_plan_task()` (line 542)
- `output_spec_started()` → `output_plan_started()` (line 660)
- `output_spec_completed()` → `output_plan_completed()` (line 681)
- `output_spec_error()` → `output_plan_error()` (line 694)
- `"Spec: {}"` format strings → `"Plan: {}"` (lines 375, 377)
- Task type filter: `Some("spec")` → `Some("plan")` (line 409)
- `"aiki/spec"` default template → `"aiki/plan"` (line 434)
- `task_type: Some("spec".to_string())` → `Some("plan".to_string())` (line 643)
- `variables.set_data("spec_path", ...)` → `variables.set_data("plan_path", ...)` (line 562)
- `MdBuilder::new("spec")` → `MdBuilder::new("plan")` (lines 675, 688, 699)
- `"## Spec Started"` → `"## Plan Started"` (line 669)
- `"## Spec Completed"` → `"## Plan Completed"` (line 683)
- `"Spec task {}: {}"` → `"Plan task {}: {}"` (line 696)
- `"begin working on this spec task"` → `"begin working on this plan task"` (line 488)
- `"Spec session cancelled"` → `"Plan session cancelled"` (line 517)
- **Tests (lines 735-911):** All test references to `SpecMode`, spec paths, etc. (~10 tests)

**`cli/src/commands/build.rs` (~272 references, massive file):**
- Module doc: "spec file" → "plan file" (lines 1-6)
- `BuildSubcommands::Show { spec_path }` → `{ plan_path }` (line 32-33)
- `BuildArgs` target help: "Spec path or plan ID" → "Plan path or epic ID" (line 40)
- `BuildArgs` restart help: "Ignore existing plan" → "Ignore existing epic" (line 47)
- `BuildChoice` enum stays but docs change (line 97-103)
- `run_build_spec()` → `run_build_plan()` (line 112)
- `run_build_plan()` (current, builds from plan ID) → `run_build_epic()` (line 249)
- All `spec_path` params → `plan_path`
- All `plan_id` params → `epic_id`
- `validate_spec_path()` → `validate_plan_path()` (line 373)
- `find_plan_for_spec()` → `find_epic_for_plan()` (line 413)
- `cleanup_stale_builds()` spec references → plan (line 443-455)
- `undo_completed_subtasks()` doc comments (line 483-486)
- `close_plan()` → `close_epic()` (line 518)
- `create_build_task()`: `data.insert("spec", ...)` → `data.insert("plan", ...)`, `data.insert("plan", ...)` → `data.insert("epic", ...)` (lines 555-557)
- `output_build_started/completed/async/show()`: "Plan ID" → "Epic ID" in output (lines 641-684)
- `output_build_show()`: `plan` params → appropriate naming (line 684)
- `<aiki_build build_id="..." plan_id="...">` → `epic_id` (lines 200, 233, 312, 336)
- Link: `"orchestrates"` target changes from plan_id to epic_id (line 582)
- **Tests (lines 849-1323):** ~30 tests referencing `find_plan_for_spec`, `validate_spec_path`, `output_build_*`, task data with "spec"/"plan" keys

**`cli/src/commands/review.rs` (16 references):**
- `ReviewScopeKind::Spec` → `ReviewScopeKind::Plan` (line 31)
- `"spec"` string → `"plan"` in `as_str()` (line 41)
- `from_str("spec")` → `from_str("plan")` (line 51)
- `ReviewScopeKind::Spec` display: `"Spec ({})"` → `"Plan ({})"` (line 79)
- Line 111: comment about non-Session scopes (Spec → Plan)
- Line 163: doc comment about spec review
- Line 327: `ReviewScopeKind::Spec` match arm
- Line 415: `ReviewScopeKind::Spec | ReviewScopeKind::Implementation` match
- Default review template for plan files: `"aiki/review/spec"` → `"aiki/review/plan"` (find the reference)
- **Tests (lines 806-1004):** `test_scope_name_spec`, `test_scope_roundtrip_spec`, `test_detect_target_md_file_spec` — ~6 tests

**`cli/src/commands/fix.rs` (3 references):**
- Line 147: `ReviewScopeKind::Spec | ReviewScopeKind::Implementation` match
- Line 552-560: test `test_fix_description_spec_scope` — `ReviewScopeKind::Spec`, expected string `"Spec (feature.md)"` → `"Plan (feature.md)"`

**`cli/src/commands/output.rs` (3 references):**
- Line 107-123: test `test_format_with_spec_scope` — `ReviewScopeKind::Spec`, expected strings `"- **Type:** spec"`, `"- **Scope:** Spec (feature.md)"`

**`cli/src/commands/task.rs` (5 references):**
- Line 489: help text `"Undo completed subtasks of a plan"` → `"...of an epic"`
- Line 538: `"Spec file this task implements"` → `"Plan file this task implements"`
- Line 542: `"Plan task this orchestrator drives"` → `"Epic this orchestrator drives"`
- Line 2852: error `"--completed requires exactly one plan task ID"` → `"...one epic task ID"`
- Line 2858: comment `"completed subtasks (direct children of the plan)"` → `"...of the epic"`

**`cli/src/main.rs` (13 references):**
- Line 131: `/// Create an implementation plan from a spec file` → `/// Create an epic from a plan file`
- Line 132: `Plan(commands::plan::PlanArgs)` → `Decompose(commands::decompose::DecomposeArgs)`
- Line 133: `/// Build from a spec file` → `/// Build from a plan file`
- Lines 135-148: `Spec { ... }` variant → `Plan { ... }` with same args, update help text
- Line 243: keep `Commands::Plan(args) => commands::plan::run(args)` for `aiki plan`
- Line 245-249: `Commands::Decompose(args) => commands::decompose::run(args)`

**`cli/src/commands/mod.rs` (2 references):**
- Line 18: `pub mod plan;` → `pub mod decompose;`
- Line 22: `pub mod spec;` → `pub mod plan;`

**`cli/src/commands/agents_template.rs` (4 references):**
- Line 258: `aiki task link <id> --implements ops/now/spec.md` → `ops/now/plan.md`
- Line 462: `implements | plan → spec | Plan implements this spec` → update description
- Line 463: `orchestrates | orchestrator → plan | Orchestrator drives this plan` → `→ epic`
- Line 471: `--implements: Link a plan task to its spec file` → update description

---

#### CLI Source — Tasks (6 files)

**`cli/src/tasks/graph.rs` (14 references):**
- Line 19: comment "orchestrator per plan" → "orchestrator per epic"
- Line 347: `data.get("spec")` → `data.get("plan")`
- Lines 348-351: `spec` variable, `format!("file:{}", spec)` — update variable names
- Line 356: `data.get("plan")` → `data.get("epic")`
- Line 363: `Some("spec") | Some("implementation")` → `Some("plan") | Some("implementation")`
- **Tests:**
  - Line 871: `test_data_spec_as_implements` — update test name + data keys
  - Lines 873-878: test data with `"spec"` key, task_type `"plan"` → `"decompose"`
  - Line 901: `test_data_plan_as_orchestrates` — update test name + data keys
  - Lines 903-904: test data with `"spec"` and `"plan"` keys → `"plan"` and `"epic"`
  - Line 930: `data.insert("scope.kind", "spec")` → `"plan"`

**`cli/src/tasks/status_monitor.rs` (5 references):**
- Lines 298-306: `data.get("plan")` → `data.get("epic")` — renders epic subtask tree under build task
- Variable names: `plan_id` → `epic_id`, `plan_task` → `epic_task`, `plan_line` → `epic_line`, `plan_subtasks` → `epic_subtasks`, `plan_subtask_count` → `epic_subtask_count`

**`cli/src/tasks/templates/resolver.rs` (~15 test references):**
- Tests use `"aiki/plan"` and `"aiki/review/spec"` as example template names
- Lines 2011-2037: test with `{% subtask aiki/plan %}` → `{% subtask aiki/decompose %}`
- Lines 2050-2058: test with `{% subtask aiki/review/spec %}` → `{% subtask aiki/review/plan %}`
- Lines 2072-2083: conditional test `aiki/review/spec if data.file_type == "spec"` → update
- Lines 2093-2151: more subtask ref tests with `aiki/plan` → `aiki/decompose`

**`cli/src/tasks/templates/conditionals.rs` (~25 test references):**
- All test references to `"aiki/plan"` → `"aiki/decompose"` (~15 occurrences)
- All test references to `"aiki/review/spec"` → `"aiki/review/plan"` (~10 occurrences)
- Lines 2149-2163: tokenize/parse tests for `aiki/plan`
- Lines 2173-2183: tokenize/parse tests for `aiki/review/spec` with `"spec"` value
- Lines 2194-2212: nested conditional tests with `aiki/plan`
- Lines 2231-2234: `parse_subtask_ref` tests for `aiki/plan`, `aiki/review/spec`
- Lines 2297-2313: `node_to_template` tests
- Lines 2320-2379: `process_conditionals` tests

**`cli/src/tasks/id.rs` (3 comments):**
- Line 478: comment "Zero child number (valid but unusual - planning task)" → "decompose task"
- Line 502: comment "Subtask with number 0 (planning task)" → "decompose task"
- Line 513: comment "Child number 0 (planning task)" → "decompose task"

**`cli/src/tasks/mod.rs` (1 comment):**
- Line 82: comment "Parent/subtask handling (.0 planning task)" → "(.0 decompose task)"

**`cli/src/tasks/README` (2 references):**
- Line 15: `spec --start` → `plan --start`
- Line 32: "auto-create + auto-start `.0` planning subtask" → "decompose subtask" or keep as-is if ".0 subtask" is generic

---

#### CLI Source — Other (2 files)

**`cli/src/error.rs` (1 reference):**
- Line 307: `"Unknown review scope type: '{0}'. Valid values: 'task', 'spec', 'implementation', 'session'"` → `'plan'` instead of `'spec'`

**`cli/src/session/mod.rs` (6 comments):**
- Line 100: "task-driven session (spawned by aiki spec" → "aiki plan"
- Line 315: "Used for task-driven sessions spawned by `aiki spec`" → "`aiki plan`"
- Line 422: "Interactive mode (default, for `aiki spec`" → "`aiki plan`"
- Line 534: "Used for sessions spawned by `aiki spec`" → "`aiki plan`"
- Line 544: "spawned by a workflow command (e.g., `aiki spec`" → "`aiki plan`"
- Line 1050: "Info about a task-driven session (spawned by `aiki spec`" → "`aiki plan`"

---

#### Integration Tests (1 file)

**`cli/tests/test_task_events.rs` (4 references):**
- Lines 38, 53: `source: Some("file:ops/plan.md")` — these are test fixture paths, not command references. Likely fine as-is (plan.md is a valid file path). Review whether they accidentally match old semantics.
- Lines 562, 579: `source: Some("file:plan.md")` — same, test fixture paths.

---

#### Documentation — ops/now/ (6 files, active specs)

**`ops/now/polish-workflow-commands-ux.md`:**
- Extensively references `aiki spec` (~50 occurrences): command examples, code snippets, output examples
- References `spec.rs` filename (~10 occurrences)
- "Spec:" prefixes in output examples
- "spec session", "spec task", "spec prompt"
- Should be updated to use new terminology throughout

**`ops/now/tui.md`:**
- References `aiki spec`, `aiki plan`, `aiki build` commands (lines 4, 28-29, 52-62, 90-91, 178-194, 198-200, 257-261, 271, 283)
- `type: spec` task type → `type: plan`
- "Spec sessions", "spec files", "Specs column" — conceptual references
- Spec visibility rules

**`ops/now/workflow-hook-commands.md`:**
- `spec:` flow action → `plan:` (lines 31-50)
- `plan:` flow action → `decompose:` (lines 52-74)
- `spec.completed` event → `plan.completed` (lines 103-116)
- `plan.completed` event → `decompose.completed` (lines 116+)
- All example YAML configs referencing `spec`, `plan`, `event.spec_path`
- Test scenarios (lines 247-266)

**`ops/now/spec-file-frontmatter.md`:**
- Title and throughout: "spec file" → "plan file"
- References to `aiki spec`, `aiki plan`, `aiki build` commands
- `spec.rs` filename references
- `plan_task` frontmatter field (refers to what is now the "epic") — consider renaming to `epic_task`
- `aiki/plan` template reference → `aiki/decompose`
- All `data.spec` references → `data.plan`

**`ops/now/instructions-cli.md`:**
- References `plan.md` template → `decompose.md` (lines 5, 9, 75, 77, 89, 91)
- "plan template" → "decompose template"
- "plan tasks and subtasks" — clarify plan vs epic

**`ops/now/the-aiki-way.md`:**
- Line 517: directory listing shows `plan.md` → update to `decompose.md`

**`ops/now/default-hooks.md`:**
- Line 13: reference to workflow-hook-commands.md — "(spec/plan/build)" → "(plan/decompose/build)"

---

#### Documentation — Root (2 files)

**`AGENTS.md`:**
- Line 258: `--implements ops/now/spec.md` → `ops/now/plan.md`
- Line 462: `implements | plan → spec` → update link description
- Line 463: `orchestrates | orchestrator → plan` → `→ epic`
- Line 471: `--implements: Link a plan task to its spec file` → update

**`cli/src/CLAUDE.md`:**
- Same references to spec.md and plan → spec link descriptions (appears to be embedded version of AGENTS.md content)

---

#### Documentation — ops/done/ (historical, don't change)

These files are historical records and should NOT be updated:
- `ops/done/impl-spec-command.md`
- `ops/done/plan-and-build-commands.md`
- `ops/done/composable-task-templates.md`
- `ops/done/template-conditionals.md`
- `ops/done/task-dag.md`
- `ops/done/review-scope-refactor.md`
- etc.

---

#### Edge Mapping — graph.rs

`cli/src/tasks/graph.rs` lines 347-363 generate edges for planning/decomposition metadata after the rename:
- `data.plan` (was `data.spec`) generates `implements` edges to plan files
- `data.epic` (was `data.plan`) generates `orchestrates` edges

---

## Implementation Plan

### Phase 1: Templates (no code changes)

1. Rename `plan.md` → `decompose.md`, update content (type, data refs, "epic" language)
2. Rename `spec.md` → `plan.md`, update content (type, data refs, "plan" language)
3. Rename `review/spec.md` → `review/plan.md`, update content
4. Update `build.md` references
5. Update `review/implementation.md` subtask ref and prose

### Phase 2: CLI Source — Commands

6. Rename `plan.rs` → `decompose.rs`, update all internal references + tests
7. Rename `spec.rs` → `plan.rs`, update all internal references + tests
8. Update `mod.rs` exports
9. Update `main.rs` command dispatch
10. Update `build.rs` — all spec/plan references + tests
11. Update `review.rs` — `ReviewScopeKind::Spec` → `Plan` + tests
12. Update `fix.rs` — scope kind references + tests
13. Update `output.rs` — scope kind references + tests
14. Update `task.rs` — help text for `--implements`, `--orchestrates`, `--completed`
15. Update `agents_template.rs` — link examples and descriptions

### Phase 3: CLI Source — Tasks and Other

16. Update `graph.rs` — data key reads, scope kind + tests
17. Update `status_monitor.rs` — `data.plan` → `data.epic` rendering
18. Update `tasks/templates/resolver.rs` — test template name references
19. Update `tasks/templates/conditionals.rs` — test template name references
20. Update `tasks/id.rs`, `tasks/mod.rs`, `tasks/README` — comments
21. Update `error.rs` — valid scope type list
22. Update `session/mod.rs` — doc comments referencing `aiki spec`

### Phase 4: Documentation

23. Update `ops/now/polish-workflow-commands-ux.md`
24. Update `ops/now/tui.md`
25. Update `ops/now/workflow-hook-commands.md`
26. Update `ops/now/spec-file-frontmatter.md`
27. Update `ops/now/instructions-cli.md`
28. Update `ops/now/the-aiki-way.md` and `ops/now/default-hooks.md`
29. Update `AGENTS.md` and `cli/src/CLAUDE.md`

### Phase 5: Validation

30. `cargo test` — ensure all ~994 unit tests pass
31. `cargo build` — ensure clean compile
32. Grep for stale `"spec"` references in CLI source (excluding ops/done/, false positives like "specific", "specify", "inspect")
33. Manual smoke test: `aiki plan`, `aiki decompose`, `aiki build`

---

## Open Questions

1. **Shell completions** — Do we generate shell completions that need updating?
2. **`spec-file-frontmatter.md` `plan_task` field** — The frontmatter field name `plan_task` (which stores the epic task ID) should be renamed to `epic_task` for consistency. But this spec hasn't been implemented yet — if it's still a draft, just rename in the spec.
