# Plan: Rename `aiki session` to `aiki conversation` and list all conversations

## Problem
Users think in terms of "conversations" not "sessions." Currently `aiki session list` only shows active sessions (PID files in `~/.aiki/sessions/`). There's no way to see past conversations. The internal concept of "session" and "conversation" being separate creates unnecessary cognitive overhead for users.

## Goal
Replace `aiki session` with `aiki conversation` as the user-facing command. Show all conversations by default (from JJ history), with `--active` to filter to live ones.

## Non-goals
- Renaming internal types (`AikiSession`, `cli/src/session/`, etc.) — keep internals as-is
- Changing the `~/.aiki/sessions/` file storage path
- Changing the `aiki/conversations` JJ branch name (already correct)

## Changes

### 1. Rename CLI command: `session` → `conversation`

**`cli/src/main.rs`**
- Rename `Commands::Session` → `Commands::Conversation`
- Update help text: "Manage conversations"
- Dispatch to `commands::conversation::run()`

**`cli/src/commands/mod.rs`**
- Rename `pub mod session` → `pub mod conversation`

**`cli/src/commands/session.rs` → `cli/src/commands/conversation.rs`**
- Rename `SessionCommands` → `ConversationCommands`
- Add `--active` flag to `List` variant
- Keep calling into `session::list_all_sessions()` for active session data (internals unchanged)

### 2. Query JJ for all conversations

**`cli/src/history/storage.rs`**
- Add `pub fn list_conversations(cwd: &Path, limit: Option<usize>) -> Result<Vec<ConversationSummary>>`
- Queries `aiki/conversations` branch for all `session_start` events
- Returns: session_id, agent_type, timestamp, turn_count (from latest prompt event)
- **Returns most recent first**: Sorts by `last_activity` timestamp in descending order
- **Limits to 10 by default**: The `limit` parameter defaults to 10 if `None`
- Uses existing `read_events()` infrastructure or a targeted JJ revset query

**New type (in `cli/src/history/types.rs` or inline):**
```rust
pub struct ConversationSummary {
    pub session_id: String,
    pub agent_type: AgentType,
    pub started_at: DateTime<Utc>,
    pub turn_count: u32,
    pub last_activity: DateTime<Utc>,
    pub repo_id: Option<String>,
}
```

### 3. Merge active + historical data in the list command

**`cli/src/commands/conversation.rs`**
- Default (`aiki conversation list`): query JJ for all conversations, enrich with active state (PID alive?) from session files
- With `--active` flag: filter to only conversations with a live PID
- Output columns: `CONVERSATION`, `AGENT`, `TURNS`, `STARTED`, `LAST ACTIVITY`, `STATUS`
- Show PID in parentheses next to agent name for active sessions (e.g., `codex (12345)`)

Example output:
```
CONVERSATION                            AGENT               TURNS  STARTED               LAST ACTIVITY         STATUS
a1b2c3d4-5678-...                       claude-code (12345) 12     2026-01-27T10:30:00   2026-01-27T11:15:00   active
e5f6g7h8-9012-...                       cursor              5      2026-01-26T14:00:00   2026-01-26T15:30:00   ended
i9j0k1l2-3456-...                       codex               3      2026-01-25T09:15:00   2026-01-25T09:45:00   ended
```

### 4. Add `show` subcommand 

**`aiki conversation show <id>`**
- Display turns for a specific conversation
- Show prompts and responses with timestamps
- Uses existing `read_events()` filtered by session_id

## Implementation order
1. Rename `session` → `conversation` in CLI (pure rename, no behavior change)
2. Add `list_conversations()` to `history/storage.rs`
3. Wire up the default list to show all conversations from JJ
4. Add `--active` flag to filter
5. (Optional) Add `show` subcommand

## Key details
- Querying all `session_start` events from JJ may be slow if there are thousands of conversations — consider `--limit` flag (default 50?)
- Active status is determined by cross-referencing `~/.aiki/sessions/` PID files
- The `aiki/conversations` branch is in `~/.aiki/.jj/`, so `cwd` for JJ queries is `global::global_aiki_dir()`
- No migration needed — the JJ branch and session files stay where they are

## Files to modify
- `cli/src/main.rs` — rename command variant
- `cli/src/commands/mod.rs` — rename module
- `cli/src/commands/session.rs` → `cli/src/commands/conversation.rs` — rename + new logic
- `cli/src/history/storage.rs` — add `list_conversations()`
- `cli/src/history/types.rs` — add `ConversationSummary` type (if not inline)

## Verification
- `cargo build` — compiles
- `cargo test` — passes
- `aiki conversation list` — shows all conversations from JJ history
- `aiki conversation list --active` — shows only live sessions
- `aiki session list` — should error with helpful message pointing to new command (or just remove)
