# Future: Autoreply safety net for orchestrator tasks

**Date**: 2026-03-24
**Status**: Future improvement
**Prerequisite**: [close-loop-task.md](../now/close-loop-task.md) (template fix + autostart guard)

---

## Summary

Add a `turn.completed` autoreply hook that catches orchestrator tasks that close themselves without closing their orchestrated target. This is a safety net beyond the template fix — it handles cases where the agent doesn't follow the template (context limit, crash, misinterpretation).

## Design: Two-strike approach

### Strike 1 — Autoreply

On `turn.completed`, a new `self.check_orchestrator_target` function checks:
1. Is `AIKI_TASK` set? (task-driven session)
2. Is the task an orchestrator? (`task_type == "orchestrator"`)
3. Is the orchestrator closed?
4. Is the orchestrated target (via `orchestrates` link) still unclosed?

If yes: write a `ORCH_CLOSE_WARNED` marker comment on the orchestrator task and return target info. The hook fires an autoreply telling the agent to close or stop the target.

### Strike 2 — Hard stop

On the next `turn.completed`, the function sees the marker comment and the target is still unclosed. It programmatically stops the target with reason `"Orchestrator error: agent failed to close target after warning"` and returns empty (no autoreply loop).

## Implementation

### New function: `self.check_orchestrator_target`

**File:** `cli/src/flows/core/functions.rs`

```rust
const ORCH_CLOSE_MARKER: &str = "ORCH_CLOSE_WARNED";

pub fn check_orchestrator_target(cwd: &Path) -> Result<ActionResult> {
    let task_id = match std::env::var("AIKI_TASK") {
        Ok(id) if !id.is_empty() => id,
        _ => return Ok(ActionResult::empty_success()),
    };

    let events = crate::tasks::storage::read_events(cwd)?;
    let graph = crate::tasks::graph::materialize_graph(&events);

    let task = match graph.tasks.get(&task_id) {
        Some(t) => t,
        None => return Ok(ActionResult::empty_success()),
    };

    if !task.is_orchestrator() || task.status != TaskStatus::Closed {
        return Ok(ActionResult::empty_success());
    }

    let target_id = match graph.edges.target(&task_id, "orchestrates") {
        Some(id) => id.to_string(),
        None => return Ok(ActionResult::empty_success()),
    };

    let target = match graph.tasks.get(&target_id) {
        Some(t) => t,
        None => return Ok(ActionResult::empty_success()),
    };

    if matches!(target.status, TaskStatus::Closed | TaskStatus::Stopped) {
        return Ok(ActionResult::empty_success());
    }

    let already_warned = task.comments.iter().any(|c| c.text == ORCH_CLOSE_MARKER);

    if already_warned {
        // STRIKE 2: Hard stop
        crate::tasks::storage::write_event(cwd, &TaskEvent::Stopped {
            task_ids: vec![target_id.clone()],
            reason: Some("Orchestrator error: agent failed to close target after warning".into()),
            session_id: None,
            turn_id: None,
            timestamp: chrono::Utc::now(),
        })?;
        return Ok(ActionResult::empty_success());
    }

    // STRIKE 1: Warn and mark
    crate::tasks::storage::write_event(cwd, &TaskEvent::Comment {
        task_id: task_id.clone(),
        text: ORCH_CLOSE_MARKER.to_string(),
        timestamp: chrono::Utc::now(),
    })?;

    let status = match target.status {
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Open => "open",
        _ => "unknown",
    };

    Ok(ActionResult {
        success: true,
        exit_code: Some(0),
        stdout: format!(
            "Target {} ({}) is still {}.\n\
             You MUST close or stop it before ending your session:\n\
             \n\
             aiki task close {} --summary \"...\"\n\
             aiki task stop {} --reason \"...\"",
            short_id(&target_id), target.name, status,
            short_id(&target_id), short_id(&target_id),
        ),
        stderr: String::new(),
    })
}
```

### Hook in `hooks.yaml`

```yaml
turn.completed:
    # Orchestrator close guard (two-strike):
    # Strike 1: autoreply asking agent to close/stop the target
    # Strike 2 (inside function): hard-stop the target
    - let: orch_target = self.check_orchestrator_target
    - if: orch_target
      then:
          - autoreply: |
                ORCHESTRATOR CLOSE REQUIRED

                Your orchestrator task has been closed, but the target it manages
                is still open:

                {{orch_target}}

                You must resolve this before your session ends.
```

### Registration in `engine.rs`

```rust
("core", "check_orchestrator_target") => {
    crate::flows::core::check_orchestrator_target(state.cwd())
}
```

## Files to Change

| File | Change |
|------|--------|
| `cli/src/flows/core/functions.rs` | New `check_orchestrator_target()` function |
| `cli/src/flows/engine.rs` | Register in both dispatch blocks |
| `cli/src/flows/core/hooks.yaml` | Add orchestrator close guard in `turn.completed` |

## Open Questions

1. **Comment structure**: Verify `task.comments` field shape and `TaskEvent::Comment` event type.
2. **Async coverage**: This naturally covers async paths since `turn.completed` fires regardless of spawn mode.
