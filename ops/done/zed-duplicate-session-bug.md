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

## Root Cause

**Claude Code now fires editor hooks even when running behind the ACP proxy.** This wasn't happening before — it's a behavior change in Claude Code.

The process topology shows the problem:

```
aiki hooks --server          ← User's hooks server (fires events via ACP)
  └─ aiki acp                ← ACP proxy (dispatches events to event_bus)
       └─ claude-code-ext    ← Agent
            └─ aiki hooks --server  ← Agent's hooks server (DUPLICATE!)
```

The ACP proxy already dispatches all lifecycle events (session.started, turn.started, etc.) via `event_bus::dispatch()` in `handlers.rs`. When Claude Code also fires its own `aiki hooks stdin claude-code <event>` calls, those hooks create a **second session** and produce duplicate workspace isolation messages.

## Fix

Set `AIKI_ACP_PROXY=1` env var when the ACP proxy spawns the agent. Editor hooks (`aiki hooks stdin`) check for this env var and exit as a no-op when set.

### Changes

1. **`cli/src/commands/acp.rs`** — Add `.env("AIKI_ACP_PROXY", "1")` to the agent spawn call
2. **`cli/src/commands/hooks.rs`** — Check `AIKI_ACP_PROXY` in `run_stdin()` and return early

## Verification

After deploying, test in Zed:
1. Start fresh Zed chat, send one message
2. `aiki session list` should show exactly ONE session
3. Agent should see exactly ONE workspace isolation message per turn
