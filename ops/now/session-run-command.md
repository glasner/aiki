# Add `aiki session next` and `aiki session wait`

## Prerequisites

- **[add-reserved-status.md](add-reserved-status.md)** — Adds `Reserved`
  status and `Reserved`/`Released` events. Must land first so that
  `session next` uses `Reserved` (not `Started`) for the claim lock, and
  `Released` (not `Stopped`) for spawn failure rollback.

## Goal

Move session-level orchestration out of `task run` into dedicated
`session next` / `session wait` commands. This eliminates the overloaded
`--next-session` and `--lane` flags from `task run` and gives the loop
orchestrator clean session-based primitives.

After this, `task run` becomes simple: "run this task ID." No resolution
logic, no chain handling, no lane awareness.

## Motivation

`task run --next-session --lane <lane> --async` does three things:
1. Resolves the next ready session from a parent's subtask DAG
2. Reserves the head task (emits Reserved event, per add-reserved-status.md)
3. Spawns an agent and returns

The return value is the **head** task ID, but `task wait` needs the **tail**
task ID (last to close in a needs-context chain). This mismatch makes
piping between commands fragile. The root issue: `task run` returns a task
ID but the caller needs a session handle.

`session next` returns a **session ID** — the natural unit for waiting.

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

### `aiki session next <parent-id> [--lane <prefix>] [--async]`

Resolve the next ready session from a parent's subtask DAG, claim it,
spawn an agent, and return the **session UUID**.

```
aiki session next <parent-id>                    # next ready session, sync
aiki session next <parent-id> --async            # next ready session, async
aiki session next <parent-id> --lane <prefix>    # scoped to lane, async
aiki session next <parent-id> --lane <prefix> --async
```

**Output:**
- Default: markdown status (via MdBuilder) showing session ID, task(s),
  agent type
- `--output id` / `-o id`: bare session UUID on stdout (for scripting)

**Returns:** session UUID (8-hex-char), not a task ID.

**Algorithm:**
1. Resolve parent ID (prefix matching)
2. Call `resolve_next_session()` or `resolve_next_session_in_lane()`
3. On `Standalone(task)` or `Chain(chain)`:
   a. Reserve head task via `Reserved` event (see add-reserved-status.md)
   b. Spawn agent with chain IDs
   c. **Return the session UUID** from the spawned agent
4. On `AllComplete` / `Blocked` / `NoSubtasks`: error or status message
5. On spawn failure: rollback reservation via `Released` event

**Key change:** `BackgroundHandle` (or the spawn path) must return the
session UUID. Currently it only returns `task_id`. Options:
- Add `session_id: Option<String>` to `BackgroundHandle`
- Compute the session UUID from `AgentSpawnOptions` before spawning (the
  UUID is deterministic from agent_type + external_session_id, so we can
  predict it)
- Read the session file after spawn to get the UUID

The cleanest approach: compute the session UUID in `prepare_task_run()`
or `session_next()` before spawning. The UUID is a deterministic
`UUIDv5(namespace, "{agent_type}:{external_id}")` — we can generate the
external ID (or read it from the session file after the session starts).

However, the external session ID is assigned by the **agent process**
(e.g., Claude Code generates its own session ID at startup). We don't
know it until the agent starts and the hook fires. So pre-computation
isn't possible.

**Practical approach:** After `spawn_background()`, poll for the session
file in `$AIKI_HOME/sessions/` that has `task=<task_id>`. The session
file is created by the hook when the agent starts (typically within
~1 second). This is already how `find_active_session` works. A short
poll (up to 5s) with backoff is acceptable since `session next --async`
returns quickly regardless.

Alternatively: have aiki generate a pre-determined session UUID and pass
it via env var (`AIKI_SESSION_ID`) so the hook uses it instead of
generating one. This makes the UUID available immediately at spawn time.

### `aiki session wait <sid1> [<sid2> ...] [--any]`

Wait for session(s) to complete. A session is "complete" when its session
file has an `ended_at` timestamp, or when the driving task reaches a
terminal state (Closed/Stopped).

```
aiki session wait $sid1 $sid2 --any     # wait for any to finish
aiki session wait $sid                   # wait for one
```

**Detection strategies** (in order of reliability):
1. **Task status:** If session has a `task` field, check if that task is
   Closed/Stopped. This is the most reliable signal.
2. **Session file:** Check for `ended_at` in the session file (written
   by session cleanup).
3. **PID liveness:** Check if `parent_pid` is still running (fallback).

**Absorption:** After detecting completion, wait for workspace absorption
(same logic as current `task wait`).

**Output:**
- Default: markdown table of completed sessions
- `--output id`: bare session IDs of completed sessions

### Loop template update

```bash
while true; do
  ready=$(aiki task lane {{data.target}} -o id)
  [ -z "$ready" ] && break

  sids=()
  for lane in $ready; do
    sid=$(aiki session next {{data.target}} --lane $lane --async -o id)
    sids+=("$sid")
  done

  [ ${#sids[@]} -eq 0 ] && break
  aiki session wait "${sids[@]}" --any
done
```

### Remove `--next-session` and `--lane` from `task run`

Delete these flags entirely. No deprecation period — this is pre-release.

## Changes

### 1. Add `Next` and `Wait` to `SessionCommands` (session.rs)

```rust
pub enum SessionCommands {
    List { ... },
    Show { ... },
    Next {
        /// Parent task ID whose next session to resolve and run
        parent_id: String,
        #[arg(long)]
        lane: Option<String>,
        #[arg(long = "async")]
        run_async: bool,
        #[arg(long, short = 'o')]
        output: Option<OutputFormat>,
    },
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

### 2. Implement `session_next()` (session.rs or new session/next.rs)

Move the `--next-session` logic from `task.rs:run_run()` into a dedicated
function. Core flow:
- Resolve next session (reuse `resolve_next_session` /
  `resolve_next_session_in_lane` from runner.rs)
- Reserve + spawn (reuse `prepare_task_run` / `task_run_async`)
- Discover session UUID (poll session file for `task=<task_id>` match)
- Return session UUID

### 3. Implement `session_wait()` (session.rs)

Poll loop checking session completion:
- Read events, check driving task's terminal status
- On completion, run absorption wait (reuse logic from `task wait`)
- Return completed session info

### 4. Update loop template (core/loop.md)

Replace `task run --next-session` with `session next`, replace
`task wait` with `session wait`.

### 5. Remove `--next-session` / `--lane` from `task run`

Delete the flags, the `next_session` / `lane` fields from the `Run` variant,
and the resolution block in `run_run()` (task.rs:5379-5491). `task run`
becomes: task ID (required) → spawn agent. No resolution, no chains, no
lanes.

Also remove the `--next-subtask` deprecation shim (task.rs:5314-5318).

## Testing

- `aiki session next <parent> --async -o id` → prints 8-hex-char session UUID
- `aiki session next <parent> --lane <prefix> --async -o id` → session UUID
- `aiki session next <parent>` (all complete) → status message, exit 0
- `aiki session next <parent>` (blocked) → error message, exit 1
- `aiki session wait <sid>` → blocks until session ends
- `aiki session wait <sid1> <sid2> --any` → returns when first finishes
- End-to-end: loop template runs correctly with session commands
- `task run <id>` — still works, no --next-session flag accepted
