# Fix Stage Not Running After Review

## Problem

When running `aiki build -f` (or `--fix`), the review stage completes and finds issues but the fix stage never runs. The build finishes with review issues unresolved.

## Observed Behavior

Epic `qkuuoous` (task-template-formalization.md) ran with `-f`:

1. **Build stage**: completed 8/8 subtasks
2. **Review stage**: review task `usmoqnux` completed, found 3 issues (1 high, 2 medium)
3. **Fix stage**: never ran — no fix tasks were created

## How Fix Is Wired

The fix stage is **not** explicitly invoked by `run_build_review` in `build.rs`. Instead:

1. `run_build_review` passes `fix_template` to `create_review` via `CreateReviewParams`
2. `create_review` stores it in scope_data as `options.fix = "true"` and `options.fix_template = "fix"`
3. These values are baked into the review task's template data
4. The **review agent** is expected to read `options.fix_template` from its task data and invoke `aiki fix` after finding issues

So the fix depends on:
- The review template including instructions to run `aiki fix` when `options.fix` is set
- The review agent actually following those instructions

## Likely Root Cause

The review agent (running template `review/code`) either:
- Doesn't have instructions in its template to check `options.fix` and run fix
- Has the instructions but the agent didn't follow them (agent compliance issue)
- The template instructions reference a different mechanism than what's implemented

## Key Files

- `cli/src/commands/build.rs:810-843` — `run_build_review`: creates review with fix_template, runs it, returns. No post-review fix invocation.
- `cli/src/commands/review.rs:640-644` — `create_review`: stores fix_template in scope_data
- Review template (likely `cli/src/tasks/templates/core/review/code.md` or `.aiki/templates/review/code.md`) — should contain fix instructions
