# Prompt History Storage

**Status**: 🟡 Design
**Priority**: Medium (enables session resume, search, context survival)

## Overview

Store prompt/response history on a JJ `aiki/prompts` branch using the same event-sourcing pattern as the task system. This enables:

1. **Session resume** - Recover context when resuming sessions
2. **Search** - Find past solutions ("what did we do about X?")
3. **Context compaction survival** - Replay history when agent context is compacted
4. **Audit trail** - Full record of agent interactions

**Key Architecture:** Event-sourced log on orphan `aiki/prompts` branch. Each prompt/response turn is a JJ change with structured metadata in the description.

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

Stored as change description on `aiki/prompts` branch:

```yaml
---
aiki_prompt: v1
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
aiki_prompt: v1
event: response
session_id: "abc123"
timestamp: "2025-01-15T10:31:45Z"
agent_type: claude-code
turn: 1
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

### Why Separate Prompt and Response Events?

1. **Streaming** - Response comes after prompt, may be interrupted
2. **Metadata differs** - Response has duration, files modified, errors
3. **Queryable** - Can query just prompts or just responses
4. **Compaction** - Can compact responses differently than prompts

---

## Branch Structure

```
aiki/prompts (orphan branch, linear append-only log)
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
aiki_prompt: v1
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
# QUERYING HISTORY
# ═══════════════════════════════════════════════════════════════════════════════

# List recent prompts
aiki history [--limit 10] [--json]

# Search prompts
aiki history search "authentication"
aiki history search --files "auth.ts"

# Show session history
aiki history session <session-id>

# Show specific turn
aiki history show <session-id> --turn 3

# ═══════════════════════════════════════════════════════════════════════════════
# SESSION RESUME
# ═══════════════════════════════════════════════════════════════════════════════

# Resume last session (inject history into PrePrompt)
aiki session resume

# Resume specific session
aiki session resume <session-id>

# ═══════════════════════════════════════════════════════════════════════════════
# MAINTENANCE
# ═══════════════════════════════════════════════════════════════════════════════

# Compact old sessions
aiki history compact --older-than 30d

# Sync to remote
aiki history sync
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

### Phase 1: Core Storage

1. **Create branch manager** (`cli/src/prompts/manager.rs`)
   - Initialize `aiki/prompts` orphan branch
   - Append prompt/response events
   - Parse events from change descriptions

2. **Add event recording**
   - Hook into `prompt.submitted` event
   - Hook into `response.received` event
   - Record metadata in change descriptions

3. **CLI commands**
   - `aiki history` - List recent
   - `aiki history search` - Search prompts
   - `aiki history session` - Show session

### Phase 2: Session Resume

1. **Session tracking**
   - Detect session resume (same working copy, recent session)
   - Store resume intent

2. **Context injection**
   - Load session history
   - Inject via PrePrompt
   - Format for agent consumption

### Phase 3: Compaction & Sync

1. **Compaction**
   - Summarize old sessions
   - Replace with compact event

2. **Remote sync**
   - Push `aiki/prompts` to remote
   - Handle multi-device scenarios

---

## JJ Querying Examples

```bash
# Find all prompts in a session
jj log -r 'aiki/prompts & description("session_id: abc123")'

# Find prompts mentioning authentication
jj log -r 'aiki/prompts & description("authentication")'

# Find responses that modified a file
jj log -r 'aiki/prompts & description("files_written:.*auth.ts")'

# Get last 10 prompts
jj log -r 'aiki/prompts' --limit 10
```

---

## Relationship to Other Systems

```
┌─────────────────────────────────────────────────────────────────┐
│  aiki/prompts                    │  aiki/tasks                  │
│  ──────────────                  │  ───────────                 │
│  Stores conversation history     │  Tracks work items           │
│  Event: prompt, response         │  Event: created, started,    │
│                                  │         closed               │
│  Query: "what did we discuss?"   │  Query: "what's left to do?" │
│  Resume: inject past context     │  Resume: show ready tasks    │
└──────────────────────────────────┴──────────────────────────────┘

Connection:
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

- [ ] Prompt/response events recorded on `aiki/prompts` branch
- [ ] `aiki history` commands work
- [ ] Session resume injects past context
- [ ] JJ revset queries work for searching
- [ ] <50ms overhead for recording events
- [ ] Works with existing task system

---

## Next Steps

1. Review this design
2. Implement Phase 1 (core storage)
3. Test with real sessions
4. Implement Phase 2 (session resume)
5. Evaluate need for Phase 3 (compaction)
