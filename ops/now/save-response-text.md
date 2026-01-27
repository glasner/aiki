# Plan: Capture agent response text in conversation history

## Problem
Response events on the `aiki/conversations` jj bookmark have an empty `summary=` field. Each editor provides different mechanisms to capture the agent's response text:

- **Claude Code**: `Stop` hook payload has `session_id` and `cwd` but also provides `transcript_path` — a JSONL file containing the full conversation including assistant responses.
- **Cursor**: `afterAgentResponse` hook fires after each assistant message with `{ text: "<response>" }` plus `conversation_id`, `generation_id`, `model`, `cursor_version`. This is an observational hook (no output fields). Cursor's `stop` hook does NOT include response text.
- **Codex**: Already solved — `agent-turn-complete` notify payload includes `last-assistant-message` field, which is already passed as `response` in `codex/mod.rs:204`.

## Approach (per editor)

### Claude Code — read `transcript_path`
Add `transcript_path` to `StopPayload`, read the JSONL transcript file, extract the last assistant response text, pass it as `response`.

### Cursor — add `afterAgentResponse` event
Add a new `AfterAgentResponse` variant to `CursorEvent`. This hook provides `{ text }` directly — no file parsing needed. Wire it to emit a turn-level response that gets recorded.

**Option A (simpler):** Store the response text in session state when `afterAgentResponse` fires, then use it when the `stop` event builds the `TurnCompleted` event.

**Option B (event-based):** Emit a new `AikiEvent` variant (e.g., `ResponseCaptured`) from `afterAgentResponse` that the event bus handles to attach text to the current turn. This avoids shared mutable state but adds a new event type.

Recommendation: **Option A** — Cursor's `afterAgentResponse` fires before `stop`, so we can stash the text and read it at turn completion.

### Codex — no changes needed
Already captures `last-assistant-message` from the notify payload (`codex/mod.rs:204`).

## Changes

### 1. `cli/src/editors/claude_code/events.rs`
- Add `transcript_path: Option<String>` to `StopPayload` struct (line 115-118)
- In `build_turn_completed_event()` (line 772-781): read the transcript file, extract the last assistant message's text content, pass it as `response`

### 2. Transcript parsing helper (Claude Code)
- Add a function to extract the last assistant response from a JSONL transcript file
- Format: one JSON object per line, `type=assistant` entries have `message.content` array with `{type: "text", text: "..."}` blocks
- Concatenate all text blocks from the last `assistant` entry
- Place inline in `events.rs` (small utility function)

### 3. `cli/src/editors/cursor/events.rs`
- Add `AfterAgentResponse` variant to `CursorEvent` enum:
  ```rust
  #[serde(rename = "afterAgentResponse")]
  AfterAgentResponse {
      #[serde(flatten)]
      payload: AfterAgentResponsePayload,
  }
  ```
- Add `AfterAgentResponsePayload` struct:
  ```rust
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
      text: String,  // The agent's response text
  }
  ```
- In the event handler: write `text` to a session-scoped cache file (e.g., `~/.aiki/sessions/{uuid}/last_response.txt`) so the `stop` handler can read it when building `TurnCompleted`
- In `build_turn_completed_event()`: read the cached response text if available

### 4. `cli/src/commands/benchmark.rs`
- Update benchmark payloads that include `transcript_path` if needed — these may already work

## Key details
- `transcript_path` is optional (`#[serde(default)]`) since older Claude Code versions may not send it
- If the transcript file can't be read or parsed, fall back to empty string (don't fail the hook)
- The response text gets truncated to `MAX_SUMMARY_SIZE` (4KB) in `record_response()` already, so no size concerns
- Cursor's `afterAgentResponse` is observational (no output fields), so the hook handler returns empty/success
- Codex already works — `last-assistant-message` flows through `NotifyPayload` to `TurnCompleted.response`

## Editor response capture summary

| Editor | Mechanism | Field/Source | Status |
|--------|-----------|-------------|--------|
| Claude Code | `Stop` hook | `transcript_path` → JSONL file → last assistant message | **Needs work** |
| Cursor | `afterAgentResponse` hook | `text` field in payload | **Needs work** |
| Codex | `agent-turn-complete` notify | `last-assistant-message` field | **Already done** |

## Files to modify
- `cli/src/editors/claude_code/events.rs` — transcript_path + parsing
- `cli/src/editors/cursor/events.rs` — afterAgentResponse handler
- `cli/src/commands/benchmark.rs` — may need no changes

## Verification
- Build: `cd cli && cargo build`
- Test: `cd cli && cargo test`
- Manual (Claude Code): start a session, send a prompt, check `cd ~/.aiki && jj log -r 'aiki/conversations' -T description` for populated summary
- Manual (Cursor): open a project with aiki hooks, send a prompt, verify response text appears in conversation history
- Manual (Codex): already working — verify no regressions

## References
- Cursor hooks docs: https://cursor.com/docs/agent/hooks#afteragentresponse
- Codex notify payload: `cli/src/editors/codex/mod.rs` (`NotifyPayload.last_assistant_message`)
