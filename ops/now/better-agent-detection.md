# Better Agent Detection

**Date**: 2026-03-17
**Status**: Draft
**Purpose**: Make agent selection smart — check what's actually installed instead of hardcoding pairings
**Supersedes**: [Dynamic Agent/Runtime Support](../future/support-all-runtimes.md)

**Related Documents**:
- [User Settings](../future/user-settings.md) - Future `~/.aiki/config.yaml` for agent preferences
- [Prerequisites](../../cli/src/prerequisites.rs) - Existing tool availability checking

---

## Executive Summary

When a user runs `aiki review` (or any command that delegates to another agent), aiki currently hardcodes the reviewer pairing (claude-code↔codex) without checking whether the target CLI is actually installed. This means users who only have one agent CLI installed get a confusing error. We need to detect which agent CLIs are available on the system and select accordingly.

User-configurable agent preferences (preferred reviewer, self-review policy) are tracked separately in [user-settings](../future/user-settings.md).

---

## The Problem

The `determine_reviewer` function in `agents/mod.rs` is hardcoded:

```rust
pub fn determine_reviewer(worker: Option<&str>) -> String {
    match worker {
        Some("claude-code") => "codex".to_string(),
        Some("codex") => "claude-code".to_string(),
        _ => "codex".to_string(),
    }
}
```

This breaks in two ways:

1. **`get_runtime()` doesn't check installation** — It returns `Some(CodexRuntime)` even if `codex` isn't on PATH. The runtime struct exists; the binary doesn't.

2. **`determine_reviewer()` is hardcoded** — Always pairs claude-code↔codex. Unknown agents fallback to codex. No availability check. Won't scale when additional agents get runtimes (Cursor, Gemini, custom agents).

**User impact:** Lance can't use `aiki review` because he doesn't have codex installed. The error message doesn't explain what's wrong or how to fix it.

---

## How It Works

### Agent Availability Detection

Each agent type gets a `cli_binary()` method returning the binary name to look for on PATH:

| Agent | Binary | Check |
|-------|--------|-------|
| ClaudeCode | `claude` | `which claude` |
| Codex | `codex` | `which codex` |
| Cursor | — | Not spawnable (no runtime) |
| Gemini | — | Not spawnable (no runtime) |

### Agent Registry

New functions to query what's actually usable on this machine:

```rust
// agents/mod.rs

/// Returns agents that have a runtime AND whose CLI binary is installed.
pub fn get_available_agents() -> Vec<AgentType> {
    AgentType::iter()
        .filter(|a| a.cli_binary().is_some())
        .filter(|a| a.is_installed())
        .collect()
}

/// Check if a named agent is available (has runtime + installed).
pub fn is_agent_available(agent: &str) -> bool {
    AgentType::from_str(agent)
        .map(|a| a.cli_binary().is_some() && a.is_installed())
        .unwrap_or(false)
}
```

### Smart Reviewer Selection

`determine_reviewer()` uses the registry instead of hardcoded pairings:

```rust
pub fn determine_reviewer(worker: Option<&str>) -> Result<String> {
    let available = get_available_agents();

    // Try to find an agent different from the worker
    if let Some(worker_str) = worker {
        if let Ok(worker_type) = AgentType::from_str(worker_str) {
            if let Some(reviewer) = available.iter()
                .find(|a| **a != worker_type)
            {
                return Ok(reviewer.as_str().to_string());
            }

            // Only the worker is installed → self-review with warning
            if available.iter().any(|a| *a == worker_type) {
                eprintln!("⚠ Only {} is available. Assigning self-review.", worker_str);
                return Ok(worker_str.to_string());
            }
        }
    }

    // No worker specified → use first available
    if let Some(agent) = available.first() {
        return Ok(agent.as_str().to_string());
    }

    // Nothing installed
    Err(anyhow!("No agent CLIs found. Install claude or codex to use task delegation."))
}
```

### Single Agent Case

When only one agent is installed: **self-review with a warning**. This is better than erroring — the user still gets a review, just from the same agent. The [user-settings](../future/user-settings.md) plan will eventually add `agents.allow_self_review` to let users control this.

---

## Implementation Plan

**Goal:** Fix Lance's immediate problem. `aiki review` works with whatever agents are installed.

1. **Add `cli_binary()` to `AgentType`** (`agents/types.rs`)
   - Returns `Option<&'static str>` — the binary name for agents that can be spawned
   - ClaudeCode → `Some("claude")`, Codex → `Some("codex")`, others → `None`

2. **Add `is_installed()` to `AgentType`** (`agents/types.rs` or `agents/mod.rs`)
   - Uses `which`-style check (reuse `check_command_version` from `prerequisites.rs`)
   - Returns `bool`

3. **Add `get_available_agents()` and `is_agent_available()`** (`agents/mod.rs`)
   - Returns `Vec<AgentType>` — agents that have a runtime AND are installed
   - This is the dynamic agent registry, replacing the hardcoded pairing

4. **Update `determine_reviewer()`** (`agents/mod.rs`)
   - Use `get_available_agents()` to find an installed reviewer
   - Self-review fallback when only one agent is installed
   - Return `Result` instead of `String` — clear error when nothing is installed

5. **Update `get_runtime()` or callers** (`agents/runtime/mod.rs` or `tasks/runner.rs`)
   - Callers check `is_installed()` before calling `get_runtime()` and give a good error
   - Keep `get_runtime()` pure (struct construction) — lets callers give context-specific error messages

6. **Improve error messages** (`error.rs`, `tasks/runner.rs`)
   - `AgentNotSupported` → include platform-specific install instructions (see table below)
   - New variant or updated message: "codex is not installed. Install it with: ..."

7. **Update all callers of `determine_reviewer()`**
   - Validate agent availability before assigning reviewers
   - Handle the new `Result` return type

---

## Affected Code

| File | Change |
|------|--------|
| `agents/types.rs` | Add `cli_binary()`, `is_installed()` to `AgentType` |
| `agents/mod.rs` | Add `get_available_agents()`, rewrite `determine_reviewer()` |
| `agents/runtime/mod.rs` | No change (keep pure), callers do availability check |
| `commands/review.rs` | Validate agent availability before assigning |
| `commands/fix.rs` | Validate agent availability before assigning |
| `flows/engine.rs` | Validate agent availability in review action |
| `tasks/runner.rs` | Check `is_installed()` before spawning, improve errors |
| `error.rs` | Update/add error variants with install instructions |

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Review requested, reviewer not installed | Error: "{agent} is not installed. Install: {instructions}. Or use --agent to specify a different reviewer." |
| Only one agent installed | Self-review with warning: "Only {agent} is available. Assigning self-review." |
| No agents installed | Error: "No agent CLIs found. Install claude or codex to use task delegation." |
| Explicit --agent override, not installed | Error: "{agent} is not installed." with install link |
| Task assigned to unavailable agent | Error at `aiki task run` time with helpful message |

---

## Install Instructions

Error messages should include platform-specific install instructions. Detect the platform at runtime via `std::env::consts::OS` and show the appropriate command.

### Claude Code

| Platform | Command |
|----------|---------|
| macOS | `brew install claude-code` or `npm install -g @anthropic-ai/claude-code` |
| Linux | `npm install -g @anthropic-ai/claude-code` |
| Windows | `npm install -g @anthropic-ai/claude-code` |

### Codex

| Platform | Command |
|----------|---------|
| macOS | `brew install codex` or `npm install -g @openai/codex` |
| Linux | `npm install -g @openai/codex` |
| Windows | `npm install -g @openai/codex` |

### Implementation

Add an `install_hint()` method to `AgentType` that returns the platform-appropriate string:

```rust
impl AgentType {
    pub fn install_hint(&self) -> &'static str {
        match (self, std::env::consts::OS) {
            (AgentType::ClaudeCode, "macos") =>
                "Install with: brew install claude-code\n  Or: npm install -g @anthropic-ai/claude-code",
            (AgentType::ClaudeCode, _) =>
                "Install with: npm install -g @anthropic-ai/claude-code",
            (AgentType::Codex, "macos") =>
                "Install with: brew install codex\n  Or: npm install -g @openai/codex",
            (AgentType::Codex, _) =>
                "Install with: npm install -g @openai/codex",
            _ => "No install instructions available for this agent.",
        }
    }
}
```

Error messages then become:
```
codex is not installed.
  Install with: brew install codex
  Or: npm install -g @openai/codex
  Or use --agent to specify a different reviewer.
```

---

## Testing

1. **Mock `is_installed()`** to test various agent configurations (both installed, one installed, none installed)
2. **Single-agent scenario** — verify self-review warning and correct assignment
3. **Unknown agent fallback** — verify error instead of silent codex default
4. **`--agent` override** — verify validation against available agents
5. **`aiki doctor` integration** — verify agent availability shows up in diagnostics

---

## Open Questions

1. **Should `aiki doctor` show agent availability?** — Probably yes, as a separate section from prerequisites.
2. **Cache `which` results?** — Probably not needed yet, the check is fast. But if we add more agents, consider caching per-session.
3. ~~**Should install instructions be per-platform?**~~ — Yes, detect via `std::env::consts::OS`. See Install Instructions section above.

---
