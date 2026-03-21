# Task Progress Comments: Health Monitoring for Long-Running Agents

**Date**: 2026-03-20
**Status**: Proposed
**Priority**: P2 — nice-to-have, improves observability and recovery
**Sourced from**: [hung-fix.md](hung-fix.md) (section 4)

---

## Problem

Long-running agents (decompose, fix, review) can hang silently. The orchestrator has no way to distinguish a working agent from a dead one — it just blocks indefinitely on `task_run` / `task_run_on_session`. Even with timeouts (see [hung-fix.md](hung-fix.md)), a fixed timeout is a blunt instrument: a legitimately slow agent gets killed while a fast-but-stuck agent wastes the full timeout window.

---

## Proposal

Agents should emit periodic progress comments via `aiki task comment add`. The runner monitors new task comments during blocking runs and stops agents that go silent.

### Agent Side: Leave Progress Comments

Agents already have access to `aiki task comment add <task-id>`. The agent instructions (CLAUDE.md) already encourage progress comments. The change is to:

1. **Make progress comments a convention** — agents SHOULD emit a comment at least every 60 seconds during active work.
2. **Keep them natural** — comments should say what the agent is doing now, for example:
   ```
   Reading the plan and outlining subtasks
   Identified 4 issues, drafting follow-up tasks
   Created 3/4 subtasks, writing the last one now
   ```

### Runner Side: Monitor and Kill

When `task_run` / `task_run_on_session` launches an agent in monitored mode, the existing status monitor tracks new comments on that task. If no new comment arrives within `comment_timeout` (default: 120s after an initial grace period), the runner assumes the agent is stuck, terminates the child process, and retries the task once before surfacing the failure.

```
t=0s:   Agent started
t=30s:  "Reading plan..." comment — alive
t=90s:  "Drafting subtasks..." comment — alive
t=210s: No comment for 120s — terminate child, retry task once
t=330s: No comment for 120s on retry — stop task and surface failure
```

---

## Design

### Progress Comment Guidance (Headless Seed Prompt)

Add to the headless seed prompt (`AgentSpawnOptions::task_prompt()`):

```
Leave task comments describing what you're working on as you make progress.
Use `aiki task comment add <task-id> "..."` at least every 60 seconds.
```

This is a soft contract. Any new comment counts as activity for the monitor. The benefit is low friction and natural-looking task history. The tradeoff is that comments are treated as both human-facing progress notes and liveness signals. Agents that never leave progress comments can still run, but they are governed only by the fixed timeout from hung-fix.md and will not get the more responsive silent-agent detection. When comment monitoring fires, the runner should treat it as a recoverable runtime failure first: kill the child, retry once, and only then escalate.

### Comment Monitoring (Runner)

Integrate comment tracking into the existing `StatusMonitor` event loop used by `task_run` and `task_run_on_session`. Do not add a separate wrapper around `decompose.rs` / `fix.rs` because the `MonitoredChild` handle that must be terminated already lives inside `runner.rs`.

```rust
struct CommentActivityMonitor {
    task_id: String,
    timeout: Duration,            // default 120s
    startup_grace: Duration,      // default 180s
    started_at: Instant,
    last_comment_at: Option<Instant>,
    last_comment_ts: Option<DateTime<Utc>>,
}

impl CommentActivityMonitor {
    /// Update comment-activity state from the already-loaded task graph.
    fn observe_task(&mut self, task: &Task) {
        let latest = task.comments.last();
        if let Some(comment) = latest {
            let is_new = self
                .last_comment_ts
                .map(|ts| comment.timestamp > ts)
                .unwrap_or(true);
            if is_new {
                self.last_comment_ts = Some(comment.timestamp);
                self.last_comment_at = Some(Instant::now());
            }
        }
    }

    /// Check whether the monitored process has gone silent.
    fn check(&self) -> Result<(), AikiError> {
        let elapsed = self
            .last_comment_at
            .map(|t| t.elapsed())
            .unwrap_or_else(|| self.started_at.elapsed());

        let allowed = if self.last_comment_at.is_some() {
            self.timeout
        } else {
            self.startup_grace
        };

        if elapsed > allowed {
            return Err(AikiError::AgentUnresponsive {
                task_id: self.task_id.clone(),
                silence_for: elapsed,
            });
        }
        Ok(())
    }
}
```

Implementation notes:

- Reuse the existing `StatusMonitor::poll()` read of the event log; do not add a second polling path such as `get_task_comments()`.
- Treat any new comment as activity.
- Encourage agents to leave regular progress comments so the natural comment stream is a useful liveness signal.
- On the first comment timeout, terminate the `MonitoredChild`, record that the run became unresponsive, and retry once with a fresh monitor window.
- If the retry also times out, or if the retry cannot be started, map the result into the existing stopped/failed handling path so the task is not left in progress.

### Integration with Timeout

The comment-activity monitor works alongside the fixed timeout from hung-fix.md:

- **Fixed timeout**: hard cap (e.g., 5 minutes). Agent is killed regardless of comments.
- **Comment timeout**: soft cap (e.g., 120s since last comment). Agent is killed if it goes silent.

The child process is terminated by whichever fires first. This means:
- A legitimately working agent that leaves progress comments can run up to the fixed timeout
- A stuck agent that stops leaving comments is killed within `comment_timeout` seconds
- A transient hang gets one automatic retry before the task is stopped and surfaced to a human

### Retry Behavior

Comment silence should be treated as a recoverable runtime failure on first occurrence:

1. Kill the unresponsive child process.
2. Emit an audit trail entry (comment or event) noting that the run went silent and is being retried.
3. Restart the task once with a fresh comment timeout window.
4. If the retry also goes silent, or if restart fails, stop the task and surface the failure.

This keeps the orchestrator autonomous for transient hangs while still bounding repeated failures.

The retry should be visible in the existing chatty pipeline output from `chatty-output.md`. When a run is killed for comment silence and restarted, the UI should emit a narrative update such as "agent became unresponsive, retrying once" so the user can see that the task did not just pause and resume invisibly.

### Chatty Output: Heartbeat Events

The following mockups show how comment-timeout events appear in the chatty pipeline output from [chatty-output.md](chatty-output.md). They follow the same chatty rules: past tense for done, present tense for active, progressive dimming, surface-bg blocks for live agents.

**What's new in blocks:**
- **Heartbeat line** — active agent blocks show the latest progress comment as a dim italic line above the footer. This is the happy-path liveness signal: visible proof the agent is working.

**New `MessageKind` usage for failures:**
- **Attention** (yellow) — "Agent unresponsive — retrying {task}"
- **Error** (red) — "Agent unresponsive — stopped {task}" (retry also failed)

No new `MessageKind` variants needed. `Attention` and `Error` already express these states.

#### Happy path: progress comments visible in blocks

The latest task comment appears as a dim line below the footer, prefixed with `⎿` to visually hang off the block. This is the primary way users see that an agent is alive and what it's doing.

**AgentBlock with heartbeat:**

[80 cols]
```
 ┃  ▸ Reviewing changes                                                       ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 18% · $0.12                                18s         ← surface bg, dim footer
 ┃  ⎿ Scanning auth handler for null checks                                   ← surface bg, dim ⎿
```

**LaneBlock with heartbeat:**

```
 ┃  ✓ Explore webhook requirements                             8s             ← surface bg, green ✓
 ┃  ✓ Create implementation plan                               6s             ← surface bg, green ✓
 ┃  ▸ Verify Stripe signatures                                                ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 42% · $0.35                                32s         ← surface bg, dim footer
 ┃  ⎿ Checking signature against test vectors                                 ← surface bg, dim ⎿
```

**Two parallel lanes, both with heartbeats:**

```
 ┃  ✓ Explore webhook requirements                             8s             ← surface bg, green ✓
 ┃  ✓ Create implementation plan                               6s             ← surface bg, green ✓
 ┃  ▸ Verify Stripe signatures                                                ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 42% · $0.35                                32s         ← surface bg, dim footer
 ┃  ⎿ Checking signature against test vectors                                 ← surface bg, dim ⎿

 ┃  ▸ Implement webhook route handler                                         ← surface bg, yellow ▸
 ┃  cursor/sonnet-4.6 · 28% · $0.14                              48s         ← surface bg, dim footer
 ┃  ⎿ Writing route handler for /api/webhooks                                 ← surface bg, dim ⎿
 ┃  ○ Write integration tests                                                 ← surface bg, dim ○

 ○ Add idempotency key tracking                                               ← dim ○
```

The heartbeat line is the agent's own words — whatever it last wrote via `aiki task comment add`. It updates in place as new comments arrive (the block always shows only the latest). When the block collapses to a `✓` line on completion, the heartbeat line disappears with it.

**Block anatomy (updated):**

```
   ▸ {task_name}                                                      ← surface bg, yellow ▸
   {agent}/{model} · {ctx_pct}% · ${cost}                {elapsed}   ← surface bg, dim footer
   ⎿ {latest_comment}                                                ← surface bg, dim ⎿
```

Three layers: **what** (task), **who** (footer), **status** (comment). The `⎿` prefix makes the comment visually subordinate to the footer — a leaf hanging off the agent identity. The comment line is optional — if no comments have been left yet (agent just started), the block renders without it:

```
   ▸ {task_name}                                                      ← surface bg, yellow ▸
   {agent}/{model} · {ctx_pct}% · ${cost}                {elapsed}   ← surface bg, dim footer
```

#### Build lane: agent goes silent, retry in progress

[80 cols]
```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Decomposed into 6 subtasks                                        12s        ← dim

 ✓ Explore webhook requirements                              8s  claude      ← green
 ✓ Create implementation plan                                6s  claude

 Agent unresponsive — retrying Verify Stripe signatures                       ← yellow, Attention

 ┃  ▸ Verify Stripe signatures (retry)                                        ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 8% · $0.02                                  4s         ← surface bg, dim footer

 ┃  ▸ Implement webhook route handler                                         ← surface bg, yellow ▸
 ┃  ○ Write integration tests                                                 ← surface bg, dim ○
 ┃  cursor/sonnet-4.6 · 28% · $0.14                              48s         ← surface bg, dim footer

 ○ Add idempotency key tracking                                               ← dim ○
```

The first attempt timed out after 120s of silence. The runner killed the child and restarted the task. The attention line tells the user what happened. The new block starts fresh (reset context, cost, elapsed). The other lane is unaffected and continues running.

The `(retry)` suffix in the task name is informational — the builder appends it when the task's run history shows a prior unresponsive termination.

#### Build lane: retry succeeds, build completes

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Built 6/6 subtasks (1 retried)                                   3m12       ← STAGE-DIM

 ┃  ▸ Reviewing changes                                                       ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 18% · $0.12                                18s         ← surface bg, dim footer
```

Build collapses to the summary line as normal. The `(1 retried)` note is the only trace — the retry attention line dims with the rest of the build stage. Review proceeds normally.

#### Build lane: retry also fails, task stopped

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Decomposed into 6 subtasks                                        12s        ← dim

 ✓ Explore webhook requirements                              8s  claude      ← green
 ✓ Create implementation plan                                6s  claude
 ✓ Implement webhook route handler                          48s  cursor      ← green
 ✓ Write integration tests                                  22s  cursor
 ✓ Add idempotency key tracking                             30s  claude

 Agent unresponsive — retrying Verify Stripe signatures                       ← dim (stage-dimmed)
 Agent unresponsive — stopped Verify Stripe signatures                        ← red, Error

 Build failed: 1 subtask stopped (unresponsive)                               ← red
```

Both attempts went silent. The first attention line is now dim (completed stage). The second line is `Error` (red). The task is stopped and surfaced as a build failure. The `✗` symbol is not used here because the task didn't error — it was killed by the monitor. The phrasing "stopped (unresponsive)" distinguishes this from a code error.

#### Review: single-task agent goes silent, retry in progress

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Built 6/6 subtasks                                                1m54      ← STAGE-DIM

 Agent unresponsive — retrying Reviewing changes                              ← yellow, Attention

 ┃  ▸ Reviewing changes (retry)                                               ← surface bg, yellow ▸
 ┃  claude/opus-4.6 · 6% · $0.04                                  8s         ← surface bg, dim footer
```

Same pattern for single-task agents (decompose, review, plan-fix, review-fix). The agent block is an `AgentBlock`, not a `LaneBlock`. The attention line appears above the new block.

#### Review: retry succeeds

```
 webhooks.md                                                                   ← hi+bold

 Created plan                                                      14:32      ← STAGE-DIM
 Edited with claude                                                2m08      ← STAGE-DIM

 Built 6/6 subtasks                                                1m54      ← STAGE-DIM

 Agent unresponsive — retried Reviewing changes                    42s        ← dim (stage-dimmed)
 Review passed — approved                                          28s        ← green
```

On success, the attention line picks up past tense ("retried") and the elapsed time of the failed attempt. It dims with the rest of the review stage. The review result renders normally.

#### Data model additions

The heartbeat line and retry events extend the existing `Chat` / `Message` / `ChatChild` model from chatty-output.md.

**Heartbeat line in blocks** — add an optional `heartbeat` field to `AgentBlock` and `LaneBlock`:

```rust
/// A single-task agent session — task line + footer + heartbeat, surface background.
AgentBlock {
    task_name: String,
    heartbeat: Option<String>,   // latest task comment, e.g. "Scanning auth handler..."
    footer: BlockFooter,
}

/// An active lane — subtask lines + footer + heartbeat, surface background.
LaneBlock {
    subtasks: Vec<LaneSubtask>,
    heartbeat: Option<String>,   // latest task comment from the active subtask
    footer: BlockFooter,
}
```

The builder populates `heartbeat` from the latest comment on the active task (for `AgentBlock`) or the active subtask (for `LaneBlock`). `None` means no comments yet — the block renders without the heartbeat line.

**Rendering rule:** The heartbeat line renders below the footer with a `⎿` prefix, `dim` foreground on `theme.surface` background. The `⎿` makes it visually subordinate — a leaf hanging off the agent identity. It is never shown outside a block (collapsed `✓` lines don't carry the heartbeat).

**Retry events** — fit existing types, no new `MessageKind` needed:

```rust
// Retry attention line — emitted when runner kills a silent agent
Message {
    stage: Build,                    // or Review, Fix — matches the agent's stage
    kind: Attention,                 // yellow
    text: "Agent unresponsive — retrying Verify Stripe signatures".into(),
    meta: None,                      // no elapsed yet (retry just started)
    children: vec![],
}

// Final failure line — emitted when retry also goes silent
Message {
    stage: Build,
    kind: Error,                     // red
    text: "Agent unresponsive — stopped Verify Stripe signatures".into(),
    meta: None,
    children: vec![],
}

// Collapsed retry line (after stage completes and dims)
Message {
    stage: Build,
    kind: Attention,
    text: "Agent unresponsive — retried Verify Stripe signatures".into(),
    meta: Some("42s".into()),        // elapsed of the failed attempt
    children: vec![],
}
```

The builder reads the task's run history to detect unresponsive terminations and emit the appropriate messages. The `(retry)` suffix on task names inside blocks is added by the builder when the current run is a retry.

#### Build summary line: retry annotation

When at least one subtask was retried during build, the collapsed summary includes the count:

```
 Built 6/6 subtasks (1 retried)                                   3m12       ← green (or STAGE-DIM)
```

If retries failed and the build stopped:
```
 Build failed: 1 subtask stopped (unresponsive)                               ← red
```

These follow the chatty rules: past tense, inline detail, no drill-down needed.

### Scope

This plan covers monitored, blocking runs only:

- `task_run(...)`
- `task_run_on_session(...)`

It does **not** cover `task_run_async(...)` yet. Background runs return immediately and currently have no resident supervisor process that can observe comments and terminate the child. Async comment supervision should be handled in a followup plan, likely via a session manager or background watchdog.

---

## Files to Change

| File | Change |
|------|--------|
| `cli/src/tasks/status_monitor.rs` | Add `CommentActivityMonitor` state and integrate checks into the existing poll loop |
| `cli/src/tasks/runner.rs` | Initialize comment-monitor config for monitored runs, terminate silent children, and retry once before stopping |
| `cli/src/error.rs` | Add `AgentUnresponsive` error variant |
| Chatty pipeline output | Surface retry/restart as a narrative update when a silent run is terminated and relaunched |
| Headless seed prompt (`AgentSpawnOptions::task_prompt`) | Add progress-comment guidance for long-running headless work |
| Followup plan | Design async/background comment supervision for `task_run_async(...)` |

---

## Open Questions

1. **Polling vs. watching** — The initial version should reuse `StatusMonitor` polling instead of adding event-log watching. Is the existing poll cadence sufficient for responsiveness?
2. **Comment timeout value** — 120s is still a guess. Should this be configurable per-command or globally?
3. **Startup grace period** — 180s is a safer default for cold-start + context-reading time, but should it vary by command or agent?
4. **Retry budget semantics** — Should the fixed timeout apply across the original run plus retry, or should each attempt get its own full timeout budget?
5. **Audit trail format** — When a retry is triggered, should we record it as a regular task comment, a structured event, or both?
6. ~~**Chatty output shape**~~ — **Resolved:** Attention message (yellow) for retries, Error message (red) for final failure. See "Chatty Output: Heartbeat Events" section above.
7. ~~**Final failure semantics**~~ — **Resolved:** Task ends as stopped. The chatty output shows `Error` line "Agent unresponsive — stopped {task}" and the build summary reads "Build failed: 1 subtask stopped (unresponsive)". See mockups above.
8. **Comment overhead** — Frequent progress comments add noise to the task history. Is that acceptable, or do we eventually want a dedicated activity signal?
