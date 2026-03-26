# Lane Runner: Thread scoping for lane agents

## Problem

Lane agents have no way to check their remaining backlog at runtime. The current prompt inlines task IDs, but these get compacted away in long sessions. Agents need a durable way to discover their assigned work.

## Concepts

```
Epic (parent task)
├── Lane A (parallel track, ID = head task ID)
│   ├── Thread 1 (shared-context group, ID = head task ID)
│   │   ├── Task: create schema
│   │   └── Task: add migrations        ← needs-context
│   └── Thread 2 (sequential, runs after thread 1)
│       └── Task: add tests
└── Lane B (parallel track)
    └── Thread 1
        └── Task: update docs
```

- **Lane** = parallel track of work. ID = head task ID. Derived from `depends-on` edges.
- **Thread** = sequential chunk within a lane, sharing agent context. ID = head task ID. Derived from `needs-context` edges. One agent session per thread.
- Task IDs are globally unique, so lane/thread IDs need no parent prefix.

## Design

### AIKI_THREAD env var

Set `AIKI_THREAD=<thread-head-id>` on spawned lane agent processes. Same pattern as `AIKI_TASK`.

When `AIKI_THREAD` is set, `aiki task list` auto-filters to only tasks in that thread. The agent just runs `aiki task` and sees its backlog.

`AIKI_THREAD` is a guardrail — highest precedence, cannot be overridden by `--thread` flag.

### Explicit `--thread` flag

`--thread <head-id>` on `aiki task list` for manual use:

```bash
aiki task list --thread wttorov        # backlog (default filters)
aiki task list --thread wttorov --all  # all tasks including closed
aiki task list --thread wttorov -o id  # just IDs
```

Precedence: `AIKI_THREAD` env var > `--thread` flag > normal scope resolution.

### Updated prompt

```
You are assigned thread `{thread_id}`. Work through all tasks in order.

SCOPE: Only tasks in this thread. Do not pick up other work.
EXIT: When `aiki task list` returns no tasks, you are done — exit immediately. Do not close parent/sibling tasks.

Run `aiki task list` to see your backlog.

```

`aiki task` auto-filters via `AIKI_THREAD` — no args needed.

## Implementation

### 1. Thread ID format and parsing

**File:** `cli/src/tasks/lanes.rs`

Add `ThreadId` struct:

```rust
/// Identifies a thread — a sequential chunk of tasks within a lane.
/// Single-task threads have head == tail.
pub struct ThreadId {
    pub head: String,  // first task ID in the chain
    pub tail: String,  // last task ID in the chain
}

impl ThreadId {
    /// Parse from wire format: `"head:tail"` or `"head"` (single-task shortcut).
    pub fn parse(input: &str) -> Result<Self>;

    /// Single-task thread (head == tail).
    pub fn single(task_id: String) -> Self;

    /// Full-ID serialization for env vars, session files, history metadata.
    pub fn serialize(&self) -> String;

    pub fn is_single(&self) -> bool { self.head == self.tail }
}

impl fmt::Display for ThreadId {
    // Uses short_id: "wttorov:zllkwzr" or "wttorov" when head == tail

}
```

Two constructors — `parse()` for stored/wire format, `resolve()` for CLI input:

- `parse(input)`: split on `:` — if no colon, head = tail = input. Error on empty halves. **No prefix resolution.** Expects full 32-char task IDs. Used by session files, env vars, history metadata — contexts where no task graph is available.
- `resolve(input, graph)`: same splitting, but resolves both halves via `resolve_task_id_in_graph`. Used only at CLI entry points (`--thread` flag). Errors if resolution fails.

Examples:
```
ThreadId::parse("full32chars...:full32chars...")  → ThreadId { head: "full32...", tail: "full32..." }
ThreadId::parse("full32chars...")                 → ThreadId { head: "full32...", tail: "full32..." }
ThreadId::resolve("wttorov", &graph)              → ThreadId { head: resolve("wttorov"), tail: resolve("wttorov") }
ThreadId::resolve("wttorov:zllkwzrv", &graph)     → ThreadId { head: resolve("wttorov"), tail: resolve("zllkwzrv") }
ThreadId::single("abc123")                        → ThreadId { head: "abc123", tail: "abc123" }
```

`ThreadId` is the internal representation. Two output formats:
- `Display`/`to_string()` — short IDs for human-facing output (`"wttorov:zllkwzr"` or `"wttorov"`)
- `serialize()` — full IDs for env vars, session files, and history metadata (`"full32chars...:full32chars..."`)

**Context boundary rule:** Prefix resolution only happens at CLI entry points. All persistence layers (env vars, session files, history) store and read full IDs via `parse()`/`serialize()`. This avoids requiring a task graph in contexts that don't have one (e.g., global session file scanning in `session/mod.rs`).

Also rename in this file:
- `LaneSession` → `Thread`
- `Lane.sessions` → `Lane.threads`
- Update all call sites

### 2. Add thread filtering to `aiki task list`

**File:** `cli/src/commands/task.rs`

- Add `--thread` arg to `TaskCommands::List` (type `Option<String>`)
- In `run_list`, resolve thread from:
  1. `AIKI_THREAD` env var (guardrail — cannot be overridden)
  2. `--thread` flag
  3. None (normal scope)
- When set via `--thread`: resolve thread ID via `ThreadId::resolve(input, graph)`. When set via `AIKI_THREAD`: parse via `ThreadId::parse(input)` (full IDs, no graph needed).
- Walk `needs-context` chain from head, but **stop at lane boundary**: only include tasks that share the same parent as the head task. If a `needs-context` link crosses to a task with a different parent, treat it as the end of the thread. This prevents accidental cross-lane/cross-parent leakage through stray `needs-context` links.
- Filter task set to the validated chain IDs

### 3. Set AIKI_THREAD on spawned sessions

**Files:** `cli/src/agents/runtime/claude_code.rs`, `cli/src/agents/runtime/codex.rs`

- Replace `.env("AIKI_TASK", &options.task_id)` with `.env("AIKI_THREAD", &options.thread.serialize())`

**File:** `cli/src/agents/runtime/mod.rs`

- Replace `task_id: String` and `chain_task_ids: Option<Vec<String>>` with `thread: ThreadId` in `AgentSpawnOptions` — threads subsume both fields
- Remove `with_chain_task_ids()` builder
- `AgentSpawnOptions::new()` takes a `ThreadId` (single-task: `ThreadId::single(id)`, multi-task: constructed from chain resolution)
- Update all call sites that set `task_id` or `chain_task_ids` (e.g., `commands/run.rs` chain resolution) to build a `ThreadId` instead
- Rename `BackgroundHandle.task_id` → `BackgroundHandle.thread: ThreadId` and update construction in `claude_code.rs` and `codex.rs`
- Update `discover_session_id(cwd, task_id)` → `discover_session_id(cwd, thread: &ThreadId)`, pass `thread.serialize()` to the history lookup. **The history lookup does an exact match on the full serialized thread ID** — not a head-only match. The caller always has the full `ThreadId`, so there is no reason to weaken the match.
- Update `spawn_and_discover` callers in `commands/run.rs` to use `handle.thread`

### 4. Rename `--next-session` to `--next-thread`

**File:** `cli/src/commands/run.rs`

- Rename `--next-session` flag to `--next-thread`
- Rename `next_session` field/variable to `next_thread`
- Update the resolved chain head as the thread ID and set it on spawn options

**File:** `cli/src/tasks/runner.rs`

- Rename `resolve_next_session` → `resolve_next_thread`
- Rename `resolve_next_session_in_lane` → `resolve_next_thread_in_lane`
- Rename `SessionResolution` → `ThreadResolution`

**File:** `.aiki/tasks/loop.md`

- Update orchestrator template: `aiki run {{data.target}} --next-thread --lane <lane-id> --async -o id`

### 5. Replace session.task with session.thread

Replace `AIKI_TASK` with `AIKI_THREAD` across all session paths. Session stores the full serialized thread ID string — parsing into head/tail happens at read time via `ThreadId::parse()` (no prefix resolution — full IDs only at this layer).

**Env var:** `AIKI_THREAD=<head>:<tail>` (or just `<head>` when head == tail)

**File:** `cli/src/session/mod.rs`

- Remove `task: Option<String>` field
- Add `thread: Option<ThreadId>` field (parsed once at construction time)
- Replace `with_task_from_env()` with `with_thread_from_env()` reading `AIKI_THREAD` → `ThreadId::parse()`
- Replace `task()` accessor with `thread()` returning `Option<&ThreadId>`
- Session file serialization: write `thread=<wire>` via `ThreadId::serialize()`, read via `ThreadId::parse()`

**Note:** `AgentSpawnOptions` and spawner env var changes are in Step 3 — not repeated here.

**File:** `cli/src/flows/core/hooks.yaml`

- Update `task.closed` flow:
  ```yaml
  - if: event.task.id == session.thread.tail && session.mode == "interactive"
  ```
  **Behavioral change:** Today, closing the session's single task triggers `session.end`. With threads, only closing the *tail* task (last in the chain) triggers it. For single-task threads (head == tail) behavior is identical. For multi-task threads, closing earlier tasks no longer terminates the session — the agent keeps working through its backlog. This is the intended semantic shift.

**File:** `cli/src/flows/engine.rs`

- Update `find_task_session` call in `execute_session_end` to use thread tail
- Expose `session.thread` (raw ID), `session.thread.head`, and `session.thread.tail` as flow variables (head/tail parsed on demand)

**File:** `cli/src/commands/plan.rs`

- Replace `.env("AIKI_TASK", plan_task_id)` with `.env("AIKI_THREAD", ThreadId::single(plan_task_id).serialize())`
- Plan sessions are always single-task threads (the plan task itself)

**File:** `cli/src/history/recorder.rs`

- Line 59: `run_task_id: session.task()` → `run_thread: session.thread().map(|t| t.serialize())`

**File:** `cli/src/history/types.rs`

- Line 81: rename `run_task_id: Option<String>` → `run_thread_id: Option<String>`

**File:** `cli/src/history/storage.rs`

- Line 477: `find_session_started_for_run_task` → `find_session_started_for_thread`
- Line 483-485: match on `run_thread_id` instead of `run_task_id`, **exact match on the full serialized thread ID**. The caller always has the full `ThreadId` and passes `thread.serialize()`. Head-only matching would alias distinct threads that share a head task, defeating the purpose of storing the full ID.
- Line 624: metadata key `run_task` → `run_thread`

**Cleanup:**

- Remove all references to `AIKI_TASK` env var

#### Call chain analysis

These are the critical paths through the AIKI_TASK → AIKI_THREAD migration:

**Path 1: `task.closed` → `session.end` (interactive session auto-termination)**

```
hooks.yaml: task.closed
  → condition: event.task.id == session.thread.tail && session.mode == "interactive"
    → engine.rs:452: resolves session.thread.tail via lazy var
      → session::find_task_session(task_id) scans session files for thread= field
        → parses thread=<head:tail>, extracts tail, compares to event task ID
    → engine.rs:465: resolves session.mode via same find_task_session
  → action: session.end
    → engine.rs:1173: execute_session_end calls find_task_session(task_id) for PID
      → needs to match on tail of thread= field, not exact match
      → returns PID for SIGTERM
```

**Change:** `find_task_session` becomes `find_thread_session`. Reads `thread=<wire>` from session file, parses via `ThreadId::parse()` (full IDs, no graph needed), and compares `thread.tail` against the lookup task ID. The function signature stays the same (takes a task ID, returns PID+mode), but matching logic changes.

**Path 2: `aiki run --async` → discover session UUID**

```
commands/run.rs:353: discover_session_id(cwd, &handle.task_id)
  → agents/runtime/mod.rs:283: polls with exponential backoff
    → history/storage.rs:477: find_session_started_for_run_task(cwd, task_id)
      → scans history events for SessionStart with run_task_id matching task_id
      → returns session UUID
```

**Changes:**

1. `BackgroundHandle.task_id` → `BackgroundHandle.thread_id` (agents/runtime/mod.rs:26)
2. `ConversationEvent::SessionStart.run_task_id` → `.run_thread` (history/types.rs:81)
3. Serialization: `run_task` metadata key → `run_thread` (history/storage.rs:624)
4. Deserialization: parse `run_thread` (history/storage.rs:793)
5. `find_session_started_for_run_task()` → `find_session_started_for_run_thread()`, exact match on full serialized `run_thread` field (history/storage.rs:477)
6. `discover_session_id(cwd, task_id)` → `discover_session_id(cwd, thread: &ThreadId)`, passes `thread.serialize()` to the renamed function (agents/runtime/mod.rs:283)
7. `spawn_and_discover` callers in commands/run.rs: `handle.task_id` → `handle.thread_id`
8. `BackgroundHandle` construction in claude_code.rs:104 and codex.rs:143: `task_id:` → `thread_id:` (value stays `options.task_id` — the head task IS the thread head)
9. `record_session_start` (history/recorder.rs:59): `session.task()` feeds `run_thread` — value unchanged until Path 3 lands

For single-task threads, `thread_id == task_id` so behavior is identical to today.

**Path 3: Session file write → read roundtrip**

```
Write:
  session/mod.rs:102: session.task() → writes "task=<id>" to session file
  agents/runtime/claude_code.rs:90: .env("AIKI_TASK", task_id)
  agents/runtime/codex.rs:128: .env("AIKI_TASK", task_id)
  commands/plan.rs:543: .env("AIKI_TASK", plan_task_id)

Read:
  session/mod.rs:601: with_task_from_env() reads AIKI_TASK
  session/mod.rs:1045: parses "task=" line from session file
```

**Change:** All writes use `ThreadId::serialize()` → `thread=<full-ids>`. All reads use `ThreadId::parse()`. Env var becomes `AIKI_THREAD`. `ThreadId` is constructed once at the boundary — no repeated parsing downstream.

**Path 4: History recording**

```
history/recorder.rs:59: run_task_id: session.task()
  → writes to conversation history as run_task=<id>
  → read back by find_session_started_for_run_task
```

**Change:** Field becomes `run_thread`, metadata key becomes `run_thread`. Stores full IDs via `ThreadId::serialize()`.

### 6. Update prompt

**File:** `cli/src/agents/runtime/mod.rs`

Replace both existing prompt branches in `AgentSpawnOptions::task_prompt()` with a single thread-based prompt. The agent discovers its backlog via `aiki task list` (auto-filtered by `AIKI_THREAD`), so the prompt no longer inlines task IDs.

**Before (two branches in current code):**
- Single task: `"You are assigned task \`{id}\`. Work autonomously..."`
- `chain_task_ids` set: `"You are assigned a chain of {count} tasks...{chain_list}"`

**After (unified thread prompt):**
```
You are assigned thread `{thread_id}`. Work through all tasks in order.

SCOPE: Only tasks in this thread. Do not pick up other work.
EXIT: When `aiki task list` returns no tasks, you are done — exit immediately.
     Do not close parent/sibling tasks.

Run `aiki task list` to see your backlog.
```

### 7. Display lane and thread structure in `aiki task lane`

**File:** `cli/src/commands/task.rs` (`run_lane`)

Update lane display to show threads within lanes, with subtasks listed under each thread using the standard checklist format:

```
Lane wttorov:  ● ready
  Thread (xkqtulml:nznsxylr):  ✓ complete
    [x] .1 create schema
    [x] .2 add migrations
  Thread (rzkkqssy):  ● ready
    [ ] .3 add tests
```

Thread IDs use `head:tail` for multi-task threads, bare `head` for single-task threads. Subtask display matches `aiki task show` format: `[x]` closed, `[>]` in-progress, `[~]` reserved, `[ ]` open.

## Testing

- Unit test: `ThreadId::parse` — `"abc"` → `{ head: abc, tail: abc }`, `"abc:def"` → `{ head: abc, tail: def }`, empty halves error, `serialize()` roundtrip, `to_string()` uses short IDs
- Unit test: prefix resolution on both halves
- Unit test: `run_list` with `AIKI_THREAD` filters to thread tasks only
- Unit test: `AIKI_THREAD` takes precedence over `--thread` flag
- Integration: create parent + subtasks with needs-context, verify thread filtering
- Unit test: thread chain walk stops at parent boundary — a `needs-context` link to a task under a different parent is not included in the thread
- Unit test: `ThreadId::parse()` rejects short/prefix IDs (expects full 32-char IDs)
- Unit test: `ThreadId::resolve()` resolves prefixes via graph
- Verify `AIKI_THREAD` env var is set on spawned processes (replaces `AIKI_TASK`)
- Verify `-o id` output mode works with thread filtering
- Verify `session.thread.tail` closes interactive session correctly
- Verify single-task thread (head == tail) works identically to old `AIKI_TASK` behavior

### 8. Update templates to use thread terminology

**File:** `cli/src/tasks/templates/core/loop.md`

- `--next-session` → `--next-thread` (lines 20, 32)
- "session" → "thread" where it refers to a dispatched unit of work (lines 21, 23, 48, 50, 51, 56)
- Keep "session" where it means the actual agent process (e.g., `aiki session wait` — that's a real session command)

**File:** `cli/src/tasks/templates/core/decompose.md`

- "same agent session" → "same thread" (lines 58, 71, 80)
- "Fresh session" → "fresh thread" (line 70)
- "session context" → "thread context" (lines 51, 79)

Other templates (`review/*`, `resolve.md`, `fix.md`, `plan.md`) don't reference sessions in the lane/thread sense — no changes needed.

### 9. Doc updates

**File:** `cli/docs/sdlc/loop.md`

- `--next-session` → `--next-thread` (lines 36, 101)
- "session" → "thread" where it refers to a dispatched unit of work (same pass as template)

**File:** `cli/docs/sdlc.md`

- Line 94: "sessions" → "threads" in the loop/orchestrator description

**File:** `cli/docs/tasks/kinds.md`

- Check for any "session" or "chain" references in lane context, update to "thread"

**File:** `cli/docs/aiki-for-clawbots.md`

- Check for any `--next-session` references, update to `--next-thread`

## Migration

This must land as a single atomic change. Session files written by old code use `task=<id>`, new code expects `thread=<wire>`. If the change is split across multiple commits with running agents in between, old-format session files won't be found by the new `find_thread_session` lookup.

**Mitigation if incremental landing is needed:** Add a fallback in `find_thread_session` that also checks for `task=` lines (treating them as `ThreadId::single(task_id)`). Remove the fallback after one release cycle. For this plan, prefer a single atomic change and skip the fallback.

## Not in scope

- Persisting thread IDs in events (remains query-time derivation)
- Changing lane derivation logic
