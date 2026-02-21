# Rename "Plan" to "Decompose"

**Date**: 2026-02-21
**Status**: Draft
**Purpose**: Rename `aiki plan` → `aiki decompose`. After this rename, there should be no references to "plan" as a verb or "plan task" as a noun.

**Compatibility**: Breaking migration. There is no backward compatibility.

---

## Executive Summary

The current `aiki plan` command breaks a document into subtasks — which is really "decomposition". The task that `aiki plan` outputs is called a "plan task", but it's really an epic (a container of implementation subtasks). This rename changes the command and all related terminology to match what it actually does.

---

## Terminology Mapping

| Current | New | What It Is |
|---------|-----|------------|
| `aiki plan` (command) | `aiki decompose` | Break a document into an epic with subtasks |
| `plan` (task type, ephemeral) | `decompose` | Task type for the decomposition process |
| "plan task" (output of decompose) | "epic" | Container task with implementation subtasks |
| `aiki/plan` (template) | `aiki/decompose` | Template for decomposition |
| `data.plan` (epic task ID) | `data.epic` | Data attribute referencing the epic task |

After the rename, all references to "plan" (verb/command) are removed. References to "plan" (noun/document) remain.

---

## User Experience

### Commands After Rename

```bash
# Decompose a plan document into an epic with subtasks
aiki decompose ops/now/feature.md

# Build (unchanged command name, but calls decompose internally)
aiki build ops/now/feature.md
```

### Help Text

```
aiki decompose    Create an epic from a plan file (implementation subtasks)
aiki build        Build from a plan file (decompose + execute all subtasks)
```

---

## How It Works

### Rename Order (to avoid conflicts)

**Templates:**
1. `.aiki/templates/aiki/plan.md` → `.aiki/templates/aiki/decompose.md`

**Source files:**
1. `cli/src/commands/plan.rs` → `cli/src/commands/decompose.rs`

### Template Changes

**`decompose.md` (was `plan.md`):**
- Front matter: `type: decompose` (was `type: plan`)
- Heading: `# Decompose: {{data.plan}}` (was `# Plan: {{data.spec}}`)
- All `{{data.spec}}` → `{{data.plan}}`
- "plan task" → "epic" in instructions
- `aiki task add "Plan: <title>"` → `aiki task add "Epic: <title>"`
- "Plan created with N subtasks" → "Epic created with N subtasks"

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

**`build.rs`:**
- References to `plan` (the old command) → `decompose`
- `data.spec` → `data.plan`
- `data.plan` (old, meaning plan task ID) → `data.epic`
- Calls to `commands::plan::run()` → `commands::decompose::run()`
- Help text: "Build from a spec file" → "Build from a plan file"

**`main.rs`:**
- `Plan(PlanArgs)` → `Decompose(DecomposeArgs)`
- Dispatch: `Decompose(args) => commands::decompose::run(args)`

**`commands/mod.rs`:**
- `pub mod plan;` → `pub mod decompose;`

### Complete File Inventory

Every file that needs changes, organized by category.

---

#### Templates (2 files)

**`.aiki/templates/aiki/plan.md` → rename to `decompose.md`:**
- Front matter: `type: plan` → `type: decompose`
- Heading: `# Plan: {{data.spec}}` → `# Decompose: {{data.plan}}`
- All `{{data.spec}}` → `{{data.plan}}`
- "plan task" → "epic" in instructions
- "Plan created with N subtasks. Plan ID:" → "Epic created with N subtasks. Epic ID:"
- "planning task" → "decompose task"

**`.aiki/templates/aiki/build.md`:**
- `{% subtask aiki/plan if not data.plan %}` → `{% subtask aiki/decompose if not data.epic %}`
- `{{data.spec}}` → `{{data.plan}}`
- `data.plan` → `data.epic` (the epic task ID)
- "plan subtasks" → "epic subtasks"

---

#### CLI Source — Commands (5 files)

**`cli/src/commands/plan.rs` → rename to `decompose.rs`:**
- Module doc: "Plan command" → "Decompose command"
- `PlanSubcommands` → `DecomposeSubcommands`
- `PlanArgs` → `DecomposeArgs`
- `spec_path` field → `plan_path`
- Default template: `"aiki/plan"` → `"aiki/decompose"`
- `PlanChoice` enum → `DecomposeChoice`
- `run_plan()` → `run_decompose()`
- `validate_spec_path()` → `validate_plan_path()`
- `find_plan_for_spec()` → `find_epic_for_plan()`
- `find_created_plan()` → `find_created_epic()`
- `close_plan()` → `close_epic()`
- `create_planning_task()` → `create_decompose_task()`
- `output_plan_created()` → `output_epic_created()`
- `output_plan_resumed()` → `output_epic_resumed()`
- `output_plan_show()` → `output_epic_show()`
- `is_spec_path()` → `is_plan_path()`
- All `data.get("spec")` → `data.get("plan")`
- All `"spec"` string literals → `"plan"` in task data
- Task type filter: `Some("plan")` → `Some("decompose")`
- Error messages: "Spec file must be markdown" → "Plan file must be markdown"
- `MdBuilder::new("plan")` → `MdBuilder::new("decompose")`
- `MdBuilder::new("plan-show")` → `MdBuilder::new("decompose-show")`
- `<aiki_plan plan_id=...>` XML output → `<aiki_epic epic_id=...>`
- `variables.set_data("spec", ...)` → `variables.set_data("plan", ...)`
- **Tests:** All test function names and assertions referencing `plan` need updating

**`cli/src/commands/build.rs`:**
- Module doc: "spec file" → "plan file"
- `BuildSubcommands::Show { spec_path }` → `{ plan_path }`
- `BuildArgs` target help: "Spec path or plan ID" → "Plan path or epic ID"
- `BuildArgs` restart help: "Ignore existing plan" → "Ignore existing epic"
- `run_build_spec()` → `run_build_plan()`
- `run_build_plan()` (current, builds from plan ID) → `run_build_epic()`
- All `spec_path` params → `plan_path`
- All `plan_id` params → `epic_id`
- `validate_spec_path()` → `validate_plan_path()`
- `find_plan_for_spec()` → `find_epic_for_plan()`
- `close_plan()` → `close_epic()`
- `create_build_task()`: `data.insert("spec", ...)` → `data.insert("plan", ...)`, `data.insert("plan", ...)` → `data.insert("epic", ...)`
- `output_build_*()`: "Plan ID" → "Epic ID" in output
- `<aiki_build build_id="..." plan_id="...">` → `epic_id`
- Link: `"orchestrates"` target changes from plan_id to epic_id
- **Tests:** ~30 tests need updates

**`cli/src/main.rs`:**
- `/// Create an implementation plan from a spec file` → `/// Create an epic from a plan file`
- `Plan(commands::plan::PlanArgs)` → `Decompose(commands::decompose::DecomposeArgs)`
- `/// Build from a spec file` → `/// Build from a plan file`
- `Commands::Decompose(args) => commands::decompose::run(args)`

**`cli/src/commands/mod.rs`:**
- `pub mod plan;` → `pub mod decompose;`

**`cli/src/commands/task.rs`:**
- Help text `"Undo completed subtasks of a plan"` → `"...of an epic"`
- `"Plan task this orchestrator drives"` → `"Epic this orchestrator drives"`
- Error `"--completed requires exactly one plan task ID"` → `"...one epic task ID"`
- Comment `"completed subtasks (direct children of the plan)"` → `"...of the epic"`

**`cli/src/commands/agents_template.rs`:**
- `orchestrates | orchestrator → plan | Orchestrator drives this plan` → `→ epic`

---

#### CLI Source — Tasks (6 files)

**`cli/src/tasks/graph.rs`:**
- Comment "orchestrator per plan" → "orchestrator per epic"
- `data.get("plan")` → `data.get("epic")`
- **Tests:** Update test data with task_type `"plan"` → `"decompose"`, data key `"plan"` → `"epic"`

**`cli/src/tasks/status_monitor.rs`:**
- `data.get("plan")` → `data.get("epic")`
- Variable names: `plan_id` → `epic_id`, `plan_task` → `epic_task`, etc.

**`cli/src/tasks/templates/resolver.rs`:**
- Test references: `{% subtask aiki/plan %}` → `{% subtask aiki/decompose %}`

**`cli/src/tasks/templates/conditionals.rs`:**
- All test references to `"aiki/plan"` → `"aiki/decompose"`

**`cli/src/tasks/id.rs`:**
- Comments: "planning task" → "decompose task"

**`cli/src/tasks/mod.rs`:**
- Comment: ".0 planning task" → ".0 decompose task"

---

#### Documentation (6 files)

**`ops/now/spec-file-frontmatter.md`:**
- `plan_task` frontmatter field → `epic_task`
- `aiki/plan` template reference → `aiki/decompose`
- `aiki plan` command → `aiki decompose`

**`ops/now/instructions-cli.md`:**
- `plan.md` template → `decompose.md`
- "plan template" → "decompose template"

**`ops/now/the-aiki-way.md`:**
- Directory listing: `plan.md` → `decompose.md`

**`ops/now/workflow-hook-commands.md`:**
- `plan:` flow action → `decompose:`
- `plan.completed` event → `decompose.completed`

**`ops/now/tui.md`:**
- `aiki plan` command → `aiki decompose`
- `type: plan` task type → `type: decompose`

**`AGENTS.md`:**
- `orchestrates | orchestrator → plan` → `→ epic`

---

## Implementation Plan

### Phase 1: Templates

1. Rename `plan.md` → `decompose.md`, update content
2. Update `build.md` references

### Phase 2: CLI Source — Commands

3. Rename `plan.rs` → `decompose.rs`, update all internal references + tests
4. Update `mod.rs` exports
5. Update `main.rs` command dispatch
6. Update `build.rs` — all plan → epic references + tests
7. Update `task.rs` — help text
8. Update `agents_template.rs` — link descriptions

### Phase 3: CLI Source — Tasks

9. Update `graph.rs` — data key reads + tests
10. Update `status_monitor.rs` — rendering
11. Update `tasks/templates/resolver.rs` — tests
12. Update `tasks/templates/conditionals.rs` — tests
13. Update `tasks/id.rs`, `tasks/mod.rs` — comments

### Phase 4: Documentation

14. Update `ops/now/spec-file-frontmatter.md`
15. Update `ops/now/instructions-cli.md`
16. Update `ops/now/the-aiki-way.md`
17. Update `ops/now/workflow-hook-commands.md`
18. Update `ops/now/tui.md`
19. Update `AGENTS.md`

### Phase 5: Validation

20. `cargo test` — ensure all tests pass
21. `cargo build` — ensure clean compile
22. Manual smoke test: `aiki decompose`, `aiki build`

---

## Notes

After this rename, "plan" only refers to the plan document (noun), never to the command (verb) or the epic task.

## Open Questions

1. **Shell completions** — Do we generate shell completions that need updating?
