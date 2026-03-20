# Hung Fix Pipeline: Decompose Agent Blocks Entire Chain

**Date**: 2026-03-20
**Status**: Proposed
**Priority**: P1 — causes indefinite hangs in automated pipelines

---

## Problem

The `aiki fix` pipeline blocks forever when the decompose agent hangs. There is no timeout, health check, or fallback — a single stuck agent stalls the entire plan → decompose → loop → review chain.

### Observed Incident

Review `wtztmmu` (Review: Code smarter-instructions.md) found 4 high-severity issues and spawned two fix cycles. The first completed successfully. The second hung:

```
wtztmmu (Review: Code smarter-instructions.md) — CLOSED, 4 high issues
  │
  ├── [RUN 1 — SUCCESS]
  │   zwlvlvqx (Plan Fix) ——→ CLOSED ✓
  │   lqsmtvwr (Fix: Code) ——→ CLOSED ✓ (all 4 subtasks done)
  │
  └── [RUN 2 — HUNG]
      ykspwtx (Plan Fix) ——→ STUCK in_progress
      │   wrote plan to /tmp/aiki/plans/ykspwtx...md
      │
      └── nkpzmsk (Decompose) ——→ STUCK in_progress
          │   last comment: "Reading plan and drafting parent/subtask breakdown."
          │   created ZERO subtasks
          │
          npuuqxtv (Fix: Code) ——→ STUCK open
              links: implements-plan, populated-by nkpzmsk, remediates wtztmmu
              subtasks: NONE
```

The decompose agent (`nkpzmsk`) posted one progress comment then never completed. It created no subtasks under the fix-parent (`npuuqxtv`). The plan file at `/tmp/aiki/plans/ykspwtx...md` has since been deleted by temp cleanup, so the plan is unrecoverable.

---

## Root Cause

The fix pipeline in `cli/src/commands/fix.rs` is a synchronous chain:

```
Step 4: task_run(plan-fix)         — agent writes fix plan
Step 5: run_decompose(plan, target) — agent breaks plan into subtasks
Step 8: run_loop(fix-parent)        — orchestrate subtasks
Step 10: create_review(fix-parent)  — review the results
```

Each step blocks on the previous. `run_decompose()` (line 120-125 of `decompose.rs`) calls `task_run_on_session()` which blocks until the agent session ends:

```rust
// decompose.rs:120-125
if let Some(session) = session {
    let result = task_run_on_session(cwd, &decompose_task_id, run_options, session)?;
    handle_session_result(cwd, &decompose_task_id, result, true)?;
} else {
    task_run(cwd, &decompose_task_id, run_options)?;
}
```

If the decompose agent hangs (API timeout, context limit, OOM, infinite loop), `task_run_on_session` never returns, and the entire fix pipeline is stuck.

Even if the decompose agent had returned without creating subtasks, `run_loop()` at line 134 of `loop_cmd.rs` would catch it:

```rust
if subtasks.is_empty() {
    return Err(AikiError::InvalidArgument(format!(
        "Parent task {} has no subtasks. Nothing to loop over.",
        &parent_id[..parent_id.len().min(8)]
    )));
}
```

But execution never reached `run_loop` because `run_decompose` itself never returned.

---

## Impact

- The fix pipeline hangs indefinitely with no user feedback
- The parent session (which launched `aiki fix`) is stuck and must be killed manually
- Orphaned tasks remain `in_progress` forever (no cleanup on kill)
- The plan file in `/tmp/aiki/plans/` gets cleaned up by the OS, making the fix unrecoverable even if the pipeline could be restarted

---

## Proposed Solutions

### 1. Timeout on agent runs (minimum viable fix)

Add a configurable timeout to `task_run` / `task_run_on_session`. If the agent doesn't complete within the timeout, kill it and return an error.

```rust
// In task_run or task_run_on_session:
let timeout = options.timeout.unwrap_or(Duration::from_secs(300)); // 5 min default
match tokio::time::timeout(timeout, run_agent(..)).await {
    Ok(result) => result,
    Err(_) => {
        kill_agent_session(..);
        close_task(task_id, "Timed out");
        Err(AikiError::AgentTimeout { task_id, timeout })
    }
}
```

### 2. Post-decompose validation (defense in depth)

After `run_decompose` returns, verify subtasks were actually created before proceeding to `run_loop`:

```rust
// fix.rs, after line 330:
run_decompose(cwd, &plan_path, fix_parent_id, decompose_options, session.as_mut())?;

// Validate decompose actually created subtasks
let events = read_events(cwd)?;
let graph = materialize_graph(&events);
let subtasks = get_subtasks(&graph, fix_parent_id);
if subtasks.is_empty() {
    return Err(AikiError::DecomposeProducedNoSubtasks {
        plan_path: plan_path.clone(),
        fix_parent_id: fix_parent_id.to_string(),
    });
}
```

This is redundant with `run_loop`'s check but catches the failure earlier with a more specific error message.

### 3. Persist plan file outside /tmp (recoverability)

Store the plan file in the repo (e.g., `.aiki/plans/`) or in the JJ change description so it survives temp cleanup. This allows manual recovery with `aiki fix --continue`.

---

## Recommended Implementation Order

1. **Post-decompose validation** (easiest, immediate value — 10 lines of code)
2. **Timeout on agent runs** (broader fix, prevents all agent hangs)
3. **Persist plans outside /tmp** (recoverability for --continue)

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/commands/fix.rs` | Add subtask validation after `run_decompose` calls (lines 330 and 516) |
| `cli/src/commands/decompose.rs` | Add timeout option to `run_decompose` |
| `cli/src/tasks/runner.rs` (or equivalent) | Add timeout support to `task_run` / `task_run_on_session` |
| `cli/src/error.rs` | Add `AgentTimeout` and `DecomposeProducedNoSubtasks` error variants |

---

## Future Improvements

- **[Task heartbeat monitoring](task-heartbeat.md)** — agents emit periodic heartbeats; orchestrator kills unresponsive agents. More responsive than a fixed timeout but adds complexity. Worth revisiting once the core timeout and validation fixes are in place.

---

## Cleanup

The orphaned tasks from this incident should be closed:

```bash
aiki task close ykspwtxlmnyzlsmrkzpsqolynnrwsuwp --wont-do --summary "Decompose agent hung, plan file lost"
aiki task close nkpzmskpuxzrrpysowkzytnmuqkkvlkt --wont-do --summary "Hung during decompose, never created subtasks"
aiki task close npuuqxtvptrttutvkpmmnwlvqpnxpzzp --wont-do --summary "Never populated — decompose agent hung before creating subtasks"
```
