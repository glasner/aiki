# Review Loop Plugin

**Date**: 2026-02-11
**Status**: Draft
**Priority**: P2
**Depends on**: `ops/now/loop-flags.md` (CLI primitives: `fix --loop`, `review --fix`)

**Related Documents**:
- [Loop Flags](loop-flags.md) - CLI primitives this plugin builds on (`fix --loop`, `review --fix`, `build --loop`)
- [Review and Fix Commands](../done/review-and-fix.md) - Core review/fix system (implemented)
- [Default Hooks](default-hooks.md) - Hookfile scaffolding and built-in plugin registry (prerequisite)
- [Better Include for Plugins](better-include-for-plugins.md) - `include:` directive and composition blocks (prerequisite)

---

## Problem

The CLI primitives (`fix --loop`, `review --fix`) give users explicit control over the review-fix cycle, but the user still has to remember to run them. There's no automated way to trigger the review loop after every agent turn.

---

## Summary

The `aiki/review-loop` hook plugin automates the review-fix cycle after every agent turn using the flow system. It triggers on `turn.completed` — reviewing tasks completed during the turn rather than firing per-task (which would be too noisy).

This builds on the CLI primitives and requires turn stamping on task events plus a `turn.tasks.completed` lazy variable.

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Hook packaging | `aiki/review-loop` plugin, included via `after:` | Composable — users opt in without modifying their existing hooks |
| Trigger event | `turn.completed` not `task.closed` | Fires once per turn, has native `autoreply:` support, avoids noise |
| Won't-do work tasks | Excluded from hook trigger | Skipped/declined work doesn't need review |

---

## Why `turn.completed` Instead of `task.closed`

Triggering on `task.closed` fires a review for every individual task closure — including subtasks, fix tasks, and other intermediate work. This creates noise: multiple reviews per turn, reviews of reviews, and unnecessary churn.

`turn.completed` fires once per agent turn and has native `autoreply:` support. No Event Gap — no engine changes needed for `autoreply:`.

---

## `turn.tasks.completed` Variable

The hook needs to know **which tasks were completed this turn** and whether they represent **original work** (not review/fix loop artifacts). We add a new event variable to `turn.completed`:

**`$event.turn.tasks.completed`** — space-separated IDs of tasks completed this turn, filtered to only include original work tasks.

A task is included if **all** of these are true:
1. **Closed this turn** — task's Closed event has a `turn_id` matching the current turn
2. **Outcome is "done"** — not won't-do (skipped/declined work doesn't need review)
3. **Type is not "review"** — review tasks are not reviewable work
4. **No review ancestor** — no review-type task in the ancestor graph (walking both `sourced-from` and `subtask-of` links)

Rule 4 walks the full ancestor graph — not just `sourced-from` (provenance chain) but also `subtask-of` (parent chain). This is necessary because subtasks of review tasks (like the fix-loop subtask) are connected via `subtask-of`, not `sourced-from`. Without walking both link types, the fix-loop subtask would leak through the filter when it closes as "done".

Concretely, `aiki fix` creates followup tasks with `sourced-from → review_task`, and the template system creates subtasks with `subtask-of → review_task`. Both paths must be checked.

**Examples:**

| Task closed this turn | Included? | Reason |
|----------------------|-----------|--------|
| "Implement feature X" (type: none, outcome: done) | Yes | Original work |
| "Review: Task (feature X)" (type: review) | No | Rule 3: review task |
| "Followup: Review abc" (sourced-from → review abc) | No | Rule 4: review in sourced-from chain |
| "Fix null check" (subtask of followup, sourced-from chain has review) | No | Rule 4: review in ancestor graph |
| "Fix Loop" (subtask-of → review task) | No | Rule 4: review in parent chain |
| "Refactor auth" (type: none, outcome: won't-do) | No | Rule 2: not done |

**Implementation:** Computed lazily when first accessed. Walks the ancestor graph (both `sourced-from` and `subtask-of` edges) for review-type checks. Empty string when no qualifying tasks exist.

> **Note — Reconciliation with `default-hooks.md`:** The default-hooks spec proposes a simpler `$event.tasks.closed` payload field populated in `handle_turn_completed`. That field is general-purpose (all tasks closed this turn, no filtering). `$event.turn.tasks.completed` is a **derived** lazy variable built on top of `tasks.closed` that applies the review-loop-specific filtering rules above. The plugin YAML in default-hooks should reference `$event.turn.tasks.completed`, not `$event.tasks.closed`. (Alternatively, the filtering can be folded into the handler that populates `tasks.closed` — but this couples the general payload to review-loop semantics.)

---

## Turn Stamping on Task Events

Task lifecycle events (`Closed`, `Started`, `Stopped`, etc.) are stamped with `turn` and `turn_id` at write time. This allows `$event.turn.tasks.completed` to join task events to turns without relying on timestamps or cross-process state.

**How stamping works:**

When `aiki task close` (or start, stop, etc.) runs mid-turn:

1. `find_active_session(cwd)` → returns session UUID (already used for session ownership)
2. `history::get_current_turn_info(&global_aiki_dir(), &session_uuid)` → returns `(turn_number, source)` from the global `~/.aiki/.jj/` conversations branch
3. `generate_turn_id(session_uuid, turn_number)` → deterministic UUID v5
4. Event written with `turn` and `turn_id` fields

**The join key is `turn_id`, not `turn`:**

`turn_id = uuid_v5(session_uuid, turn.to_string())` encodes both session identity and turn sequence in a single value. This eliminates two classes of bugs:

- **Cross-session contamination** — Two concurrent sessions both on "turn 3" have different `turn_id`s because different session UUIDs produce different UUID v5 namespaces.
- **Off-by-one races** — Even if a query somehow saw a turn N+1 prompt event, the `turn_id` for N and N+1 are different values, so the filter is exact.

The `$event.turn.tasks.completed` resolution is:
```
task_closed_events.where(|e| e.turn_id == $event.turn.id)
```

**Race condition analysis:**

The `turn.completed` handler runs synchronously:

```
turn.started N  →  writes turn=N to aiki/conversations
      ↓
agent works     →  aiki task close stamps turn_id(session, N) on Closed event
      ↓
turn.completed  →  handler queries get_current_turn_info() → N
                   sets $event.turn.id = turn_id(session, N)
                   hook resolves $event.turn.tasks.completed
                     → queries task events WHERE turn_id == turn_id(session, N)
                   returns autoreply (if any)
      ↓
turn.started N+1  →  fires AFTER handler returns (cannot race)
```

There is no race window because:

1. **`turn.started` for N+1 cannot fire before `turn.completed` for N returns** — the handler is synchronous, and `turn.started` is triggered by the next agent interaction (which happens after the autoreply from `turn.completed` is delivered).

2. **Task events are stamped during the turn** — `aiki task close` runs as a tool call within the turn. The prompt event for turn N is already written to `aiki/conversations` (by `turn.started`), so `get_current_turn_info()` returns N. The prompt event for N+1 doesn't exist yet.

3. **`turn_id` is the join key** — even in a hypothetical race, `turn_id(session, N) != turn_id(session, N+1)`, so wrong-turn events can never match.

**Graceful fallback:** If session detection or turn lookup fails (e.g., `aiki task close` called outside a session), `turn` defaults to 0 and `turn_id` to empty string. These events won't match any `turn.completed`'s `$event.turn.id`, which is fine — untracked tasks simply don't appear in `$event.turn.tasks.completed`.

---

## The Plugin

```yaml
# .aiki/hooks/aiki/review-loop.yml
name: aiki/review-loop
description: "Review-fix cycle after each agent turn"
version: "1"

turn.completed:
  - if: $event.turn.tasks.completed
    then:
      - autoreply: |
          Tasks completed this turn: $event.turn.tasks.completed

          Review your work and fix any issues:

          aiki review --fix --start
```

The `if: $event.turn.tasks.completed` guard ensures the hook only fires when original work was completed. Empty string is falsy, so turns that only close review/fix tasks (or close nothing) are skipped.

The agent receives the autoreply and runs `aiki review --fix --start`:

1. `aiki review` (no task ID) → scopes to all done tasks in the session (excluding review/fix loop tasks)
2. `--fix` → adds the fix-loop subtask to the review
3. `--start` → agent takes over the review in its current session
4. When the review closes, the fix-loop subtask runs `aiki fix --loop`
5. `fix --loop` handles re-review and iteration until clean

**Loop prevention:** When the agent completes a review turn or fix turn, the hook fires again — but `$event.turn.tasks.completed` is empty (only review/fix tasks were closed), so the `if:` guard skips the autoreply. The loop breaks naturally.

---

## Prerequisites

| Component | Status | Needed By |
|-----------|--------|-----------|
| `aiki review` command | Implemented | — |
| `aiki fix` command | Implemented | — |
| `aiki fix --loop` | **Not implemented** | [Loop Flags](loop-flags.md) |
| `aiki review --fix` | **Not implemented** | [Loop Flags](loop-flags.md) |
| `autoreply:` on `turn.completed` | Implemented | — |
| `include:` directive and composition blocks ([Better Include](better-include-for-plugins.md)) | Implemented | — |
| **Turn stamping on task events** | **Not implemented** | This spec |
| **`turn.tasks.completed` event variable** | **Not implemented** | This spec |
| **Built-in plugin registry** | **Not implemented** | [Default Hooks](default-hooks.md) |
| **Default hookfile scaffolding** (`aiki init` creates `.aiki/hooks.yml`) | **Not implemented** | [Default Hooks](default-hooks.md) |
| Built-in hook file | **Not created** | This spec |

---

## Enabling / Disabling

Users get `aiki/review-loop` automatically via `include: - aiki/default` in their hookfile (created by `aiki init` — see `ops/now/default-hooks.md`). The review loop runs as an `after:` inline handler inside `aiki/default`:

```yaml
# .aiki/hooks.yml (created by aiki init)
name: hooks
include:
  - aiki/default  # Includes review loop in its after: block
```

To disable, users can either remove `aiki/default` from `include:` or override `aiki/default` with a custom version that omits the review loop.

---

## Customizing

Users can override by creating their own `.aiki/hooks/aiki/review-loop.yml` or creating a different plugin:

```yaml
# .aiki/hooks/myorg/strict-review-loop.yml
name: myorg/strict-review-loop
version: "1"

turn.completed:
  - if: $event.turn.tasks.completed
    then:
      - autoreply: |
          Review your work for security issues:

          aiki review --fix --start --agent codex --template myorg/security-review
```

---

## Iteration Lifecycle (Hook)

When the hook plugin is enabled, the flow is agent-driven:

### Step 1: Agent Completes Turn

The agent finishes work (closes task A, maybe task B). The `turn.completed` event fires. `$event.turn.tasks.completed` resolves to IDs of tasks A and B (assuming they're original work, outcome "done", not part of a review/fix loop).

### Step 2: Hook Guard Fires

The `if: $event.turn.tasks.completed` check passes (non-empty). The autoreply is sent:

```
Tasks completed this turn: <task-A-id> <task-B-id>

Review your work and fix any issues:

aiki review --fix --start
```

### Step 3: Agent Reviews and Loops

The agent runs `aiki review --fix --start`:

1. `aiki review` (no task ID) creates a review scoped to done tasks in the session
2. `--start` means the agent does the review itself
3. `--fix` adds the fix-loop subtask
4. Agent reviews, closes the review task
5. Fix-loop subtask fires → `aiki fix <review> --loop`
6. Loop handles re-review and iteration until clean or depth limit

### Step 4: Review Turn Completes — Hook Skips

The agent's review turn completes. `turn.completed` fires again. But `$event.turn.tasks.completed` is empty — the only task closed this turn was a review task (filtered out by rule 3). The `if:` guard is falsy, so no autoreply is sent. **No infinite loop.**

### Step 5: Fix Turn Completes — Hook Skips

If `fix --loop` ran a fix iteration, the fix followup task closes. `turn.completed` fires again. `$event.turn.tasks.completed` is empty — the fix task has a review task in its ancestor graph (filtered out by rule 4). **No infinite loop.**

### Step 6: Termination

Same as CLI: clean review, depth limit, or manual stop.

---

## User Experience

### What the User Sees

| Phase | User-visible output | Duration |
|-------|-------------------|----------|
| Work completes | Agent closes task | Instant |
| Review started | "Review started in background" on stderr | Instant |
| Waiting for review | Agent is idle (blocked on `aiki wait`) | Seconds to minutes |
| Review complete (issues) | Fix loop iteration message | Instant |
| Fix in progress | Agent working on fixes | Varies |
| Fix complete | Re-review starts | Instant |
| All clean | "Approved — no issues found" | Instant |

### Observability

```bash
aiki task              # See what's in progress
aiki review list       # See all review tasks for the session
aiki review show <id>  # See details of a specific review
aiki task show <id>    # See task details + sources (iteration chain)
```

---

## Variants

### Self-Review (Default)

The default hook plugin does self-review (the agent reviews its own work via `--start`). To use codex as a separate reviewer instead, override the hook:

```yaml
# .aiki/hooks/aiki/review-loop.yml (codex variant)
name: aiki/review-loop
version: "1"

turn.completed:
  - if: $event.turn.tasks.completed
    then:
      - autoreply: |
          Tasks completed this turn: $event.turn.tasks.completed

          Submit your work for codex review:

          aiki review --fix --agent codex
```

Without `--start`, the review runs as a background task assigned to codex. The agent would need to wait for the review to complete (e.g., via `aiki wait`).

### Pre-Push Gate (Aspirational)

> **Note:** This variant requires flow actions (`review:`, `block:`) and variables (`$review.issues_found`) that don't exist yet. Included to illustrate the design direction, not as an implementable example.

Review as a quality gate before pushing:

```yaml
# .aiki/hooks/aiki/pre-push-review.yml
name: aiki/pre-push-review
version: "1"

shell.permission_asked:
  - if: $event.command | contains("git push")
    then:
      - review:
          agent: codex
      - if: $review.issues_found > 0
        then:
          - block: |
              Cannot push — review found ${review.issues_found} issue(s).
              Run `aiki fix ${review.task_id}` to address findings.
```

### Done-Only Review (Skip Won't-Do)

Built into `$event.turn.tasks.completed` — only tasks with outcome "done" are included. Tasks closed as won't-do are excluded from the variable, so the hook never fires for them. No separate variant needed.

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Agent closes task with no code changes | `$event.turn.tasks.completed` includes it (outcome done) → review runs |
| Multiple work tasks closed in same turn | All appear in `$event.turn.tasks.completed`; single review covers all |
| Agent closes fix task as won't-do | Won't-do outcome → excluded from `$event.turn.tasks.completed` |
| Hook fires after review turn | `$event.turn.tasks.completed` empty (review task filtered) → hook skips |
| Hook fires after fix turn | `$event.turn.tasks.completed` empty (fix task has review in ancestor graph) → hook skips |
| No tasks closed this turn | `$event.turn.tasks.completed` empty → hook skips |
| Task closed as won't-do | Excluded from `$event.turn.tasks.completed` (outcome not "done") |

---

## Implementation Plan

### Phase 1: Turn stamping on task events

Add `turn` and `turn_id` to task lifecycle events:

1. Add `turn: Option<u32>` and `turn_id: Option<String>` to `TaskEvent::Closed`, `Started`, `Stopped`, `CommentAdded` in `cli/src/tasks/types.rs`
2. Update serialization in `cli/src/tasks/storage.rs` — write `turn=N` / `turn_id=<uuid>` fields, parse on read (backward compatible: missing fields default to `None`)
3. In task commands (`cmd_close`, `cmd_start`, `cmd_stop`), load turn context:
   - `find_active_session(cwd)` → session UUID
   - `history::get_current_turn_info(&global_aiki_dir(), &session_uuid)` → turn number
   - `generate_turn_id(session_uuid, turn_number)` → turn_id
   - Graceful fallback: if session detection or turn lookup fails, `turn=None` / `turn_id=None`
4. Tests (unit: roundtrip serialization; integration: stamp correctness)

### Phase 2: `turn.tasks.completed` lazy variable

Add the lazy variable to `turn.completed` events in the flow engine:

1. Register `turn.tasks.completed` as a lazy variable on `turn.completed` events
2. Resolution: query task `Closed` events where `turn_id == $event.turn.id`, then filter:
   - Outcome is "done"
   - Task type is not "review"
   - No review-type task in ancestor graph (walk both `sourced-from` and `subtask-of` links)
3. Return space-separated task IDs (empty string if none)
4. Tests

### Phase 3: Hook plugin

1. **Embed plugin as built-in** — add `aiki/review-loop` to the built-in plugin registry (see `ops/now/default-hooks.md` Phase 2). The plugin is shipped inside the binary via `include_str!()`, resolved at the loader level when no user override exists on disk.
2. Tests

---

## Open Questions

1. **Default hook inclusion** — Should `aiki init` automatically include `aiki/review-loop` in the default hook's `include:` list? Or should users add it manually?

2. **Notification on completion** — Should there be a prominent visual signal when the loop terminates? Currently just the "approved" message from `aiki fix`.
