# Global Aiki State

## Summary

Consolidate aiki state into a global location (`~/.aiki/`):
- **Sessions** - Store in `~/.aiki/sessions/` instead of per-repo
- **Conversations** - Store in global JJ repo at `~/.aiki/jj/` on `aiki/conversations` branch
- **Codex unification** - Replace Codex-specific storage with standard `AikiSession` pattern

## Motivation

1. **Sessions can span repos** - An agent conversation might work across multiple repositories. Per-repo storage doesn't capture this.

2. **Conversations should follow sessions** - If a session spans repo A and repo B, conversation events (prompts, responses) shouldn't be fragmented across per-repo `aiki/conversations` branches. Need unified history.

3. **Codex uses separate storage** - Currently stores in `~/.aiki/codex-sessions/{external_id}.json` with a different format than standard session files. This is confusing and duplicates logic.

4. **Simpler cleanup** - Single location to scan for TTL/PID cleanup instead of per-repo.

5. **Session is about the conversation, not the repo** - Events already capture `cwd` per-event, so we know where work happened.

## Current State

### Standard Sessions (Claude Code, Cursor)
- Location: `{repo_path}/.aiki/sessions/{session_uuid}`
- Format: `[aiki]...[/aiki]` metadata block
- Created via `AikiSessionFile::create()`
- Fields: agent, external_session_id, session_id, started_at, agent_version, parent_pid

### Codex Sessions
- Location: `~/.aiki/codex-sessions/{external_id}.json`
- Format: JSON
- Created via `state::update_state()`
- Fields: external_id, agent, agent_version, agent_pid, current_turn, last_turn_started, session_started, modified_files, cwd, last_event_at

### Key Differences
| Aspect | Standard | Codex |
|--------|----------|-------|
| Location | Per-repo | Global |
| Format | [aiki] block | JSON |
| ID in filename | session_uuid | external_id |
| Turn tracking | `.turn` sidecar file | In main JSON |
| Modified files | Not tracked in session | Accumulated per-turn |

### Conversation History (Current)
- Location: `{repo}/.aiki/jj/` on `aiki/conversations` branch
- Each event is a JJ change with metadata in description
- Problem: If session spans repos A and B, events are split between two branches

## Design (v1)

### Directory Layout

**Global:**
```
~/.aiki/
├── sessions/           # Active session state (flat files)
│   ├── {uuid}          # Session metadata [aiki]...[/aiki]
│   └── {uuid}.turn     # Turn state sidecar (turn number + source)
└── jj/                 # Global JJ repository for conversation history
    └── .jj/            # JJ internals
        └── (aiki/conversations branch lives here)
```

**Per-repo:**
```
{repo}/.aiki/
└── jj/                 # Per-repo JJ repository
    └── .jj/            # JJ internals
        └── (aiki/tasks branch and working copy tracking)
```

**Note:** Per-repo `.aiki/jj/` holds only JJ-tracked data (tasks, working copy changes). Sessions live globally.

### 1. Global JJ Repository

`~/.aiki/jj/` is a dedicated JJ repository for global aiki state.

**Contains:**
- `aiki/conversations` branch - All conversation events (prompts, responses, session lifecycle)
- Future: Could hold other global branches (e.g., cross-repo task coordination)

**Why a JJ repo instead of flat files?**
- Keep using JJ's revsets for querying (`jj log -r 'description("session=abc")'`)
- Change tracking for free
- Existing `aiki/conversations` code works with minimal changes
- Consistent with how we store data in project repos

**Initialization:**
- Created by `aiki init` (not lazy)
- `jj init` in `~/.aiki/jj/` (non-colocated, no git)
- No working copy needed (all operations via `--no-edit`)

**Relation to session files:**
- Session files (`~/.aiki/sessions/{uuid}`) track active state (PID, turn)
- Conversation events (`~/.aiki/jj/` on `aiki/conversations`) track history
- Linked by `session_id`

### 2. Global Session Location

All session files live in:
```
~/.aiki/sessions/{session_uuid}
```

The session UUID is deterministic: `UUIDv5(NAMESPACE, "{agent_type}:{external_session_id}")`, so it's stable across processes.

### 3. Unified Session File Format

Keep the `[aiki]...[/aiki]` format but add fields needed for all agents:

```
[aiki]
agent=codex
external_session_id=019bf548-9109-7f52-bce2-b66bb20c68dd
session_id=a1b2c3d4-...
started_at=2026-01-25T12:00:00Z
agent_version=0.89.0
parent_pid=12345
repos=/Users/user/project-a
repos=/Users/user/project-b
[/aiki]
```

**New field:**
- `repos` - All repositories (cwds) touched by this session (append-only, deduplicated)

**Why track all repos?**
- Enables "what sessions affected this repo" queries
- Provides complete picture of session scope
- Useful for cleanup and debugging

**Turn state** stays in `.turn` sidecar file with format:
```
<turn_number> <source>
```

Example: `3 user` or `5 autoreply`

### 4. Codex-Specific State

Codex needs to track `modified_files` between OTel events. Store in extended `.turn` file:

```
3 user
/path/to/file1.rs
/path/to/file2.md
```

Format:
- Line 1: `<turn_number> <source>`
- Remaining lines: Modified file paths (newline-separated)

**Why newline-separated instead of comma-separated?**
- File paths can contain commas, spaces, etc.
- Simpler parsing (just split on newline)
- Consistent with other multi-value formats

### 5. `aiki init` Behavior

When running `aiki init` in a repository:

1. Creates `~/.aiki/sessions/` (if not exists)
2. Creates `~/.aiki/jj/` with `jj init` (if not exists)
3. Creates `{repo}/.aiki/jj/` with `jj init` (non-colocated, if not exists)
4. Installs git hooks
5. Prints summary

**No lazy initialization** - directories are created upfront to avoid race conditions and simplify logic.

## API Changes

### AikiSessionFile

```rust
impl AikiSessionFile {
    /// Create session file handle (always uses global location)
    pub fn new(session: &AikiSession) -> Self;
}
```

**Changes:**
- Removed `repo_path` parameter - always global
- Removed `global()` method - redundant since `new()` is always global
- No migration/fallback code - clean break

### Session Lookup

```rust
/// Find active session by PID ancestry
pub fn find_active_session() -> Option<SessionMatch>;
```

**Changes:**
- Removed `repo_path` parameter
- Always searches `~/.aiki/sessions/`

### Session Tracking

```rust
/// Update session with new repo path
pub fn add_repo_to_session(session_id: &str, repo_path: &Path) -> Result<()>;

/// Get all repos touched by a session
pub fn get_session_repos(session_id: &str) -> Result<Vec<PathBuf>>;

/// Find sessions that touched a specific repo
pub fn find_sessions_by_repo(repo_path: &Path) -> Result<Vec<String>>;
```

## Implementation Changes

### Modified Files
- `cli/src/session/mod.rs` - Remove per-repo session support, add global-only paths
- `cli/src/session/turn_state.rs` - Add modified_files parsing for extended format
- `cli/src/history/storage.rs` - Write to global repo
- `cli/src/editors/codex/mod.rs` - Use standard sessions
- `cli/src/editors/codex/otel.rs` - Update event handlers
- `cli/src/editors/claude_code/session.rs` - Remove repo_path parameter
- Event handlers that create sessions

### New Files
- None (use existing session module)

### Deleted Files
- `cli/src/editors/codex/state.rs` - Replaced by standard session handling

## Scope

**In scope for this change:**
- Global session storage
- Global conversation history
- Unified session format

**Out of scope (stays per-repo):**
- `aiki/tasks` branch - Tasks are project-specific and belong in the repo's JJ history
- Working copy tracking - Per-repo `.aiki/jj/` still tracks changes

## Open Questions - RESOLVED

### #1: Should we track all cwds a session has touched?
**Resolution:** Yes, via `repos` field. Append-only, deduplicated list of all repositories touched by the session.

### #2: What about repo-specific session queries?
**Resolution:** Not needed. Sessions are global. Use `repos` field to filter if needed.

### #3: Lock contention
**Status:** Keep as-is. Global location means all agents compete for same directory, but current per-session locking should be fine since filenames are unique UUIDs.

### #4: Global repo disk usage
**Status:** Defer. Conversation history accumulates forever. Need TTL/pruning strategy eventually, but not blocking for v1.

### #5: Per-repo queries after migration
**Resolution:** Filter global conversations by `repos` field instead of substring matching on cwd. Each event will include session_id, which links to session file with `repos` list.

### #6: What about aiki/tasks?
**Resolution:** Tasks stay per-repo (in `{repo}/.aiki/jj/`). Out of scope for this document.

## Migration

**No gradual migration.** Clean break:

1. Implement global session support
2. Update all agents to use global sessions
3. Delete per-repo session code
4. Existing per-repo sessions will TTL out naturally (or provide one-time migration script)

**No backwards compatibility:** Remove `#[deprecated]` annotations and old code paths. Keep it simple.

## Future Work

### User-Controlled Trust Boundaries

**Use case:** Consultants, contractors, or users with mixed personal/work repositories may want to isolate aiki state by project or client.

**Example scenarios:**
- Contractor working on ClientA and ClientB projects - don't want conversation history mixed
- Enterprise requirement to keep work sessions separate from personal projects
- Developer with multiple identities (personal GitHub, work GitLab, client Bitbucket)

**Sketch of solution:**
- Allow users to place `.aiki-boundary` marker file in a directory
- Walk up from cwd looking for marker (like `.git`)
- Use `{boundary_dir}/.aiki/` instead of `~/.aiki/`
- Provide UX indicator showing which boundary is active (like `direnv` or Python venv)

**Prior art:**
- `direnv` - per-directory environment variables
- Git's `includeIf` - conditional config based on path
- Firefox containers - isolate browsing context by domain/project

**Requirements:**
- Visible indicator of active boundary (status line, shell prompt integration)
- Clear documentation on security model (what's isolated, what's not)
- Tool to list/switch boundaries
- Default to `~/.aiki/` when no boundary marker found

**Status:** Out of scope for v1. Document for future reference.
