# Global Aiki State

## Summary

Consolidate aiki state into a global location (`~/.aiki/`):
- **Sessions** - Store in `~/.aiki/sessions/` instead of per-repo
- **Conversations** - Store in global JJ repo at `~/.aiki/.jj/` on `aiki/conversations` branch
- **Codex unification** - Replace Codex-specific storage with standard `AikiSession` pattern

## Motivation

1. **Sessions can span repos** - An agent conversation might work across multiple repositories. Per-repo storage doesn't capture this.

2. **Conversations should follow sessions** - If a session spans repo A and repo B, conversation events (prompts, responses) shouldn't be fragmented across per-repo `aiki/conversations` branches. Need unified history.

3. **Codex uses separate storage** - Currently stores in `~/.aiki/codex-sessions/{external_id}.json` with a different format than standard session files. This is confusing and duplicates logic.

4. **Simpler cleanup** - Single location to scan for TTL/PID cleanup instead of per-repo.

5. **Session is about the conversation, not the repo** - Events capture both `repo` and `cwd` per-event, so we know where work happened.

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
│   └── {uuid}          # Session metadata [aiki]...[/aiki]
└── .jj/                # Global JJ repository for conversation history
    └── (aiki/conversations branch lives here)
```

**Per-repo:**
```
{repo}/.aiki/
├── .jj/                # Per-repo JJ repository
│   └── (aiki/tasks branch and working copy tracking)
└── repo-id             # Root commit hash, computed by aiki init
```

**Note:** Per-repo `.aiki/.jj/` holds only JJ-tracked data (tasks, working copy changes). Sessions live globally.

### 1. Global JJ Repository

`~/.aiki/.jj/` is a dedicated JJ repository for global aiki state.

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
- `jj init` in `~/.aiki/.jj/` (non-colocated, no git)
- No working copy needed (all operations via `--no-edit`)
- Verified by `aiki doctor` (checks existence and health)

**Relation to session files:**
- Session files (`~/.aiki/sessions/{uuid}`) track active state (PID, repos)
- Conversation events (`~/.aiki/.jj/` on `aiki/conversations`) track history (including turn state)
- Linked by `session`

**Concurrency handling:**
Multiple agents may write to the global JJ repo concurrently. Strategy:
- Wrap JJ write operations in retry-with-backoff (3 attempts, exponential backoff)
- On persistent failure, log a warning and continue (conversation history is valuable but not critical-path)
- No daemon or write queue needed - optimistic concurrency with graceful degradation

### 2. Global Base Directory

The global aiki directory defaults to `~/.aiki/` but can be overridden:

```
AIKI_HOME=/custom/path aiki ...
```

**Resolution order:**
1. `AIKI_HOME` environment variable (if set)
2. `~/.aiki/` (default)

**Use cases for override:**
- Tests (isolated directories)
- Containers with read-only home
- CI environments

**Note:** XDG Base Directory support is deferred to future work. See `ops/future/xdg-support.md`.

### 3. Global Session Location

All session files live in:
```
$AIKI_HOME/sessions/{session_uuid}  # typically ~/.aiki/sessions/{session_uuid}
```

The session UUID is deterministic: `UUIDv5("{agent_type}", external_session_id)`, using the agent type as the namespace. This is stable across processes and avoids separator collision issues.

### 4. Unified Session File Format

Keep the `[aiki]...[/aiki]` format but add fields needed for all agents:

```
[aiki]
agent=codex
external_session_id=019bf548-9109-7f52-bce2-b66bb20c68dd
session=a1b2c3d4-...
started_at=2026-01-25T12:00:00Z
agent_version=0.89.0
parent_pid=12345
repo=abc123def456
repo=789xyz012345
[/aiki]
```

**New field:**
- `repo` - Stable repository identifiers for all repos touched by this session (append-only, deduplicated). Repeated keys are parsed as an array. Values are read from `{repo}/.aiki/repo-id`.

**Why track all repos?**
- Enables "what sessions affected this repo" queries
- Provides complete picture of session scope
- Useful for cleanup and debugging

**Turn state:** See `ops/now/remove-turn-files.md` - turn state is stored in JJ history, not sidecar files.

### 5. Event Metadata

Each conversation event (prompt, response, etc.) includes per-event location metadata:

```
[aiki]
event_type=prompt
session=a1b2c3d4-...
repo=abc123def456
cwd=/Users/user/project-a/src/lib
timestamp=2026-01-25T12:00:00Z
[/aiki]
```

**Fields:**
- `repo` - Stable repository identifier (read from `{repo}/.aiki/repo-id`)
- `cwd` - Current working directory (for context/debugging - "agent was in src/lib subdirectory")

**Why both?**
- `cwd` is the absolute path where the event occurred - useful for display and debugging
- `repo` is the stable identifier - reliable for per-repo queries across machines, survives moves/renames
- Both together give full context

### Repository Identification

**Problem:** Absolute paths like `/Users/user/project-a` don't work when:
- Same repo cloned at different paths on different machines
- Sessions span laptop and desktop
- Containers with different mount points

**Solution:** Use the git root commit hash as a stable repository identifier.

The root commit (first commit in git history) has a hash that:
- Never changes for a given repository
- Is the same across all clones
- Survives moves, renames, and re-clones
- Is human-verifiable (`git rev-list --max-parents=0 HEAD`)

**Storage:** Computed once during `aiki init`, stored in `{repo}/.aiki/repo-id`.

**Fallback for local-only repos:** Repositories without commits or without Git remotes use:
- Format: `local-{hash(canonical_repo_path)}`
- Example: `local-a4f3b8e2c1d9`
- Purpose: Distinguishable from repos with remotes, stable for a given path
- Upgrade path: When user adds a remote or makes first commit, `aiki init` or `aiki doctor` can update the repo-id to use the root commit hash

**Benefits:**
- Every repo gets a stable identifier immediately
- Local-only repos are clearly marked with `local-` prefix
- Can be upgraded to root commit hash when repo matures

**Format in session files:**
```
[aiki]
repo=abc123def456
repo=789xyz012345
[/aiki]
```

**Format in event metadata:**
```
[aiki]
event_type=prompt
session=a1b2c3d4-...
repo=abc123def456
cwd=/Users/user/project-a/src/lib
timestamp=2026-01-25T12:00:00Z
[/aiki]
```

Events include both:
- `cwd` — absolute path where event occurred (for display/debugging)
- `repo` — stable identifier read from `{repo}/.aiki/repo-id` (cached in memory during session)

### 6. `aiki init` Behavior

When running `aiki init` in a repository:

1. Creates `~/.aiki/sessions/` (if not exists)
2. Creates `~/.aiki/.jj/` with `jj init` (if not exists)
3. Creates `{repo}/.aiki/.jj/` with `jj init` (non-colocated, if not exists)
4. Computes root commit hash (`git rev-list --max-parents=0 HEAD`) and writes to `{repo}/.aiki/repo-id`
   - If repo has no commits yet, writes empty file
   - If file already exists with content, skips (idempotent)
   - If file exists but empty and repo now has commits, updates it
5. Installs git hooks
6. Prints summary

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
/// Find active session by PID ancestry, filtered by repo
pub fn find_active_session(cwd: &Path) -> Option<SessionMatch>;
```

**Changes:**
- `cwd` parameter is used to derive the repo ID for fallback filtering
- Always searches `$AIKI_HOME/sessions/`
- Process:
  1. Apply PID ancestry matching across all sessions
  2. If PID match found, return it immediately (we're in that session's process tree)
  3. If no PID match, fall back to most-recent session that includes current repo ID (read `{repo}/.aiki/repo-id`, filter sessions by `repo` list)

**Why PID first:**
- PID ancestry is definitive - if we're in a parent process, that's our session
- No need to verify repo on PID match - the process tree is authoritative
- Repo filtering only used for fallback when PID lookup fails

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
- `cli/src/session/turn_state.rs` - Already updated to query JJ (per `remove-turn-files.md`)
- `cli/src/history/storage.rs` - Write to global repo (already uses `session=` field per `remove-turn-files.md`)
- `cli/src/editors/codex/mod.rs` - Use standard sessions
- `cli/src/editors/codex/otel.rs` - Update event handlers
- `cli/src/editors/claude_code/session.rs` - Remove repo_path parameter
- Event handlers that create sessions

**Note:** Session activity tracking already queries JJ for latest event timestamp (implemented in `remove-turn-files.md`), which aligns with the global state design.

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
**Resolution:** Yes, via `repo` field (repeated keys parsed as array). Append-only, deduplicated list of repository IDs (root commit hashes) touched by the session.

### #2: What about repo-specific session queries?
**Resolution:** Not needed. Sessions are global. Use `repos` field to filter if needed.

### #3: Lock contention
**Status:** Keep as-is. Global location means all agents compete for same directory, but current per-session locking should be fine since filenames are unique UUIDs.

### #4: Global repo disk usage
**Status:** Defer. Conversation history accumulates forever. Need TTL/pruning strategy eventually, but not blocking for v1.

### #5: Per-repo queries after migration
**Resolution:** Filter global conversations by event-level `repo` field (not session-level). Each event includes its own `repo` (stable identifier from `{repo}/.aiki/repo-id`), enabling accurate filtering even when sessions span multiple repos. To query events for a specific repo, read its `{repo}/.aiki/repo-id` and filter by that value.

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

See `ops/future/state-trust-boundaries.md` for plans around user-controlled workspace isolation (isolating sessions/conversations by client/project).


## Implementation order suggestions

1. ✅ Add `AIKI_HOME` env var support and helper to get global aiki path
2. ✅ Add `repo-id` file generation to `aiki init` (non-breaking)
3. ✅ Add `repo` field parsing to session files (non-breaking)
4. ✅ Add `repo` field to event metadata (non-breaking)
5. ✅ Create global directory initialization in `aiki init`
6. ✅ Migrate session file operations to global
7. ✅ Migrate conversation storage to global JJ (with retry/backoff)
8. ✅ Update TTL cleanup to query global JJ
9. ✅ Update `find_active_session` to filter by repo then PID ancestry
10. ✅ Delete per-repo session code (Codex state.rs migrated to AikiSessionFile)
