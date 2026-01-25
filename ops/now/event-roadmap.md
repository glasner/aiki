# Event System Redesign: Complete Implementation Plan

## Problem Statement

The current event system conflates turn completion with session termination, causing sessions to end after every turn for hook-based agents. This breaks:
- Stable session tracking across turns
- `--source prompt` resolution
- Task scoping and in-progress visibility (cross-pollination)

Additionally:
- Event naming (`response.received`, `prompt.submitted`) doesn't clearly convey turn-based semantics
- Session lifecycle management relies on implicit behavior rather than explicit signals
- No TTL-based cleanup means stale sessions accumulate indefinitely

## Goals

1. Decouple turn completion from session termination
2. Introduce turn-based event semantics: `turn.started` / `turn.completed`
3. Add explicit session end hooks for proper session lifecycle
4. Implement TTL-based stale session cleanup
5. Use JJ events as single source of truth for session activity

---

## Implementation Phases

### Phase 1: Rename Events to Turn-Based Semantics

**Breaking changes - ships first**

#### Event Renames

**`prompt.submitted` → `turn.started`**
- **Purpose:** Mark the beginning of a turn (user submits prompt OR autoreply)
- **Fires:** When a turn begins (user prompt OR autoreply)
- **Hook mappings:**
  - Claude Code: `UserPromptSubmit` → `turn.started`
  - Cursor: `beforeSubmitPrompt` → `turn.started`
  - ACP: `session/prompt` request → `turn.started`
  - Aiki autoreply → synthetic `turn.started` (source: autoreply)
- **Behavior:**
  - Execute user-defined turn start flows (validation, prompt modification)
  - Track turn boundaries for analytics/logging
  - Autoreply fires synthetic `turn.started` with `source: autoreply`

**`response.received` → `turn.completed`**
- **Purpose:** Mark the end of a turn (agent finishes processing)
- **Fires:** When the agent loop ends (after all iterations in a turn)
- **Hook mappings:**
  - Claude Code: `Stop` → `turn.completed`
  - Cursor: `stop` → `turn.completed`
  - ACP: `session/prompt` response → `turn.completed`
- **Behavior:**
  - Execute user-defined turn completion flows
  - **Does NOT auto-trigger `session.ended`** (that's what explicit session end hooks are for)
  - Every `turn.started` has exactly one `turn.completed` (1:1 correspondence)
  - If autoreply generated: new `turn.started` fires with `source: autoreply`

#### Rationale: Why Separate Events?

**Option A: Separate `turn.completed` event** (CHOSEN)
- ✅ Clear separation of concerns: message-level vs turn-level events are distinct
- ✅ Semantic clarity: event name conveys meaning
- ✅ Future-proof: can add message-level `response.received` later without breaking changes
- ✅ Aligns with agent terminology: both Claude Code and Cursor separate turn completion from session end
- ✅ Simpler flows: no conditional logic needed for common cases

**Option B: `turn_completed` field in payload** (REJECTED)
- ❌ Requires conditional flow syntax (`if/then/else`)
- ❌ Semantic ambiguity: `response.received` doesn't clearly indicate "this might be turn end"
- ❌ Confusing for Claude Code: field would always be `true`
- ❌ Harder to discover: requires reading payload docs

**Conclusion:** Separate events provide better UX and align with agent conceptual models.

#### Payload Structures

```rust
/// Source of a turn (user prompt or autoreply)
pub enum TurnSource {
    /// User-initiated turn (from prompt submission)
    User,
    /// Aiki-initiated turn (from autoreply context injection)
    Autoreply,
}

pub struct AikiTurnStartedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Sequential turn number within session (starts at 1)
    pub turn: u32,
    /// Deterministic turn identifier (uuid_v5 of session_id + turn)
    pub turn_id: String,
    /// Source of this turn (user or autoreply)
    pub source: TurnSource,
    /// The prompt text (user input or autoreply context)
    pub prompt: String,
    /// Injected context references
    pub injected_refs: Vec<String>,
}

pub struct AikiTurnCompletedPayload {
    pub session: AikiSession,
    pub cwd: PathBuf,
    pub timestamp: DateTime<Utc>,
    /// Sequential turn number within session
    pub turn: u32,
    /// Deterministic turn identifier
    pub turn_id: String,
    /// Source of this turn (user or autoreply)
    pub source: TurnSource,
    /// The agent's response text for this turn
    pub response: String,
    /// Files modified during this turn
    pub modified_files: Vec<PathBuf>,
}
```

#### Turn Tracking in Change Metadata

All changes created during a session include turn tracking metadata:

```
[aiki]
session_id=abc123
turn=3
turn_id=a1b2c3d4
turn_source=autoreply
author=claude
tool=Edit
[/aiki]
```

**Turn ID Generation:**
- `turn_id = uuid_v5(session_id, turn.to_string())` (deterministic, reproducible)
- Same session + same turn number = same turn_id across sessions/resumes

**Benefits:**
- Human-readable ordering via sequential `turn` number
- Unique turn references via deterministic `turn_id`
- Query all changes from a specific turn
- Filter changes by turn source (user vs autoreply)

**JJ Revset Query Examples:**
```bash
# All changes in session
jj log -r 'description("session_id=abc123")'

# All changes in turn 3
jj log -r 'description("turn_id=a1b2c3d4")'

# All changes from user turns only (exclude autoreply)
jj log -r 'description("session_id=abc123") & description("turn_source=user")'
```

#### Session State Management

```rust
pub struct SessionState {
    pub session_id: String,
    pub current_turn: u32,
    pub current_turn_id: String,
    pub current_turn_source: TurnSource,
}

impl SessionState {
    /// Start a new turn
    pub fn start_turn(&mut self, source: TurnSource) {
        self.current_turn += 1;
        self.current_turn_id = uuid_v5(self.session_id, self.current_turn.to_string());
        self.current_turn_source = source;
    }
}
```

**Session Resume Behavior:**
- Query JJ for max `turn` value in session:
  ```bash
  # Use JJ template to extract turn value from description (more robust than grep)
  jj log -r 'description("session_id=abc123")' \
    --template 'description' \
    --no-graph | parse_turn_from_description
  ```
- Parse all `turn=N` values from change descriptions
- Restore `current_turn` from max value found
- Regenerate `current_turn_id = uuid_v5(session_id, current_turn.to_string())`
- **Note:** Could use custom JJ template if available, but description parsing is sufficient

#### Task Metadata Updates

Tasks include the turn they were created in:

```
[aiki]
type=task
task_id=task-uuid
session_id=abc123
created_turn=2
created_turn_id=b2c3d4e5
status=pending
name=Fix auth bug
[/aiki]
```

**Query tasks by turn:**
```bash
# All tasks created in a specific turn
jj log -r 'description("created_turn_id=b2c3d4e5")'
```

#### Implementation Steps

1. Add `TurnStarted` variant to `AikiEvent` enum (replaces `PromptSubmitted`)
2. Add `TurnCompleted` variant to `AikiEvent` enum (replaces `ResponseReceived`)
3. Add `turn.started` to `FlowType` struct (replaces `prompt_submitted`)
4. Add `turn.completed` to `FlowType` struct (replaces `response_received`)
5. Map hooks:
   - Claude Code: `UserPromptSubmit` → `TurnStarted`
   - Cursor: `beforeSubmitPrompt` → `TurnStarted`
   - ACP: `session/prompt` request → `TurnStarted`
6. Map hooks:
   - Claude Code: `Stop` → `TurnCompleted`
   - Cursor: `stop` → `TurnCompleted`
   - ACP: `session/prompt` response → `TurnCompleted`
7. In `event_bus`, remove auto-emit of `session.ended` when turn completes without autoreply
8. Update embedded core flow to use `turn.started` and `turn.completed` sections
9. **No backward compatibility:** Users must update flows to use new event names

#### Tests

- `turn.started` fires when user submits prompt (all agents) with `source: User`
- `turn.completed` fires when `Stop`/`stop` hooks execute
- `turn.completed` fires when ACP `session/prompt` response completes
- Autoreply emits synthetic `turn.started` with `source: Autoreply`
- Every `turn.started` has exactly one `turn.completed` (1:1 correspondence)
- `turn.completed` without autoreply → session continues (no auto `session.ended`)
- Flows using `turn.started` and `turn.completed` sections execute correctly
- Flows can filter on `$event.source == 'user'` to exclude autoreply turns

---

### Phase 2: Session Persistence with TTL Cleanup

**Ships with Phase 1 - prevents session accumulation**

All items in this phase ship together. Without TTL cleanup, sessions would accumulate indefinitely since we removed auto-session-end.

#### Decision: Use JJ Events for `last_seen`

**Instead of updating session files on every event**, query `aiki/conversations` branch for latest event timestamp.

**Why JJ events over session files?**

| Criterion | File Updates | JJ Query | Winner |
|-----------|--------------|----------|--------|
| Write performance | Fast (~1ms/turn) | None (already writing events) | JJ Query |
| Read performance | Fast | Slower (~20-50ms) | File Updates |
| Data consistency | Can drift | Always accurate | JJ Query |
| Query flexibility | Limited | Rich (JJ revsets) | JJ Query |
| Code complexity | Higher (locking, parsing) | Lower (reuse existing) | JJ Query |
| Crash safety | Needs locking | Built-in (JJ transactions) | JJ Query |

**Overall:** JJ Query wins on most criteria. Read performance can be optimized with caching if needed.

#### Session File Contents

```
[aiki]
agent=claude-code
external_session_id=abc123
session_id=uuid-v5-hash
started_at=2026-01-23T12:00:00Z
agent_version=0.10.6
parent_pid=12345
[/aiki]
```

**Fields:**
- `agent` - Agent identifier for TTL threshold selection (e.g., `claude-code`, `cursor`, `acp-cli`) - **required for cleanup**
- `external_session_id` - Agent's session ID
- `session_id` - UUID v5 hash for session identification (renamed from `aiki_session_id` for consistency with JJ change descriptions) - **required for cleanup**
- `started_at` - Session creation timestamp
- `agent_version` - Agent version string
- `parent_pid` - Process ID for liveness checks - **required for cleanup**

**Removed:** `cwd` field (not needed, can be inferred from repo location)

**No `last_seen` field needed** - computed from JJ events on demand.

#### Finding `last_seen`

Query JJ for latest event per session:

```bash
jj log -r 'aiki/conversations & description("session_id=<uuid>")' --limit 1
```

Parse timestamp from change metadata.

#### Stale Session Cleanup

- Keep existing parent PID liveness checks (fast path)
- Add TTL cleanup with per-agent defaults:
  - **Editor agents** (Cursor, Claude Code with IDE): **8h**
  - **CLI agents** (standalone tools): **2h**
- TTL cleanup logic:
  1. Check if parent PID is alive (fast - no JJ query needed)
  2. If alive, query latest event timestamp from `aiki/conversations`
  3. If `last_seen < now() - TTL`, delete session file (older than threshold)
- TTL cleanup runs at session start (existing `cleanup_stale_sessions` in `session_started.rs:24`)
- When TTL cleanup removes a session, emit synthetic `session.ended` event **to history only** (does NOT execute `session.ended` flow section — the agent is disconnected, so context/autoreply actions are meaningless). Reasons:
  - **`pid_dead`** - Parent process no longer alive
  - **`ttl_expired`** - No activity within TTL threshold
  - **`no_events`** - Orphaned session (no events found in conversation history)
- Configuration: hardcoded constants in code (no override for now)

#### Implementation Helpers

```rust
fn query_latest_event(repo_path: &Path, session_id: &str) -> Result<Option<DateTime<Utc>>> {
    // jj log -r 'aiki/conversations & description("session_id=<id>")' --limit 1
    // Parse timestamp from change metadata
    // Returns:
    //   Ok(Some(timestamp)) - events found
    //   Ok(None) - no events found (orphaned session)
    //   Err(e) - JJ query failed (repo lock, jj not in PATH, etc.)
}

pub fn cleanup_stale_sessions(repo_path: &Path) {
    let sessions = scan_session_files(repo_path);
    
    for session in sessions {
        // Fast path: check PID (takes precedence over TTL)
        if !process_alive(session.parent_pid) {
            delete_session_file(&session);
            emit_synthetic_session_ended(&session, "pid_dead");
            continue;
        }
        
        // Slow path: TTL check via JJ query
        if let Some(ttl) = get_ttl_threshold(&session.agent) {
            match query_latest_event(repo_path, &session.id) {
                Ok(Some(last_event)) if last_event < now() - ttl => {
                    // Session has events but they're too old
                    delete_session_file(&session);
                    emit_synthetic_session_ended(&session, "ttl_expired");
                }
                Ok(None) => {
                    // No events found = orphaned session (created but never used)
                    delete_session_file(&session);
                    emit_synthetic_session_ended(&session, "no_events");
                }
                Err(e) => {
                    // JJ query failed - don't delete (could be transient error)
                    // Log error and skip this session
                    eprintln!("Warning: Failed to query events for session {}: {}", session.id, e);
                    // Session will be checked again on next cleanup
                }
                Ok(Some(_)) => {
                    // Session is active (events within TTL)
                }
            }
        }
    }
}
```

#### Tests

- `query_latest_event` returns correct timestamp from JJ
- `query_latest_event` returns `None` when no events found for session
- Session selection prefers session with most recent event when multiple match PID
- TTL cleanup removes old sessions but keeps active ones
- TTL cleanup emits synthetic `session.ended` with `reason="ttl_expired"`
- PID-based cleanup emits synthetic `session.ended` with `reason="pid_dead"`
- Orphaned session (no events) cleanup emits synthetic `session.ended` with `reason="no_events"`
- PID-based cleanup is fast (no JJ query needed)
- Session with dead PID but recent events → still cleaned up (PID takes precedence over TTL)
- Integration test: full session lifecycle (start → multiple turns → explicit end) verifying session file persists across turns and is cleaned up at end

---

### Phase 3: Explicit Session End Hooks

**Ships after Phase 1+2 - proper session lifecycle**

These hooks/notifications trigger `session.ended` event explicitly.

#### Claude Code

- Map `SessionEnd` hook → `session.ended` event
- **Hook provides:**
  - `session_id` - Session identifier
  - `transcript_path` - Path to conversation transcript
  - `cwd` - Working directory
  - `reason` - Termination reason: `clear`, `logout`, `prompt_input_exit`, `other`
- **Reference:** https://code.claude.com/docs/en/hooks
- **Note:** `SessionEnd` may not fire on crashes — TTL cleanup handles those cases

#### Cursor

- Map `sessionEnd` hook → `session.ended` event
- **Hook provides:**
  - `reason` - Termination reason: `completed`, `aborted`, `error`, `window_close`, `user_close`
  - `duration_ms` - Session duration in milliseconds
  - `is_background_agent` - Whether this was a background agent
- **Reference:** https://cursor.com/docs/agent/hooks
- **Note:** `sessionEnd` is fire-and-forget — TTL cleanup handles crash cases

#### ACP Agents

**Event Mapping:**
- `session/prompt` request → `prompt.submitted` (current: Phase 1 will rename to `turn.started`)
- `session/prompt` response (with `stopReason: "end_turn"`) → `response.received` (current: Phase 1 will rename to `turn.completed`)
- `session/update` notifications → Triggers `change.completed` events for file-modifying tool calls
- `session/request_permission` → Triggers `change.permission_asked` for file-modifying tools (Edit, Delete, Move)

**Turn Source:**
- User prompts: `source: User` (from IDE via `session/prompt` request)
- Autoreplies: `source: Autoreply` (generated by aiki flows, sent as synthetic `session/prompt` to agent)
- ACP agents **DO support autoreplies** via aiki's flow system (flows return autoreply context, aiki sends new `session/prompt`)

**Session Tracking:**
- ACP protocol **provides session IDs** in messages (e.g., `params.sessionId` in `session/prompt`, `session/update`)
- Agent creates sessions via `session/new` response, which returns a `sessionId`
- Aiki tracks sessions in `.aiki/sessions/` using the session ID from the ACP protocol
- Session is associated with stdin/stdout connection to the ACP agent process
- Agent PID extracted from `session/new` response for subprocess detection

**Session End Detection:**
- **Turn completion**: `response.received` fires when `session/prompt` response has `stopReason: "end_turn"`
- **Autoreplies**: If flows return autoreply context, aiki sends synthetic `session/prompt` to agent (max 5 per turn)
- **Connection close**: When agent process exits, aiki can detect pipe closure (future: explicit `session.ended` event)
- **Error handling**: Non-end_turn stopReasons (max_tokens, refusal, etc.) clean up state without firing `response.received`

**Implementation Location:**
- ACP proxy: `cli/src/commands/acp.rs` (3-thread bidirectional proxy)
- Protocol types: `cli/src/editors/acp/protocol.rs` (JSON-RPC message handling)
- Event handlers: `cli/src/editors/acp/handlers.rs` (event firing logic)
- State management: `cli/src/editors/acp/state.rs` (autoreply counters, session tracking)

#### session.ended Handler

- Execute user-defined `session.ended` flow section
- Record session end to `aiki/conversations` history
- Delete session file
- Clean up any session-specific resources

---

### ~~Phase 4: Message-Level Tracking~~ (Removed)

**Decision:** Not implementing. Moved to `ops/future/events/individual-agent-responses.md`.

**Rationale:**
- Most use cases satisfied by `turn.completed`
- Agent support is inconsistent:
  - Cursor has `afterAgentResponse` hook (fires per message)
  - Claude Code hooks don't expose message-level events
  - ACP could support via `session/update` notifications
- No clear user demand
- Adds complexity without proven value

**Deferred until:** User demand is proven and all agents support it.

---

## Event Lifecycle (After All Phases)

### Claude Code (Hooks)

| Before (Current - Broken) | After (Proposed - Fixed) |
|---------------------------|--------------------------|
| `session.started` | `session.started` |
| ↓ | ↓ |
| `prompt.submitted` | `turn.started` (user) |
| ↓ | ↓ |
| `[tool events...]` | `[tool events...]` |
| ↓ | ↓ |
| `response.received` (Stop hook) | `turn.completed` |
| ↓ | ↓ |
| **[no autoreply]** | `turn.started` (autoreply) ✅ |
| ↓ | ↓ |
| **`session.ended`** ❌ Auto-triggered! | `turn.completed` |
| | ↓ |
| | `turn.started` (autoreply) ✅ |
| | ↓ |
| | `turn.completed` |
| | ↓ |
| | `session.ended` ✅ Explicit SessionEnd hook |

### Cursor (Hooks)

| Before (Current - Broken) | After (Proposed - Fixed) |
|---------------------------|--------------------------|
| `prompt.submitted` | `turn.started` (user) |
| ↓ | ↓ |
| `[tool events...]` | `[tool events...]` |
| ↓ | ↓ |
| `response.received` (stop hook) | `turn.completed` |
| ↓ | ↓ |
| **[no autoreply]** | `turn.started` (autoreply) ✅ |
| ↓ | ↓ |
| **`session.ended`** ❌ Auto-triggered! | `turn.completed` |
| | ↓ |
| | `turn.started` (autoreply) ✅ |
| | ↓ |
| | `turn.completed` |
| | ↓ |
| | `session.ended` ✅ Explicit sessionEnd hook |

**Key Changes:**
1. Events renamed to turn-based semantics: `turn.started` / `turn.completed`
2. Sessions persist across turns (no auto-trigger of `session.ended`)
3. Autoreply creates new turn with synthetic `turn.started` (source: autoreply)
4. Explicit session end via hooks or connection close detection

### ACP Agents (Protocol)

| Before (Current - Broken) | After (Proposed - Fixed) |
|---------------------------|--------------------------|
| `session.started` | `session.started` |
| ↓ | ↓ |
| `prompt.submitted` | `turn.started` (user) |
| ↓ | ↓ |
| `[session/update ...]` | `[session/update ...]` → `change.completed` |
| ↓ | ↓ |
| `response.received` (session/prompt response) | `turn.completed` |
| ↓ | ↓ |
| **[no autoreply]** | `turn.started` (autoreply) ✅ |
| ↓ | ↓ |
| **`session.ended`** ❌ Auto-triggered! | `turn.completed` |
| | ↓ |
| | `turn.started` (autoreply) ✅ |
| | ↓ |
| | `turn.completed` |
| | ↓ |
| | **[connection closed]** ✅ Detected |
| | ↓ |
| | `session.ended` ✅ Explicit (connection close)

---

## Migration Guide for Users

### Breaking Changes in Phase 1

**Before:**
```yaml
prompt.submitted:
  - action: shell
    command: "validate-prompt.sh '{{$event.prompt}}'"

response.received:
  - action: shell
    command: "aiki review --auto"
  - action: context
    value: "Review complete"
```

**After:**
```yaml
turn.started:
  - action: shell
    command: "validate-prompt.sh '{{$event.prompt}}'"

turn.completed:
  - action: shell
    command: "aiki review --auto"
  - action: context
    value: "Review complete"
```

**Migration:**
1. Find/replace `prompt.submitted:` → `turn.started:` in all flow files
2. Find/replace `response.received:` → `turn.completed:` in all flow files

**Filtering user-only turns:**
```yaml
turn.started:
  # Only process user prompts, skip autoreply turns
  - when: "{{ $event.source == 'user' }}"
    action: shell
    command: "log-user-prompt.sh '{{$event.prompt}}'"
```

**No backward compatibility** - Users will see errors if they use old event names.

---

## Agent Hook Reference

### Claude Code Hooks

(Source: https://code.claude.com/docs/en/hooks)

| Hook | Fires | Data | Aiki Mapping |
|------|-------|------|--------------|
| `SessionStart` | Session begins | session_id, cwd, permission_mode | `session.started` ✅ |
| `UserPromptSubmit` | User submits prompt | session_id, prompt, cwd | `turn.started` ✅ (Phase 1) |
| `PreToolUse` | Before tool execution | tool_name, tool_input | `*.permission_asked` ✅ |
| `PostToolUse` | After tool execution | tool_name, result | `*.completed` ✅ |
| `Stop` | Agent loop ends | session_id, cwd | `turn.completed` ✅ (Phase 1) |
| `SessionEnd` | Session ends | session_id, reason, transcript_path | `session.ended` ✅ (Phase 3) |

### Cursor Hooks

(Source: https://cursor.com/docs/agent/hooks)

| Hook | Fires | Data | Aiki Mapping |
|------|-------|------|--------------|
| `beforeSubmitPrompt` | Before prompt submit | conversation_id, prompt | `turn.started` ✅ (Phase 1) |
| `afterAgentResponse` | After each message | response text | NOT MAPPED (Phase 4 removed) |
| `afterAgentThought` | After reasoning step | thought text | NOT MAPPED |
| `stop` | Agent loop ends | status, loop_count | `turn.completed` ✅ (Phase 1) |
| `beforeShellExecution` | Before shell exec | command, cwd | `shell.permission_asked` ✅ |
| `afterShellExecution` | After shell exec | command, output | `shell.completed` ✅ |
| `beforeMCPExecution` | Before MCP call | tool, args | `mcp.permission_asked` ✅ |
| `afterMCPExecution` | After MCP call | result | `mcp.completed` ✅ |
| `afterFileEdit` | After file edit | file_path, operation | `change.completed` ✅ |
| `sessionEnd` | Session ends | reason, duration_ms | `session.ended` ✅ (Phase 3) |

**Key Observations:**
1. Both agents separate turn completion (`Stop`/`stop`) from session end (`SessionEnd`/`sessionEnd`)
2. Cursor distinguishes message-level (`afterAgentResponse`) from turn-level (`stop`)
3. Current `response.received` semantics are unclear (name suggests message, behavior is turn)

---

## Decisions

All questions resolved:

1. **No migration tooling** - Users do find/replace `prompt.submitted:` → `turn.started:` and `response.received:` → `turn.completed:`
2. **TTL configuration** - Hardcoded constants: 8h (editors), 2h (CLI), no override mechanism for now
3. **No Phase 4 (message-level tracking)** - Moved to `ops/future/events/individual-agent-responses.md`, defer until user demand proven
4. **No backward compatibility** - Clean break, `prompt.submitted` and `response.received` flow sections will error
5. **Separate events (Option A) over field (Option B)** - Better UX, semantic clarity, aligns with agent models
6. **JJ events for `last_seen`** - Single source of truth, better consistency, acceptable performance
7. **ACP connection close detection** - Monitor stdin/stdout to emit `session.ended` immediately (don't rely solely on TTL)
8. **Turn-based semantics** - Both events renamed: `turn.started` / `turn.completed` for perfect symmetry
9. **Autoreply = new turn** - Each autoreply emits synthetic `turn.started` with `source: autoreply`, maintaining 1:1 `turn.started`/`turn.completed` correspondence
10. **Turn tracking with explicit turn_id** - Sequential turn number plus deterministic turn_id (uuid_v5 of session_id + turn) in all change/task metadata; enables both human-readable ordering and unique turn references
11. **TTL cleanup: history only, no flows** - Synthetic `session.ended` from TTL/PID cleanup records to history but does NOT execute `session.ended` flow section (agent is disconnected, context actions are meaningless). Flows only run on explicit session end hooks (Phase 3).
12. **No session file migration** - Old session files (with `aiki_session_id` field) will be treated as orphans and cleaned up naturally. Acceptable for pre-1.0 tool.
13. **Hierarchical UUID namespaces** - Refactor session UUID generation: `agent_ns = uuid_v5(AIKI_NAMESPACE, agent_type)`, `session_id = uuid_v5(agent_ns, external_id)`, consistent with `turn_id = uuid_v5(session_id, turn)`
14. **Cursor resume: accept TTL gap** - If TTL cleanup removes a Cursor session file during inactivity, the next prompt is treated as a new session (consistent with TTL having already "ended" it)

---

## Implementation Checklist

### Phase 1: Turn-Based Events

#### Session Resumption Fix

**Problem:** `session.resumed` event is never emitted. Vendors emit `SessionStart` hook for both new and resumed sessions.

**Fix:** Use Claude Code's `source` field to detect resumption:
- [ ] Add `source` field to `SessionStartPayload` struct in `cli/src/editors/claude_code/events.rs:58`
  - [ ] Capture values: `"startup"`, `"resume"`, `"clear"`, `"compact"`
- [ ] Update `build_session_started_event` in `cli/src/editors/claude_code/events.rs:148`
  - [ ] If `source == "resume"` → emit `AikiEvent::SessionResumed`
  - [ ] Otherwise → emit `AikiEvent::SessionStarted`
- [ ] For Cursor: no `source` field available in `sessionStart` hook (only provides `session_id`, `is_background_agent`, `composer_mode`)
  - [ ] Use session file detection: check if session file exists for the given `session_id`
  - [ ] If session file exists → emit `AikiEvent::SessionResumed`
  - [ ] If session file doesn't exist → emit `AikiEvent::SessionStarted`
  - [ ] Add Cursor-specific tests for session file detection
- [ ] Add tests: `source="resume"` → `session.resumed` fires
- [ ] Add tests: `source="startup"` → `session.started` fires
- [ ] Add tests: `session.resumed` flow section executes correctly
- [ ] Document `source` field values in payload struct comments

**Claude Code `source` field values:**
- `"startup"` - New session started
- `"resume"` - Session resumed (from `--resume`, `--continue`, or `/resume`)
- `"clear"` - Session after `/clear` command
- `"compact"` - Session after compaction

**Rationale:** Without this, flows cannot differentiate between fresh starts and continuations, creating a hole in session tracking.

#### Hook Installation (Claude Code)

- [ ] Update `config::install_claude_code_hooks_global` to install all required hooks:
  - [ ] `SessionStart` → `aiki hooks handle --agent claude-code --event SessionStart`
  - [ ] `UserPromptSubmit` → `aiki hooks handle --agent claude-code --event UserPromptSubmit`
  - [ ] `PreToolUse` → `aiki hooks handle --agent claude-code --event PreToolUse`
  - [ ] `PostToolUse` → `aiki hooks handle --agent claude-code --event PostToolUse`
  - [ ] `Stop` → `aiki hooks handle --agent claude-code --event Stop`
  - [ ] `SessionEnd` → `aiki hooks handle --agent claude-code --event SessionEnd` (Phase 3)
- [ ] Use comprehensive tool matcher for Pre/PostToolUse: `Edit|Write|MultiEdit|NotebookEdit|Read|Glob|Grep|LS|Bash|WebFetch|WebSearch|mcp__.*`
- [ ] Extend `doctor` checks to validate all required Claude Code hooks are installed
- [ ] `doctor --fix` should update Claude Code hooks to new template
- [ ] Add tests: Claude Code hook installation includes all required hooks with correct matchers

#### Hook Installation (Cursor)

- [ ] Update `config::install_cursor_hooks` to install all required hooks:
  - [ ] `beforeSubmitPrompt` → `aiki hooks handle --agent cursor --event beforeSubmitPrompt`
  - [ ] `beforeShellExecution` → `aiki hooks handle --agent cursor --event beforeShellExecution`
  - [ ] `afterShellExecution` → `aiki hooks handle --agent cursor --event afterShellExecution`
  - [ ] `beforeMCPExecution` → `aiki hooks handle --agent cursor --event beforeMCPExecution`
  - [ ] `afterMCPExecution` → `aiki hooks handle --agent cursor --event afterMCPExecution`
  - [ ] `afterFileEdit` → `aiki hooks handle --agent cursor --event afterFileEdit`
  - [ ] `stop` → `aiki hooks handle --agent cursor --event stop`
  - [ ] `sessionEnd` → `aiki hooks handle --agent cursor --event sessionEnd` (Phase 3)
- [ ] Extend `doctor` checks to validate all required Cursor hooks are installed
- [ ] `doctor --fix` should update Cursor hooks to new template
- [ ] Add tests: Cursor hook installation includes all required hooks

#### Event Mapping

- [ ] Add `TurnSource` enum with `User` and `Autoreply` variants
- [ ] Add `TurnStarted` variant to `AikiEvent` enum (replaces `PromptSubmitted`)
- [ ] Add `TurnCompleted` variant to `AikiEvent` enum (replaces `ResponseReceived`)
- [ ] Refactor session UUID generation to use hierarchical namespaces:
  - [ ] `agent_namespace = uuid_v5(AIKI_NAMESPACE, agent_type)`
  - [ ] `session_id = uuid_v5(agent_namespace, external_session_id)`
  - [ ] Consistent with turn_id pattern: each level uses parent UUID as namespace
- [ ] Add `SessionState` struct with fields: session_id, current_turn, current_turn_id, current_turn_source
- [ ] Implement `SessionState::start_turn()` method:
  - [ ] Increment `current_turn`
  - [ ] Generate `current_turn_id = uuid_v5(session_id, current_turn.to_string())`
  - [ ] Set `current_turn_source`
- [ ] Add `AikiTurnStartedPayload` struct with fields: session, cwd, timestamp, turn, turn_id, source, prompt, injected_refs
- [ ] Add `AikiTurnCompletedPayload` struct with fields: session, cwd, timestamp, turn, turn_id, source, response, modified_files
- [ ] Add `turn.started` to `FlowType` struct (replaces `prompt_submitted`)
- [ ] Add `turn.completed` to `FlowType` struct (replaces `response_received`)
- [ ] Map hooks: Claude Code `UserPromptSubmit` → `TurnStarted`
- [ ] Map hooks: Cursor `beforeSubmitPrompt` → `TurnStarted`
- [ ] Map hooks: ACP `session/prompt` request → `TurnStarted`
- [ ] Map hooks: Claude Code `Stop` → `TurnCompleted`
- [ ] Map hooks: Cursor `stop` → `TurnCompleted`
- [ ] Map hooks: ACP `session/prompt` response → `TurnCompleted`
- [ ] Confirm `editors/claude_code/events.rs` supports all configured hook events
- [ ] Add missing tool parsing in `claude_code/tools.rs` for new tool types
- [ ] Remove auto-trigger of `session.ended` from turn completion logic in `event_bus`
- [ ] Emit synthetic `turn.started` when autoreply is generated (before context injection)
  - [ ] Call `SessionState::start_turn(TurnSource::Autoreply)` to increment turn counter
  - [ ] Set `source: TurnSource::Autoreply`
  - [ ] Use autoreply context as `prompt` field
  - [ ] Include current `turn` and `turn_id` in payload
- [ ] Include `turn`, `turn_id`, and `turn_source` in all change metadata written to JJ
- [ ] On session resume, query max `turn` from JJ to restore counter:
  - [ ] Use `jj log --template 'description'` to extract descriptions
  - [ ] Parse `turn=N` values from all changes in session
  - [ ] Restore `current_turn` from max value
  - [ ] Regenerate `turn_id`
- [ ] Include `created_turn` and `created_turn_id` in task metadata
- [ ] Update embedded core flow (`cli/src/flows/core/flow.yaml`)
  - [ ] Rename `prompt.submitted:` → `turn.started:`
  - [ ] Rename `response.received:` → `turn.completed:`
  - [ ] Update task injection in `turn.started` to handle both user and autoreply sources
  - [ ] Consider filtering: only inject tasks on user turns (`$event.source == 'user'`)
  - [ ] Update comments to reflect turn-based semantics
- [ ] Remove `PromptSubmitted` and `ResponseReceived` variants (breaking change)
- [ ] Add tests: turn events fire correctly for all agents with `source: User`
- [ ] Add tests: autoreply emits new `turn.started` with `source: Autoreply`
- [ ] Add tests: 1:1 correspondence between `turn.started` and `turn.completed`
- [ ] Add tests: Turn counter increments on each `turn.started`
- [ ] Add tests: Turn ID is deterministic (same session_id + turn = same turn_id)
- [ ] Add tests: Change metadata includes correct turn, turn_id, and turn_source
- [ ] Add tests: Session resume restores correct turn counter from JJ history
- [ ] Add tests: Tasks include created_turn and created_turn_id matching current turn
- [ ] Add tests: Revset queries filter by turn_id correctly
- [ ] Add tests: no auto `session.ended` when turn completes
- [ ] Add tests: flows using `turn.started` and `turn.completed` execute correctly
- [ ] Add tests: flows can filter on `$event.source == 'user'`
- [ ] Add tests: hook parsing for new tool names (MultiEdit, NotebookEdit, Web*, mcp__*)

### Phase 2: Session Persistence with TTL Cleanup

- [ ] Rename session file field: `aiki_session_id` → `session_id` (for consistency with JJ change descriptions)
- [ ] Remove `cwd` field from session file format (not needed)
- [ ] Update session file writer in `cli/src/session/mod.rs:43` (remove cwd, rename session_id)
- [ ] Update session file parser in `cli/src/session/mod.rs:632` (rename session_id)
- [ ] Session file format already includes `agent` field (used for TTL threshold selection)
- [ ] Implement `query_latest_event(repo_path, session_id)` helper
  - [ ] Shell out to `jj log -r 'aiki/conversations & description("session_id=<id>")' --limit 1`
  - [ ] Parse timestamp from change metadata
  - [ ] Return `Option<DateTime<Utc>>` (None if no events found)
- [ ] Update `cleanup_stale_sessions()` in `session_started.rs:24`
  - [ ] Fast path: check PID liveness (no JJ query), takes precedence over TTL
  - [ ] Slow path: query latest event, handle outcomes separately:
    - [ ] `Ok(Some(timestamp))` - check TTL, delete if expired
    - [ ] `Ok(None)` - orphaned session, delete with `reason="no_events"`
    - [ ] `Err(e)` - JJ query failed (transient error), log warning and skip (don't delete)
  - [ ] Delete session file only for: PID dead, TTL expired, or confirmed orphan
  - [ ] Emit synthetic `session.ended` to history only (no flow execution) with reason: `pid_dead`, `ttl_expired`, or `no_events`
- [ ] Add TTL threshold constants: 8h (editors), 2h (CLI) — hardcoded, no override mechanism
- [ ] Update session selection logic: prefer session with most recent event when multiple match PID
- [ ] Add tests: `query_latest_event` returns correct timestamp
- [ ] Add tests: `query_latest_event` returns `None` when no events found
- [ ] Add tests: JJ query succeeds with old timestamp → TTL cleanup removes session (last_seen < now() - TTL)
- [ ] Add tests: JJ query succeeds with recent timestamp → TTL cleanup does NOT delete (within TTL)
- [ ] Add tests: PID-based cleanup doesn't query JJ (fast path)
- [ ] Add tests: Session with dead PID but recent events → still cleaned up (PID precedence)
- [ ] Add tests: JJ query returns `None` (orphaned session) → cleaned up with `reason="no_events"`
- [ ] Add tests: JJ query returns `Err` (transient failure) → session NOT deleted (skip cleanup, log warning)
- [ ] Add tests: Synthetic `session.ended` events recorded with correct reasons (`pid_dead`, `ttl_expired`, `no_events`)
- [ ] Integration test: Full session lifecycle (start → turns → explicit end → cleanup)

### Phase 3: Explicit Session End Hooks

- [ ] Add `SessionEnded` variant to `AikiEvent` enum (if not already exists)
- [ ] Add `AikiSessionEndedPayload` struct with fields: session, cwd, timestamp, reason
- [ ] Map Claude Code `SessionEnd` hook → `SessionEnded` event
  - [ ] Extract: session_id, transcript_path, cwd, reason
  - [ ] Map reason values: clear, logout, prompt_input_exit, other
- [ ] Map Cursor `sessionEnd` hook → `SessionEnded` event
  - [ ] Extract: reason, duration_ms, is_background_agent
  - [ ] Map reason values: completed, aborted, error, window_close, user_close
- [ ] Implement ACP connection close detection
  - [ ] Monitor stdin/stdout for pipe closure
  - [ ] Monitor agent process for exit
  - [ ] Emit `session.ended` with `reason="connection_closed"` when detected
- [ ] Execute `session.ended` flow section on explicit end
- [ ] Record session end to `aiki/conversations` history
- [ ] Delete session file on explicit end (immediate cleanup)
- [ ] Add tests: Claude Code `SessionEnd` hook triggers `session.ended`
- [ ] Add tests: Cursor `sessionEnd` hook triggers `session.ended`
- [ ] Add tests: ACP connection close triggers `session.ended`
- [ ] Add tests: `session.ended` flow section executes
- [ ] Add tests: Session files cleaned up on explicit end
- [ ] Add tests: TTL cleanup still works as fallback (crash scenarios)

---

## Related Documentation

### External References
- Claude Code hooks: https://code.claude.com/docs/en/hooks
- Cursor hooks: https://cursor.com/docs/agent/hooks
- ACP protocol: https://agentclientprotocol.com/protocol/schema

### Codebase Documentation
- `AGENTS.md` - Aiki task system and workflow requirements
- `ops/CHANGE_ID_IMPLEMENTATION.md` - Change vs commit terminology
- `cli/src/events/` - Current event handler implementations
- `cli/src/flows/types.rs` - Flow type definitions
- `ops/future/events/individual-agent-responses.md` - Deferred message-level tracking (Phase 4)


---

## Performance Estimates

### Per-Turn Overhead

| Approach | Write Cost | Read Cost |
|----------|-----------|-----------|
| Session file updates (rejected) | ~1ms × N events/turn | Fast (~1ms) |
| JJ events (chosen) | 0ms (already writing) | ~20-50ms (only on cleanup) |

**Cleanup frequency:** Once per `session.started` event (infrequent)

**Overall:** JJ approach has lower per-turn cost (no file updates). Query overhead only matters during cleanup, which is infrequent.

### Optimization Strategy

If JJ query performance becomes an issue:
1. Add in-memory cache: `{ session_id → latest_event_timestamp }`
2. Refresh cache on new events or periodically
3. Benchmark to validate actual overhead in real repos

---

## Next Steps

1. Review and approve this implementation plan
2. Implement Phase 1 (turn-based events)
3. Test with real Claude Code and Cursor sessions
4. Implement Phase 2 (TTL cleanup with JJ events)
5. Implement Phase 3 (explicit session end hooks)
6. Delete superseded design documents
7. Defer Phase 4 until user demand is proven
