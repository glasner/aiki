# Remove .turn Files - Query JJ Instead

## Goal

Eliminate `.aiki/sessions/<uuid>.turn` and `.turn.autoreply` files by storing all turn state in JJ history.

## Current State

- `.turn` files store: `<turn_number> <source>` (e.g., `3 user`)
- `.turn.autoreply` flag files signal pending autoreply for next turn
- Fallback already exists: `restore_turn_from_jj()` queries JJ for max turn

## Changes

### 1. Remove TurnState file management

**File:** `cli/src/session/turn_state.rs`

- Remove `state_path` field
- Remove `save()` method
- Remove `delete()` cleanup of .turn files
- Remove `set_pending_autoreply()` / `take_pending_autoreply()` / `.turn.autoreply` files
- Keep `TurnState` struct but make it ephemeral (computed on load, not persisted)

### 2. Align conversation event schema with provenance

**File:** `cli/src/history/storage.rs`

Change `session_id=` to `session=` in metadata blocks to match provenance format:

```rust
// Before
add_metadata("session_id", session_id, &mut lines);

// After
add_metadata("session", session_id, &mut lines);
```

Update parser to read `session=` instead of `session_id=`.

### 3. Add turn field to conversation events

**File:** `cli/src/history/types.rs`

Add `turn: u32` and `source: TurnSource` to Prompt and Response events:

```rust
Prompt {
    session_id: String,
    agent_type: AgentType,
    turn: u32,           // NEW
    source: TurnSource,  // NEW - "user" or "autoreply"
    content: String,
    ...
}
```

This allows `turn_completed` to query the Prompt event for the current turn's source.

**File:** `cli/src/history/recorder.rs`

Update `record_prompt()` and `record_response()` to accept turn number.

### 4. Query JJ for turn number

**In `TurnState::load()`:**

```rust
// Always query JJ - no file fallback needed
let current_turn = query_max_turn_from_jj(session_uuid, repo_path).unwrap_or(0);
```

Query `aiki/conversations` branch for `session=<uuid>` events, find max `turn=N`.

### 5. Add Autoreply event to conversation history

**File:** `cli/src/history/types.rs`

Add new event type:

```rust
/// Autoreply generated (pending injection into next turn)
Autoreply {
    session_id: String,
    agent_type: AgentType,
    turn: u32,  // turn that generated this autoreply
    content: String,
    timestamp: DateTime<Utc>,
}
```

**File:** `cli/src/history/recorder.rs`

Add `record_autoreply()` using same truncation as prompts:

```rust
pub fn record_autoreply(
    cwd: &Path,
    session: &AikiSession,
    content: &str,
    turn: u32,
    timestamp: DateTime<Utc>,
) -> Result<()> {
    let event = ConversationEvent::Autoreply {
        session_id: session.uuid().to_string(),
        agent_type: session.agent_type(),
        turn,
        content: truncate_with_marker(content, MAX_PROMPT_SIZE),  // Same limit as prompts
        timestamp,
    };
    write_event(cwd, &event)
}
```

### 6. Update turn_completed to query source from history

**File:** `cli/src/events/turn_completed.rs`

Instead of reading `current_turn_source` from TurnState:

```rust
// Before
payload.source = turn_state.current_turn_source.clone();

// After - query the Prompt event for this session's current turn
let (turn, source) = history::get_current_turn_info(&payload.cwd, payload.session.uuid())?;
payload.turn = turn;
payload.source = source;
```

### 7. Save autoreply to aiki/conversations

**File:** `cli/src/events/turn_completed.rs`

Instead of `set_pending_autoreply()`:

```rust
if let Some(autoreply_content) = context.as_ref() {
    // Best-effort - log and continue on failure (matches existing error handling)
    if let Err(e) = history::record_autoreply(
        &payload_cwd,
        &payload.session,
        autoreply_content,
        Utc::now(),
    ) {
        debug_log(|| format!("Failed to record autoreply: {}", e));
    }
}
```

### 8. Check for pending autoreply from history

**File:** `cli/src/events/turn_started.rs`

Instead of `take_pending_autoreply()`:

```rust
// Check if there's a pending autoreply (autoreply event after last prompt)
if history::has_pending_autoreply(&payload.cwd, payload.session.uuid())? {
    payload.source = TurnSource::Autoreply;
}
```

Query logic: find the latest event for this session. If it's an `Autoreply` event (not a `Prompt`), then we're in autoreply mode.

### 9. Update session activity tracking

**File:** `cli/src/session/mod.rs`

In `find_session_by_ancestor_pid()`, replace `.turn` file mtime check with JJ query:

```rust
// Before: .turn file mtime
let turn_file = path.with_extension("turn");
let last_activity = turn_file.metadata()...

// After: query aiki/conversations for latest event
let last_activity = query_latest_event_timestamp(repo_path, &aiki_id)
    .ok()
    .flatten()
    .map(|dt| dt.into())
    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
```

This is consistent with how `cleanup_stale_sessions()` already determines session activity.

### 10. Clean up session_ended.rs

**File:** `cli/src/events/session_ended.rs`

Remove `turn_state.delete()` call - no files to clean up.

### 11. Update cleanup_session_file()

**File:** `cli/src/session/mod.rs`

Remove `.turn` and `.turn.autoreply` file deletion from `cleanup_session_file()` - these files no longer exist.

### 12. Update tests

- Remove tests for file persistence
- Add tests for JJ query behavior
- Add tests for autoreply event flow

## Files to Modify

1. `cli/src/session/turn_state.rs` - Simplify to JJ-only queries
2. `cli/src/history/types.rs` - Add `turn` field to events, add `Autoreply` event type
3. `cli/src/history/recorder.rs` - Add `turn` param, add `record_autoreply()` function
4. `cli/src/history/storage.rs` - Change `session_id=` to `session=`, add `has_pending_autoreply()` query
5. `cli/src/events/turn_started.rs` - Use history query instead of flag file
6. `cli/src/events/turn_completed.rs` - Record autoreply instead of flag file
7. `cli/src/events/session_ended.rs` - Remove .turn file cleanup

## Migration

No migration needed - `.turn` files are ephemeral session state. Old files will be orphaned but harmless. Can add a cleanup pass later if desired.

## Benefits

- Single source of truth (JJ history)
- No ephemeral files to manage
- Autoreplies become part of conversation history (queryable, auditable)
- Simpler code - less state synchronization
