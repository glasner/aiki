# Dynamic Agent/Runtime Support

**Date**: 2026-01-30
**Status**: Future Work
**Purpose**: Make reviewer/assignee selection dynamic based on available runtimes

---

## Current State

The `determine_reviewer` function in `agents/mod.rs` is hardcoded to the claude-code/codex pairing:

```rust
pub fn determine_reviewer(worker: Option<&str>) -> String {
    match worker {
        Some("claude-code") => "codex".to_string(),
        Some("codex") => "claude-code".to_string(),
        _ => "codex".to_string(),
    }
}
```

This works because only claude-code and codex have execution runtimes today (see `agents/runtime/mod.rs::get_runtime()`).

---

## Problem

When additional agents get runtimes (Cursor, Gemini, custom agents), the hardcoded reviewer logic won't work correctly:

1. **Unknown agents fallback to codex** - may not be installed
2. **No way to specify reviewer preferences** - users may want specific agents for reviews
3. **No validation of agent availability** - could assign to non-existent runtime

---

## Proposed Changes

### 1. Agent Registry

Add a way to query available agents:

```rust
// agents/mod.rs
pub fn get_available_agents() -> Vec<AgentType> {
    AgentType::iter()
        .filter(|a| get_runtime(*a).is_some())
        .collect()
}

pub fn is_agent_available(agent: &str) -> bool {
    AgentType::from_str(agent)
        .map(|a| get_runtime(a).is_some())
        .unwrap_or(false)
}
```

### 2. Dynamic Reviewer Selection

Update `determine_reviewer` to use the registry:

```rust
pub fn determine_reviewer(worker: Option<&str>) -> String {
    let available = get_available_agents();

    // Try to find an agent different from the worker
    if let Some(worker_str) = worker {
        if let Ok(worker_type) = AgentType::from_str(worker_str) {
            // Find first available agent that isn't the worker
            if let Some(reviewer) = available.iter()
                .find(|a| **a != worker_type)
            {
                return reviewer.as_str().to_string();
            }
        }
    }

    // Fallback to first available, or codex if none
    available.first()
        .map(|a| a.as_str().to_string())
        .unwrap_or_else(|| "codex".to_string())
}
```

### 3. Single Agent Case

When only one agent is available, we have options:

- **Self-review**: Allow the same agent to review its own work
- **Error**: Require at least 2 agents for reviews
- **Skip**: Don't create review tasks

Recommendation: Self-review with a warning in the output.

### 4. Reviewer Preferences (Optional)

Allow users to configure reviewer preferences:

```yaml
# .aiki/config.yaml
review:
  preferred_reviewer: codex
  allow_self_review: false
```

---

## Affected Code

| File | Change |
|------|--------|
| `agents/mod.rs` | Add `get_available_agents()`, update `determine_reviewer()` |
| `commands/review.rs` | Validate agent availability before assigning |
| `commands/fix.rs` | Validate agent availability before assigning |
| `flows/engine.rs` | Validate agent availability in review action |

---

## Testing

1. Mock `get_runtime()` to test various agent configurations
2. Test single-agent scenario
3. Test unknown agent fallback
4. Test preference configuration (if implemented)
