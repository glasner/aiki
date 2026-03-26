# Won't-Do Visibility in Task List

**Date**: 2026-03-26
**Status**: Draft
**Purpose**: Align this design note with shipped behavior for won't-do visibility in reviews, so the documented template flow and task-list output match the current implementation.

**Related Documents**:
- [smarter-task-diff.md](smarter-task-diff.md) — Workspace isolation leak investigation (discovered this problem)
- [.aiki/tasks/review/task.md](../../.aiki/tasks/review/task.md) — Current review template behavior referenced by this note

---

## Executive Summary

Fix loops waste rounds re-raising issues that were previously closed as won't-do. Observed: the "unrelated file in diff" issue was raised 5 times across 9 review rounds on epic `tnnuxwm`, each time going through plan→decompose→won't-do — pure overhead. The fix is to surface won't-do exclusions to reviewers so they skip known dismissals.

---

## Shipped Behavior

### Task list flags

```bash
# Outcome filters (refine --closed)
aiki task list --closed              # all closed (existing, unchanged)
aiki task list --done                # closed with outcome=done only
aiki task list --wont-do             # closed with outcome=wont_do only

# Scoping filter (accepts short or long task IDs)
aiki task list --descendant-of <id>  # only tasks in subtree of <id>

# Combined: "what was dismissed in this epic?"
aiki task list --wont-do --descendant-of tnnuxwm
```

### Output

Filtered task-list views use the generic filtered-list renderer:

```
Tasks (2):
[p2] ktvrsqt  Close unrelated plan file issue as won't-do
  ↳ Process issue, not a code fix. The unrelated plan file was included inadvertently.
[p2] xxother  Some other dismissed issue
  ↳ Already handled by existing code in graph.rs
```

### Flag interactions

| Flags | Behavior |
|---|---|
| `--closed` | All closed tasks (done + wont_do) — existing behavior |
| `--done` | Only closed tasks with outcome=Done |
| `--wont-do` | Only closed tasks with outcome=WontDo |
| `--done --wont-do` | Same as `--closed` (both outcomes) |
| `--done --open` | Open tasks + done-closed tasks (composable) |
| `--descendant-of X` | Filter to subtree of task X (accepts short or long IDs; works with any status filter) |

---

## How It Works

### 1. Outcome filtering in `run_list`

`run_list` supports outcome-aware filtering for closed tasks, so reviewers can ask
for only done tasks, only won't-do tasks, or both.

**File:** `cli/src/commands/task.rs`

The `List` subcommand includes these flags:
```rust
/// Filter to closed tasks with outcome=done
#[arg(long)]
done: bool,

/// Filter to closed tasks with outcome=wont_do
#[arg(long)]
wont_do: bool,
```

The status filter parsing treats these flags as closed-task filters:
```rust
let mut filter_done = done;
let mut filter_wont_do = wont_do;

// --done and --wont-do imply closed
if filter_done || filter_wont_do {
    filter_closed = true;
}
```

When `filter_closed` is active, task matching narrows the result to the requested
closed outcome:
```rust
if task.status == TaskStatus::Closed {
    if filter_done && !filter_wont_do {
        task.closed_outcome == Some(TaskOutcome::Done)
    } else if filter_wont_do && !filter_done {
        task.closed_outcome == Some(TaskOutcome::WontDo)
    } else {
        true // --closed or --done --wont-do: both outcomes
    }
}
```

The existing `--status` flag also accepts `"done"` and `"wont_do"`, so
`--status done` works as an alias.

### 2. Descendant-of scoping

The list command also supports subtree scoping:
```rust
/// Filter to descendants (subtree) of a task (accepts short or long IDs)
#[arg(long)]
descendant_of: Option<String>,
```

`run_list` resolves the requested ancestor task first, then walks that task's
full descendant set and filters the list against those IDs:

```rust
let descendant_set: Option<HashSet<String>> = if let Some(ref ancestor_id) = descendant_of {
    let resolved = find_task(tasks, ancestor_id)?;
    let descendants = get_all_descendants(&graph, &resolved.id);
    Some(descendants.into_iter().map(|t| t.id.clone()).collect())
} else {
    None
};

// In matching closure:
let matches_descendant = |task: &Task| -> bool {
    descendant_set.as_ref().map_or(true, |set| set.contains(&task.id))
};
```

This is what lets reviewers ask "what was dismissed in this review scope?" without
leaking won't-do tasks from unrelated work. If the requested task ID does not
exist, `find_task(...)` returns an error and the command exits cleanly rather
than implying an `unwrap()` panic path.

### 3. Review template scoping

**File:** `.aiki/tasks/review/task.md`

The review template scopes the won't-do lookup to the reviewed task's subtree by
reusing the existing `scope.id` field:

```markdown
Then check for prior exclusions (issues closed as won't-do in earlier rounds):

\```bash
aiki task list --wont-do --descendant-of {{data.scope.id}}
\```

If any won't-do tasks are listed, note them — these are explicitly out of scope.
```

This keeps the template data model simple: the review already knows which task it
is validating via `scope.id`, and `--descendant-of` expands that task to its full
subtree. No additional `root_id` template field is required or implied.

In the "Review & Record Issues" subtask, the shipped guidance says:

```markdown
**Before recording an issue**, check the won't-do list from the explore phase.
Do NOT re-raise issues that were previously closed as won't-do unless you have
new evidence that the original dismissal was wrong.
```

---

## Documentation Alignment

This note should describe the current system, not propose additional template or
CLI work:

- The review template already scopes the won't-do lookup through `{{data.scope.id}}`.
- The lookup stays within the reviewed task's subtree via `--descendant-of`.
- The filtered task-list renderer emits the generic `Tasks (N):` header.
- There is no separate `root_id` template field in this flow.

---

## Testing

- `aiki task list --done` shows only done-closed tasks
- `aiki task list --wont-do` shows only wont_do-closed tasks
- `aiki task list --done --wont-do` equals `--closed`
- `aiki task list --status done` works as alias
- `aiki task list --wont-do --descendant-of <epic-id>` returns only won't-do tasks in that subtree
- `aiki task list --descendant-of <id>` with non-existent ID errors cleanly
- Short IDs work for `--descendant-of`

---

## Resolution

The implementation has already settled the open question from earlier drafts: review templates use `data.scope.id`, and the won't-do query scopes itself through `--descendant-of {{data.scope.id}}`. The renderer also emits `Tasks (N):` for filtered task-list output. This document should treat that shipped behavior as the source of truth.
