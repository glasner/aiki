# Session System

## Session Identity

**`AikiSession`** is a data object with:
- `uuid` - deterministic UUIDv5 hash of `{agent_type}:{external_session_id}`
- `agent_type` - claude-code, cursor, etc.
- `external_id` - session ID from the agent
- `parent_pid` - agent's process ID (for PID-based detection)
- `agent_version`, `client_name`, `client_version`, `detection_method`

## Session Files

Stored in `.aiki/sessions/<uuid>`:
```
[aiki]
agent=claude-code
external_session_id=abc123
aiki_session_id=<uuid>
parent_pid=1234
started_at=2024-01-17T10:30:00Z
cwd=/path/to/repo
[/aiki]
```

## Lifecycle

### Hook Mode (Claude Code, Cursor)

**1. Agent starts session**
```
Claude Code starts
    |
Spawns: aiki-events claude-code (reads JSON from stdin)
    |
cli/src/main.rs -> commands/event.rs::run_event()
    |
editors/claude_code/events.rs::build_aiki_event_from_stdin()
    |
Parses ClaudeEvent::SessionStart { session_id, cwd }
    |
build_session_started_event()
    |
editors/claude_code/session.rs::create_session()
    |
AikiSession::for_hook(AgentType::ClaudeCode, session_id, agent_version)
    +-- captures parent_pid via sysinfo (parent = Claude Code process)
    |
Returns AikiEvent::SessionStarted(AikiSessionStartPayload {
    session: AikiSession,  // has parent_pid set
    cwd: PathBuf,
    timestamp: DateTime<Utc>,
})
```

**2. Event dispatch**
```
commands/event.rs::run_event()
    |
events/mod.rs::dispatch_event()
    |
match AikiEvent::SessionStarted(payload) =>
    events/session_started.rs::handle_session_started(payload)
```

**3. Session file creation**
```
handle_session_started(payload)
    |
cleanup_stale_sessions(&payload.cwd)
    +-- scans .aiki/sessions/*, removes files where parent_pid is dead
    |
AikiSessionFile::new(&payload.session, &payload.cwd)
    |
session_file.create(&payload.cwd)
    +-- writes to .aiki/sessions/<uuid> with O_EXCL (atomic)
    |
history::record_session_start()  // conversation history in aiki/conversations branch
    |
execute_hook()  // runs hooks.yaml actions (e.g., aiki init)
    |
returns HookResult { decision: Allow/Block, ... }
```

**4. Session end**
```
Claude Code exits or sends Stop event
    |
aiki-events claude-code (Stop payload)
    |
AikiEvent::SessionEnded
    |
events/session_ended.rs::handle_session_ended(payload)
    |
payload.session.end(&payload.cwd)
    +-- session.file(repo_path).delete()
    |
history::record_session_end()  // conversation history
```

### ACP Mode (Server-based)

**1. Agent connects**
```
Agent --TCP--> aiki hooks acp (server process)
    |
commands/acp.rs - bidirectional proxy
    |
Agent sends: { "method": "session/new", "params": { "agent_pid": 1234, ... } }
    |
IDE->Agent thread extracts agent_pid, sends StateMessage::TrackNewSession { request_id, agent_pid }
    |
Agent->IDE thread stores in session_new_requests HashMap
```

**2. Session response matched**
```
Agent responds to session/new with session_id
    |
Agent->IDE thread matches request_id -> retrieves agent_pid
    |
editors/acp/handlers.rs::fire_session_start_event()
    |
create_session_with_pid(agent_type, session_id, agent_version, agent_pid)
    |
AikiSession::new(...).with_parent_pid(agent_pid)
    |
Dispatches AikiEvent::SessionStarted
    |
handle_session_started() (same as hook mode from here)
```

**3. Session end**
```
Agent sends session/close or disconnects
    |
fire_session_end_event()
    |
handle_session_ended() (same as hook mode)
```

## PID-Based Detection

When `aiki task list` runs as a subprocess of the agent:
```
aiki task list
    |
find_session_by_ancestor_pid()
    |
get_ancestor_pids()  -> walks process tree up
    |
scan .aiki/sessions/* for matching parent_pid
    |
returns SessionMatch { agent_type, external_session_id, aiki_session_id }
```

## Cleanup

On each session start, `cleanup_stale_sessions()` removes session files where the `parent_pid` process no longer exists (crashed agents).

## Key Files

| File | Role |
|------|------|
| `session/mod.rs` | `AikiSession` struct, `for_hook()`, `find_session_by_ancestor_pid()`, `cleanup_stale_sessions()` |
| `events/session_started.rs` | `handle_session_started()` - cleanup, file creation, history, flow |
| `events/session_ended.rs` | `handle_session_ended()` - file deletion, history |
| `editors/claude_code/session.rs` | `create_session()` - hook mode session factory |
| `editors/acp/handlers.rs` | `create_session_with_pid()`, `fire_session_start_event()` |
| `commands/acp.rs` | ACP proxy, extracts `agent_pid` from messages |
