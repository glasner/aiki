# Zed Duplicate Session Bug

## Problem

When running agents through Zed + ACP proxy, **TWO workspace isolation messages appear** with different session IDs:

```
WORKSPACE ISOLATION: ... /tmp/aiki/7f50e063/3305719c
WORKSPACE ISOLATION: ... /tmp/aiki/7f50e063/41db0730
```

This indicates that either:
1. Zed is creating duplicate sessions for the same conversation
2. Hook context is being merged across multiple sessions
3. Hooks are being invoked for multiple sessions simultaneously

## Evidence

### Session List Output

```
$ aiki session list
3305719c — Zed Editor — Last activity: 12 seconds ago (14 turns)
41db0730 — Zed Editor — Last activity: 12 seconds ago (14 turns)
```

**Key observations:**
- Both sessions have exactly 14 turns
- Both show identical "last activity" times
- Both sessions started within 1 second of each other (based on file timestamps)
- Both workspace directories exist and contain working files

### Process Topology

```
$ ps aux | grep aiki | grep -v grep
glasner  21234  aiki hooks --server
  └─ 21235  aiki acp
       └─ 21236  claude-code-ext (agent chain)
            └─ 21237  aiki hooks --server
```

**Key observations:**
- Only ONE `aiki hooks` process per agent chain
- No duplicate ACP processes
- Process tree looks clean and normal

### Workspace Directories

Both workspace directories exist and contain files:
- `/tmp/aiki/7f50e063/3305719c/` — Contains updated loop-flags.md and other working files
- `/tmp/aiki/7f50e063/41db0730/` — Also exists

### Hook Context Behavior

Every turn receives **BOTH** workspace isolation messages:
```
WORKSPACE ISOLATION: ... /tmp/aiki/7f50e063/3305719c
WORKSPACE ISOLATION: ... /tmp/aiki/7f50e063/41db0730
```

This happens consistently across all turns in the session.

## Root Cause Analysis

### Theory 1: Zed Creates Duplicate Sessions (Most Likely)

**Hypothesis:** Zed intentionally or unintentionally creates two sessions for redundancy, failover, or parallel processing.

**Evidence:**
- Both sessions start at nearly the same time
- Both have exactly the same number of turns
- Both show identical last activity times
- Process tree shows only one agent chain (not two)

**Questions:**
- Does Zed create backup sessions?
- Is there retry/failover logic that creates a second session?
- Could this be a race condition in session creation?

### Theory 2: Hook Context Merged Across Sessions

**Hypothesis:** The hooks framework is somehow invoking `workspace_create_if_concurrent` for multiple sessions and merging their context into a single agent prompt.

**Evidence:**
- Hook YAML calls `workspace_create_if_concurrent` once per turn
- Function is session-specific (takes `&AikiSession` parameter)
- Agent sees both workspace paths in every turn

**Questions:**
- Could hooks be invoked multiple times with different sessions?
- Is there context merging happening at the ACP proxy level?
- Could compaction be merging context from multiple sessions?

### Theory 3: Session ID Not Maintained Between Turns

**Hypothesis:** Session UUID is regenerated or changes between turns, causing multiple workspaces to be created.

**Evidence:**
- Session list shows two distinct sessions with stable UUIDs
- Both sessions persist across turns (not transient)

**Counterevidence:**
- Session UUIDs are stable (can list same sessions repeatedly)
- Workspace paths are deterministic based on session UUID

**Verdict:** Less likely — session IDs appear stable.

## Next Steps

### 1. Add Debug Logging to ACP Handlers

File: `cli/src/editors/acp/handlers.rs`

Add logging at key session lifecycle points:
- Line 790-791: Log session UUID when firing `TurnStarted`
- Line 634: Log when firing `SessionStarted`
- Line 212-239: Log in `create_session_with_pid`

```rust
eprintln!("[DEBUG] Session created: uuid={}, editor={}", session.uuid(), editor);
eprintln!("[DEBUG] TurnStarted fired: session_uuid={}", session.uuid());
```

### 2. Add Debug Logging to workspace_create_if_concurrent

File: `cli/src/flows/core/functions.rs:1093`

```rust
pub fn workspace_create_if_concurrent(
    session: &crate::session::AikiSession,
    cwd: &Path,
) -> Result<ActionResult> {
    let session_uuid = session.uuid();
    eprintln!("[DEBUG] workspace_create_if_concurrent: session_uuid={}", session_uuid);
    // ... rest of function
}
```

### 3. Check ACP Protocol Message Handling

File: `cli/src/commands/acp.rs`

- Review session detection logic in ACP proxy
- Check if multiple sessions are created intentionally
- Look for race conditions in session creation

### 4. Test with Minimal Reproduction

Create a minimal test case:
1. Start Zed
2. Open a new chat
3. Send one message
4. Check how many sessions are created
5. Check if both sessions receive turn events

### 5. Review Zed Extension Code

Check if Zed's Claude Code extension creates multiple sessions:
- Look for session management code
- Check for retry/failover logic
- Look for parallel request handling

## Files to Review

- `cli/src/editors/acp/handlers.rs` — Session event handling
- `cli/src/commands/acp.rs` — ACP proxy implementation
- `cli/src/session/mod.rs` — Session creation and management
- `cli/src/flows/core/functions.rs` — workspace_create_if_concurrent
- `cli/src/flows/core/hooks.yaml` — Hook definitions

## Test Cases

1. **Single message test:**
   - Start fresh Zed chat
   - Send one message
   - Check `aiki session list` — how many sessions?

2. **Sequential messages test:**
   - Send 3 messages in a row
   - Check if both sessions advance to 3 turns

3. **Concurrent isolation test:**
   - Open two Zed chats simultaneously
   - Each should get ONE isolated workspace
   - Check that we don't get 4 total sessions (2x2)

## Impact

**Critical:** This bug causes:
- Confusion about which workspace to work in
- Potential for changes to go to wrong workspace
- Workspace absorption conflicts
- Wasted resources (duplicate workspaces created)
- Difficulty debugging workspace issues

**Workaround:** Agent can safely use the first workspace path mentioned, since both appear to be functional. However, this doesn't address the root cause.
