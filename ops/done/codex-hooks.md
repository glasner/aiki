# Codex Hooks Plan

## Context (from Codex docs)
- `notify` runs an external program for supported events (currently only `agent-turn-complete`).
- Notify payload includes: `type`, `thread-id`, `turn-id`, `cwd`, `input-messages`, `last-assistant-message`.
- OTel export is opt-in via `[otel]` with `otlp-http` or `otlp-grpc` exporters; no file/stdout exporter is documented.
- `log_user_prompt = false` by default (user prompt content is redacted unless enabled).
- OTel events include: `codex.conversation_starts`, `codex.user_prompt`, `codex.api_request`, `codex.sse_event`,
  `codex.tool_decision`, `codex.tool_result`.
- **Identity:** Codex uses `conversation.id` in OTel event metadata and `thread-id` in notify payloads.
  These are the same value (the conversation/thread identifier).

## Goals
1. Support Codex sessions with full turn tracking via OTel + notify (complementary).
2. OTel captures `session.started`, `turn.started`, and `modified_files` (from `tool_result`).
3. Notify captures `turn.completed` with response text (from `last-assistant-message`).
4. Default to prompt content capture while still recording redacted prompts when disabled.

## Event Name Context

This document uses the **new turn-based event names** from event-roadmap.md Phase 1:
- `turn.started` (replaces `prompt.submitted`)
- `turn.completed` (replaces `response.received`)
- `session.started` (unchanged)
- `session.ended` (unchanged)

### Payload Mapping

OTel provides `turn.started`; notify provides `turn.completed`. Both share the same
session identity via `conversation.id` / `thread-id`.

**OTel `codex.user_prompt` → Aiki `turn.started` payload:**
- `session`: Created from `conversation.id` (external session id)
- `cwd`: Inferred from workspace or session context
- `timestamp`: From OTel event timestamp
- `turn`: Maintain counter per session (in session state file)
- `turn_id`: `{conversation_id}:{turn_number}` (deterministic, no external turn-id used)
- `source`: Always `User`
- `prompt`: From event payload (requires `log_user_prompt = true`, otherwise "[redacted]")
- `injected_refs`: Empty (MVP limitation)

**Notify `agent-turn-complete` → Aiki `turn.completed` payload:**
- `session`: Created from `thread-id` (same as OTel `conversation.id`)
- `cwd`: From notify payload `cwd` field
- `timestamp`: Current time when notify fires
- `turn`: Current turn number from session state
- `turn_id`: `{thread-id}:{current_turn}` (deterministic, matches turn.started)
- `source`: Always `User`
- `response`: From notify payload `last-assistant-message` (complete, guaranteed)
- `modified_files`: From session state (accumulated by OTel `tool_result` events; cleared on next `turn.started`, not here)

## Prerequisites

Before implementing the parser, verify Codex's actual OTel signal type:
1. Point Codex's OTel endpoint at a debug receiver (e.g., `nc -l 19876` or OTel Collector with debug exporter)
2. Run a Codex session with a few prompts
3. Inspect what arrives: which endpoint (`/v1/logs` vs `/v1/traces`), what protobuf schema

This determines whether we parse `ExportLogsServiceRequest` or `ExportTraceServiceRequest`.

**Working assumption:** Codex uses OTel **logs** (the event names like `codex.user_prompt` match
log semantics, not span semantics). The install defaults and HTTP parser target `/v1/logs`.
If verification reveals traces instead, change the endpoint to `/v1/traces` and parse
`ExportTraceServiceRequest` — the event extraction logic is the same (read attributes from
log records vs span events). The `aiki otel-receive` binary should accept both content types
and auto-detect based on the protobuf message type.

## Plan

OTel and notify are **complementary**, not sequential phases. OTel provides session lifecycle,
turn starts, and tool tracking. Notify provides turn completion with response text.
Both are installed together.

### 1. OTel Receiver (session + turn start + tool events)
1. **Socket activation (no persistent process)**
   - Implement `aiki otel-receive` that accepts a single OTLP/HTTP request on stdin,
     parses it, writes provenance, and exits 0.
   - macOS: launchd socket activation with `StandardInput=socket` and a fixed port.
   - Linux: systemd socket activation with `StandardInput=socket`.
   - Bind to `127.0.0.1:19876` explicitly (not `0.0.0.0`).
2. **HTTP parsing requirements**
   - Handle `Content-Length`, `Content-Type`, and `Content-Encoding: gzip`.
   - Support OTLP/HTTP log payloads:
     - `application/x-protobuf` (proto, default with `protocol = "binary"`).
   - Respond `200 OK` with an empty body.
   - Each OTLP/HTTP request may contain multiple events (OTel batches by default).
     Process events in order within each batch.
3. **Event mapping (OTel)**
   - `codex.conversation_starts` → `session.started` (use `conversation.id` as external session id).
   - `codex.user_prompt` → `turn.started` with `source: User` (content requires `log_user_prompt = true`).
     Clear `modified_files` from session state (belongs to previous turn).
   - `codex.tool_result` → accumulate `modified_files` from `arguments` field (extract file paths for write/edit tools).
     - **Robustness:** `arguments` may be missing, empty, or contain non-path data.
       Only extract paths from tools that modify files (write, edit, patch, shell with known patterns).
       Skip gracefully if `arguments` is absent or unparseable.
     - **Path normalization:** Resolve relative paths against `cwd` from session state.
       Deduplicate paths (same file modified multiple times in one turn).
   - Resource attributes: `service.version` → `agent_version` (stored in session file on first receive).
   - Deferred events (`codex.api_request`, `codex.sse_event`, `codex.tool_decision`): acknowledge with 200 OK, do not map.
4. **State persistence**
   - State lives in the session file at `~/.aiki/sessions/{session-id}.json`.
   - Each `aiki otel-receive` invocation reads, updates, and writes atomically (write tmp → rename).
   - **Concurrency:** Multiple OTel batches or simultaneous OTel+notify can race on the state file.
     Use `flock(LOCK_EX)` on a lockfile (`{session-id}.json.lock`) around the read-modify-write cycle.
     Lock is held only during the state update (microseconds), so blocking is negligible.
     If the lock cannot be acquired within 100ms, skip the update and log a warning (non-fatal).
   - `modified_files` is cleared on `turn.started` (not on `turn.completed`) to avoid a race
     where notify fires before late-arriving OTel `tool_result` events. Notify reads whatever
     has accumulated; late arrivals roll into the next turn.
   - Schema:
     ```json
     {
       "external_id": "conv_abc123",
       "agent": "codex",
       "agent_version": "1.2.3",
       "current_turn": 3,
       "modified_files": ["src/foo.rs", "src/bar.rs"],
       "last_event_at": "2025-01-24T10:30:00Z"
     }
     ```
5. **Error handling**
   - All errors are non-fatal. `aiki otel-receive` always returns 200 OK (never block Codex).
   - Malformed OTel payload or protobuf parse failure → log to stderr, return 200.
   - Corrupt session file → delete and recreate (lose current turn's `modified_files`).
6. **Prompt capture policy**
   - `aiki init` should set `log_user_prompt = true` in Codex config when present.
   - `aiki doctor --fix` should attempt to flip `log_user_prompt` to true if false.
   - If `log_user_prompt = false`, still emit `turn.started` with redacted content
     (for example: "[redacted]") and record prompt length if available.

### 2. Notify Handler (turn completion)
1. **Hook entry point**
   - Extend `aiki hooks handle` to accept a JSON payload passed as a CLI argument (Codex `notify` style).
   - Add a Codex handler that parses the JSON payload and dispatches aiki events.
2. **Event mapping (notify)**
   - Use `thread-id` (aka `conversation.id`) as the external session id to correlate with OTel events.
   - Emit `turn.completed` when `agent-turn-complete` fires:
     - `response`: from notify payload `last-assistant-message` (complete, guaranteed).
     - `modified_files`: read from session state (accumulated by OTel `tool_result` events). **Do not clear** —
       clearing happens on the next `turn.started` to avoid losing late-arriving OTel events.
   - **Do NOT emit `turn.started`** from notify (OTel handles this).
   - Sessions persist across turns (no auto-trigger of `session.ended` after turn completion).

### 3. Hook Installation
`aiki hooks install codex` adds both OTel and notify config to `~/.codex/config.toml`:
```toml
[otel]
log_user_prompt = true

[otel.exporter.otlp-http]
endpoint = "http://127.0.0.1:19876/v1/logs"
protocol = "binary"

notify = ["aiki", "hooks", "handle", "--agent", "codex", "--event", "agent-turn-complete"]
```
Note: `exporter` is a tagged enum in codex's config (OtelExporterKind).
Unit variants: `"none"`, `"statsig"`. Struct variants: `{ "otlp-http": { endpoint, protocol } }`.
Note: Codex appends the JSON payload as a final CLI arg to the notify command.

**Existing config handling:**
- If NO `[otel]` section exists: create with aiki's full defaults (endpoint, exporter, protocol, `log_user_prompt = true`).
- If `[otel]` exists with a different endpoint/exporter: **warn** that aiki's OTel receiver won't receive events.
  Provide manual instructions to update the endpoint. Do NOT overwrite their endpoint/exporter settings.
- `log_user_prompt` is always safe to set/update regardless of existing `[otel]` config (it doesn't
  affect the user's telemetry pipeline, only what content Codex includes in events).
  `aiki hooks install codex` sets it to `true`; `aiki doctor --fix` proposes the change interactively.

### 4. Session End (TTL cleanup)
- No `codex.conversation_ends` event exists in Codex.
- Use **2h TTL** (CLI agent default). `cleanup_stale_sessions()` checks `last_event_at`.
- On expiry: if `modified_files` is non-empty, emit `turn.completed` with empty response
  but include the accumulated `modified_files` (these were never cleared because no next
  `turn.started` arrived). Then emit `session.ended` with `reason: ttl_expired`.

### 5. Tests
- Unit tests for OTel payload parsing and event mapping.
- Unit tests for notify payload parsing.
- Integration test: pipe OTLP/HTTP payload to `aiki otel-receive` stdin, verify `session.started` and `turn.started` emitted.
- Integration test: invoke notify handler with `agent-turn-complete` payload, verify `turn.completed` emitted with `modified_files` from session state.
- Test OTel batch handling (multiple events in one request, processed in order).
- Test `cleanup_stale_sessions()` emits final `turn.completed` + `session.ended` for expired Codex sessions.
- Test atomic file writes (concurrent OTel requests don't corrupt session state).
- Test race condition: notify fires before OTel `tool_result` arrives — late `tool_result` is not lost (included in next turn's `modified_files`).
- Test `modified_files` extraction edge cases:
  - `tool_result` with no `arguments` field → no crash, no paths accumulated.
  - `tool_result` with non-file tool (e.g., web search) → ignored.
  - `tool_result` with relative paths → resolved against session `cwd`.
  - Duplicate file paths within a turn → deduplicated in `modified_files`.

### 6. Socket Activation Verification

**`aiki doctor` check (ongoing health):**
- Send a minimal OTLP/HTTP POST to `http://127.0.0.1:19876/v1/logs` with an empty payload.
- Expect 200 OK response (confirms socket activation spawns `aiki otel-receive` correctly).
- If connection refused: socket activation not configured or port not bound.
- If timeout: process spawned but hung (check launchd/systemd logs).
- `aiki doctor --fix` should attempt to install/reload the socket activation config.

**Manual test procedure (development/debugging):**

macOS (launchd):
```bash
# 1. Install the plist
cp dev/com.aiki.otel-receive.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.aiki.otel-receive.plist

# 2. Verify socket is listening
lsof -i :19876  # should show launchd

# 3. Send a test payload
curl -s -X POST http://127.0.0.1:19876/v1/logs \
  -H "Content-Type: application/x-protobuf" \
  -d '' && echo "OK"

# 4. Check process was spawned and exited
log show --predicate 'process == "aiki"' --last 1m
```

Linux (systemd):
```bash
# 1. Install socket + service units
sudo cp dev/aiki-otel-receive.{socket,service} /etc/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now aiki-otel-receive.socket

# 2. Verify socket is listening
systemctl --user status aiki-otel-receive.socket
ss -tlnp | grep 19876

# 3. Send a test payload
curl -s -X POST http://127.0.0.1:19876/v1/logs \
  -H "Content-Type: application/x-protobuf" \
  -d '' && echo "OK"

# 4. Check service logs
journalctl --user -u aiki-otel-receive.service --since "1 min ago"
```

## Known Limitations
- **Requires both OTel + notify:** Both must be configured for complete turn tracking. OTel alone gives starts but no response; notify alone gives responses but no starts or file tracking.
- **Notify without OTel is a no-op:** If notify fires before OTel creates the session (or OTel is broken), the `turn.completed` is dropped with a warning. No bootstrapping — OTel must arrive first.
- **No `conversation_ends`:** Session end relies on TTL cleanup (2h). Last turn's `modified_files` may be lost if notify doesn't fire before TTL expiry.
- **Socket activation complexity:** Requires launchd (macOS) or systemd (Linux) configuration.
- **Prompt content opt-in:** `log_user_prompt = true` required for prompt text; default is redacted.

## Codex-Specific Considerations

### Turn Tracking
- **Turn counter**: Maintained in session state. OTel `user_prompt` increments; notify reads current value.
- **Turn ID**: Deterministic, generated as `{conversation_id}:{turn_number}`. Shared across OTel and notify
  via session state (OTel doesn't carry Codex's `turn-id`, only notify does — so we don't use it).
- **Turn source**: All Codex turns are `source: User` (Codex doesn't support autoreplies in the same way as Claude Code/Cursor)

### Session Lifecycle
- **Session creation**: First `thread-id` seen triggers `session.started`
- **Turn completion**: `agent-turn-complete` notify event triggers `turn.completed`
- **Session persistence**: Sessions remain alive across turns (no auto-cleanup after turn)
- **Session end**: Explicit `session.ended` requires detecting Codex process exit or implementing TTL cleanup

### Autoreply Support
- Codex doesn't have native autoreply support like Claude Code/Cursor
- Aiki flows can generate autoreply context, but there's no mechanism to send it back to Codex
- Consider: `codex exec` for programmatic follow-up prompts triggered by flows

## Decisions

### 1. Session End Detection
**Question:** How to detect session end for Codex? (notify only fires after turn completion, not on exit)

**Decision:** Use **TTL-based cleanup** (event-roadmap.md Phase 2).
- Codex is a **CLI agent**, so use **2h TTL** (same as standalone tools)
- Rely on TTL cleanup since Codex notify doesn't fire on process exit
- PID tracking won't help since notify is fire-and-forget (no persistent connection to monitor)
- Sessions are cleaned up via `cleanup_stale_sessions()` at next session start
- Emit synthetic `session.ended` event to history (not to flows) with reason `ttl_expired`

### 2. Hook Installation
**Question:** Should `aiki hooks install` modify `~/.codex/config.toml`, or just print instructions?

**Decision:** **Modify the config file** (consistent with Claude Code/Cursor behavior).
- `aiki hooks install` already modifies `~/.claude/settings.json` and `~/.cursor/hooks.json`
- For consistency, it should modify `~/.codex/config.toml`
- Print confirmation message showing the config change
- Provide manual instructions as fallback if config file modification fails

`aiki hooks install codex` adds both OTel and notify config together:
```toml
# Added by `aiki hooks install` to ~/.codex/config.toml
[otel]
log_user_prompt = true

[otel.exporter.otlp-http]
endpoint = "http://127.0.0.1:19876/v1/logs"
protocol = "binary"

notify = ["aiki", "hooks", "handle", "--agent", "codex", "--event", "agent-turn-complete"]
```

If `[otel]` already exists, warn the user and provide manual instructions. Do not overwrite.

### 3. Autoreply Support
**Question:** Can we use `codex exec` or Codex API to implement autoreply functionality?

**Decision:** **Not in MVP** - defer to future work.
- `codex exec` is designed for non-interactive CI/CD use, not interactive follow-ups
- It doesn't support resuming a session with a new prompt in the same way Claude Code/Cursor do
- `codex exec resume` exists but is designed for retrying failed runs, not adding new turns
- **Alternative:** Flows could generate a shell script that runs `codex exec <new-prompt>` in a new thread
- **Blocker:** This would create a **new thread** (session), not continue the existing one
- **Future:** Investigate Codex SDK/API for programmatic session continuation

**MVP Behavior:**
- Codex flows can return messages via `context` (shown to user in stderr)
- No automatic autoreply execution (user must manually run follow-up commands)
- Document this limitation in Codex integration docs

## Deferred Events

The following OTel events are **not processed** in the current design:

- **`codex.api_request`** — Per-API-call metadata (duration, status, cf_ray, attempt). Useful for latency/error dashboards but not needed for turn tracking. Could be stored as metrics in a future iteration.
- **`codex.sse_event`** — Per-SSE-chunk metadata (event.kind, duration, token counts). High volume, informational only. Does NOT contain response text — only token usage stats. Could feed cost tracking later.
- **`codex.tool_decision`** — Tool approval/denial events. No cross-agent correlation with Claude Code/Cursor permission models.

These events are acknowledged and returned 200 OK but not mapped to aiki events.
