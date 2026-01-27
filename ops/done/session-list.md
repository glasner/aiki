# `aiki session list`

## Summary

Add an `aiki session list` command that shows active sessions from `~/.aiki/sessions/`. This is the first subcommand under `aiki session`, establishing the session management CLI surface.

## Motivation

1. **Visibility** - No way to see what sessions are active. Users and agents need to inspect session state for debugging (e.g., "why isn't my session detected?").
2. **Multi-agent awareness** - When multiple agents are running (Claude Code + Cursor), need to see all of them.
3. **Cleanup insight** - Before/after TTL cleanup, see what's alive vs stale.
4. **Foundation** - `aiki session list` is the read-only entry point. Future: `aiki session show <id>`, `aiki session end <id>`, `aiki session clean`.

## Current State

- Session files live in `~/.aiki/sessions/{uuid}` (global, not per-repo)
- Each file uses `[aiki]...[/aiki]` metadata format with fields: `agent`, `external_session_id`, `session_id`, `started_at`, `agent_version`, `parent_pid`, `repo` (repeated)
- Existing helpers: `count_sessions()`, `find_active_session()`, `cleanup_stale_sessions()`
- No CLI surface for session management exists yet

## Design

### Command Interface

```
aiki session list
```

No options for v1. Just list all sessions with human-readable output.

### Output Format

```
SESSION                               AGENT        PID    STARTED              REPOS
c7b18204-4028-543e-a81d-b02314eb66e4  claude-code  42310  2026-01-27T10:30:00  2
84019c64-8060-537b-9217-64aba69b7e92  cursor       41892  2026-01-27T09:15:00  1

2 sessions
```

When no sessions exist:
```
No active sessions
```

### Implementation

#### 1. Add `Session` subcommand to CLI (`cli/src/main.rs`)

New variant in `Commands` enum:

```rust
/// Manage sessions
Session {
    #[command(subcommand)]
    command: SessionCommands,
},
```

With a `SessionCommands` enum:

```rust
#[derive(Subcommand)]
enum SessionCommands {
    /// List active sessions
    List,
}
```

#### 2. New command module: `cli/src/commands/session.rs`

Single `run_list()` function:

1. Read `global_sessions_dir()` entries
2. Parse each session file (extract `[aiki]` block fields)
3. Print table to stdout

#### 3. Session file parsing

Reuse existing `AikiSessionFile` read logic from `cli/src/session/mod.rs`. Add a `list_all_sessions()` function that returns `Vec<SessionInfo>`:

```rust
pub struct SessionInfo {
    pub session_id: String,
    pub agent: String,
    pub started_at: String,
    pub parent_pid: Option<u32>,
    pub repos: Vec<String>,
}
```

This struct is the parsed representation of a session file, used by the list command.

### File Changes

| File | Change |
|------|--------|
| `cli/src/main.rs` | Add `Session` variant to `Commands` enum, dispatch to `commands::session` |
| `cli/src/commands/mod.rs` | Add `pub mod session;` |
| `cli/src/commands/session.rs` | New file: `run_list()` + `SessionCommands` enum |
| `cli/src/session/mod.rs` | Add `list_all_sessions() -> Result<Vec<SessionInfo>>` + `SessionInfo` struct |

### Not In Scope (Future)

- Filtering by agent type, repo, or staleness
- JSON output
- `aiki session show <id>` - Detailed single-session view
- `aiki session end <id>` - Manual session termination
- `aiki session clean` - Manual stale cleanup
- Turn count display
