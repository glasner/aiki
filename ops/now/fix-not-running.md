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

## Root Cause (confirmed)

The bug is in `run_build_review` (`build.rs:846-880`). After running the review task to completion, it **does not check for issues or invoke fix**. It just outputs a message and returns.

The standalone `aiki review` command (`review.rs:798-832`) has the correct pattern: after the review completes, it checks `data.issue_count` and calls `super::fix::run_fix()` if issues exist and `fix_template` is set. `run_build_review` is missing this logic entirely.

The `fix_template` is passed to `create_review` which stores it in `scope_data`, but nobody reads it back — it's dead data in the build path.

### The working pattern (review.rs:798-832)

```rust
// Run to completion
task_run(cwd, &review_id, options)?;

// Check for issues
let events = read_events(cwd)?;
let graph = materialize_graph(&events);
let has_issues = find_task(&graph.tasks, &review_id)
    .map(|t| t.data.get("issue_count")
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0) > 0)
    .unwrap_or(false);

// Invoke fix if needed
if fix_template_for_async.is_some() && has_issues {
    super::fix::run_fix(cwd, &review_id, ...)?;
}
```

### The broken pattern (build.rs:846-880)

```rust
// Run to completion
task_run(cwd, &result.review_task_id, options)?;

// ← Missing: no issue check, no fix invocation

output_build_review_completed(&result.review_task_id, plan_path, with_fix)?;
Ok(())
```

## Fix

Add the same post-review issue check and `run_fix` invocation to `run_build_review`, after the review task completes and before the output message. Specifically, in `build.rs` `run_build_review`:

1. After the review task runs (line ~875), read events and check `data.issue_count`
2. If `with_fix` is true and issues were found, call `super::fix::run_fix()` with the review task ID
3. Pass through `fix_template` (already available as a parameter)

```rust
// After task_run completes (around line 875):
let events = read_events(cwd)?;
let graph = materialize_graph(&events);
let has_issues = find_task(&graph.tasks, &result.review_task_id)
    .map(|t| t.data.get("issue_count")
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0) > 0)
    .unwrap_or(false);

if with_fix && has_issues {
    super::fix::run_fix(
        cwd,
        &result.review_task_id,
        false,                  // not async
        None,                   // no continue-async
        fix_template,           // forward caller's fix template
        None,                   // default decompose template
        None,                   // default loop template
        None,                   // default review template
        None,                   // no agent override
        false,                  // not autorun
        false,                  // not --once
        None,                   // no output format override
    )?;
}
```

## Key Files

- `cli/src/commands/build.rs:846-880` — `run_build_review`: **the broken function** — creates review, runs it, but never invokes fix
- `cli/src/commands/review.rs:798-832` — `run_review` blocking path: **the working reference** — has the correct post-review fix logic
- `cli/src/commands/review.rs:640-644` — `create_review`: stores fix_template in scope_data (not the bug, but context)
