# Add `Reserved` status to task lifecycle

## Goal

Add a `Reserved` status between `Open` and `InProgress` so that the
orchestrator can reserve a task (preventing double-pick) without
pretending work has started. The spawned agent then transitions from
`Reserved → InProgress` when it actually begins working, at which point
`claimed_by_session` gets populated with the real session ID.

## Motivation

Today `task run --next-session` emits a `Started` event with
`session_id: None` as a reservation lock. This conflates "reserved so
nobody else grabs it" with "an agent is actively working." Problems:

1. **Agent can't start its own task** — the orchestrator already moved
   the task to `InProgress`, so when the spawned agent runs
   `aiki task start`, the guard rejects it (already in progress). The
   agent that does the work never gets to claim the task itself.
2. **Wrong provenance** — the `Started` event has no session_id because
   the agent hasn't launched yet
3. **Misleading status** — `InProgress` implies active work; the agent
   may still be spawning
4. **Ugly rollback** — spawn failure requires `Stopped` event, making it
   look like real work happened then failed
5. **session-run-command prerequisite** — the planned `session next`
   command needs clean claim semantics before we can separate
   orchestration from `task run`

## New lifecycle

```
Open → [Reserved event] → Reserved → [Started event] → InProgress → Stopped/Closed
                                                                        ↓
                                                                     InProgress (re-start)
```

Rollback on spawn failure:
```
Reserved → [Released event] → Open
```

**Vocabulary:**
- **Reserve** = lock/hold (orchestrator, no session yet)
- **Release** = give up reservation (spawn failed, or orchestrator changed
  its mind)
- **Claim** = ownership with identity (`claimed_by_session` populated by
  `Started` event when agent actually starts)

## Design

### New `TaskStatus::Reserved`

```rust
pub enum TaskStatus {
    Open,
    Reserved,     // NEW: reserved by orchestrator, agent not yet working
    InProgress,
    Stopped,
    Closed,
}
```

- Display: `"reserved"`
- **Not terminal** — `is_terminal()` unchanged (only Closed/Stopped)
- **Not visible in ready queue** — `resolve_next_session` skips Reserved
  tasks (same as InProgress)
- **`claimed_by_session` is `None`** — no session exists yet; the real
  claim happens at `Started`

### New `TaskEvent::Reserved`

```rust
TaskEvent::Reserved {
    task_ids: Vec<String>,
    /// Who is reserving (e.g., "claude-code") — known at reserve time
    agent_type: String,
    timestamp: DateTime<Utc>,
}
```

**`agent_type` is a raw `String`** — same as `TaskEvent::Started`. The
existing pattern across all events uses `AgentType::as_str()` serialized
to `String`, not the typed enum. Follow that convention for consistency.

**No `session_id`** — that's the whole point. The session doesn't exist
yet.

### New `TaskEvent::Released`

Release a reservation. Transitions `Reserved → Open`. Used when a spawn
fails or when an orchestrator decides to give up a reservation.

```rust
TaskEvent::Released {
    task_ids: Vec<String>,
    reason: Option<String>,
    timestamp: DateTime<Utc>,
}
```

This replaces the current pattern of emitting `Stopped` on spawn failure
(which is semantically wrong — the task was never started).

### `Started` event — unchanged

`TaskEvent::Started` keeps its existing shape. It still transitions to
`InProgress` and carries `session_id` (the real claim). The only
behavioral change: it can now transition from both `Open` and `Reserved`
(not just `Open`).

## Changes

### 1. `tasks/types.rs` — Add status variant and events

- Add `Reserved` to `TaskStatus` enum (between Open and InProgress)
- Add `Display` arm: `"reserved"`
- Add `TaskEvent::Reserved { task_ids, agent_type, timestamp }`
- Add `TaskEvent::Released { task_ids, reason, timestamp }`

### 2. `tasks/graph.rs` — Handle new events in materialization

In `materialize_graph_refs`:

```rust
TaskEvent::Reserved { task_ids, .. } => {
    for task_id in task_ids {
        if let Some(task) = tasks.get_mut(task_id) {
            if task.status != TaskStatus::Open {
                eprintln!("warn: Reserved event for task {} in status {}, expected Open — skipping",
                    &task_id[..6], task.status);
                continue;
            }
            task.status = TaskStatus::Reserved;
            // Don't set claimed_by_session — no session yet
            // Don't set started_at — work hasn't started
        }
    }
}
TaskEvent::Released { task_ids, .. } => {
    for task_id in task_ids {
        if let Some(task) = tasks.get_mut(task_id) {
            if task.status != TaskStatus::Reserved {
                eprintln!("warn: Released event for task {} in status {}, expected Reserved — skipping",
                    &task_id[..6], task.status);
                continue;
            }
            task.status = TaskStatus::Open;
        }
    }
}
```

### 3. `tasks/storage.rs` — Serialize/deserialize new events

Add serialization for `Reserved` and `Released` events in `write_event`
and `read_events`. Follow existing patterns (YAML-based event files on
`aiki/tasks` branch).

### 4. `tasks/runner.rs` — Two reserve sites

**`prepare_task_run` (line ~229):**
Change the pre-spawn event from `Started` to `Reserved` when
`task.status == TaskStatus::Open`:

```rust
if task.status == TaskStatus::Open {
    let reserve = TaskEvent::Reserved {
        task_ids: vec![task_id.to_string()],
        agent_type: agent_type.as_str().to_string(),
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &reserve)?;
}
```

The agent's hook (via `aiki task start`) still emits `Started` with
session_id, transitioning `Reserved → InProgress` and populating
`claimed_by_session`.

**Spawn failure rollback (line ~631):**
Change from `Stopped` to `Released`:

```rust
if task.status == TaskStatus::Reserved {
    let rollback = TaskEvent::Released {
        task_ids: vec![task_id.to_string()],
        reason: Some(format!("Spawn failed: {}", e)),
        timestamp: chrono::Utc::now(),
    };
    write_event(cwd, &rollback)?;
}
```

**Second `prepare_task_run` site (~line 609):** Same change.

### 5. `commands/task.rs` — `--next-session` path emits `Reserved`

In `run_run` (lines 5401 and 5428), change `TaskEvent::Started` to
`TaskEvent::Reserved`:

```rust
// Reserve the subtask to prevent double-pick
let reserved_event = TaskEvent::Reserved {
    task_ids: vec![task.id.clone()],
    agent_type: agent.clone().unwrap_or_default(),
    timestamp: chrono::Utc::now(),
};
write_event(cwd, &reserved_event)?;
```

And change the rollback at line ~5510 from `Stopped` to `Released`.

### 6. `tasks/runner.rs` — Ready queue filters

`resolve_next_session` and `resolve_next_session_in_lane` filter
`t.status == TaskStatus::Open`. This already excludes `Reserved` since
it's a different variant. **No change needed** — the existing filter
is correct by default.

### 7. `tasks/runner.rs` — `prepare_task_run` guard

If `run_run` already emitted `Reserved`, the task is no longer `Open`,
so `prepare_task_run` skips the reserve. The agent's `aiki task start`
handles `Reserved → InProgress` via the existing `Started` event path.
**No change needed.**

### 8. `commands/task.rs` — `task start` command

The `task start` command (used by agents via CLAUDE.md) emits `Started`.
It needs to work for `Reserved → InProgress` (the spawned agent starting
its reserved task).

**Main path** (`run_start` with explicit IDs): Only guards against
`Closed` (needs `--reopen`) and blocked tasks. No explicit status guard
against InProgress/Reserved — it just emits `Started`. So
`Reserved → InProgress` already works here. **No change needed.**

**Autorun path** (auto-starting linked tasks after close): Guards with
`task.status != TaskStatus::Open && task.status != TaskStatus::Stopped`.
Update to also allow `Reserved`:

```rust
if !matches!(task.status, TaskStatus::Open | TaskStatus::Reserved | TaskStatus::Stopped) {
    continue;
}
```

**Behavior is identical** for `Open → InProgress` and
`Reserved → InProgress` — same `Started` event, same output. No special
messaging for Reserved (the agent doesn't need to know it was reserved
vs freshly picked).

### 9. `commands/task.rs` — `aiki task release` command

Add a `Release` subcommand to `TaskCommands`:

```rust
Release {
    /// Task ID(s) to release
    ids: Vec<String>,
    /// Reason for releasing
    #[arg(long)]
    reason: Option<String>,
}
```

Implementation:
- Resolve task ID (prefix matching)
- Validate `task.status == TaskStatus::Reserved` — error otherwise
- Emit `TaskEvent::Released { task_ids, reason, timestamp }`
- Print confirmation: `Released: <short_id>`

This is the manual escape hatch for stuck reservations (crashed
orchestrator, silent spawn failure, etc.). Follows the same pattern as
`task start`, `task stop`, `task close` — one command, one event.

### 10. Display / markdown formatting

- `tasks/md.rs` — Add display for Reserved status (icon, label)
- `tui/` screens — Handle Reserved in any status match arms
- `commands/task.rs` — Task list should show Reserved tasks distinctly
  (e.g., "spawning..." or a different icon)

### 11. Audit all `TaskStatus` match sites

269 occurrences across 26 files. Most are exhaustive matches that will
get a compiler error when the new variant is added — the compiler does
the work. Key categories:

- **Exhaustive matches** — add `Reserved` arm (compiler-enforced)
- **`is_terminal()`** — no change (Reserved is not terminal)
- **Ready filters** (`== TaskStatus::Open`) — no change (Reserved is
  excluded by being a different variant, which is correct)
- **"Is active" checks** (`== TaskStatus::InProgress`) — decide per-site
  whether Reserved counts as "active." Generally no — Reserved means
  held but not yet working.

## Testing

- Unit test: `Reserved` event transitions `Open → Reserved`
- Unit test: `Released` event transitions `Reserved → Open`
- Unit test: `Started` event transitions `Reserved → InProgress`
- Unit test: `resolve_next_session` skips Reserved tasks
- Unit test: `task start` works on Reserved tasks
- Integration: `task run` emits Reserved (not Started) before spawn
- Integration: spawn failure emits Released (not Stopped)
- Unit test: `aiki task release <id>` on Reserved task → Open
- Unit test: `aiki task release <id>` on non-Reserved task → error
- Unit test: transition guard warns on `Reserved` event for non-Open task
- Unit test: transition guard warns on `Released` event for non-Reserved task

## Ordering

This plan should be implemented **before** session-run-command.md, since
`session next` needs clean claim/start separation.
