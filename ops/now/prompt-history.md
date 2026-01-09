# Prompt History & Code Archaeology

**Status**: 🟡 Design
**Priority**: Medium (enables search, code archaeology)

## Overview

Store prompt/response history on a JJ `aiki/conversations` branch using the same event-sourcing pattern as the task system. Combined with existing provenance tracking, this enables:

1. **Code archaeology** - `aiki blame` / `aiki why` to understand code origins
2. **Search** - Find past solutions ("what did we do about X?")
3. **Session listing** - See what sessions have occurred
4. _(Future)_ **Session resume** - Recover context when resuming sessions

**Key Architecture:** Event-sourced log on orphan `aiki/conversations` branch (local-only by default). Each prompt/response turn is a JJ change with structured metadata, linked to code changes via `change_id` range.

**Privacy:** Prompts may contain secrets, PII, or proprietary code. The `aiki/conversations` branch is **not synced to remote by default**. Users can opt-in to sync with `aiki history sync --remote` (future feature).

---

## Command Structure

```bash
# CODE ARCHAEOLOGY
aiki blame <file>[:line]             # Who changed this code, when, which session
aiki why <file>[:line]               # Why does this code exist? (intent + narrative)

# LOG (view AI changes)
aiki log                             # Recent AI changes
aiki log "query"                     # Search by intent
aiki log --files <file>              # Changes that touched a file
aiki log --session <id>              # Filter to session
aiki log --agent <type>              # Filter by agent
aiki log --since "1 week"            # Time filter

# SESSION MANAGEMENT
aiki sessions list                    # List sessions
aiki sessions list --agent <type>     # Filter by agent
```

### `aiki blame` - Attribution

Who changed the code, when, which session.

```bash
$ aiki blame src/auth.ts:42
Line 42: claude-code (session s-abc123, turn 3) 2025-01-15 10:30

$ aiki blame src/auth.ts
src/auth.ts:
  L12-15: claude-code (s-abc123) 2025-01-15
  L42:    claude-code (s-abc123) 2025-01-15
  L67-89: human 2025-01-10
```

### `aiki why` - The Narrative

Code-centric view showing **intent** (why), not raw prompts.

```bash
$ aiki why src/auth.ts

src/auth.ts (3 sessions, 5 changes)
───────────────────────────────────

Origin: 2025-01-14 by claude-code
  └─ "JWT authentication service"

Changes:
  +login(), logout()     "JWT authentication service"
  +validateToken()       "JWT authentication service"
  +.validate() call      "add validation step"
  +?. optional chaining  "fix null check in auth"

$ aiki why src/auth.ts:42

const user = await getUser(id)?.validate();
────────────────────────────────────────────

Created: 2025-01-14 by claude-code
  └─ "JWT authentication service"

+.validate(): 2025-01-14
  └─ "add validation step"

+?. (optional chaining): 2025-01-15
  └─ "fix null check in auth"

$ aiki why src/auth.ts:42 --verbose    # Full prompts if needed

Line 42: `const user = await getUser(id)?.validate();`

2025-01-15 s-abc123 turn 3:
  Prompt: "fix the null check in auth, it's causing crashes in prod"
  Agent: "Added optional chaining to prevent null pointer when user not found"

2025-01-14 s-def456 turn 7:
  Prompt: "add validation step before returning user"
  Agent: "Added .validate() call per security requirements"
```

### Line Attribution Strategy

`aiki why file:line` uses a **two-step lookup** leveraging existing infrastructure:

```
aiki why src/auth.ts:42
         │
         ▼
┌─────────────────────────────────────┐
│ 1. BlameContext::blame_file()       │
│    (existing, uses JJ FileAnnotator)│
└─────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│ LineAttribution {                   │
│   change_id: "xyz789",  ◄── from JJ │
│   session_id: "abc123", ◄── from provenance
│   agent_type: Claude,               │
│ }                                   │
└─────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│ 2. Query aiki/conversations:        │
│    Find response where change_id    │
│    is in first_change_id..last_change_id
└─────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│ ResponseEvent {                     │
│   intent: "fix null check in auth", │
│   turn: 3,                          │
│   ...                               │
│ }                                   │
└─────────────────────────────────────┘
```

**Why this works:** The existing `aiki blame` already uses JJ's `FileAnnotator` to get per-line `change_id` attribution. We store the change_id range in response events, enabling reverse lookup from any line → conversation turn.

### `aiki log` - View AI Changes

```bash
$ aiki log
xyz789  2025-01-15 10:30  claude-code  "fix null check in auth"
  src/auth.ts
abc456  2025-01-15 10:25  claude-code  "add rate limiting"
  src/middleware/rateLimit.ts (new)
def123  2025-01-15 10:20  claude-code  "refactor auth module"
  src/auth.ts, src/routes/login.ts
ghi012  2025-01-14 15:22  claude-code  "add validation step"
  src/auth.ts

$ aiki log "null check"
xyz789  2025-01-15 10:30  claude-code  "fix null check in auth"
  src/auth.ts:42

$ aiki log --files src/auth.ts
xyz789  2025-01-15 10:30  claude-code  "fix null check in auth"
ghi012  2025-01-14 15:22  claude-code  "add validation step"
def123  2025-01-14 15:00  claude-code  "JWT authentication service"
```

### `aiki sessions list` - Session Management

```bash
$ aiki sessions list
s-abc123  2025-01-15 10:30  claude-code  12 turns  "auth refactor"
s-def456  2025-01-14 15:00  claude-code   8 turns  "security fixes"
s-ghi789  2025-01-14 09:00  cursor        3 turns  "quick fix"

$ aiki sessions list --agent claude-code
s-abc123  2025-01-15 10:30  claude-code  12 turns  "auth refactor"
s-def456  2025-01-14 15:00  claude-code   8 turns  "security fixes"
```

---

## Why JJ for Prompt History?

| Approach | Pros | Cons |
|----------|------|------|
| **SQLite** | Fast queries, indexes | Separate sync, migration burden |
| **Flat files** | Simple | No structure, hard to query |
| **JJ branch** | Native dedup, revsets, consistent with task system | Linear scan (acceptable for <1000 entries) |

**JJ wins because:**
- Consistent with `aiki/tasks` branch pattern
- Revsets provide powerful querying (`jj log -r 'description("session=abc")'`)
- Change descriptions are mutable (can annotate later)
- No additional infrastructure
- Optional sync to remote (user opt-in for privacy)

---

## Data Model

### Prompt Event

Stored as change description on `aiki/conversations` branch:

```yaml
---
aiki_conversation: v1
event: prompt
session_id: "abc123"
timestamp: "2025-01-15T10:30:00Z"
agent_type: claude-code
turn: 1
---

# User Prompt

How do I add authentication to my Express app?

---
# Injected Context (from PrePrompt)

[Contents of .aiki/arch/backend.md were injected]
```

### Response Event

```yaml
---
aiki_conversation: v1
event: response
session_id: "abc123"
timestamp: "2025-01-15T10:31:45Z"
agent_type: claude-code
turn: 1
first_change_id: "xyz788"              # First JJ change in this turn
last_change_id: "xyz790"               # Last JJ change in this turn (for revset range)
duration_ms: 105000
files_read: ["src/auth.ts", "src/middleware.ts"]
files_written: ["src/auth.ts", "src/routes/login.ts"]
tools_used: ["Read", "Edit", "Bash"]
errors_detected: 0
intent: "add JWT authentication"       # Extracted from agent summary
intent_source: agent_summary           # How intent was derived
---

# Response Summary

Added JWT authentication with:
- Login endpoint at /api/login
- Auth middleware for protected routes
- Token validation

# Key Changes

- src/auth.ts: Added validateToken() function
- src/routes/login.ts: New file with login handler
```

**Key fields:**
- `first_change_id` / `last_change_id` - JJ change range (enables who/why lookups via revset)
- `intent` - Short summary of WHY, derived from agent response (see Intent Summaries below)
- `intent_source` - How intent was derived (agent_summary, prompt_first_line, etc.)

---

## Intent Summaries

The key to making `aiki why` useful is capturing **intent** at write time, not just raw prompts.

### What is Intent?

Intent answers "why was this change made?" in a single line:

| Raw Prompt | Intent |
|------------|--------|
| "fix the null check in auth, it's causing crashes in prod" | "null safety fix - production crashes" |
| "can you add validation before returning the user object" | "add validation step" |
| "create an auth service with JWT support for our Express app" | "JWT authentication service" |

### How Intent is Captured

Intent is derived from multiple sources, in priority order:

```yaml
intent_sources:
  1. explicit_tag:      # User tags intent: "intent: security fix"
  2. agent_summary:     # Agent's "I did X" from response (preferred)
  3. prompt_first_line: # First substantive line of prompt
  4. file_action:       # Fallback: "modified src/auth.ts"
```

**Why agent summary is preferred:** The agent's response summary (e.g., "Added optional chaining to prevent null pointer") is more reliable than the user's prompt, which may be conversational or ambiguous ("can you fix that thing?").

**Example derivation:**

```
User prompt: "fix the null check in auth, it's causing crashes"
Agent response: "Added optional chaining to prevent null pointer when user not found..."

Intent extraction:
  - No explicit tag
  - Agent summary first line: "Added optional chaining to prevent null pointer"
  - Truncate to ~50 chars

Stored intent: "add optional chaining for null safety"
intent_source: agent_summary
```

### Response Event with Intent

```yaml
---
aiki_conversation: v1
event: response
session_id: "abc123"
turn: 3
first_change_id: "xyz788"
last_change_id: "xyz790"
intent: "add optional chaining for null safety"
intent_source: agent_summary
files_written: ["src/auth.ts"]
---
```

### Per-File Intent (Phase 2)

When a turn modifies multiple files, we use a single intent for v1. Per-file intent granularity is deferred to Phase 2:

```yaml
# Phase 1: Single intent for all files
intent: "add JWT authentication"
files_written: ["src/auth.ts", "src/routes/login.ts", "src/middleware/auth.ts"]

# Phase 2 (future): Per-file breakdown
# file_intents:
#   src/auth.ts: "core auth service"
#   src/routes/login.ts: "login endpoint"
#   src/middleware/auth.ts: "JWT validation middleware"
```

### How `aiki why` Uses Intent

```bash
$ aiki why src/auth.ts:42

const user = await getUser(id)?.validate();
────────────────────────────────────────────

Created: 2025-01-14 by claude-code
  └─ "JWT authentication service"

+.validate(): 2025-01-14
  └─ "add validation step"

+?. (optional chaining): 2025-01-15
  └─ "fix null check in auth"
```

The output shows **intent**, not raw prompts. This is:
- Shorter and scannable
- Focused on WHY, not conversation details
- Useful for understanding code at a glance

### Explicit Intent Tags (Future)

Users could explicitly tag intent in prompts:

```
User: "intent: security hardening

Please add rate limiting to the auth endpoints"
```

This would be extracted and stored verbatim, overriding automatic derivation

### Why Separate Prompt and Response Events?

1. **Streaming** - Response comes after prompt, may be interrupted
2. **Metadata differs** - Response has duration, files modified, errors
3. **Queryable** - Can query just prompts or just responses
4. **Compaction** - Can compact responses differently than prompts

---

## Branch Structure

```
aiki/conversations (orphan branch, linear append-only log)
├── change-001: [prompt] session=abc, turn=1, "How do I add auth..."
├── change-002: [response] session=abc, turn=1, files=[auth.ts, login.ts]
├── change-003: [prompt] session=abc, turn=2, "Now add rate limiting"
├── change-004: [response] session=abc, turn=2, files=[middleware.ts]
├── change-005: [prompt] session=def, turn=1, "Fix the type error in..."
└── ...
```

**Why orphan branch?**
- No connection to main working copy
- Append-only (never rebase/squash)
- Local-only by default (privacy)

### Concurrency

Multiple agents or terminals may write to `aiki/conversations` simultaneously. We accept **eventual consistency**:

- Events include timestamps for ordering
- On read, sort by timestamp to reconstruct order
- Interleaved events from different sessions are fine (filter by session_id)
- No locking required for v1 (single-agent is the common case)

---

## Storage Considerations

### Full Text vs Summary

| What | Store Full? | Rationale |
|------|-------------|-----------|
| User prompt | ✅ Yes | Small, essential for search |
| Injected context | ❌ References only | Large, can regenerate |
| Response | 🟡 Summary + key changes | Full too large, summary searchable |

### Estimated Size

- Average prompt: ~500 bytes
- Average response summary: ~1KB
- Metadata: ~200 bytes
- **Per turn: ~1.7KB**
- **1000 turns: ~1.7MB** (very manageable)

### Compaction (Future)

For long-running projects, compact old sessions:

```yaml
---
aiki_conversation: v1
event: session_summary
session_id: "abc123"
original_turns: 47
compacted_at: "2025-02-01T00:00:00Z"
---

# Session Summary

Implemented authentication system over 47 turns.

## Key Outcomes
- Added JWT auth with refresh tokens
- Created login/logout/register endpoints
- Added rate limiting middleware

## Files Created/Modified
- src/auth.ts
- src/routes/auth/*.ts
- src/middleware/rateLimit.ts
```

---

## CLI Commands

```bash
# ═══════════════════════════════════════════════════════════════════════════════
# CODE ARCHAEOLOGY
# ═══════════════════════════════════════════════════════════════════════════════

# Who changed this code?
aiki blame <file>[:line] [--json]

# Why does this code exist?
aiki why <file>[:line] [--json]

# ═══════════════════════════════════════════════════════════════════════════════
# LOG (view AI changes)
# ═══════════════════════════════════════════════════════════════════════════════

# Recent AI changes
aiki log [--limit 20] [--json]

# Filtering
aiki log "query"                     # Search by intent
aiki log --files <file>              # Changes that touched this file
aiki log --session <id>              # Filter to session
aiki log --agent <type>              # Filter by agent (claude-code, cursor)
aiki log --since "1 week"            # After date
aiki log --until "yesterday"         # Before date

# Output options
aiki log --stat                      # Show files changed per change
aiki log --verbose                   # Show full details
aiki log --reverse                   # Oldest first

# ═══════════════════════════════════════════════════════════════════════════════
# SESSION MANAGEMENT
# ═══════════════════════════════════════════════════════════════════════════════

# List sessions
aiki sessions list [--limit 10] [--json]
aiki sessions list --agent claude-code
```

---

## Flow Integration

### Recording Prompts (prompt.submitted)

```yaml
prompt.submitted:
  - history.record:
      event: prompt
      session_id: $session.id
      turn: $session.turn
      content: $prompt.original
      injected_refs: $prompt.injected_files
```

### Recording Responses (response.received)

```yaml
response.received:
  - history.record:
      event: response
      session_id: $session.id
      turn: $session.turn
      duration_ms: $response.duration_ms
      files_read: $session.files_read
      files_written: $session.files_written
      tools_used: $session.tools_used
      summary: $response.first_paragraph  # Or use LLM summarization
```

### Session Resume (session.started with resume flag)

```yaml
session.started:
  - if: $session.resuming
    then:
      - let: history = self.get_session_history($session.resume_from)
      - prompt:
          prepend: |
            # Previous Session Context

            You're resuming a previous session. Here's what happened:

            $for turn in $history.turns:
              ## Turn $turn.number
              **User:** $turn.prompt_summary
              **You:** $turn.response_summary
              **Files:** $turn.files_written

            Continue from where we left off.
```

---

## Implementation Plan

### Phase 1: Core Storage & Commands

1. **Create history manager** (`cli/src/history/manager.rs`)
   - Initialize `aiki/conversations` orphan branch (local-only)
   - Append prompt/response events
   - Parse events from change descriptions
   - Link responses to JJ changes via `first_change_id`/`last_change_id` range

2. **Add event recording**
   - Hook into `prompt.submitted` event
   - Hook into `response.received` event
   - Capture change_id range (first/last) during turn
   - Extract intent from agent summary

3. **CLI commands**
   - `aiki log` - List/search AI changes
   - `aiki sessions list` - List sessions

4. **`aiki why`** (new command)
   - Two-step lookup: JJ blame → change_id → conversation
   - Use existing `BlameContext::blame_file()` for line attribution
   - Query conversation by change_id range
   - Show intent (not raw prompts)

### Phase 2 (Future): Extended Features

1. **Per-file intent** - Granular intent per file in multi-file changes

2. **Compaction** - Summarize old sessions, replace with compact event

3. **Remote sync (opt-in)** - `aiki history sync --remote` with privacy warnings

---

## JJ Querying Examples

```bash
# Find all prompts in a session
jj log -r 'aiki/conversations & description("session_id: abc123")'

# Find prompts mentioning authentication
jj log -r 'aiki/conversations & description("authentication")'

# Find responses that modified a file
jj log -r 'aiki/conversations & description("files_written:.*auth.ts")'

# Get last 10 prompts
jj log -r 'aiki/conversations' --limit 10
```

---

## Relationship to Other Systems

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              CODE ARCHAEOLOGY                                │
│                                                                             │
│   aiki blame file:line        aiki why file:line                            │
│        │                            │                                       │
│        ▼                            ▼                                       │
│   ┌─────────┐                 ┌─────────────┐                               │
│   │ Facts   │                 │ Narrative   │                               │
│   │ WHO     │───change_id────▶│ WHY         │                               │
│   │ WHEN    │                 │ INTENT      │                               │
│   └─────────┘                 └─────────────┘                               │
│   (from JJ change              (from aiki/conversations                     │
│    descriptions)                branch)                                     │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│  aiki/conversations              │  aiki/tasks                        │
│  ───────────────────             │  ───────────                       │
│  Stores conversation history     │  Tracks work items                 │
│  Event: prompt, response         │  Event: created, started, closed   │
│  Query: aiki log                 │  Query: aiki task ready            │
│  List: aiki sessions list        │  List: aiki task list              │
└──────────────────────────────────┴──────────────────────────────────────────┘

Connections:
- Response events include change_id range → enables who/why queries via two-step lookup
- Response events can reference task IDs worked on
- Task close events can reference the turn that fixed it
- Session resume loads both history AND pending tasks
```

---

## Resolved Questions

1. **Privacy** - Local-only by default. `aiki/conversations` is not synced to remote. Opt-in sync available as future feature.

2. **Line attribution** - Two-step lookup: JJ blame → change_id → conversation. Uses existing `BlameContext::blame_file()`.

3. **Multi-change linking** - Store `first_change_id` + `last_change_id` to define revset range for lookup.

4. **Intent extraction** - Prefer agent summary over prompt first line. More reliable for deriving intent.

5. **Per-file intent** - Defer to Phase 2. Single intent per response for v1.

6. **Concurrency** - Accept eventual consistency. Timestamp-based ordering on read.

7. **Session boundaries** - Already handled in aiki (via existing session tracking).

8. **Performance overhead** - Already handled elsewhere (async/buffered recording).

## Open Questions

1. **Response summarization** - LLM-based or heuristic (first paragraph)?
   - Start with agent's first paragraph, add LLM option later

2. **Storage limits** - When to force compaction?
   - Start with manual, add auto-compaction based on size later

---

## Success Criteria

- [ ] Prompt/response events recorded on `aiki/conversations` branch (local-only)
- [ ] Response events include `first_change_id`/`last_change_id` range
- [ ] `aiki log` lists and searches AI changes
- [ ] `aiki sessions list` works
- [ ] `aiki blame` shows attribution (agent, session, timestamp) - already implemented
- [ ] `aiki why` shows narrative using two-step lookup (blame → conversation)
- [ ] JJ revset queries work for searching
- [ ] Intent extracted from agent summary

---

## Next Steps

1. ~~Review this design~~ ✓
2. Implement Phase 1 (core storage + log/sessions commands)
3. Implement Phase 2 (blame enhancement + why command)
4. Test with real sessions
