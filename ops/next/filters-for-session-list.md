# Better discoverability for `aiki session list` filters

## Problem

`aiki session list` already has useful filters (`--active`, `--background`, `--interactive`, `--limit`), but they're not discoverable. During a debugging session investigating orchestrator timing, the full unfiltered list was used instead of `--active` — producing 800KB+ of output when only 2 active sessions existed.

This is a discoverability/documentation issue, not a missing feature.

## Current filters

```
--active         Show only active sessions (with running agent process)
--background     Show only background sessions (created by `aiki run`)
--interactive    Show only interactive sessions (user-driven)
--limit <LIMIT>  Maximum number of sessions to show (default: all)
```

## Proposed additions

### 1. `--recent <N>` (alias for `--limit`)

More intuitive for the common case. "Show me recent sessions" is the natural phrasing.

```bash
aiki session list --recent 5
```

### 2. `--task <task-id>`

Filter sessions by task association. Show only sessions that worked on a specific task.

```bash
aiki session list --task sunmrlvpprnomtzqtmwpuwryltkvmmnv
```

This requires cross-referencing task events (the `started` event records `session_id`) with the session list. Useful for answering "which sessions touched this task?"

### 3. Default limit when TTY

When stdout is a terminal, default to `--limit 20` instead of showing all. Override with `--all` or `--limit 0`.

## Implementation

### Step 1: Add `--recent` as alias for `--limit`

**File:** CLI arg parser for session list.

Trivial — just add a `--recent` alias that maps to the same field as `--limit`.

### Step 2: Add `--task` filter

**File:** Session list command.

After loading sessions, filter to only those whose ID appears in the task's event history (from `started` events). Use `read_task_events()` to get session IDs.

### Step 3: Default limit on TTY

Check `stdout().is_terminal()`. If true and no explicit `--limit`/`--recent`/`--all`, default to 20.

### Step 4: Update CLAUDE.md

Add session list filters to the quick reference or a new "Debugging" section so agents discover them.

## Testing

- `--recent 5` shows exactly 5 sessions
- `--task <id>` shows only sessions associated with that task
- TTY default: verify 20-row default, `--all` overrides
- Pipe to file: verify no default limit
