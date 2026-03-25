# Add `aiki run` and `aiki session wait`

## Prerequisites

- **[add-reserved-status.md](add-reserved-status.md)** — Adds `Reserved`
  status and `Reserved`/`Released` events. Must land first so that
  `aiki run` uses `Reserved` (not `Started`) for the claim lock, and
  `Released` (not `Stopped`) for spawn failure rollback.

## Goal

Replace `task run` with a top-level `aiki run` command that takes a task
and returns a session. Move session lifecycle commands (`wait`, `list`,
`show`) under the `session` namespace.

This eliminates the overloaded `--next-session` and `--lane` flags from
`task run`, gives the loop orchestrator clean primitives, and makes the
most common operation (`run`) a top-level verb.

After this, `task run` is removed entirely. The `task` namespace is
pure CRUD (`add`, `start`, `close`, `show`, `link`, etc.).

## Motivation

`task run --next-session --lane <lane> --async` does three things:
1. Resolves the next ready session from a parent's subtask DAG
2. Reserves the head task (emits Reserved event, per add-reserved-status.md)
3. Spawns an agent and returns

The return value is the **head** task ID, but `task wait` needs the **tail**
task ID (last to close in a needs-context chain). This mismatch makes
piping between commands fragile. The root issue: `task run` returns a task
ID but the caller needs a session handle.

`aiki run` returns a **session ID** — the natural unit for waiting. As a
top-level verb it's the natural bridge: it takes a task (what to do) and
returns a session (the execution).

## Current state

### Session infrastructure (already exists)
- `AikiSession` (`session/mod.rs:336`) — deterministic UUID from
  `(agent_type, external_session_id)`, 8-hex-char truncation
- `AikiSessionFile` (`session/mod.rs:51`) — file at
  `$AIKI_HOME/sessions/{uuid}` with metadata (agent, mode, started_at,
  parent_pid, task)
- `SessionInfo` (`session/mod.rs:685`) — parsed session for listing
- `BackgroundHandle` (`agents/runtime/mod.rs:24`) — currently only holds
  `task_id`, no session UUID
- `find_active_session()` — PID-based session lookup

### Task ↔ session links (already exists)
- `Task.claimed_by_session` — session UUID that claimed the task
- `Task.last_session_id` — most recent session that worked on the task
- `TaskEvent::Started { session_id, .. }` — session ID on start events
- `TaskEvent::Closed { session_id, .. }` — session ID on close events

### What `task run --next-session` currently does (task.rs:5379-5491)
1. Calls `resolve_next_session()` / `resolve_next_session_in_lane()` from
   `runner.rs`
2. Handles `SessionResolution::{Standalone, Chain, AllComplete, Blocked,
   NoSubtasks}`
3. Reserves the head task via `Reserved` event
4. Passes chain IDs to `TaskRunOptions::with_chain()`
5. Calls `run_task_async_with_output()` or `run_task_with_output()`
6. On spawn failure, emits `Released` rollback event

### Existing `session` command (session.rs)
- `aiki session list` — list sessions (active/background/interactive)
- `aiki session show <id>` — show turns for a session

## Design

### `aiki run <task-id> [flags]`

Top-level command. Spawns an agent session for a task and returns the
**session UUID**.

```
aiki run <task-id>                          # spawn + block until session ends (default)
aiki run <task-id> --async                  # spawn + return session UUID after Started event
aiki run <task-id> --agent codex            # override agent type
aiki run --template deploy --data env=prod  # create task from template, then run
aiki run <parent-id> --next-session                 # resolve next ready task
aiki run <parent-id> --next-session --lane <prefix> # resolve next in lane
aiki run <parent-id> --next-session --async         # resolve next, return after spawn
```

The agent is always a background session. `--async` controls whether the
**command** blocks until the session ends (default) or returns immediately
after spawn with the session UUID.

**Flags:**
- `--async` — return after spawn (once `Started` event provides session
  UUID) instead of blocking until the session ends. Both modes block
  through spawn to discover the session UUID.
- `--next-session` — resolve the next ready task from the parent's subtask DAG
  instead of running the given task directly.
- `--lane <prefix>` — scope `--next-session` resolution to a lane (requires
  `--next-session`).
- `--agent <type>` — override the assignee agent type (e.g., `claude-code`,
  `codex`).
- `--template <name>` — create a task from a template before running
  (conflicts with positional `id`).
- `--data <key=value>` — template variable bindings (repeatable, requires
  `--template`).
- `-o id` / `--output id` — bare session UUID on stdout (for scripting).

**Future:** `-i` / `--interactive` mode (open agent in terminal, sync) is
planned separately — see [interactive-aiki-run.md](../next/interactive-aiki-run.md).

**Output:**
- Default: markdown status (via MdBuilder) showing session ID, task(s),
  agent type
- `-o id`: bare session UUID on stdout. **Must produce no output at all**
  (not even a newline) on no-work paths (`AllComplete`, `Blocked`,
  `NoSubtasks`) — callers use empty-string checks to detect these cases.

**Returns:** session UUID (8-hex-char), not a task ID.

**Exit codes:**
- `0` — session spawned successfully (session UUID on stdout with `-o id`)
- `2` — no work available (`AllComplete` — all subtasks done)
- `1` — error (`Blocked`, `NoSubtasks`, spawn failure, invalid args)

**Algorithm:**
1. If `--next-session`: resolve parent ID → call `resolve_next_session()` /
   `resolve_next_session_in_lane()`. On `AllComplete`: exit 2. On
   `Blocked` / `NoSubtasks`: exit 1.
2. Reserve head task via `Reserved` event (see add-reserved-status.md)
3. Spawn background agent with chain IDs
4. Discover session UUID from task events (see below)
5. Return session UUID
6. On spawn failure: rollback reservation via `Released` event

**Session UUID discovery:** The UUID cannot be pre-computed because it's
derived from `(agent_type, external_session_id)` and the external session
ID is assigned by the **agent process** at startup (e.g., Claude Code
generates its own session ID). We don't know it until the agent starts
and the session-start hook fires.

After spawning, poll the task's event log for a `Started { session_id,
.. }` event. The session-start hook emits this event when the agent
process starts (typically within ~1s). Once the `session_id` is read
from the event, return it immediately.

This follows the same event-watching pattern used elsewhere in aiki
(e.g., `task wait` watches for terminal status events). Poll up to 5s
with backoff; timeout is a spawn failure.

Add `session_id: Option<String>` to `BackgroundHandle` so the resolved
UUID is available to callers after the spawn-and-wait returns.

### `aiki session wait <sid1> [<sid2> ...] [--any]`

Wait for session(s) to complete. A session is "complete" when its
session-end event is observed — the session lifecycle is the authoritative
signal, not task status. (A session may close multiple tasks, or close
zero tasks if it crashes; task completion is not equivalent to session
completion.)

```
aiki session wait $sid1 $sid2 --any     # wait for any to finish
aiki session wait $sid                   # wait for one
```

**Detection:** Poll the session's event log for the session-end event
(emitted by the session-stop hook when the agent process exits). This
follows the same event-watching pattern as `aiki run` for the `Started`
event.

**Absorption:** After detecting completion, wait for workspace absorption
(same logic as current `task wait`).

**Output:**
- Default: reuse `print_session_table` (same format as `session list`)
  with status reflecting the terminal state (e.g., "ended", "crashed").
  With `--any`, only completed sessions are shown.
- `--output id` / `-o id`: bare session IDs of completed sessions (one
  per line), for scripting.

### Loop template update

```bash
while true; do
  ready=$(aiki task lane {{data.target}} -o id)
  [ -z "$ready" ] && break

  sids=()
  for lane in $ready; do
    sid=$(aiki run {{data.target}} --next-session --lane $lane --async -o id) || {
      rc=$?
      [ $rc -eq 2 ] && continue  # AllComplete for this lane
      exit $rc                    # real error
    }
    [ -n "$sid" ] && sids+=("$sid")
  done

  [ ${#sids[@]} -eq 0 ] && break
  aiki session wait "${sids[@]}" --any
done
```

### Remove `task run` entirely

Delete the `Run` variant from `TaskCommands`, the `run_run()` function,
and all supporting code (resolution block at task.rs:5379-5491,
`--next-subtask` deprecation shim at task.rs:5314-5318). No deprecation
period — this is pre-release.

The `task` namespace becomes pure CRUD: `add`, `start`, `stop`, `close`,
`show`, `link`, `comment`, `lane`, `diff`, `wait`, `template`.

## Changes

### 1. Add top-level `Run` command (main.rs + new commands/run.rs)

```rust
// In main.rs CLI enum
/// Spawn an agent session for a task
Run {
    /// Task ID to run (or parent ID with --next-session)
    id: Option<String>,
    /// Return after spawn instead of blocking until session ends
    #[arg(long = "async")]
    run_async: bool,
    #[arg(long)]
    next_session: bool,
    #[arg(long, requires = "next_session")]
    lane: Option<String>,
    /// Override assignee agent (claude-code, codex)
    #[arg(long)]
    agent: Option<String>,
    /// Create task from template before running
    #[arg(long, conflicts_with_all = ["id", "next_session"])]
    template: Option<String>,
    /// Key=value pairs for template variables
    #[arg(long, requires = "template")]
    data: Option<Vec<String>>,
    #[arg(long, short = 'o')]
    output: Option<OutputFormat>,
},
```

### 2. Implement `run()` (commands/run.rs)

Move the spawn logic from `task.rs:run_run()` into a dedicated module.
Core flow:
- If `--next-session`: resolve next session (reuse `resolve_next_session` /
  `resolve_next_session_in_lane` from runner.rs)
- Reserve + spawn (reuse `prepare_task_run` / agent spawn path)
- Discover session UUID (poll task events for `Started { session_id }`)
- Return session UUID

### 3. Add `Wait` to `SessionCommands` (session.rs)

```rust
pub enum SessionCommands {
    List { ... },
    Show { ... },
    Wait {
        /// Session IDs to wait for
        ids: Vec<String>,
        #[arg(long)]
        any: bool,
        #[arg(long, short = 'o')]
        output: Option<OutputFormat>,
    },
}
```

### 4. Implement `session_wait()` (session.rs)

Poll loop checking session completion:
- Poll for session-end event
- On completion, wait for workspace absorption
- Output via `print_session_table` (reuse existing)
- Return completed session info

### 5. Update loop template (core/loop.md)

Replace `task run --next-session` with `aiki run --next-session`, replace
`task wait` with `session wait`.

### 6. Remove `task run`

Delete the `Run` variant from `TaskCommands`, `run_run()`, and all
`--next-session` / `--lane` / `--next-subtask` handling from task.rs.

## Testing

- `aiki run <task> -o id` → prints 8-hex-char session UUID
- `aiki run <parent> --next-session -o id` → resolves next, prints session UUID
- `aiki run <parent> --next-session --lane <prefix> -o id` → lane-scoped
- `aiki run <parent> --next-session` (all complete) → status message, exit 0
- `aiki run <parent> --next-session` (blocked) → error message, exit 1
- `aiki session wait <sid>` → blocks until session ends
- `aiki session wait <sid1> <sid2> --any` → returns when first finishes
- End-to-end: loop template runs correctly with new commands
- `aiki task run` → unrecognized command (removed)
