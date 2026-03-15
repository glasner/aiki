# `aiki build --fix` Flag Issues

Bugs found tracing `--fix` through the full `build → review → fix` pipeline.

## Bug 1 (High): Stopped/Detached agent results silently continue the pipeline

`handle_session_result` in `runner.rs:529-583` returns `Ok(())` for both `Stopped` and `Detached` variants:

```rust
// runner.rs:545
AgentSessionResult::Stopped { reason } => {
    // ... emits stop event ...
    // returns Ok(()) — pipeline continues!
}
// runner.rs:575
AgentSessionResult::Detached => {
    // ... prints message ...
    // returns Ok(()) — pipeline continues!
}
```

In `run_build_review` (`build.rs:868-875`), after `task_run_on_session` + `handle_session_result`, the code proceeds to check `has_issues`. If the review agent was stopped or the user detached (Ctrl+C):

1. Review task has no `issue_count` data
2. `has_issues` returns `false`
3. Fix step is silently skipped
4. `output_build_review_completed` prints success

**Impact:** Ctrl+C during review loading screen → review still running in background, but CLI declares success and skips fix.

### Fix

`run_build_review` should check the `AgentSessionResult` variant before proceeding. `Stopped` and `Detached` should either return early or propagate an error — not silently continue the pipeline.

## Bug 2 (Medium): `has_issues` check in build.rs diverges from fix.rs

`build.rs:880-884`:
```rust
let has_issues = find_task(&graph.tasks, &result.review_task_id)
    .map(|t| t.data.get("issue_count")
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0) > 0)  // returns false if missing
    .unwrap_or(false);
```

`fix.rs:635-646`:
```rust
fn has_actionable_issues(review_task: &Task) -> bool {
    if let Some(issue_count) = review_task.data.get("issue_count") {
        match issue_count.parse::<usize>() {
            Ok(n) => n > 0,
            Err(_) => !super::review::get_issue_comments(review_task).is_empty(),
        }
    } else {
        !review_task.comments.is_empty()  // fallback to comments
    }
}
```

`build.rs` returns `false` when `issue_count` is missing or unparseable. `fix.rs` falls back to checking comments. If the review agent records issues as comments but doesn't set `data.issue_count`, `build.rs` silently skips the fix. The same divergent check also exists in `review.rs:806-810`.

### Fix

Replace the inline check in `build.rs` (and `review.rs`) with a call to `fix::has_actionable_issues`. Make the function `pub(crate)` if it isn't already.

## Bug 3 (Low): Async flag forwarding inconsistency

`run_build_plan` async (`build.rs:353-356`):
```rust
if let Some(ref tmpl) = fix_template {
    spawn_args.push("--fix-template".to_string());
    spawn_args.push(tmpl.clone());
}
```

`run_build_epic` async (`build.rs:529-536`):
```rust
if fix_after {
    if let Some(ref tmpl) = fix_template {
        spawn_args.push("--fix-template".to_string());
        spawn_args.push(tmpl.clone());
    } else {
        spawn_args.push("--fix".to_string());
    }
}
```

`run_build_plan` is missing the outer `if fix_after` guard and the `else` branch for `--fix`. Not a functional bug today (the resolution at line 130 ensures `fix_template` is always `Some` when `fix` is true), but the inconsistency could become a bug if the resolution logic changes.

### Fix

Align `run_build_plan` async forwarding with `run_build_epic` — add the `if fix_after` guard and the `--fix` fallback.

## Bug 4 (Low): UX — `output_build_completed` always shows review hint

`build.rs:948-951`:
```rust
content.push_str(&format!(
    "\n---\nRun `aiki review {}` to review.\n",
    epic_id
));
```

This is unconditional — even when `--fix`/`--review` is about to auto-trigger the review. Users see "Run `aiki review ...` to review" right before the review auto-starts.

### Fix

Pass `review_after` to `output_build_completed` and suppress the hint when the review is about to auto-run.

## Summary

| # | Severity | Bug | Location |
|---|----------|-----|----------|
| 1 | **High** | Stopped/Detached agent results don't fail pipeline | `runner.rs:545,575` → `build.rs:868-875` |
| 2 | **Medium** | `has_issues` check diverges from `fix.rs` (no fallback) | `build.rs:880-884` vs `fix.rs:635-646` |
| 3 | Low | Async flag forwarding inconsistency | `build.rs:353` vs `build.rs:529` |
| 4 | Low | Review hint shown even when auto-review is about to run | `build.rs:948` |
