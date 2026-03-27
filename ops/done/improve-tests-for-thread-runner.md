# Test plan: Thread-runner integration gaps

Covers the 7 untested areas from the thread-runner audit. All tests below exercise wiring/integration paths — the data model layer (ThreadId parsing, lane derivation, ThreadResolution) is already well-covered.

## 1. Thread filtering in `aiki task list`

**File:** `cli/src/commands/task.rs` (unit tests in `#[cfg(test)]` module)

The function under test is `resolve_thread_task_ids(graph, head_id)` (line 1738). It walks `needs-context` edges from the head and stops at parent boundaries. Testing it directly avoids needing CLI arg parsing or env var manipulation.

### Tests

**1a. `test_resolve_thread_task_ids_follows_needs_context`**
- Build graph: parent P, subtasks A→B→C linked by `needs-context`
- Call `resolve_thread_task_ids(&graph, A)` → set {A, B, C}

**1b. `test_resolve_thread_task_ids_stops_at_parent_boundary`**
- Build graph: parent P1 with subtasks A→B (`needs-context`), parent P2 with subtask C
- Link B→C with `needs-context` (cross-parent)
- Call `resolve_thread_task_ids(&graph, A)` → set {A, B} (C excluded)

**1c. `test_resolve_thread_task_ids_single_task`**
- Build graph: parent P, single subtask A (no needs-context edges)
- Call `resolve_thread_task_ids(&graph, A)` → set {A}

**1d. `test_resolve_thread_task_ids_ignores_depends_on`**
- Build graph: parent P, subtasks A, B. A `depends-on` B (not `needs-context`)
- Call `resolve_thread_task_ids(&graph, A)` → set {A} (B not included)

### Env var precedence (integration-level)

These test the resolution logic at lines 1321–1346. They need a materialized graph and can be tested by extracting the resolution block into a helper, or by writing integration tests that invoke `run_list` with env var set.

**1e. `test_thread_env_var_takes_precedence_over_flag`**
- Set `AIKI_THREAD` env var to full 32-char ID
- Pass different `--thread` flag value
- Assert the env var's thread is used (env var wins)

**1f. `test_thread_flag_used_when_env_unset`**
- Ensure `AIKI_THREAD` is unset
- Pass `--thread` flag with a prefix
- Assert prefix resolution is used

**1g. `test_output_id_format_with_thread_filter`**
- Set `--thread` and `-o id`
- Assert output contains only task IDs from the thread, one per line

## 2. Session thread persistence roundtrip

**File:** `cli/src/session/mod.rs` (unit tests in `#[cfg(test)]` module)

### Tests

**2a. `test_with_thread_sets_thread_field`**
- Create session, call `.with_thread(Some(ThreadId::single(id)))`
- Assert `session.thread()` returns the ThreadId

**2b. `test_with_thread_none_clears_field`**
- Create session with thread, call `.with_thread(None)`
- Assert `session.thread()` is None

**2c. `test_with_thread_from_env_parses_single`**
- Set `AIKI_THREAD=<32-char-id>` in env
- Call `.with_thread_from_env()`
- Assert `session.thread().unwrap().is_single()` and head matches

**2d. `test_with_thread_from_env_parses_pair`**
- Set `AIKI_THREAD=<32-char-head>:<32-char-tail>` in env
- Call `.with_thread_from_env()`
- Assert head and tail match the env var halves

**2e. `test_with_thread_from_env_unset_is_none`**
- Ensure `AIKI_THREAD` is unset
- Call `.with_thread_from_env()`
- Assert `session.thread()` is None

**2f. `test_with_thread_from_env_invalid_is_none`**
- Set `AIKI_THREAD=bad` (not 32 chars)
- Call `.with_thread_from_env()`
- Assert `session.thread()` is None (parse fails silently)

### Session file roundtrip

**2g. `test_session_file_writes_thread`**
- Create session with thread, call `session.file().create()`
- Read the file back, assert it contains `thread=<full-ids>`

**2h. `test_find_thread_session_matches_on_tail`**
- Write a session file with `thread=<head>:<tail>`
- Call `find_thread_session(tail)` → returns match
- Call `find_thread_session(head)` → returns None (tail-only match)

**2i. `test_find_thread_session_single_task_thread`**
- Write a session file with `thread=<id>` (single-task, head==tail)
- Call `find_thread_session(id)` → returns match
- Backward-compat: identical behavior to old `AIKI_TASK` single-task sessions

## 3. History `run_thread_id` serialization

**File:** `cli/src/history/storage.rs` (unit tests) or `cli/tests/test_event_storage.rs`

### Tests

**3a. `test_session_start_serialization_with_thread`**
- Create `SessionStart` event with `run_thread_id: Some("head:tail")`
- Serialize via `serialize_event()` → assert output contains `run_thread=head:tail`
- Deserialize back → assert `run_thread_id` matches

**3b. `test_session_start_serialization_without_thread`**
- Create `SessionStart` event with `run_thread_id: None`
- Serialize → assert no `run_thread` key in output
- Deserialize back → assert `run_thread_id` is None

**3c. `test_find_session_started_for_thread_exact_match`**
- Write two `SessionStart` events:
  - Session A with `run_thread_id: Some("aaa...aaa:bbb...bbb")`
  - Session B with `run_thread_id: Some("aaa...aaa:ccc...ccc")`
- Query `find_session_started_for_thread(cwd, "aaa...aaa:bbb...bbb")` → session A
- Query `find_session_started_for_thread(cwd, "aaa...aaa")` → None (no head-only match)
- Confirms exact-match semantics: shared head doesn't alias distinct threads

**3d. `test_find_session_started_for_thread_no_match`**
- Write events with no `run_thread_id`
- Query → returns None

## 4. History recorder wiring

**File:** `cli/src/history/recorder.rs` (unit test or integration)

### Tests

**4a. `test_record_session_start_includes_thread`**
- Create session with `thread: Some(ThreadId { head: H, tail: T })`
- Call `record_session_start(jj_cwd, &session, ts, repo_id, cwd)`
- Read back the event file, assert `run_thread=H:T` is present

**4b. `test_record_session_start_no_thread`**
- Create session without thread
- Record → assert no `run_thread` key

## 5. Flow engine `session.thread.*` variables

**File:** `cli/src/flows/engine.rs` (unit tests in `#[cfg(test)]` module)

The flow engine resolves `session.thread.tail`, `session.thread.head`, and `session.thread` as lazy variables during `task.closed` handling (lines 464–494). These read from `find_thread_session()` which scans session files.

### Tests

**5a. `test_task_closed_resolves_session_thread_tail`**
- Write a session file with `thread=<head>:<tail>`, `parent_pid=<pid>`, `mode=interactive`
- Simulate `task.closed` event for the tail task
- Assert `session.thread.tail` resolves to the tail ID
- Assert `session.thread.head` resolves to the head ID
- Assert `session.thread` resolves to `<head>:<tail>`

**5b. `test_task_closed_session_thread_empty_when_no_session`**
- No session files on disk
- Simulate `task.closed` event
- Assert `session.thread.tail` resolves to `""` (empty string)
- Assert `session.mode` resolves to `""` (empty string)

**5c. `test_task_closed_only_tail_triggers_session_end`**
- Write session file with `thread=<head>:<tail>`, `mode=interactive`
- Close the head task (not tail) → `session.thread.tail != event.task.id` → no session.end
- Close the tail task → condition matches → session.end fires

**5d. `test_task_closed_single_task_thread_triggers_session_end`**
- Write session file with `thread=<id>` (single-task, head==tail)
- Close that task → `event.task.id == session.thread.tail` → session.end fires
- Backward compat: identical to old AIKI_TASK behavior

## 6. Agent spawn options prompt and env var

**File:** `cli/src/agents/runtime/mod.rs` (unit tests)

### Tests

**6a. `test_task_prompt_contains_thread_id`**
- Create `AgentSpawnOptions` with `ThreadId { head: H, tail: T }`
- Call `task_prompt()` → assert output contains the thread display (short ID)
- Assert prompt contains "thread", "SCOPE", "EXIT", "aiki task list"

**6b. `test_task_prompt_single_task_thread`**
- Create with `ThreadId::single(id)`
- Call `task_prompt()` → assert thread display shows bare short ID (no colon)

**6c. `test_spawn_options_thread_serialization`**
- Create `AgentSpawnOptions` with multi-task thread
- Assert `options.thread.serialize()` returns `"<full-head>:<full-tail>"`
- This is the value that gets passed to `.env("AIKI_THREAD", ...)` in the runtimes

### Runtime env var (integration-level, may need to mock Command)

**6d. `test_claude_code_sets_aiki_thread_env`**
- Verify that `ClaudeCodeRuntime::spawn_background` builds a Command with `.env("AIKI_THREAD", options.thread.serialize())`
- Can be tested by inspecting the Command construction or by extracting the env-building into a testable helper

**6e. `test_codex_sets_aiki_thread_env`**
- Same verification for `CodexRuntime`

## 7. `discover_session_id` wiring

**File:** `cli/src/agents/runtime/mod.rs`

This function (line 256) polls `find_session_started_for_thread()` with exponential backoff. It's difficult to unit test due to the polling loop, but the key contract is testable:

**7a. `test_discover_session_id_uses_serialized_thread`**
- Verify that `discover_session_id` passes `thread.serialize()` to the storage function
- Can be tested by writing a SessionStart event with `run_thread=<serialized>` before calling discover, and asserting it returns the session UUID

## Priority order

Tests should be written in this order, highest-risk gaps first:

1. **Session file roundtrip** (2g, 2h, 2i) — if these break, `task.closed → session.end` path breaks silently
2. **History serialization** (3a–3d) — if roundtrip fails, `discover_session_id` can't find spawned sessions
3. **Flow engine variables** (5a–5d) — the `task.closed` hook is the behavioral heart of the thread migration
4. **Thread filtering** (1a–1d) — `resolve_thread_task_ids` parent-boundary logic is subtle
5. **Session builder** (2a–2f) — straightforward but fills a coverage gap
6. **Prompt and env var** (6a–6e) — lower risk, prompt is simple string formatting
7. **Env var precedence** (1e–1g) — integration-level, lower priority
8. **Recorder wiring** (4a–4b) — thin wrapper, low risk
9. **Discover polling** (7a) — hard to test without side effects

## Estimated scope

~30 tests across 5 files. Most are small unit tests (5–15 lines each). The flow engine tests (5a–5d) are the most involved since they require session file fixtures on disk.
