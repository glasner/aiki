# New Codex Hooks Plan

## Goal

Move Aiki's Codex integration off OTLP-dependent session bootstrap and onto
Codex's new native hooks engine, while preserving any OTLP-derived telemetry
that still provides unique value.

## Current State

Aiki's current Codex integration is split across three mechanisms:

1. `codex exec --full-auto` spawning in `cli/src/agents/runtime/codex.rs`
2. OTLP log/traces ingestion in `cli/src/commands/otel_receive.rs` and
   `cli/src/editors/codex/otel.rs`
3. `notify = ["aiki", "hooks", "stdin", "--agent", "codex", "--event",
   "agent-turn-complete"]` configured by `install_codex_hooks_global()` in
   `cli/src/config.rs` — legacy hook, superseded by native hooks

The current design assumes Codex emits enough OTLP lifecycle data for Aiki to
infer:

- session started
- prompt submitted / turn started
- tool activity

The recent production issue shows that assumption is not reliable for
background `codex exec` sessions: the worker starts, but Aiki may never record
`session.started`, so `aiki run` cannot discover the session UUID.

## New Upstream Capability

OpenAI Codex now has a native hooks engine (PR #13276, merged March 10, 2026).
The supported hook events are:

- `sessionStart` — fires when a session begins
- `stop` — fires per-turn when the agent finishes responding (turn-completion
  gate, **not** session end); handlers can block, continue, or stop
- `userPromptSubmit` — fires when the user submits a prompt
- `preToolUse` — fires before tool execution

Important properties from the new hook model:

- hooks are configured in `~/.codex/config.toml` (global) or repo-local config
- hooks run synchronously inside Codex's own lifecycle
- hook commands receive structured JSON on stdin (each event has a schema)
- `sessionStart` stdout can inject context into the model prompt path
- `stop` payload carries both `session_id` and `turn_id` — confirms per-turn
  semantics; handlers return decisions (block/continue/stop)
- hooks are lifecycle-native, not inferred from telemetry
- there is **no session-end hook** — session cleanup remains TTL-based

This is the missing primitive Aiki needs. Session bootstrap should be driven by
the native `sessionStart` hook, not inferred from OTLP.

Reference links:

- PR: https://github.com/openai/codex/pull/13276
- Discussion/demo reference:
  https://x.com/LLMJunky/status/2031582374064951414?s=20
- Hook engine source: https://github.com/openai/codex/tree/main/codex-rs/hooks
- Event definitions: https://github.com/openai/codex/tree/main/codex-rs/hooks/src/events
- Input schemas: https://github.com/openai/codex/tree/main/codex-rs/hooks/schema/generated
- HookEventName type: https://github.com/openai/codex/blob/main/codex-rs/app-server-protocol/schema/typescript/v2/HookEventName.ts
- Config reference: https://developers.openai.com/codex/config-reference

### Hook Payloads (stdin JSON)

All Codex hook payloads share a common base: `hook_event_name`, `session_id`,
`cwd`, `model`, `permission_mode`, `transcript_path` (nullable). Turn-scoped
hooks also carry `turn_id`.

#### SessionStart input

```json
{
  "hook_event_name": "SessionStart",
  "session_id": "abc-123",
  "cwd": "/home/user/project",
  "model": "o3",
  "permission_mode": "default",
  "source": "startup",
  "transcript_path": null
}
```

Fields: `source` is `"startup"`, `"resume"`, or `"clear"` (same semantics as
Claude Code's `source` field).

#### UserPromptSubmit input

```json
{
  "hook_event_name": "UserPromptSubmit",
  "session_id": "abc-123",
  "cwd": "/home/user/project",
  "model": "o3",
  "permission_mode": "default",
  "prompt": "Fix the login bug",
  "transcript_path": "/tmp/transcript.jsonl",
  "turn_id": "turn-001"
}
```

#### PreToolUse input

```json
{
  "hook_event_name": "PreToolUse",
  "session_id": "abc-123",
  "cwd": "/home/user/project",
  "model": "o3",
  "permission_mode": "default",
  "tool_name": "Bash",
  "tool_input": { "command": "cargo test" },
  "tool_use_id": "tool-xyz",
  "transcript_path": "/tmp/transcript.jsonl",
  "turn_id": "turn-001"
}
```

Note: `tool_name` is `"Bash"` in current Codex. Other tool types may be added
upstream. The `tool_input` shape varies by tool (here, Bash carries `command`).

#### Stop input

```json
{
  "hook_event_name": "Stop",
  "session_id": "abc-123",
  "cwd": "/home/user/project",
  "model": "o3",
  "permission_mode": "default",
  "last_assistant_message": "Fixed the login bug by updating...",
  "stop_hook_active": true,
  "transcript_path": "/tmp/transcript.jsonl",
  "turn_id": "turn-001"
}
```

Key: `last_assistant_message` is provided directly (unlike Claude Code, which
requires transcript file parsing). `stop_hook_active` indicates whether the
stop hook is the decision-maker for turn flow.

### Hook Outputs (stdout / stderr / exit code)

Codex hooks use a shared output wire format with these universal fields:
`continue` (bool), `stopReason` (string), `suppressOutput` (bool),
`systemMessage` (string). But each event type has specific behaviors.

All hooks also support **exit code 2** as a special blocking path — stderr
becomes the block reason, stdout is ignored.

Source: `codex-rs/hooks/src/events/*.rs` and
`codex-rs/hooks/src/engine/output_parser.rs`

#### SessionStart output

Codex accepts **plain text OR JSON** on stdout. Plain text is treated
directly as `additionalContext` for the model.

Plain text (simplest — just context injection):

```
WORKSPACE ISOLATION: ...
Task context: ...
```

JSON (when you need systemMessage or structured output):

```json
{
  "systemMessage": "aiki initialized",
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "WORKSPACE ISOLATION: ..."
  }
}
```

No-op (empty stdout, exit 0):

```
(empty)
```

Note: `additionalContext` can also be a top-level field outside
`hookSpecificOutput`.

#### Stop output

No-op (allow normal stop — empty stdout or `{}`):

```json
{}
```

Block for continuation (autoreply — agent retries with reason as prompt):

```json
{ "decision": "block", "reason": "Review required before continuing" }
```

**Exit code 2 alternative** — stderr becomes the continuation prompt:

```
(stdout ignored, exit code 2, stderr: "Run the tests before stopping")
```

Force stop with reason:

```json
{ "continue": false, "stopReason": "Task complete" }
```

#### PreToolUse output

No-op (allow tool — empty stdout or any non-JSON text):

```
(empty)
```

Deny tool execution:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Blocked by aiki flow"
  }
}
```

Legacy deny format (deprecated but supported):

```json
{ "decision": "block", "reason": "Blocked by aiki flow" }
```

**Exit code 2 alternative** — stderr is the deny reason:

```
(stdout ignored, exit code 2, stderr: "Not allowed in this workspace")
```

Important Codex-specific behaviors:
- Only `"deny"` is meaningful for `permissionDecision` — allow is the
  default when the hook returns nothing or exits 0
- `"approve"` in legacy `decision` field **causes a failure** (not supported)
- `additionalContext` in PreToolUse output causes a **fail-open** (tool runs
  anyway) — do not use it for this event

#### UserPromptSubmit output

No-op (approve — empty stdout):

```
(empty)
```

Approve with context injection (plain text):

```
Task context: working on task xyz...
```

Approve with context injection (JSON):

```json
{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": "Task context: ..."
  }
}
```

Block the prompt:

```json
{ "decision": "block", "reason": "Task is blocked" }
```

**Exit code 2 alternative**:

```
(stdout ignored, exit code 2, stderr: "Cannot submit — task is blocked")
```

## Design Direction

For Codex, native hooks become the source of truth for lifecycle events.
OTLP remains required to fill gaps that hooks don't cover.

### Event coverage: hooks vs OTLP

| Aiki Event | Source | Codex Hook | `source` | OTLP? |
|---|---|---|---|---|
| `session.started` | hook | `sessionStart` | `"startup"` | No |
| `session.resumed` | hook | `sessionStart` | `"resume"` | No |
| `session.cleared` | hook | `sessionStart` | `"clear"` | No |
| `turn.started` | hook | `userPromptSubmit` | — | No |
| `turn.completed` | hook | `stop` | — | No |
| `*.permission_asked` | hook | `preToolUse` | — | No |
| `*.completed` (post-tool) | OTLP | — (no hook) | — | **Yes** |
| `session.ended` | TTL | — (no hook) | — | No |

### Target split

- `sessionStart` hook: authoritative for `session.started`
- `userPromptSubmit` hook: authoritative for `turn.started`
- `stop` hook: authoritative for `turn.completed` (replaces legacy `notify`)
- `preToolUse` hook: authoritative for `*.permission_asked`
- Session end: TTL-based (no Codex hook exists)
- OTLP: **required** for post-tool `*.completed` events (change, shell, read,
  mcp, web) and file-write tracking — hooks don't cover these

This aligns Codex with how Aiki treats first-class hook ecosystems in other
editors: use lifecycle-native events when available, use telemetry for the rest.

## Proposed Architecture

### 1. Global Codex hook config

Update `install_codex_hooks_global()` to add native hook entries to the global
`~/.codex/config.toml`, matching how Claude Code hooks are installed globally in
`~/.claude/settings.json`. No repo-local config file is needed — this keeps the
installation model consistent across editors.

Initial hook wiring (added to global config):

```toml
# Added by `aiki hooks install` to ~/.codex/config.toml

[hooks.sessionStart]
command = ["aiki", "hooks", "stdin", "--agent", "codex", "--event", "sessionStart"]

[hooks.userPromptSubmit]
command = ["aiki", "hooks", "stdin", "--agent", "codex", "--event", "userPromptSubmit"]

[hooks.preToolUse]
command = ["aiki", "hooks", "stdin", "--agent", "codex", "--event", "preToolUse"]

[hooks.stop]
command = ["aiki", "hooks", "stdin", "--agent", "codex", "--event", "stop"]
```

OTLP config (required — covers post-tool `*.completed` events that hooks
don't provide):

```toml
[otel]
log_user_prompt = true

[otel.exporter.otlp-http]
endpoint = "http://127.0.0.1:19876/v1/logs"
protocol = "binary"
```

With native hooks as the lifecycle source of truth, OTLP is no longer needed
for session bootstrap correctness. It remains required for tool-activity
tracking (no `postToolUse` hook exists in Codex).

### 2. Codex stdin payload adapter

Add a Codex-native hook payload parser to `aiki hooks stdin` so Aiki can map
Codex hook JSON directly into Aiki events. Follow the same architecture as the
Claude Code handler (`editors/claude_code/`):

| Codex stdin `hook_event_name` | `source` / context | → Aiki Event |
|---|---|---|
| `SessionStart` | `source: "startup"` | `session.started` |
| `SessionStart` | `source: "resume"` | `session.resumed` |
| `SessionStart` | `source: "clear"` | `session.cleared` |
| `UserPromptSubmit` | — | `turn.started` |
| `PreToolUse` | — | `shell.permission_asked` (Codex only has Bash) |
| `Stop` | — | `turn.completed` |

#### SessionStart source discrimination

Codex provides a `source` field on `SessionStart` with the same semantics as
Claude Code. The handler must dispatch to different Aiki events based on it:

```rust
// editors/codex/events.rs — mirrors claude_code/events.rs:221-248
fn build_session_event(payload: SessionStartPayload) -> AikiEvent {
    match payload.source.as_str() {
        "resume" => AikiEvent::SessionResumed(..),
        "clear"  => AikiEvent::SessionCleared(..),
        _        => AikiEvent::SessionStarted(..),  // "startup" or unknown
    }
}
```

Differences from Claude Code:
- **No `"compact"` source** — Codex has no `PreCompact` hook and doesn't emit
  `source: "compact"`. If Codex adds it later, the `_` fallback handles it
  safely (maps to `session.started`).
- **`"clear"` needs the same re-injection trick** — Claude Code's `/clear`
  fires `SessionEnd(reason="clear")` then `SessionStart(source="clear")`.
  If Codex's clear follows the same pattern, the `session.cleared` handler
  must re-inject workspace + task context (same as `mod.rs:27-56` in
  claude_code). If Codex only fires `SessionStart(source="clear")` without
  a preceding end, the handler is simpler — just re-inject context.

#### Target module structure

Mirroring `editors/claude_code/`:

- `editors/codex/events.rs` — Parse Codex stdin JSON → `AikiEvent` (with
  `source` discrimination on `SessionStart`)
- `editors/codex/session.rs` — `AikiSession::for_hook(AgentType::Codex, ...)`
- `editors/codex/output.rs` — `HookResult` → Codex-format stdout JSON.
  Key differences from Claude Code's output.rs:
  - `SessionStart`: can emit plain text (treated as additionalContext) instead
    of JSON — simpler path for context-only injection
  - `PreToolUse`: only emit `deny` — allow is the no-op default (empty stdout).
    Do NOT emit `additionalContext` (causes fail-open). Do NOT emit `approve`
    in legacy decision field (causes failure).
  - `Stop`: support exit code 2 path for blocking (stderr = continuation prompt)
  - `UserPromptSubmit`: plain text stdout = additionalContext injection

#### Requirements

- discriminate event type via `hook_event_name` field in stdin JSON
- discriminate `SessionStart` by `source` field → different `AikiEvent`
  variants (same pattern as claude_code)
- resolve cwd from payload `cwd` field
- use `session_id` (not `thread-id` — different from legacy `notify` payload)
- carry `AIKI_TASK` / `AIKI_SESSION_MODE` through to the created Aiki session
- `sessionStart` handler must output context injection via stdout
  (`hookSpecificOutput.additionalContext`) — especially critical for
  `source: "clear"` to re-inject workspace/task context
- `stop` handler reads `last_assistant_message` directly from payload (no
  transcript parsing needed, unlike Claude Code)

### 3. Session identity and mode

Codex hook-driven `session.started` must create the same Aiki session identity
that later operations expect.

Requirements:

- deterministic session UUID derivation remains stable
- `run_task_id` is attached when `AIKI_TASK` is present
- `SessionMode` is set correctly for `codex exec` background runs
- background Codex runs no longer depend on OTLP to become visible to
  `aiki run`

### 4. OTLP scoped to post-tool events

After hook-based lifecycle is working, OTLP is no longer responsible for
session bootstrap or turn lifecycle. It remains **required** for:

- `*.completed` events (change, shell, read, mcp, web) — no `postToolUse` hook
- file write tracking
- rich turn metadata not exposed via hook stdin

Remove from OTLP — **delete the code** that infers these from telemetry:

- session creation → delete OTLP session-start inference in
  `editors/codex/otel.rs`; `sessionStart` hook is now authoritative
- run session discovery → delete OTLP-based discovery bypass in `aiki run`;
  `sessionStart` hook creates the session file directly
- prompt submit bootstrap → delete any OTLP prompt-start inference;
  `userPromptSubmit` hook handles this
- turn completion → delete legacy `notify` handler in `editors/codex/mod.rs`
  (`handle_turn_complete`); `stop` hook replaces it
- tool permission gating → `preToolUse` hook handles this (OTLP never did)

After this cleanup, `editors/codex/otel.rs` should only contain handlers for
post-tool events (`*.completed`) and file-write tracking. Any OTLP handler
that duplicates a native hook path must be removed to avoid double-dispatch.

## Migration Plan

### Phase 1: Design ✅ Done

Payload schemas documented, event mapping complete, module structure designed,
output behaviors verified from upstream source. See sections above.

Remaining gap: confirm `codex exec` payloads match interactive payloads
(open question #1). Can be validated during Phase 2 implementation.

### Phase 2: Implement `editors/codex/` module

Build the new hook-based handler mirroring `editors/claude_code/`:

1. Create `editors/codex/events.rs`:
   - Codex event enum discriminated by `hook_event_name`
   - Payload structs for `SessionStart`, `UserPromptSubmit`, `PreToolUse`,
     `Stop`
   - `SessionStart` source discrimination (`startup`/`resume`/`clear`)
   - Builder functions for each `AikiEvent` variant
2. Create `editors/codex/session.rs`:
   - `AikiSession::for_hook(AgentType::Codex, session_id, version)`
   - Version detection for Codex binary
3. Create `editors/codex/output.rs`:
   - Per-event output builders using Codex output format
   - Plain text path for `SessionStart` context injection
   - Deny-only for `PreToolUse`
   - Exit code support where needed
4. Update `editors/codex/mod.rs`:
   - New `handle(event_name: &str)` entry point reading stdin JSON
   - `source: "clear"` re-injection handling
   - Keep existing `handle(event_name, payload_json)` for legacy `notify`
     until Phase 4 removes it

Exit criteria:

- `aiki hooks stdin --agent codex --event sessionStart` parses a real Codex
  `SessionStart` payload and creates an Aiki session file.
- `aiki hooks stdin --agent codex --event stop` emits `turn.completed`.
- `aiki run <task> --agent codex` discovers the session UUID from the
  hook-created session file.

### Phase 3: Implement `preToolUse` handler

1. Add `PreToolUse` → `shell.permission_asked` mapping in `events.rs`
2. Wire output.rs to emit deny-only responses (empty stdout = allow)
3. Test with actual Codex `preToolUse` payloads

Exit criteria:

- Aiki can block Codex tool calls via `preToolUse` hook.

### Phase 4: Installer + OTLP cleanup

1. Update `install_codex_hooks_global()` in `config.rs`:
   - Add `[hooks.sessionStart]`, `[hooks.stop]`, `[hooks.userPromptSubmit]`,
     `[hooks.preToolUse]` entries
   - Remove `notify` config entirely
   - Keep `[otel]` config (required for post-tool events)
2. Delete OTLP code that duplicates hook functionality:
   - Session-start inference in `editors/codex/otel.rs`
   - OTLP-based session discovery bypass in `aiki run`
   - Prompt-start inference from OTLP
3. Delete legacy `notify` handler (`handle_turn_complete` in
   `editors/codex/mod.rs`, `NotifyPayload` struct)
4. Verify `editors/codex/otel.rs` only retains post-tool `*.completed`
   handlers and file-write tracking — no lifecycle inference
5. Add doctor checks for both native hooks and OTLP in global config

Exit criteria:

- `aiki init` installs native hooks globally.
- No OTLP code path handles session start, turn start, or turn completion.
- Legacy `notify` code is deleted.

### Phase 5: Verification

1. Add tests exercising hook-driven lifecycle without OTLP session events
2. Test `aiki run <task> --agent codex` in background mode end-to-end:
   - task spawned → `sessionStart` fires → session file created →
     session UUID discovered → `stop` fires → turn recorded
3. Verify interactive and `codex exec` flows separately
4. Verify OTLP-only features (post-tool events) still work
5. Verify behavior when OTLP is absent (lifecycle works, post-tool degrades
   gracefully)

Exit criteria:

- Codex integration works end-to-end with native hooks as lifecycle source.
- `aiki run` no longer depends on OTLP for session discovery.
- Regression tests cover the background session-start failure that triggered
  this work.

## Open Questions

1. ~~What exact JSON payload does Codex pass?~~ **Partially resolved:** Schemas
   documented above. Remaining question: do `codex exec` sessions use the
   same payload shapes, or do fields differ (e.g., `source`, `permission_mode`)?
2. ~~Does `Stop` represent session end, turn end, or both?~~ **Resolved:**
   Per-turn. The `stop` payload carries `turn_id` — maps to `turn.completed`.
   Session end stays TTL-based. (Source: stop.command.input.schema.json,
   hooks/src/events/stop.rs)
3. ~~Should `notify` remain for `agent-turn-complete`?~~ **Resolved:** No —
   `Stop` hook subsumes it. Remove `notify` during migration.
4. ~~Does Codex expose enough tool metadata through hooks to retire OTLP?~~
   **Resolved:** No. `preToolUse` covers pre-tool gating, but there is no
   `postToolUse` hook. OTLP remains required for `*.completed` events.
5. If Codex later supports repo-local `.codex/hooks.json`, should we migrate
   from global config to match, or keep both?

## Risks

- Codex hook payloads may differ between interactive and `exec` sessions.
- Hook invocation may not expose all metadata Aiki currently infers from OTLP.
- Hook-driven context injection on `SessionStart` must not duplicate Aiki's
  existing session-start injection logic.
- Global hook config must coexist with existing OTLP entries without breaking
  them during migration. `notify` will be removed outright.

## Success Criteria

- `aiki run <task> --agent codex` no longer fails because background Codex
  sessions omitted OTLP session-start events.
- Aiki records Codex `session.started` and `turn.completed` from native hooks.
- Session end remains TTL-based (no Codex session-end hook exists).
- `run_task_id` and session UUID discovery work for background `codex exec`
  runs.
- OTLP no longer needed for session bootstrap; remains required for post-tool
  `*.completed` events.
