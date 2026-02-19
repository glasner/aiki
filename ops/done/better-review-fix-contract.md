# Structured Review Issues: `aiki review issue` + `data.issues_found`

**Date**: 2026-02-18
**Status**: Draft
**Priority**: P2

**Related Documents**:
- [Conditional Task Spawning](spawn-tasks.md) — Uses `data.issues_found` in spawn conditions
- [Cleanup Review Templates](cleanup-review-templates.md) — Restructures review templates
- [Review-Fix Workflow](loop-flags.md) — Fix loop uses review outcome

---

## Problem

The current review/fix contract is too rigid and relies on implicit conventions:

1. **Issue detection is heuristic.** `aiki fix` decides whether to create followup tasks by checking if `review_task.comments` is non-empty. Any comment — progress updates, questions, notes — is treated as an issue. There's no way to distinguish "this is a finding" from "this is a note."

2. **Issue count is fragile.** `aiki review list` counts `task.comments.len()` as the issue count. This breaks when comments are used for anything other than issues.

3. **Spawn conditions can't reliably check review outcome.** The `spawns:` system (spawn-tasks.md) needs `data.issues_found > 0` to decide whether to spawn a fix task, but there's no reliable way to set that field today — it depends on agents following prose instructions.

4. **The template contract is too loose.** The review template tells agents to "leave a separate comment for each issue" and "the number of comments you left determines whether the review passes or has issues." This works when agents follow instructions perfectly, but conflates the comment mechanism with the issue-tracking mechanism.

---

## Solution

Three changes that tighten the contract:

### 1. `aiki review issue add` command

A new subcommand that adds a comment with `data.issue = true`:

```bash
aiki review issue add <review-id> "Description of the issue"
```

This is sugar for:
```bash
aiki task comment <review-id> --data issue=true "Description of the issue"
```

The `--data` mechanism already exists on `aiki task comment`. The new command provides a dedicated entry point that's easier to discover and harder to misuse.

### 2. Count issues on review close → `data.issues_found`

When a review task closes, the close codepath (not the hook system) counts comments where `data.issue == "true"` and writes a `TaskDataSet` event setting `data.issues_found = N`. This runs synchronously in `close_task()` using the shared `get_issue_comments()` function.

### 3. `aiki fix` uses `data.issues_found` instead of `comments.len()`

Update `fix.rs` to check `data.issues_found > 0` instead of `comments.is_empty()`. When filtering comments to create fix subtasks, only include comments where `data.issue == "true"`.

---

## Design Details

### `aiki review issue` subcommand

```
aiki review issue add <REVIEW_ID> <TEXT>

Arguments:
  <REVIEW_ID>  The review task to add an issue to
  <TEXT>        Description of the issue

Options:
  -h, --help   Print help
```

**Validation:**
- The target task must be a review task (task_type == "review")
- The target task must not be closed

**Implementation:** Calls the existing `comment_on_task()` with `data: { "issue": "true" }`. No new event type needed — `CommentAdded` already supports a `data` HashMap.

> **Note:** This command must go through the same codepath as `aiki task comment --data issue=true`. Do not duplicate the comment-writing logic — call the shared implementation directly. This prevents behavioral divergence if the comment or data persistence logic ever changes.

### `aiki review issue list` subcommand

Lists all issue comments on a review task:

```bash
aiki review issue list <REVIEW_ID>
```

Output: one line per issue comment (id, text). This is the canonical read path for issue data — both `aiki fix` and any future tooling should use this (or its underlying function) rather than filtering `task.comments` directly.

### Exposing comment data on `TaskComment`

The `TaskComment` struct currently has `id`, `text`, and `timestamp`. The `CommentAdded` event already carries a `data: HashMap<String, String>` but it's discarded during materialization.

Add `data: HashMap<String, String>` to `TaskComment` so it's available for filtering:

```rust
pub struct TaskComment {
    pub id: Option<String>,
    pub text: String,
    pub timestamp: DateTime<Utc>,
    pub data: HashMap<String, String>,  // NEW
}
```

Update `graph.rs` materialization to populate this field from the event.

### Count issues on review close

When a review task is closed, `data.issues_found` is set as part of the task close codepath — not via the hook system. This keeps the logic in Rust where it's easy to test and reason about, and avoids depending on hook expression capabilities.

**Implementation:** In `close_task()` (or equivalent), after writing the `TaskClosed` event, check if the task is a review task. If so, call `get_issue_comments()` (the shared function from R2b), count the results, and write a `TaskDataSet` event setting `data.issues_found = N`.

**Timing:** This runs synchronously as part of closing the task, before control returns to the caller. Spawn conditions (which check `data.issues_found`) are evaluated after close, so the value is always available.

**Idempotency:** Setting `data.issues_found` is idempotent — re-closing or replaying produces the same value.

### Updated `aiki fix` behavior

Current flow:
1. Check `review_task.comments.is_empty()` → if empty, approve
2. If comments exist, create fix task from all comments

New flow:
1. Check `review_task.data["issues_found"]` → if missing or "0", approve
2. If `issues_found > 0`, create fix task
3. **Call the same underlying function as `aiki review issue list`** to get the issue comments — do not filter `task.comments` inline in `fix.rs`

**Backward compatibility:** If `data.issues_found` is not set (older review tasks), fall back to `comments.len() > 0` to avoid breaking existing workflows. In fallback mode, treat all comments as issues (existing behavior).

### Updated review template

Replace the prose instruction:

```markdown
## Review

For **each issue** found, use the issue command:

```bash
aiki review issue add {{parent.id}} "Description of the issue"
```

Each issue becomes a trackable fix item. Regular comments (`aiki task comment`) are for progress notes and won't trigger fixes.
```

### Updated `aiki review list` output

Change the "Issues" column to use `data.issues_found` instead of `comments.len()`:

```
| ID | Status | Outcome | Issues | Name |
```

Fall back to comment count for older reviews without `data.issues_found`.

---

## Requirements

### R1: Add `data` field to `TaskComment`
- Add `pub data: HashMap<String, String>` to `TaskComment` struct
- Update `graph.rs` materialization to populate from `CommentAdded` event data
- Update `graph.rs` (change_id pass) to also preserve data

### R2: Add `aiki review issue add` subcommand
- New subcommand under `aiki review issue`
- Takes review task ID and issue text as arguments
- Validates target is a review task and not closed
- Calls existing comment infrastructure with `data.issue = "true"`
- **Must share the same codepath as `aiki task comment --data`** — do not duplicate comment logic
- Lives in `cli/src/commands/review.rs` (extends `ReviewSubcommands`)

### R2b: Add `aiki review issue list` subcommand
- New subcommand under `aiki review issue`
- Takes review task ID as argument
- Returns all comments where `data.issue == "true"`, one per line
- Implemented via a shared `get_issue_comments(task)` function (not inline filtering)
- `aiki fix` must use this same function — not its own inline filter

### R3: Set `data.issues_found` in task close codepath
- In `close_task()`, after writing `TaskClosed`, check if task is a review task
- Call `get_issue_comments()` (from R2b) and count results
- Write a `TaskDataSet` event setting `data.issues_found = N` (as string, consistent with data field types)
- Must complete before control returns to caller (spawn conditions depend on this value)
- No hook system involvement — pure Rust in the close codepath

### R4: Update `aiki fix` to use structured issues
- Check `data.issues_found` > 0 instead of `comments.is_empty()`
- Use the shared `get_issue_comments()` function (from R2b) to get issues — do not filter inline
- Fall back to `comments.len()` for backward compatibility (treat all comments as issues)

### R5: Update review template
- Replace "leave a comment" instructions with `aiki review issue add`
- Clarify that regular comments are for progress notes

### R6: Update `aiki review list` issue count
- Use `data.issues_found` when available
- Fall back to `comments.len()` for older reviews

---

## Non-Goals

- Severity levels on issues (blocking vs. nit) — keep it boolean for now
- File path / location references on issues — just free text
- A separate `issue` event type — reuses `CommentAdded` with data
- Changing how `aiki task comment --data` works — it already supports this

---

## Acceptance Criteria

- [ ] `TaskComment` has a `data` field populated from event data
- [ ] `aiki review issue add <id> "text"` creates a comment with `data.issue = true`
- [ ] `aiki review issue add` validates the target is a review task
- [ ] `aiki review issue add` rejects closed review tasks
- [ ] `aiki review issue list <id>` returns only comments where `data.issue == "true"`
- [ ] `aiki fix` and `aiki review issue list` share the same `get_issue_comments()` function
- [ ] Closing a review task sets `data.issues_found = N` in the close codepath (not via hook)
- [ ] `aiki fix` creates followup only when `data.issues_found > 0`
- [ ] `aiki fix` uses `get_issue_comments()` to get issues (not inline filtering)
- [ ] `aiki fix` falls back to comment count for older reviews without `data.issues_found`
- [ ] Review template uses `aiki review issue add` instead of `aiki task comment`
- [ ] `aiki review list` shows `data.issues_found` in Issues column
- [ ] Non-issue comments (progress notes) don't affect fix behavior

---
