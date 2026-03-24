# Token Usage Tracking

**Date**: 2026-03-23
**Status**: Draft
**Purpose**: Surface per-turn and per-session token usage from Claude Code and Codex into aiki's event system.

---

## Executive Summary

Neither Claude Code nor Codex expose token usage through their hook/event systems today. However, both write JSONL session files that contain per-turn token data. We can parse these files at turn boundaries (Stop/SessionEnd hooks) to extract usage without needing a new data source. Codex's OTEL traces are another viable path we already partially parse.

---

## Current State of Token Data

### Claude Code

**Hook events do NOT include token data.** The `Stop` hook gives us `session_id`, `cwd`, and `transcript_path` — but no usage fields. There's an open feature request ([#11535](https://github.com/anthropics/claude-code/issues/11535), 17 upvotes) to expose token data to statusline scripts, and a related [#30784](https://github.com/anthropics/claude-code/issues/36056) for rate-limit data. Neither is implemented yet.

**JSONL transcripts DO contain token data.** Each assistant message in the transcript includes:
```json
{
  "type": "assistant",
  "message": {
    "model": "claude-sonnet-4-20250514",
    "usage": {
      "input_tokens": 1234,
      "output_tokens": 567,
      "cache_read_input_tokens": 890,
      "cache_creation_input_tokens": 0
    }
  }
}
```

**What we already do:** In `cli/src/editors/claude_code/events.rs`, the `Stop` hook handler already reads the transcript file to extract the last assistant response text (`extract_last_assistant_response`). Extending this to also extract usage data is straightforward — the data is right there in the same JSON objects we already parse.

**File locations:** `~/.claude/projects/<project>/<session>.jsonl`

### Codex

**OTEL traces contain token data.** Codex emits OpenTelemetry protobuf traces that we already decode in `cli/src/editors/codex/otel.rs`. The `token_count` events in session JSONL files report cumulative totals with fields for input, cached input, output, reasoning, and total tokens.

**Session JSONL files:** `${CODEX_HOME:-~/.codex}/sessions/<session>.jsonl` — each `token_count` event has cumulative totals; per-turn usage is computed by subtracting the previous event's totals.

**Codex hooks:** Codex has experimental hooks (SessionStart, Stop, UserPromptSubmit) but they don't pass token data either.

---

## Approach: Parse Transcripts at Hook Boundaries

Since both editors fire hooks at turn boundaries and give us file paths, we parse the session files ourselves rather than waiting for upstream to expose the data through hooks.

### For Claude Code (recommended approach)

**When:** `Stop` hook fires (end of each turn)
**How:** We already receive `transcript_path`. Walk the JSONL backwards (we already do this) and sum `usage` fields from all assistant messages since the last turn boundary.
**Granularity:** Per-turn (each Stop) and cumulative per-session (sum all turns).

The extraction is ~10 extra lines in `extract_last_assistant_response` or a parallel function.

---

## Design Decisions

### Naming: `TokenUsage` struct, `tokens` field

Provider convention comparison:

| Provider | Parent field | Input field | Output field | Total |
|---|---|---|---|---|
| Anthropic | `usage` | `input_tokens` | `output_tokens` | (not returned) |
| OpenAI | `usage` | `prompt_tokens` | `completion_tokens` | `total_tokens` |

We use `tokens` as the field name and short sub-field names to avoid stutter:

```rust
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.input + self.output
    }
}
```

- `event.tokens.input` reads cleanly — parent provides "what" (tokens), child provides "which" (input/output)
- `total()` is derived, not stored — no point storing what you can compute
- `model` is **not** in `TokenUsage` — it's a peer field, not a child (see below)

### Model tracking: lazy discovery via `ModelChanged` event

Neither Claude Code nor Codex provide the model at session start. Claude Code's `SessionStart` hook gives `session_id`, `cwd`, `source` — no model. Codex has no session start hook at all.

The model is reliably available in transcript responses (each assistant message includes `model`). We extract it at turn completion — same source, same time as token data.

**Flow:**
```
SessionStarted          → session.model = None
TurnCompleted (turn 1)  → extract model from transcript → emit ModelChanged
TurnCompleted (turn N)  → model unchanged → no event
TurnCompleted (turn M)  → model differs → emit ModelChanged
```

- `model` starts as `None` on the session, set on first `TurnCompleted`
- `ModelChanged` is a synthetic event emitted whenever the observed model differs from the stored model
- Consumers (TUI, logging) subscribe to `ModelChanged` and don't care how/when it was discovered
- The gap between session start and first turn (user is typing) is acceptable

**On `TurnCompleted`:**
```rust
pub struct TurnCompleted {
    pub model: Option<String>,        // from transcript response
    pub tokens: Option<TokenUsage>,   // from transcript response
    // ...existing fields...
}
```

`model` and `tokens` are peers — both come from the same source at the same time, but serve different purposes.

### For Codex (recommended approach)

**When:** Turn-end event (we'd need to identify the equivalent boundary — likely after OTEL trace processing)
**How:** Parse the session JSONL's `token_count` events and compute deltas.
**Granularity:** Per-turn (from cumulative deltas) and per-session.

### Alternative: statusLine polling (Claude Code only, fragile)

Claude Code's `statusLine.command` receives JSON with `cost` and `context_window` info (but NOT token counts today). If [#11535](https://github.com/anthropics/claude-code/issues/11535) ships, this would give real-time token data. Not worth depending on since it's unimplemented and we can parse transcripts reliably.

### Alternative: ccusage library

[ccusage](https://github.com/ryoppippi/ccusage) (4.8k stars) is a Node.js tool that parses both Claude Code and Codex JSONL files. We could port its parsing logic to Rust, or reference it as a specification. It handles model→price mapping, cache token accounting, and 5-hour billing windows. Useful as a reference implementation but we don't need the dependency.

---

## Use Cases

1. **Per-turn usage in aiki events** — `TurnCompleted` event includes token counts, enabling downstream consumers (TUI, logging, cost tracking) to display/aggregate usage.
2. **Session cost summary** — On `SessionEnd`, emit cumulative session usage for billing/reporting.
3. **Task-level cost attribution** — Since we track which turns belong to which tasks, we can attribute token costs to specific tasks.
4. **Budget guardrails** — Could warn or stop if a session/task exceeds a token budget (future).

---

## Implementation Plan

### Phase 1: Claude Code turn-level tokens + model (low effort)

1. Add `TokenUsage` struct to `cli/src/events/` types
2. Add `tokens: Option<TokenUsage>` and `model: Option<String>` fields to `AikiTurnCompletedPayload`
3. In `build_turn_completed_event`, extract tokens and model from the transcript (reuse the same file read we already do for response text)
4. Add `ModelChanged` event type; emit it when the model observed on `TurnCompleted` differs from the session's stored model
5. Propagate through event bus

**Effort:** Small — we already read the file and parse the JSON. Just extract two more fields.

### Phase 2: Codex turn-level tokens + model (medium effort)

1. Parse `token_count` events from Codex session JSONL
2. Track cumulative state to compute per-turn deltas
3. Emit same `TokenUsage` struct and model on Codex turn completion

**Effort:** Medium — need to understand and parse the Codex session JSONL format. Reference ccusage's Codex parser.

### Phase 3: Session-level aggregation

1. Accumulate `TokenUsage` across turns in session state
2. On `SessionEnd`, emit cumulative tokens
3. Add `tokens: Option<SessionTokenUsage>` to `AikiSessionEndedPayload`

### Phase 4: Task-level attribution (future)

1. Track which turns belong to which tasks (we already do this via turn_state)
2. Sum turn usage per task
3. Include in task close/summary

---

## Open Questions

1. **Should we also track cost ($)?** We'd need model→price mapping. ccusage has this but it changes frequently. Could be a follow-up.
2. **Codex OTEL vs JSONL?** We already parse OTEL traces — should we extract tokens from there instead of (or in addition to) the session JSONL?
3. **Upstream hook improvements** — If Claude Code ships [#11535](https://github.com/anthropics/claude-code/issues/11535), should we switch to consuming token data from hook payloads instead of parsing transcripts?

---
