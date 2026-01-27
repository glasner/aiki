# Fix: Empty Codex Prompt Content in Conversation History

## Problem

Prompts recorded via the Codex OTel path have empty `content=` fields in JJ conversation history.

## Root Cause

The prompt is recorded via the OTel receive path for Codex:

1. Codex sends a `codex.user_prompt` OTel event
2. In `cli/src/editors/codex/otel.rs:363-369`, the parser looks for a `"prompt"` or `"content"` attribute on the event — but it's `Option<String>`, so it can be `None`
3. In `cli/src/commands/otel_receive.rs:620`, the `None` is converted to an empty string:
   ```rust
   maybe_emit_turn_started(&conversation_id, context, &cwd, prompt.unwrap_or_default());
   ```
4. That empty string flows through `turn_started.rs` → `recorder.rs` → `storage.rs` and gets written as `content=` (empty) in the JJ change description

## Why the Attribute is Missing

Codex may not always include the prompt text in the OTel `codex.user_prompt` event attributes. The code defensively handles this with `unwrap_or_default()`, but that silently produces an empty record.

## Alternative Path That Works

In `cli/src/editors/codex/mod.rs:172-180`, the **notify handler** has a separate `extract_prompt_from_input_messages()` function that can pull the prompt from the `input-messages` JSON payload. But this path is only used for the notify flow, not the OTel receiver.

## Key Files

- `cli/src/editors/codex/otel.rs:363-369` — OTel event parsing, extracts `prompt` field
- `cli/src/commands/otel_receive.rs:620` — converts `None` prompt to empty string
- `cli/src/editors/codex/mod.rs:172-180` — notify handler with `extract_prompt_from_input_messages()`
- `cli/src/events/turn_started.rs` — turn started event handler, calls `history::record_prompt`
- `cli/src/history/recorder.rs` — records prompts with size handling
- `cli/src/history/storage.rs` — serializes events to JJ metadata format

## Possible Fixes

1. Look for additional attribute names on the OTel event (e.g., `"text"`, `"message"`)
2. Fall back to reading Codex session files for the prompt content
3. Skip recording the prompt if content is empty, and let the notify handler record it instead
