# Add timestamps to `aiki task show`

## Problem

`aiki task show` displays status but no timestamps. To figure out *when* a task was created, started, or closed, you have to dig through JJ commit history and parse `[aiki-task]` blocks manually. This makes debugging orchestration timing, identifying bottlenecks, and answering "what happened when?" unnecessarily hard.

## Current state

The `Task` struct already has the fields:
- `created_at: DateTime<Utc>` (always set)
- `started_at: Option<DateTime<Utc>>`
- `closed_at: Option<DateTime<Utc>>`

The show output (`cli/src/commands/task.rs:4154`) renders Task/ID/Status/Priority but never uses these timestamps.

## Design

Add a timestamps section to `aiki task show` output, after Priority and before Summary:

```
Task: Fix auth bug
ID: qotysworupowzkxyknzkworuwlyksmls
Status: closed (done)
Priority: p2
Created:  2026-03-26 15:16 UTC
Started:  2026-03-26 15:20 UTC
Closed:   2026-03-26 16:02 UTC  (42m)
Summary: Fixed by updating X
```

Rules:
- Always show `Created`
- Show `Started` only if `started_at` is set
- Show `Closed` only if `closed_at` is set; append elapsed duration from `started_at` (or `created_at` if never started) in parentheses
- Format: `YYYY-MM-DD HH:MM UTC` (minute precision, UTC always)

## Implementation

### Step 1: Add timestamp lines to show output

**File:** `cli/src/commands/task.rs` around line 4169 (after Priority line)

Insert after the Priority format block:

```rust
// Timestamps
content.push_str(&format!("Created:  {}\n", task.created_at.format("%Y-%m-%d %H:%M UTC")));
if let Some(started) = task.started_at {
    content.push_str(&format!("Started:  {}\n", started.format("%Y-%m-%d %H:%M UTC")));
}
if let Some(closed) = task.closed_at {
    let base = task.started_at.unwrap_or(task.created_at);
    let elapsed = closed - base;
    let elapsed_str = format_duration(elapsed);
    content.push_str(&format!("Closed:   {} ({})\n", closed.format("%Y-%m-%d %H:%M UTC"), elapsed_str));
}
```

### Step 2: Add `format_duration` helper

Simple humanized duration: `<1m`, `3m`, `42m`, `1h 12m`, `2h`, `1d 3h`.

### Testing

- `aiki task show` on a ready task: shows Created only
- `aiki task show` on an in-progress task: shows Created + Started
- `aiki task show` on a closed task: shows Created + Started + Closed with elapsed
- Verify CLAUDE.md "Task Output Format" examples still match (update if needed)
