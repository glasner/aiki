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

### `--agent` Propagation Gaps

Beyond the core detection problem, an audit of `build`, `review`, and `fix` revealed that `--agent` is not consistently propagated through internal pipeline steps:

1. **`fix.rs`: `--agent` not propagated to re-reviews in quality loop** — All internal `create_review()` calls pass `agent_override: None` (lines 357, 388, 538, 571). When you run `aiki fix --agent codex`, the initial fix tasks get assigned to codex, but re-reviews within the quality loop fall back to `determine_reviewer()` hardcoded logic.

2. **`build.rs`: `--agent` not propagated to post-build review** — `run_build_review()` calls `create_review()` with `agent_override: None` (line 857) and `run_fix()` with `agent: None` (line 900). Even if you run `aiki build --agent codex --review`, the post-build review ignores the agent override.

3. **`fix.rs`: `determine_followup_assignee()` hardcodes fallback to claude-code** — When no agent override is provided and the reviewed task has no assignee, it falls back to `"claude-code"` (line 700) instead of using the agent registry.

4. **`review.rs`: `run_continue_async()` doesn't apply `--agent` to `TaskRunOptions`** — It creates `TaskRunOptions::new()` without the agent override (line 859). This works by accident because the review task already has the agent set as its assignee, but it's fragile.

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

Uses a const slice rather than `strum::EnumIter` to avoid adding a new dependency. Only spawnable agent types are listed, so `cli_binary()` filtering is redundant.

```rust
// agents/mod.rs

/// Agent types that have a runtime and can be spawned.
const SPAWNABLE_AGENTS: &[AgentType] = &[AgentType::ClaudeCode, AgentType::Codex];

/// Returns agents that have a runtime AND whose CLI binary is installed.
pub fn get_available_agents() -> Vec<AgentType> {
    SPAWNABLE_AGENTS.iter()
        .filter(|a| a.is_installed())
        .cloned()
        .collect()
}

/// Check if a named agent is available (has runtime + installed).
pub fn is_agent_available(agent: &str) -> bool {
    AgentType::from_str(agent)
        .map(|a| SPAWNABLE_AGENTS.contains(&a) && a.is_installed())
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
   - Pure PATH-existence check: `which::which(binary).is_ok()` (or fall back to `std::process::Command::new("which").arg(binary).status().map(|s| s.success()).unwrap_or(false)` if avoiding the `which` crate)
   - Do NOT use `check_command_version()` or run `<binary> --version` — a CLI may exist on PATH but not support `--version`, and we only need to know whether the binary is present
   - Returns `bool`
   - *Note:* `check_command_version()` from `prerequisites.rs` remains useful for deep health checks in `aiki doctor` (e.g. verifying minimum versions), but is not appropriate for `is_installed()` which only needs binary existence

3. **Add `get_available_agents()` and `is_agent_available()`** (`agents/mod.rs`)
   - Returns `Vec<AgentType>` — agents that have a runtime AND are installed
   - This is the dynamic agent registry, replacing the hardcoded pairing

4. **Update `determine_reviewer()`** (`agents/mod.rs`)
   - Use `get_available_agents()` to find an installed reviewer
   - Self-review fallback when only one agent is installed
   - Return `Result` instead of `String` — clear error when nothing is installed

5. **Centralize availability check in `create_review()`** (`commands/review.rs`, `agents/runtime/mod.rs`)
   - After `determine_reviewer()` returns a reviewer name, `create_review()` calls `is_agent_available()` on it before creating the task — this is the single validation point that all callers (CLI, `fix.rs` quality loop, `build.rs` post-build review) go through
   - If the reviewer is not available, return a clear error with install instructions (no task gets created)
   - Keep `get_runtime()` pure (struct construction, no side effects) — the availability gate lives in `create_review()`, not in the runtime layer

6. **Improve error messages** (`error.rs`, `tasks/runner.rs`)
   - `AgentNotSupported` → include platform-specific install instructions (see table below)
   - New variant or updated message: "codex is not installed. Install it with: ..."

7. **Update all callers of `determine_reviewer()`**
   - Handle the new `Result` return type from `determine_reviewer()`
   - Caller-side availability validation is **not needed** for review creation — `create_review()` (Step 5) handles it centrally
   - Callers only need to propagate the `Result` error (no redundant `is_agent_available()` checks at call sites)

8. **Thread `--agent` through `fix.rs` quality loop** (`commands/fix.rs`)
   - Pass `agent_override` to all internal `create_review()` calls (lines 357, 388, 538, 571)
   - The quality loop creates multiple review→fix cycles; the agent must propagate through every cycle
   - Store agent override in a variable accessible to the entire loop, not just the initial call

9. **Thread `--agent` through `run_build_review()`** (`commands/build.rs`)
   - Add `agent: Option<String>` parameter to `run_build_review()`
   - Pass it to `create_review()` as `agent_override`
   - Pass it to `run_fix()` as `agent`
   - Update all call sites: `run_build_plan()`, `run_build_epic()`, `run_continue_async()`

10. **Replace hardcoded claude-code fallback in `determine_followup_assignee()`** (`commands/fix.rs`)
    - Change return type from `Option<String>` to `Result<String>` to properly surface errors
    - Replace the current fallback logic with a 5-tier resolution:
      1. If `agent_override` is set, use it (unchanged)
      2. If the reviewed task has an assignee, use it (unchanged)
      3. If exactly one spawnable agent is installed (`get_available_agents()` returns 1 result), use it — this is unambiguous so it's safe to pick automatically
      4. If multiple agents are installed, return `Err`: "Cannot determine which agent to assign fix tasks to. Use `--agent` to specify."
      5. If no agents are installed, return `Err`: "No agent CLIs found. Install claude or codex to use task delegation."
    - The single-agent fallback (tier 3) is safe because there's no ambiguity — only one agent exists on the system. This covers non-session flows (plan reviews, code reviews run outside an active agent session) where `resolve_agent_type()` would also fail due to no session or process tree.

11. **Make `run_continue_async()` explicit about agent override** (`commands/review.rs`)
    - Apply `--agent` to `TaskRunOptions` when running the review task in the async path
    - Currently works by accident via task assignee; make it explicit for robustness

---

## Affected Code

| File | Change |
|------|--------|
| `agents/types.rs` | Add `cli_binary()`, `is_installed()`, `install_hint()` to `AgentType` |
| `agents/mod.rs` | Add `get_available_agents()`, rewrite `determine_reviewer()` |
| `agents/runtime/mod.rs` | No change (keep pure), callers do availability check |
| `commands/review.rs` | Validate agent availability before assigning; make `run_continue_async()` explicit about `--agent` in `TaskRunOptions` |
| `commands/fix.rs` | Thread `--agent` to all `create_review()` calls in quality loop; replace hardcoded claude-code fallback in `determine_followup_assignee()` with dynamic resolution |
| `commands/build.rs` | Thread `--agent` through `run_build_review()` to `create_review()` and `run_fix()` |
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
| `--agent` on `build --review`/`--fix` | Agent propagates to post-build review and fix pipeline |
| `--agent` on `fix` quality loop re-reviews | Agent propagates to all create_review calls within the loop |
| Followup assignee fallback (no `--agent`, no original assignee, single agent installed) | Uses the single installed agent (unambiguous) |
| Followup assignee fallback (no `--agent`, no original assignee, multiple agents installed) | Error: "Cannot determine which agent to assign fix tasks to. Use `--agent` to specify." |
| Followup assignee fallback (no `--agent`, no original assignee, no agents installed) | Error: "No agent CLIs found. Install claude or codex to use task delegation." |

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
5. **`--agent` propagation in fix quality loop** — verify `aiki fix --agent codex` assigns codex to all re-reviews within the loop, not just the initial fix tasks
6. **`--agent` propagation in build review** — verify `aiki build --agent codex --review` passes codex to post-build review and subsequent fix pipeline
7. **`determine_followup_assignee()` fallback** — verify 5-tier resolution: returns `Ok(agent)` when single agent installed, returns error when multiple agents installed (ambiguous), returns error when no agents installed

---

## Open Questions

1. ~~**Should `aiki doctor` show agent availability?**~~ — **Resolved**: Out of scope for this plan. `aiki doctor` already shows ACP Agent Binaries which partially covers this. A dedicated doctor enhancement can be tracked separately if needed.
2. **Cache `which` results?** — Probably not needed yet, the check is fast. But if we add more agents, consider caching per-session.
3. ~~**Should install instructions be per-platform?**~~ — Yes, detect via `std::env::consts::OS`. See Install Instructions section above.

---
