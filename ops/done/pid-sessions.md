# PID-Based Session Detection

**Goal**: Enable automatic session detection for `aiki task` commands without requiring explicit flags.

---

## Problem

When agents run `aiki task` as bash commands, they don't have access to their session ID:

1. **Hook events** receive session ID in JSON payload from Claude Code
2. **Bash commands** run as subprocesses with no session context
3. **No environment variable** is set by agents before running bash commands

---

## Solution: Store Parent PID in Session Files

Both hook handlers and bash commands share the same parent process (the agent). We store the parent PID in the session file:

```
Claude Code (PID 1234, session_id=abc123)
├── aiki-events claude-code       ← SessionStart hook
│   └── Writes parent_pid=1234 to .aiki/sessions/<uuid>
│
└── aiki task start "foo"         ← bash command
    └── Scans .aiki/sessions/* for parent_pid matching ancestor
```

---

## Design

### Updated Session File Format

Add `parent_pid` field to existing `.aiki/sessions/<uuid>` files:

```
[aiki]
agent=claude-code
external_session_id=claude-session-abc123
aiki_session_id=<uuid>
parent_pid=1234
started_at=2024-01-17T10:30:00Z
cwd=/path/to/repo
[/aiki]
```

**Canonical agent identifiers:** `claude-code`, `cursor`, `codex`, `gemini`, `unknown`

These match the `AgentType` enum in `cli/src/agents/types.rs`.

### Session Lookup Algorithm

```
ancestor_pids = get_ancestor_pids()
for each session file in .aiki/sessions/:
    pid = parse parent_pid from file
    if pid in ancestor_pids:
        agent_type = validate against process tree
        return (agent, external_session_id)
return None  // No session found (human terminal)
```

Performance: O(sessions) file reads, but sessions count is typically 1-3.

---

## Implementation Phases

### Phase 1: Add parent_pid to Session Files

Modify session file creation in `cli/src/session.rs`:
- Include `parent_pid` field when writing session metadata
- Hook mode: use `parent_id()`
- ACP mode: use `agent_pid` from session.start message

### Phase 2: Session Lookup Function

Add to `cli/src/session.rs`:
- `find_session_by_ancestor_pid(repo_path)` → `Option<(AgentType, String)>`
- Walk ancestor PIDs, scan session files for match

### Phase 3: Task Command Integration

Modify `cli/src/commands/task.rs`:
- Remove `--session` flag from List and Start commands
- Use `find_session_by_ancestor_pid()` as the primary session detection method

---

## Edge Cases

### Stale Sessions and PID Reuse

**Orphaned sessions:** If agent crashes without SessionEnd, session file with old PID remains.

**PID reuse:** OS may reassign PID to unrelated process.

**Validation strategy:**
1. Check that `parent_pid` matches an ancestor in current process tree
2. Verify `agent` field matches detected agent type from process tree
3. Optionally verify `started_at` is recent (within reasonable window)

**Cleanup strategy:**
- On SessionStart: scan for session files where `parent_pid` process no longer exists
- Lazy cleanup acceptable - stale files only cause issues if PID is reused AND ancestor chain matches

### Cross-Platform Process APIs

Use `sysinfo` crate for all platforms (already a dependency):

**Ancestor PID lookup:**
- `System::refresh_processes()` to populate process table
- `system.process(pid).parent()` to walk up the tree
- Collect all ancestor PIDs into a `HashSet<u32>` for O(1) lookup

**Process exists check (for cleanup):**
- `system.process(Pid::from_u32(pid)).is_some()`
- Or use `std::process::Command` with platform-specific tools as fallback

**Platform coverage:**
- Linux: `/proc` filesystem
- macOS: `sysctl` / libproc
- Windows: `CreateToolhelp32Snapshot` API

All handled transparently by `sysinfo` crate.

### Nested Process Calls

```
Claude Code (PID 1234)
└── bash script.sh (PID 2345)
    └── aiki task start "foo" (PID 3456)
```

**Handling:** Walk the entire ancestor chain until matching session found.

### Multiple Active Sessions

Each agent process has its own PID, stored in its session file:

```
.aiki/sessions/
├── <uuid-a>   (parent_pid=1234, agent=claude-code)  ← Claude Code session A
├── <uuid-b>   (parent_pid=5678, agent=claude-code)  ← Claude Code session B
└── <uuid-c>   (parent_pid=9012, agent=cursor)       ← Cursor session
```

**Why no `--session` override needed:**
- PID-based lookup is deterministic - a subprocess can only have one ancestor chain
- Ambiguity only arises if same PID appears in multiple files (cleaned up on SessionStart)

**If manual override ever needed:** Could add `AIKI_SESSION_ID` env var as escape hatch.

### ACP Mode

In ACP mode, aiki runs as a server process. The agent is NOT the parent.

```
Agent (PID 1234)                    Aiki Server (PID 5678)
    │                                      │
    ├── TCP/HTTP ──── session.start ──────►│
    │                                      │
    ├── TCP/HTTP ──── tool.call ──────────►│
```

**ACP Message Schema:**

```json
{
  "jsonrpc": "2.0",
  "method": "session/start",
  "params": {
    "session_id": "external-session-abc123",
    "agent_type": "claude-code",
    "agent_pid": 1234,              // OPTIONAL - agent's process ID
    "cwd": "/path/to/repo"
  }
}
```

**When `agent_pid` is provided:**
- Store in session file as `parent_pid`
- PID-based lookup works normally

**When `agent_pid` is absent:**
- Store `parent_pid=0` (or omit field) in session file
- Session will NOT match PID-based lookup
- Task commands fall back to human terminal mode for this session
- Agent must use `AIKI_SESSION_ID` env var if it needs session context

**Rationale:** Making `agent_pid` optional allows ACP agents that don't need task filtering to skip it, while agents that spawn subprocesses can provide it for full PID-based detection.

---

## Testing

1. **Unit tests**: Session file creation with parent_pid, ancestor PID walking
2. **Integration test**: Create session file, verify lookup from child process
3. **Validation test**: Stale session with mismatched agent type is ignored
4. **Cleanup test**: Orphaned session files cleaned on SessionStart
5. **ACP test**: Session created with provided `agent_pid`, not `parent_id()`
6. **Manual test**: Start Claude Code session, run `aiki task list`, verify auto-detection

---

## Files to Modify

| File | Changes |
|------|---------|
| `cli/src/session.rs` | Add `parent_pid` to session file creation |
| `cli/src/session.rs` | Add `find_session_by_ancestor_pid()` with validation |
| `cli/src/session.rs` | Add stale session cleanup on SessionStart |
| `cli/src/commands/task.rs` | Remove `--session` flag, use PID-based lookup |
| `cli/src/editors/acp/handlers.rs` | Pass `agent_pid` from ACP message to session creation |

---

## Benefits

1. **Cross-platform**: No symlinks, works on Windows without elevated privileges
2. **Single source of truth**: All session data in one file
3. **Zero friction**: Agents don't need to pass anything
4. **Multi-session safe**: Each session file stores its own parent PID
5. **Simpler API**: No `--session` flag needed
