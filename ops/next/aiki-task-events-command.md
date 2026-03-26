# `aiki task events` command

## Problem

Task lifecycle events (created, reserved, started, stopped, closed, comment_added, link_added, absorbed) are stored as JJ commits on the `aiki/tasks` branch but have no CLI surface. Investigating task timing requires parsing raw JJ log output with regex, manually decoding `[aiki-task]` blocks, and sorting by timestamp. This came up when debugging orchestrator behavior — understanding what happened between loop completion and review dispatch required ~10 manual JJ queries.

## Current state

Events are stored as JJ commits with structured descriptions:
```
[aiki-task]
event=started
task_id=abc123...
session_id=55c6178d
timestamp=2026-03-26T20:02:41.018775+00:00
[/aiki-task]
```

The `TaskEvent` enum (`cli/src/tasks/types.rs:113`) has variants: Created, Started, Stopped, Closed, Reserved, Released, CommentAdded, LinkAdded, Absorbed, Set, Unset, Reopened.

The storage layer (`cli/src/tasks/storage.rs`) already reads and parses all events via `read_events()` / `read_task_events()`. The materialization pipeline sorts by timestamp.

## Design

New subcommand: `aiki task events <task-id>`

Output: chronological list of events with timestamps, one per line.

```
$ aiki task events sunmrlvpprnomtzqtmwpuwryltkvmmnv

2026-03-26 20:02:15  created      Review: Code (thread-runner.md)
2026-03-26 20:02:15  link_added   sourced-from → file:ops/now/thread-runner.md
2026-03-26 20:02:15  link_added   validates → onnlrwn
2026-03-26 20:02:16  reserved     agent: claude-code
2026-03-26 20:02:41  started      session: b1801688
2026-03-26 20:07:34  comment      [issue/high] spawn_blocking missing AIKI_THREAD...
2026-03-26 20:07:43  comment      [issue/med] redundant find_thread_session calls...
2026-03-26 20:08:28  closed       done (5m 47s)
```

### Flags

- `--include-children` — also show events for subtasks (interleaved chronologically), prefixed with short task ID
- `--json` — machine-readable output (array of event objects)

### With `--include-children`

```
$ aiki task events onnlrwntommtvtnzovwromnkyulorwtz --include-children

2026-03-26 15:16:51  onnlrwn  created      Epic: Lane Runner: Thread scoping
2026-03-26 15:16:52  lsptzqn  created      Add ThreadId struct...
2026-03-26 15:16:53  mvrstlq  created      Add --thread flag...
...
2026-03-26 15:21:16  lsptzqn  started      session: b61f6c7c
2026-03-26 15:25:35  svykouy  closed       done (3m 23s)
...
```

## Implementation

### Step 1: Add `events` subcommand to CLI

**File:** `cli/src/commands/task.rs`

Add `Events` variant to the task subcommand enum with args:
- `id: String` — task ID
- `--include-children` flag
- `--json` flag

### Step 2: Read and filter events

Use existing `read_task_events()` from storage layer. For `--include-children`, collect subtask IDs from the task graph edges (`subtask-of` referrers) and merge their events.

Sort all events by timestamp.

### Step 3: Format output

Each event type maps to a display string:
- `Created` → `created` + task name
- `Started` → `started` + session ID
- `Stopped` → `stopped` + reason (if any)
- `Closed` → `closed` + outcome + elapsed since last start
- `Reserved` → `reserved` + agent type
- `CommentAdded` → `comment` + truncated text (80 chars), prefixed with `[issue/severity]` if issue
- `LinkAdded` → `link_added` + kind + target
- `Absorbed` → skip (internal bookkeeping, noisy)

### Step 4: Update CLAUDE.md

Add `aiki task events <id>` to the quick reference section.

### Testing

- Single task: verify chronological order, correct event types
- With `--include-children` on a parent: verify interleaved output
- `--json`: verify valid JSON array output
- Task with no events beyond creation: verify single-line output
