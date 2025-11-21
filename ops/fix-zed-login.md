# Fix: Zed Login Browser Not Opening

**Date:** 2025-11-20
**Status:** Fixed (Revised Solution)
**Related Files:** `cli/src/commands/acp.rs`

---

## Context

Aiki includes an ACP (Agent Client Protocol) proxy that sits between IDEs (like Zed) and AI agents (like Claude Code). This proxy:

1. Observes agent ↔ IDE communication
2. Records provenance metadata from tool calls
3. Acts as a transparent proxy (forwards messages bidirectionally)

When using Zed with Claude Code through our proxy:
- User clicks "Log in with Claude Code" button in Zed
- Zed sends `authenticate` JSON-RPC request
- Agent should handle OAuth (open browser, complete flow)
- Credentials saved to `~/.claude.json`

---

## The Problem

**Symptom:** After clicking the login button in Zed, the browser was not opening as expected.

**Root Cause:** The proxy was **intercepting** the `authenticate` request and trying to handle authentication itself, instead of forwarding it to the Claude Code agent.

### What the Broken Code Did

```rust
"authenticate" => {
    // ❌ WRONG: Proxy tried to handle auth itself

    // Check if ~/.claude.json exists
    if auth_file.exists() {
        // Return success
    } else {
        // Spawn a new terminal running `claude /login`
        osascript -e 'tell application "Terminal" to do script "claude /login"'
    }

    // Send response directly to IDE
    println!("{}", response);

    // ❌ CRITICAL: Don't forward to agent
    continue;  // <-- This prevented agent from ever seeing the request
}
```

### Why This Failed

1. **`claude /login` doesn't work as a CLI command** - `/login` is an interactive slash command that only works when typed inside a running Claude session, not as a CLI argument

2. **The proxy shouldn't handle auth** - The Claude Code agent already has full OAuth implementation:
   - Starts local OAuth server
   - Opens browser automatically
   - Handles OAuth callback
   - Saves credentials

3. **Breaking the proxy pattern** - A transparent proxy should observe and forward, not intercept and replace functionality

---

## The Fix (Final)

**Changed:** `cli/src/commands/acp.rs` lines 205-210

### The Actual Problem

The Aiki proxy was **intercepting** the `authenticate` request and trying to handle authentication itself, instead of forwarding it to the agent. This violated the transparent proxy pattern.

### The Solution

The proxy now **forwards** the `authenticate` request to the agent, letting the agent handle OAuth:

```rust
"authenticate" => {
    // Just observe and forward - let the agent handle authentication
    if std::env::var("AIKI_DEBUG").is_ok() {
        eprintln!("ACP Proxy: Forwarding authenticate request to agent");
    }
}
```

### How Authentication Works Now

1. **Zed sends** `authenticate` request to proxy
2. **Proxy forwards** request to agent (claude-code-acp)
3. **Agent handles OAuth:**
   - Starts local OAuth server
   - Opens browser automatically
   - Handles callback
   - Saves credentials to `~/.claude.json`
4. **Agent responds** to Zed via proxy
5. **Proxy forwards** response back to Zed

### Key Change

We removed ~100 lines of authentication interception code. The proxy now acts as a **transparent proxy**, observing and forwarding messages without trying to handle authentication itself.

---

## How It Works Now

```
┌──────┐                  ┌──────────────┐                  ┌──────────────────┐
│ Zed  │                  │ Aiki Proxy   │                  │ claude-code-acp  │
│ IDE  │                  │ (Observe)    │                  │ Agent            │
└──────┘                  └──────────────┘                  └──────────────────┘
    │                             │                                  │
    │  authenticate request       │                                  │
    ├────────────────────────────>│                                  │
    │                             │                                  │
    │                             │  Forward authenticate           │
    │                             ├─────────────────────────────────>│
    │                             │                                  │
    │                             │                                  │ Start OAuth server
    │                             │                                  │ Open browser
    │                             │                                  │ Complete OAuth
    │                             │                                  │ Save ~/.claude.json
    │                             │                                  │
    │                             │  authenticate response          │
    │  Forward response           │<─────────────────────────────────┤
    │<────────────────────────────┤                                  │
    │                             │                                  │
```

**Key principle:** The proxy is **transparent** - it observes traffic for provenance tracking but doesn't intercept or replace agent functionality.

---

## Testing

To verify the fix works:

```bash
# 1. Backup existing auth (if any)
mv ~/.claude.json ~/.claude.json.bak

# 2. Rebuild aiki with the fix
cd cli && cargo build --release

# 3. Start the proxy
cargo run --release -- acp claude-code

# 4. In Zed:
#    - Open a project
#    - Click "Log in with Claude Code" button
#    - Browser should open automatically
#    - Complete OAuth flow
#    - Verify ~/.claude.json is created

# 5. Restore backup if needed
mv ~/.claude.json.bak ~/.claude.json
```

**Expected behavior:** Browser opens automatically when clicking the login button, OAuth flow completes, credentials saved.

---

## Lessons Learned

1. **Trust the agent** - The agent already has complete OAuth implementation. Don't reimplement it in the proxy.

2. **Transparent proxy pattern** - A proxy should observe and forward messages, not intercept and replace functionality.

3. **Forward by default** - Only intercept messages when absolutely necessary for provenance tracking. Authentication is not one of those cases.

4. **Simplicity wins** - We removed ~100 lines of complex authentication code and replaced it with a simple comment. The simpler solution is often the correct one.

---

## Related Work

- **ACP Protocol Integration:** `cli/src/commands/acp.rs` - Full bidirectional proxy implementation
- **Zed Detection:** `cli/src/commands/zed_detection.rs` - Auto-detect Claude Code from Zed config
- **Provenance Recording:** Event bus captures tool calls for `session/update` notifications

---

## Status

✅ **Fixed** - The proxy now correctly forwards `authenticate` requests to the agent, allowing the browser to open for OAuth authentication.
