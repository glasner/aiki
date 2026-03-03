# Restore decompose as a top-level command

## Problem

`aiki decompose` is currently a deprecated shim that delegates to `epic add`. But after the loop refactor, decompose is a shared pipeline stage used by both build (epics) and fix (fix-parents). The current code has two separate `create_decompose_task()` implementations — one in `epic.rs` and one in `fix.rs` — doing the same thing with slightly different wiring.

Additionally, `implements-plan` is currently written by `epic.rs::create_epic_task()`, making it epic-specific. But fix-parents also implement plans (the plan/fix output). The `implements-plan` link should be owned by decompose, since it's the stage that connects any parent task to a plan.

The decompose template uses `{{data.epic}}` which is semantically wrong for fix-parents. All pipeline stages should use `data.target` for "the task this operation acts on" (matching `aiki/loop`'s existing convention).

## Design

### `decompose` as a generic pipeline stage

`aiki decompose` becomes the canonical entry point for "take a plan file, create subtasks under a target task."

```bash
aiki decompose <plan-path> --target <task-id> [--template <template>] [--agent <agent>]
```

Core function `run_decompose()` — shared by the CLI, `epic.rs`, and `fix.rs`:

```rust
pub fn run_decompose(cwd: &Path, plan_path: &str, target_id: &str, options: DecomposeOptions) -> Result<String>
```

Steps:
1. Write `implements-plan` link: target → file:\<plan-path>
2. Create decompose task from template (default: `aiki/decompose`)
   - `data.plan` = plan_path
   - `data.target` = target_id
3. Write `decomposes-plan` link: decompose task → file:\<plan-path>
4. Write `depends-on` link: target → decompose task
5. `task_run(decompose_task)`
6. Return decompose task ID

### Callers simplified

**`epic.rs`** — `create_epic()` creates the epic task, then calls `run_decompose(epic_id, plan_path)`. Removes its own `create_decompose_task()`, `write_link_event("implements-plan", ...)`.

**`fix.rs`** — pipeline step 5-6 calls `run_decompose(fix_parent_id, plan_path)`. Removes its own `create_decompose_task()`.

**CLI** — `aiki decompose <plan-path> --target <task-id>` calls `run_decompose()` directly.

### Template update

`{{data.epic}}` → `{{data.target}}` throughout `decompose.md`. The template already doesn't care what kind of parent it's filling — the rename makes that explicit.

## Changes

### 1. Rewrite `cli/src/commands/decompose.rs`

Replace the deprecated shim with a real command module:

**CLI args:**
- `plan_path: String` — path to plan file
- `--target <task-id>` — required, the parent task to decompose into
- `--template <template>` — decompose template override (default: `aiki/decompose`)
- `--agent <agent>` — agent override (default: claude-code)
- `-o, --output <format>` — output format (`id` for bare task ID)

**`DecomposeOptions` struct:**
```rust
pub struct DecomposeOptions {
    pub template: Option<String>,
    pub agent: Option<AgentType>,
}
```

**`run_decompose()` function:**
1. Write `implements-plan` link: target → `file:<plan-path>`
2. Create decompose task from template with `data.target` and `data.plan`
3. Write `decomposes-plan` link: decompose task → `file:<plan-path>`
4. Write `depends-on` link: target → decompose task
5. `task_run(decompose_task)` with agent options
6. Return decompose task ID

Remove the deprecated shim (`DecomposeSubcommands`, delegation to `EpicCommands`).

Remove `decompose show` subcommand — this was a shim for `epic show`, which still works via `aiki epic show`.

### 2. Update `cli/src/commands/epic.rs`

**In `create_epic()`:**
- Remove the inline decompose task creation (lines 186-209)
- Remove the `implements-plan` link from `create_epic_task()` (lines 257-265)
- Call `decompose::run_decompose(cwd, plan_path, &epic_id, options)` instead
- Pass through template and agent options

**In `create_decompose_task()`:**
- Delete entirely — replaced by `decompose::run_decompose()`

**In `find_or_create_epic()`:**
- Update to call through new `create_epic()` (which now uses `run_decompose()`)

### 3. Update `cli/src/commands/fix.rs`

**In the quality loop (step 5-6):**
- Remove `create_decompose_task()` function (lines 375-394)
- Replace with call to `decompose::run_decompose(cwd, &plan_path, &fix_parent_id, options)`
- This also gives fix-parents the `implements-plan` link automatically

### 4. Update `.aiki/templates/aiki/decompose.md`

- `{{data.epic}}` → `{{data.target}}` (5 occurrences)
- Update heading from "Create Implementation Plan" to just reflect generic decompose
- Update close message to reference `{{data.target}}` instead of epic

### 5. Update `cli/src/main.rs`

Update the decompose command dispatch to use the new `DecomposeArgs` (no longer delegating to epic).

### 6. Update tests

- `cli/src/commands/epic.rs` tests — remove any tests for `create_decompose_task()` if they exist
- `cli/src/commands/fix.rs` tests — update references from `data.epic` to `data.target`
- `cli/tests/end_to_end_tests.rs` — update any decompose-related tests
- Add unit tests for `run_decompose()` in decompose.rs

## Task Graph

```
1.  Rewrite decompose.rs (new CLI + run_decompose function)
2.  Update epic.rs (use run_decompose, remove implements-plan from create_epic_task)
    └── depends-on: 1
3.  Update fix.rs (use run_decompose, remove create_decompose_task)
    └── depends-on: 1
4.  Update decompose.md template (data.epic → data.target)
5.  Update main.rs (new decompose command dispatch)
    └── depends-on: 1
6.  Update tests
    └── depends-on: 2, 3, 4, 5
7.  Verify build (cargo build + cargo test)
    └── depends-on: 6
```

## Out of Scope

- Changing `aiki epic` command (show, list, add still work — add just calls run_decompose internally)
- Changing `aiki build` command (it calls `find_or_create_epic` which calls `create_epic` which calls `run_decompose`)
- Changing the decompose template's instructions or behavior (only renaming data fields)
- Adding `--async` to decompose (can be added later)
- Changing PlanGraph or `find_epic_for_plan` logic
