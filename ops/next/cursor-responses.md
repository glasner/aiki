# Plan: Capture Cursor agent response text

## Problem

Cursor's `stop` hook does not include the agent's response text, so `TurnCompleted.response` is always empty for Cursor sessions. Claude Code and Codex already capture responses.

## Approach

Cursor provides an `afterAgentResponse` hook that fires after each assistant message with `{ text: "<response>" }`. Since each hook invocation is a separate CLI process, we need filesystem-based caching to pass the response text from `afterAgentResponse` to the subsequent `stop` handler.

**Caching strategy:** Write the response text to a temp file keyed by `conversation_id` at `$TMPDIR/aiki-cursor-response-{conversation_id}`. The `stop` handler reads and deletes it. This is ephemeral data that only needs to survive between two hook calls in the same turn — `$TMPDIR` is the natural place for it.

## Changes

### 1. `cli/src/editors/cursor/events.rs` — Add `AfterAgentResponse` event

Add a new variant to `CursorEvent`:

```rust
#[serde(rename = "afterAgentResponse")]
AfterAgentResponse {
    #[serde(flatten)]
    payload: AfterAgentResponsePayload,
}
```

Add payload struct:

```rust
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct AfterAgentResponsePayload {
    #[serde(rename = "conversationId")]
    conversation_id: String,
    #[serde(rename = "generationId")]
    generation_id: String,
    model: String,
    #[serde(rename = "cursorVersion")]
    cursor_version: String,
    #[serde(rename = "workspaceRoots")]
    workspace_roots: Vec<String>,
    text: String,
}
```

### 2. `cli/src/editors/cursor/events.rs` — Cache response text on `afterAgentResponse`

Add a handler that:
1. Writes `payload.text` to `$TMPDIR/aiki-cursor-response-{conversation_id}`
2. Returns a no-op `AikiEvent` (this is an observational hook — no output fields)

Need to decide what `AikiEvent` variant to return. Options:
- Add a new `AikiEvent::Noop` or `AikiEvent::ResponseCached` variant
- Reuse an existing event that the event bus can ignore
- Return `AikiEvent::TurnCompleted` with the response text (but this fires too early)

Recommendation: Add an `AikiEvent::ResponseCaptured` variant that the event bus handles by updating the current turn's pending response. This avoids the temp file entirely if we can hold state in the event bus for the duration of the process — but we can't, since each hook is a separate process.

**Simplest path:** Write to temp file, return a minimal event that the recorder ignores (or a new `Noop` variant). The response gets picked up by `stop`.

### 3. `cli/src/editors/cursor/events.rs` — Read cached response in `build_turn_completed_event`

In `build_turn_completed_event()`:
1. Compute the temp file path from `payload.conversation_id`
2. Read the file contents if it exists
3. Delete the file after reading
4. Pass the text as `response` in `AikiTurnCompletedPayload`
5. Fall back to empty string if the file doesn't exist or can't be read

```rust
fn read_cached_response(conversation_id: &str) -> String {
    let path = std::env::temp_dir()
        .join(format!("aiki-cursor-response-{}", conversation_id));
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let _ = std::fs::remove_file(&path);
            text
        }
        Err(_) => String::new(),
    }
}
```

### 4. Wire up in `build_aiki_event_from_stdin`

Add the match arm:

```rust
CursorEvent::AfterAgentResponse { payload } => build_response_cached_event(payload),
```

## Event handling for `afterAgentResponse`

The `afterAgentResponse` hook is observational — Cursor doesn't read any output from it. We still need to return an `AikiEvent` from `build_aiki_event_from_stdin`. Options:

- **Option A:** Return a `TurnCompleted` with the response text and let the normal flow record it. Problem: this fires *before* `stop`, so the turn hasn't ended yet.
- **Option B:** Add a lightweight `AikiEvent` variant (e.g., `ResponseCaptured`) that the recorder skips but the cache write happens as a side effect in the event builder. The event bus just logs it.
- **Option C:** Do the cache write as a side effect in the event builder, then return an existing benign event.

Recommendation: **Option B** — cleanest separation. The event builder writes the temp file and returns `ResponseCaptured`. The event bus / recorder can log it but takes no history action.

## Key details

- `afterAgentResponse` is observational (no output fields) — Cursor ignores our stdout
- The temp file is conversation-scoped, so concurrent Cursor sessions don't collide
- If `afterAgentResponse` never fires (older Cursor versions), `stop` falls back to empty response — same as today
- Response text gets truncated to `MAX_SUMMARY_SIZE` (4KB) in `record_response()` already, so no size concerns at the temp file level
- `$TMPDIR` is cleaned by the OS; stale files from crashed sessions are not a concern

## Files to modify

- `cli/src/editors/cursor/events.rs` — new event variant, payload struct, cache read/write
- `cli/src/events/mod.rs` (or wherever `AikiEvent` is defined) — add `ResponseCaptured` variant
- `cli/src/event_bus.rs` — handle `ResponseCaptured` (log + no-op)

## Verification

- Build: `cd cli && cargo build`
- Test: `cd cli && cargo test`
- Manual: Open a project in Cursor with aiki hooks, send a prompt, verify `aiki session show` includes response text
