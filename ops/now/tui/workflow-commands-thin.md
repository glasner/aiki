# Thin Commands: Move Orchestration + Helpers into `workflow/`

**Date**: 2026-03-26
**Status**: Plan
**Prerequisite**: workflow-module-split.md (step functions moved to `workflow/steps/`)

---

## Goal

After phase 1 (workflow-module-split.md), step functions live in
`workflow/steps/` but the orchestration logic and its helpers still live in
command files. This phase moves all remaining domain logic so that command
files become thin CLI shells: parse args → assemble workflow → call
`Workflow::run()` or `drive_build()` → format output.

## What Moves

### From `commands/build.rs` → `workflow/`

| Function | Destination | Why it's domain logic |
|----------|-------------|----------------------|
| `build_workflow()` | `workflow/builders.rs` | Assembles step sequences from options |
| `build_workflow_from_epic()` | `workflow/builders.rs` | Same, for epic resume path |
| `drive_build()` | `workflow/mod.rs` (already there) | Already in workflow |
| `has_review_issues()` | `workflow/mod.rs` (already there) | Already in workflow |
| `validate_plan_path()` | `workflow/steps/plan.rs` | Only used by plan step + async precheck |
| `cleanup_stale_builds()` | `workflow/steps/plan.rs` | Same |
| `check_epic_blockers()` | `workflow/steps/decompose.rs` | Epic readiness check |
| `undo_completed_subtasks()` | `workflow/steps/decompose.rs` | Epic restart logic |
| `close_epic()` | `workflow/steps/decompose.rs` | Epic lifecycle |
| `close_epic_as_invalid()` | `workflow/steps/decompose.rs` | Epic lifecycle |
| `restart_epic()` | `workflow/steps/decompose.rs` | Epic lifecycle |

**Note:** `validate_plan_path` and `cleanup_stale_builds` are also called from
the `--async` precheck path in `run_build_plan`. After the move, the async path
imports them from `workflow::steps::plan` (or they become `pub(crate)` helpers
there). The duplicate copies in `commands/epic.rs` should be consolidated — see
step 4.

### From `commands/fix.rs` → `workflow/`

| Function | Destination | Why it's domain logic |
|----------|-------------|----------------------|
| `fix_workflow()` | `workflow/builders.rs` | Assembles fix step sequences |
| `fix_pass_workflow()` | `workflow/builders.rs` | Assembles single fix pass |
| `run_fix()` | `workflow/orchestrate.rs` | Core fix pipeline (quality loop setup) |
| `run_fix_continue()` | `workflow/orchestrate.rs` | Async continuation of fix pipeline |
| `run_quality_loop()` | `workflow/orchestrate.rs` | Iterative fix→review loop |
| `determine_review_outcome()` | `workflow/orchestrate.rs` | Pure decision logic |
| `ReviewOutcome` enum | `workflow/orchestrate.rs` | Used only by orchestration |
| `create_fix_parent()` | `workflow/steps/fix.rs` | Task creation for fix pipeline |
| `create_plan_fix_task()` | `workflow/steps/fix.rs` | Task creation for fix plan |
| `has_actionable_issues()` | `workflow/orchestrate.rs` | Review result inspection |
| `run_task_with_show_tui()` | `workflow/steps/fix.rs` | Task execution wrapper |
| `determine_followup_assignee()` | `workflow/orchestrate.rs` | Agent routing |
| `is_review_task()` | `workflow/orchestrate.rs` | Input validation |
| `resolve_plan_template()` | `workflow/orchestrate.rs` | Template resolution |
| `resolve_fix_template_name()` | `workflow/orchestrate.rs` | Template resolution |

### From `commands/review.rs` → `workflow/`

| Function | Destination | Why it's domain logic |
|----------|-------------|----------------------|
| `review_workflow()` | `workflow/builders.rs` | Assembles review step sequence |
| `create_review()` | `workflow/steps/review.rs` | Creates review task + links |
| `CreateReviewParams` | `workflow/steps/review.rs` | Used by steps and orchestration |
| `CreateReviewResult` | `workflow/steps/review.rs` | Used by steps and orchestration |
| `detect_target()` | `workflow/steps/review.rs` | Scope detection logic |
| `ReviewScope`, `ReviewScopeKind` | `workflow/steps/review.rs` | Core domain types |
| `build_async_review_args()` | `workflow/steps/review.rs` | Async spawn args |
| `get_issue_comments()` | `workflow/steps/review.rs` | Review data access |
| `parse_locations()`, `format_locations()` | `workflow/steps/review.rs` | Review data types |
| `Location` | `workflow/steps/review.rs` | Review data type |

### From `commands/epic.rs` → consolidate

| Function | Destination | Notes |
|----------|-------------|-------|
| `validate_plan_path()` | **Delete** — use `workflow::steps::plan::validate_plan_path` | Duplicate of build.rs version |
| `cleanup_stale_builds()` | Check if present, consolidate | May be duplicated |
| `undo_completed_subtasks()` | **Delete** — use workflow version | Duplicate |
| `close_epic()` | **Delete** — use workflow version | Duplicate |
| `close_epic_as_invalid()` | **Delete** — use workflow version | Duplicate |
| `create_epic_task()` | `workflow/steps/decompose.rs` | Called from decompose step |

## What Stays in Command Files

### `commands/build.rs`

| Function | Why |
|----------|-----|
| `run()` | Clap arg parsing, dispatches to `run_build_plan` / `run_build_epic` |
| `run_build_plan()` | Parses flags → calls `workflow::builders::build_workflow()` → `run_build()` → output. The `--async` precheck + spawn stays here (it's CLI process management). |
| `run_build_epic()` | Same pattern for epic path |
| `run_continue_async()` | Background process entry point (parses hidden `--_continue-async` flag) |
| `run_show()` | CLI subcommand |
| `output_after_workflow()` | Output formatting |
| `output_build_show()` | Output formatting |
| `anyhow_to_aiki()` | Error conversion |
| `BuildArgs`, `BuildOpts`, `BuildSubcommands` | Clap types |

### `commands/fix.rs`

| Function | Why |
|----------|-----|
| `run()` | Clap arg parsing, reads stdin, delegates to `workflow::orchestrate::run_fix()` |
| `extract_task_id()` | CLI input parsing |
| `read_task_id_from_stdin()` | CLI input parsing |
| `output_approved()` | Output formatting |
| `FixOpts` | Options struct (may move to builders.rs if workflow assembly needs it) |

### `commands/review.rs`

| Function | Why |
|----------|-----|
| `run()` | Clap arg parsing, dispatches subcommands |
| `run_review()` | Parses targets → calls `create_review()` → spawns async or runs sync |
| `run_continue_async()` | Background process entry point |
| `run_issue_add()` | CLI subcommand |
| `run_issue_list()` | CLI subcommand |
| `list_reviews()` | CLI subcommand |
| `show_review()` | CLI subcommand |
| `parse_severity()` | CLI arg parsing |
| `looks_like_task_id()` | CLI input parsing |
| `output_nothing_to_review()` | Output formatting |
| `output_review_started()` | Output formatting |
| `output_review_async()` | Output formatting |
| `output_review_completed()` | Output formatting |
| `render_review_workflow()` | Output formatting |
| `ReviewArgs`, `ReviewSubcommands`, `ReviewIssueSubcommands` | Clap types |

## Target Layout After Phase 2

```
cli/src/workflow/
├── mod.rs            ← Step enum, Workflow, WorkflowContext, RunMode,
│                        StepResult, drive_build, has_review_issues, tests
├── builders.rs       ← build_workflow, build_workflow_from_epic,
│                        fix_workflow, fix_pass_workflow, review_workflow
├── orchestrate.rs    ← run_fix, run_fix_continue, run_quality_loop,
│                        determine_review_outcome, ReviewOutcome,
│                        has_actionable_issues, is_review_task,
│                        determine_followup_assignee, resolve_*_template
└── steps/
    ├── mod.rs
    ├── plan.rs       ← run(), validate_plan_path, cleanup_stale_builds
    ├── decompose.rs  ← run(), epic lifecycle helpers (check_epic_blockers,
    │                    undo_completed_subtasks, close_epic, etc.),
    │                    create_epic_task
    ├── loop.rs       ← run()
    ├── review.rs     ← run_standalone(), run_build(), run_regression(),
    │                    create_review, CreateReviewParams, CreateReviewResult,
    │                    ReviewScope, ReviewScopeKind, detect_target,
    │                    get_issue_comments, Location, parse/format_locations,
    │                    build_async_review_args
    └── fix.rs        ← run_plan(), run_decompose(), run_loop(), run_review(),
                         run_build(), create_fix_parent, create_plan_fix_task,
                         run_task_with_show_tui
```

## Implementation Steps

Each step ends with `cargo test -p aiki-cli` passing.

### 1. Create `workflow/builders.rs`

Move workflow assembly functions:
- `build_workflow()`, `build_workflow_from_epic()` from `build.rs`
- `fix_workflow()`, `fix_pass_workflow()` from `fix.rs`
- `review_workflow()` from `review.rs`

Update imports in command files to use `crate::workflow::builders::*`.

### 2. Move review domain types to `workflow/steps/review.rs`

Move from `commands/review.rs`:
- `ReviewScope`, `ReviewScopeKind`, `Location`
- `CreateReviewParams`, `CreateReviewResult`
- `create_review()`, `detect_target()`
- `parse_locations()`, `format_locations()`
- `get_issue_comments()`, `build_async_review_args()`

These are imported widely — update all call sites.

### 3. Move fix helpers to `workflow/steps/fix.rs`

Move from `commands/fix.rs`:
- `create_fix_parent()`, `create_plan_fix_task()`
- `run_task_with_show_tui()`

### 4. Move epic helpers to `workflow/steps/decompose.rs`

Move from `commands/build.rs`:
- `check_epic_blockers()`, `undo_completed_subtasks()`
- `close_epic()`, `close_epic_as_invalid()`, `restart_epic()`

Move from `commands/epic.rs`:
- `create_epic_task()`

Delete duplicate copies in `commands/epic.rs` (`validate_plan_path`,
`undo_completed_subtasks`, `close_epic`, `close_epic_as_invalid`).
Update `commands/epic.rs` to import from workflow modules.

### 5. Move plan helpers to `workflow/steps/plan.rs`

Move from `commands/build.rs`:
- `validate_plan_path()`, `cleanup_stale_builds()`

Update the `--async` precheck path in `run_build_plan` to import from
`workflow::steps::plan`.

### 6. Create `workflow/orchestrate.rs`

Move from `commands/fix.rs`:
- `run_fix()`, `run_fix_continue()`, `run_quality_loop()`
- `determine_review_outcome()`, `ReviewOutcome`
- `has_actionable_issues()`, `is_review_task()`
- `determine_followup_assignee()`
- `resolve_plan_template()`, `resolve_fix_template_name()`

Update `commands/fix.rs::run()` to delegate to
`workflow::orchestrate::run_fix()`.

### 7. Clean up imports and visibility

- All moved functions need `pub(crate)` at minimum.
- Run `cargo clippy -p aiki-cli` for unused imports.
- Verify no command file imports from `workflow::steps` except through
  the `workflow` re-exports.

## Risks

- **Large diff** — Steps 2 and 6 touch many files. Move one function at
  a time within each step to isolate compile errors.
- **Import tangles** — `orchestrate.rs` uses types from `steps/review.rs`
  and `steps/fix.rs`. This is fine (sibling modules within `workflow/`).
  But commands must not import from `workflow::steps` directly — only
  from `workflow` (public re-exports).
- **`FixOpts` placement** — Currently defined in `commands/fix.rs`, used
  by both `fix_workflow()` and `run_quality_loop()`. If both move to
  `workflow/`, the type should move too. May belong in `builders.rs` or
  `orchestrate.rs` depending on which uses it more.
- **Test migration** — Unit tests in command files that test moved
  functions should move with them. Integration tests that test CLI
  behavior stay in command files.
