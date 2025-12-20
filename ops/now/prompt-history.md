# Prompt History & Code Archaeology

**Status**: 🟡 Design
**Priority**: Medium (enables session resume, search, code archaeology)

## Overview

Store prompt/response history on a JJ `aiki/conversations` branch using the same event-sourcing pattern as the task system. Combined with existing provenance tracking, this enables:

1. **Code archaeology** - `aiki blame` / `aiki why` to understand code origins
2. **Session resume** - Recover context when resuming sessions
3. **Search** - Find past solutions ("what did we do about X?")
4. **Context compaction survival** - Replay history when agent context is compacted

**Key Architecture:** Event-sourced log on orphan `aiki/conversations` branch. Each prompt/response turn is a JJ change with structured metadata, linked to code changes via `change_id`.

---

## Command Structure

```bash
# CODE ARCHAEOLOGY
aiki blame <file>[:line]             # Who changed this code, when, which session
aiki why <file>[:line]               # Why does this code exist? (intent + narrative)

# HISTORY (search the past)
aiki history                         # Recent prompts across all sessions
aiki history "query"                 # Search prompts
aiki history --files <file>          # Find prompts that touched a file

# SESSION MANAGEMENT
aiki session list                    # List sessions
aiki session show [id]               # Show session details (--last for most recent)
aiki session resume [id]             # Resume with context injection
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

### `aiki history` - Search the Past

```bash
$ aiki history
2025-01-15 10:30  s-abc123  "fix the null check in auth"
2025-01-15 10:25  s-abc123  "now add rate limiting"
2025-01-15 10:20  s-abc123  "help me refactor the auth module"
2025-01-14 15:22  s-def456  "add validation step before returning user"
2025-01-14 15:00  s-def456  "create an auth service with JWT support"

$ aiki history "null check"
2025-01-15 10:30  s-abc123  "fix the null check in auth"
  → Edited src/auth.ts:42

$ aiki history --files src/auth.ts
2025-01-15 10:30  s-abc123  "fix the null check in auth"
2025-01-14 15:22  s-def456  "add validation step before returning user"
2025-01-14 15:00  s-def456  "create an auth service with JWT support"
```

### `aiki session` - Session Management

```bash
$ aiki session list
s-abc123  2025-01-15 10:30  claude-code  12 turns  "auth refactor"
s-def456  2025-01-14 15:00  claude-code   8 turns  "security fixes"
s-ghi789  2025-01-14 09:00  cursor        3 turns  "quick fix"

$ aiki session show s-abc123
Session: s-abc123
Agent: claude-code
Started: 2025-01-15 10:30
Turns: 12

Turn 1: "help me refactor the auth module"
  → Read 5 files, edited 2 files

Turn 2: "now add rate limiting"
  → Edited src/middleware/rateLimit.ts (new file)

Turn 3: "fix the null check in auth"
  → Edited src/auth.ts:42

$ aiki session resume s-abc123
Resuming session s-abc123...
Context injected (12 turns, 8 files touched)
```

---

## Why JJ for Prompt History?

| Approach | Pros | Cons |
|----------|------|------|
| **SQLite** | Fast queries, indexes | Separate sync, migration burden |
| **Flat files** | Simple | No structure, hard to query |
| **JJ branch** | Native dedup, revsets, already synced | Linear scan (acceptable for <1000 entries) |

**JJ wins because:**
- Already syncing JJ to remotes (prompts come free)
- Revsets provide powerful querying (`jj log -r 'description("session=abc")'`)
- Change descriptions are mutable (can annotate later)
- No additional infrastructure

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
change_id: "xyz789"                    # Links to JJ change (for aiki who/why)
duration_ms: 105000
files_read: ["src/auth.ts", "src/middleware.ts"]
files_written: ["src/auth.ts", "src/routes/login.ts"]
tools_used: ["Read", "Edit", "Bash"]
errors_detected: 0
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
- `change_id` - Links to JJ change (enables who/why lookups)
- `intent` - Short summary of WHY (see Intent Summaries below)

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
  2. prompt_first_line: # First line of prompt (often states goal)
  3. agent_summary:     # Agent's "I did X" from response
  4. file_action:       # Fallback: "modified src/auth.ts"
```

**Example derivation:**

```
User prompt: "fix the null check in auth, it's causing crashes"

Intent extraction:
  - No explicit tag
  - First line: "fix the null check in auth"  ← use this
  - Truncate to ~50 chars

Stored intent: "fix null check in auth"
```

### Response Event with Intent

```yaml
---
aiki_conversation: v1
event: response
session_id: "abc123"
turn: 3
change_id: "xyz789"
intent: "fix null check in auth"           # ← NEW: extracted intent
intent_source: prompt_first_line           # ← how it was derived
files_written: ["src/auth.ts"]
---
```

### Per-File Intent (for multi-file changes)

When a turn modifies multiple files, capture per-file intent:

```yaml
---
aiki_conversation: v1
event: response
session_id: "abc123"
turn: 2
change_id: "xyz789"
intent: "add JWT authentication"
file_intents:                              # ← per-file breakdown
  src/auth.ts: "core auth service"
  src/routes/login.ts: "login endpoint"
  src/middleware/auth.ts: "JWT validation middleware"
---
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
- Independent sync

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
# HISTORY (search the past)
# ═══════════════════════════════════════════════════════════════════════════════

# Recent prompts
aiki history [--limit 10] [--json]

# Search prompts
aiki history "query"
aiki history --files <file>
aiki history --since yesterday

# ═══════════════════════════════════════════════════════════════════════════════
# SESSION MANAGEMENT
# ═══════════════════════════════════════════════════════════════════════════════

# List sessions
aiki session list [--limit 10] [--json]
aiki session list --agent claude-code

# Show session details
aiki session show <session-id> [--json]
aiki session show --last

# Resume session (inject context via PrePrompt)
aiki session resume [session-id]     # Defaults to --last

# ═══════════════════════════════════════════════════════════════════════════════
# MAINTENANCE
# ═══════════════════════════════════════════════════════════════════════════════

# Compact old sessions
aiki session compact --older-than 30d

# Sync to remote
aiki session sync
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

### Phase 1: Core Storage & Session Commands

1. **Create branch manager** (`cli/src/sessions/manager.rs`)
   - Initialize `aiki/conversations` orphan branch
   - Append prompt/response events
   - Parse events from change descriptions
   - Link responses to JJ changes via `change_id`

2. **Add event recording**
   - Hook into `prompt.submitted` event
   - Hook into `response.received` event
   - Capture `change_id` from working copy

3. **CLI commands**
   - `aiki history` - List/search prompts
   - `aiki session list` - List sessions
   - `aiki session show` - Show session details

### Phase 2: Code Archaeology Commands

1. **`aiki blame`** (existing, enhanced)
   - Quick facts: agent, session, timestamp
   - Link to session/turn via change_id

2. **`aiki why`** (new command)
   - Look up change_id in aiki/conversations
   - Show intent (not raw prompts)
   - Display layered narrative of code evolution

### Phase 3: Session Resume

1. **Session tracking**
   - Detect session resume (same working copy, recent session)
   - Store resume intent

2. **Context injection**
   - Load session history
   - Inject via PrePrompt
   - Format for agent consumption

### Phase 4: Compaction & Sync

1. **Compaction**
   - Summarize old sessions
   - Replace with compact event

2. **Remote sync**
   - Push `aiki/conversations` to remote
   - Handle multi-device scenarios

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
│  aiki/conversations                    │  aiki/tasks                              │
│  ──────────────                  │  ───────────                             │
│  Stores conversation history     │  Tracks work items                       │
│  Event: prompt, response         │  Event: created, started, closed         │
│  Query: aiki session search      │  Query: aiki task ready                  │
│  Resume: aiki session resume     │  Resume: show ready tasks                │
└──────────────────────────────────┴──────────────────────────────────────────┘

Connections:
- Response events include change_id → enables who/why queries
- Response events can reference task IDs worked on
- Task close events can reference the turn that fixed it
- Session resume loads both history AND pending tasks
```

---

## Open Questions

1. **Response summarization** - LLM-based or heuristic (first paragraph)?
   - Start with heuristic, add LLM option later

2. **Privacy** - Should prompts be encrypted at rest?
   - Defer to user preference (future feature)

3. **Multi-agent** - How to handle multiple agents in same repo?
   - Include agent_type in events, filter on query

4. **Storage limits** - When to force compaction?
   - Start with manual, add auto-compaction based on size later

---

## Success Criteria

- [ ] Prompt/response events recorded on `aiki/conversations` branch
- [ ] Response events include `change_id` linking to JJ changes
- [ ] `aiki history` lists and searches prompts
- [ ] `aiki session list/show` commands work
- [ ] `aiki blame` shows attribution (agent, session, timestamp)
- [ ] `aiki why` shows narrative from prompt history
- [ ] `aiki session resume` injects past context via PrePrompt
- [ ] JJ revset queries work for searching
- [ ] <50ms overhead for recording events

---

## Next Steps

1. Review this design
2. Implement Phase 1 (core storage + session commands)
3. Implement Phase 2 (who/why commands)
4. Test with real sessions
5. Implement Phase 3 (session resume)
6. Evaluate need for Phase 4 (compaction)
