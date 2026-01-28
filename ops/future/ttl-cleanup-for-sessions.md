# TTL-Based Stale Session Cleanup

Deferred from the event system redesign (see `ops/now/event-roadmap.md`). PID-based cleanup is implemented; TTL cleanup is not yet needed.

## Problem

Without TTL-based cleanup, sessions from living-but-idle processes can accumulate indefinitely. PID-based cleanup handles crashed processes, but a long-running editor process that stops using aiki will leave stale session files.

## Design: Use JJ Events for `last_seen`

**Instead of updating session files on every event**, query `aiki/conversations` branch for latest event timestamp.

**Why JJ events over session files?**

| Criterion | File Updates | JJ Query | Winner |
|-----------|--------------|----------|--------|
| Write performance | Fast (~1ms/turn) | None (already writing events) | JJ Query |
| Read performance | Fast | Slower (~20-50ms) | File Updates |
| Data consistency | Can drift | Always accurate | JJ Query |
| Query flexibility | Limited | Rich (JJ revsets) | JJ Query |
| Code complexity | Higher (locking, parsing) | Lower (reuse existing) | JJ Query |
| Crash safety | Needs locking | Built-in (JJ transactions) | JJ Query |

**Overall:** JJ Query wins on most criteria. Read performance can be optimized with caching if needed.

## Finding `last_seen`

Query JJ for latest event per session:

```bash
jj log -r 'aiki/conversations & description("session_id=<uuid>")' --limit 1
```

Parse timestamp from change metadata.

**Note:** `query_latest_event()` helper already exists in `cli/src/session/mod.rs` but is not yet wired into TTL cleanup logic.

## TTL Thresholds

Per-agent defaults (hardcoded constants, no override mechanism for now):
- **Editor agents** (Cursor, Claude Code with IDE): **8h**
- **CLI agents** (standalone tools): **2h**

## Cleanup Logic

TTL cleanup runs at session start (existing `cleanup_stale_sessions` path).

```rust
pub fn cleanup_stale_sessions(repo_path: &Path) {
    let sessions = scan_session_files(repo_path);

    for session in sessions {
        // Fast path: check PID (takes precedence over TTL) — ALREADY IMPLEMENTED
        if !process_alive(session.parent_pid) {
            delete_session_file(&session);
            emit_synthetic_session_ended(&session, "pid_dead");
            continue;
        }

        // Slow path: TTL check via JJ query — NOT YET IMPLEMENTED
        if let Some(ttl) = get_ttl_threshold(&session.agent) {
            match query_latest_event(repo_path, &session.id) {
                Ok(Some(last_event)) if last_event < now() - ttl => {
                    // Session has events but they're too old
                    delete_session_file(&session);
                    emit_synthetic_session_ended(&session, "ttl_expired");
                }
                Ok(None) => {
                    // No events found = orphaned session (created but never used)
                    delete_session_file(&session);
                    emit_synthetic_session_ended(&session, "no_events");
                }
                Err(e) => {
                    // JJ query failed - don't delete (could be transient error)
                    eprintln!("Warning: Failed to query events for session {}: {}", session.id, e);
                }
                Ok(Some(_)) => {
                    // Session is active (events within TTL)
                }
            }
        }
    }
}
```

When TTL cleanup removes a session, emit synthetic `session.ended` event **to history only** (does NOT execute `session.ended` flow section — the agent is disconnected, so context/autoreply actions are meaningless). Reasons:
- **`ttl_expired`** — No activity within TTL threshold
- **`no_events`** — Orphaned session (no events found in conversation history)

## Implementation Checklist

- [ ] Add TTL threshold constants: 8h (editors), 2h (CLI) — hardcoded, no override mechanism
- [ ] Wire `query_latest_event()` into `cleanup_stale_sessions()` slow path
- [ ] Handle outcomes:
  - [ ] `Ok(Some(timestamp))` — check TTL, delete if expired with `reason="ttl_expired"`
  - [ ] `Ok(None)` — orphaned session, delete with `reason="no_events"`
  - [ ] `Err(e)` — JJ query failed (transient error), log warning and skip (don't delete)
- [ ] Emit synthetic `session.ended` to history only (no flow execution)
- [ ] Add tests: JJ query succeeds with old timestamp → TTL cleanup removes session
- [ ] Add tests: JJ query succeeds with recent timestamp → TTL cleanup does NOT delete
- [ ] Add tests: JJ query returns `None` (orphaned session) → cleaned up with `reason="no_events"`
- [ ] Add tests: JJ query returns `Err` (transient failure) → session NOT deleted
- [ ] Add tests: Synthetic `session.ended` events recorded with correct reasons

## Performance Estimates

| Approach | Write Cost | Read Cost |
|----------|-----------|-----------|
| Session file updates (rejected) | ~1ms × N events/turn | Fast (~1ms) |
| JJ events (chosen) | 0ms (already writing) | ~20-50ms (only on cleanup) |

**Cleanup frequency:** Once per `session.started` event (infrequent)

**Optimization strategy** if JJ query performance becomes an issue:
1. Add in-memory cache: `{ session_id → latest_event_timestamp }`
2. Refresh cache on new events or periodically
3. Benchmark to validate actual overhead in real repos

## Related Decisions

- **TTL configuration** — Hardcoded constants: 8h (editors), 2h (CLI), no override mechanism for now
- **JJ events for `last_seen`** — Single source of truth, better consistency, acceptable performance
- **TTL cleanup: history only, no flows** — Synthetic `session.ended` from TTL cleanup records to history but does NOT execute `session.ended` flow section
- **No session file migration** — Old session files (with `aiki_session_id` field) will be treated as orphans and cleaned up naturally
- **Cursor resume: accept TTL gap** — If TTL cleanup removes a Cursor session file during inactivity, the next prompt is treated as a new session
