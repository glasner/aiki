# Fix: Codex session creation race condition (cwd lookup)

## Problem

Codex sessions never appear in `aiki session list` because the OTel receiver fails to create them.

**Root cause:** A race condition in the `conversation_starts` handler:

1. Codex starts up and fires `codex.conversation_starts` via OTel
2. OTel receiver gets the event, but Codex doesn't include `cwd` or `process.pid` in its resource attributes
3. The receiver falls back to `lookup_cwd_from_codex_session(conversation_id)`, which searches `~/.codex/sessions/` for a `.jsonl` file matching the conversation ID
4. **The `.jsonl` file doesn't exist yet** — Codex writes it at roughly the same time as the OTel event fires
5. Lookup returns `None`, `process_event` silently returns, no session file is created in `~/.aiki/sessions/`

Later `user_prompt` events also fail because `maybe_emit_turn_started` checks `session_file.exists()` and bails when the session was never created.

## Fix

Add a retry loop with short polling to `lookup_cwd_from_codex_session` when the file isn't found on the first attempt. The Codex session file is being written concurrently and typically appears within tens of milliseconds.

### Changes

**File: `cli/src/commands/otel_receive.rs`**

In `lookup_cwd_from_codex_session`:
- After `find_file_with_suffix` returns `None`, retry up to ~500ms (polling every 10ms)
- If found on retry, proceed normally
- If still not found after timeout, return `None` (same as today)
- Add debug_log for retry attempts so we can observe timing in `/tmp/aiki-otel-receive.err`

### Why this approach

- **Simplest fix** — no deferred state, no changes to event flow
- **Full SessionStarted flow preserved** — cwd is available, `aiki init` runs, repo ID is computed, history is recorded
- **Minimal latency** — the file typically appears in <100ms; worst case 500ms timeout
- **No impact on Codex** — OTel exporter is async/fire-and-forget; the 200 OK delay doesn't block the Codex UI
- **Graceful degradation** — if the file never appears (format change, etc.), behavior is identical to today

### Not doing

- Deferring session creation to `user_prompt` — adds complexity, requires carrying state between OTel receiver invocations (each is a separate process)
- Creating sessions without cwd — breaks `aiki init`, repo ID computation, and history recording
- Scanning `~/.codex/sessions/` in `list_all_sessions` — doesn't create proper aiki sessions with JJ tracking

---

## Follow-up: Missing PID + session disappearing on cleanup

### Problems found

1. **Missing PID:** Codex doesn't send `process.pid` in OTel resource attributes. The OTel receiver runs as a separate socket-activated process (not a child of Codex), so `find_ancestor_by_name("codex")` can't work from within the receiver.

2. **Session disappearing:** When another agent (e.g. Claude) starts, `handle_session_started` calls `cleanup_stale_sessions()`. A Codex session with no PID and no JJ events yet gets classified as `NoEvents` (orphaned) and deleted.

### Fix: Socket peer PID + ancestor walk

**File: `cli/src/commands/otel_receive.rs`**

In inetd-compatibility mode (launchd/systemd socket activation), stdin (fd 0) IS the accepted socket. We resolve the peer PID via:
- Unix sockets: `LOCAL_PEERPID` (macOS) / `SO_PEERCRED` (Linux)
- TCP loopback (actual case): `getpeername(0)` → peer ephemeral port → `lsof -i TCP@127.0.0.1:{port} -sTCP:ESTABLISHED -t` → PID

Note: `LOCAL_PEERPID` only works on `AF_UNIX` sockets. The launchd config uses TCP (`127.0.0.1:19876`), so the lsof fallback is the path that actually runs.

Then walk up the peer's process tree via `sysinfo` to find the actual "codex" ancestor process. This gives us the Codex PID even though the OTel receiver is not in Codex's process tree.

Applied only in `maybe_emit_session_started` (session creation), not on every event.

The socket cwd also eliminates the `.jsonl` race condition for cwd — we get cwd directly from the running Codex process rather than waiting for the `.jsonl` file to appear on disk. The `.jsonl` polling retry is kept as a final fallback if the socket peer lookup fails.
