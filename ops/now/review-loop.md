# Review Loop: End-to-End Workflow

**Date**: 2026-02-11
**Status**: Draft
**Priority**: P2
**Depends on**: `ops/done/review-and-fix.md`

**Related Documents**:
- [Review and Fix Commands](../done/review-and-fix.md) - Core review/fix system (implemented)
- [Review and Fix Non-Task Targets](review-and-fix-files.md) - Spec/implementation review scopes
- [Needs Review Status](../next/needs-review.md) - Close outcome for unreviewed work
- [Default Hooks](default-hooks.md) - Hookfile scaffolding and built-in plugin registry (Layer 2 prerequisite)

---

## Problem

The review-fix cycle exists as individual commands (`aiki review`, `aiki fix`), but there's no integrated workflow that automates the iteration loop. Today a user must manually:

1. Run `aiki review` after work completes
2. Run `aiki fix` to address findings
3. Manually re-review to verify fixes
4. Repeat until clean

This is tedious and error-prone. The user has to remember to re-review, track which iteration they're on, and decide when to stop.

---

## Summary

The review loop has **two layers** that build on the existing `review` and `fix` commands:

1. **CLI primitives** — `aiki fix --loop` and `aiki review --fix` give users and agents explicit control over the review-fix cycle. These are pure Rust implementations with no flow system dependencies.

2. **Hook plugin** — `aiki/review-loop` automates the cycle after every agent turn using the flow system. This builds on the CLI primitives and requires turn stamping on task events plus a `turn.tasks.completed` lazy variable.

The CLI layer ships first and is independently useful. The hook layer adds automation for users who want hands-off review loops.

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Core primitive | `aiki fix --loop` | Loop logic lives in one place; both CLI and hooks can use it |
| Review sugar | `aiki review --fix` adds a subtask | Review command stays a task factory; task system handles sequencing |
| Max iterations | 10 | Generous enough for complex fix chains, tight enough to prevent runaway loops |
| Hook packaging | `aiki/review-loop` plugin, included via `after:` | Composable — users opt in without modifying their existing hooks |
| Won't-do fix in loop | `fix --loop` re-reviews after won't-do fix | Verifies the agent's decision to skip a fix was appropriate (note: won't-do *work* tasks are excluded from hook trigger) |

---

## Layer 1: CLI Primitives

### `aiki fix --loop`

The core loop primitive. Takes a review task ID and iterates until the code is clean or the depth limit is reached.

**Syntax:**

```
aiki fix <review-id> --loop
```

**Algorithm:**

```
fix_loop(review_id, max_iterations=10):
  for iteration in 1..=max_iterations:
    # Step 1: Fix the review's findings
    result = fix(review_id)
    if result == approved:
      return approved  # No issues, loop ends

    followup_id = result.followup_id

    # Step 2: Wait for fix to complete
    wait(followup_id)  # Blocks until fix task closes

    # Step 3: Re-review the original task
    new_review = create_review(original_task_id)
    run_to_completion(new_review)
    review_id = new_review.id

  return max_iterations_reached
```

**Behavior at each step:**

1. Calls existing `fix()` logic — if no comments, prints "approved" and exits
2. If comments found, creates followup task and runs it to completion (blocking)
3. After fix completes, creates a new review of the original task (same scope)
4. Runs review to completion (blocking), then loops back to step 1
5. Depth guard: stops after N iterations (default 10) with a warning

**Execution modes:**

| Flags | Behavior |
|-------|----------|
| `fix <review> --loop` | Blocking — waits for entire loop to finish |
| `fix <review> --loop --async` | Error — async doesn't make sense for `--loop` (use `review --fix --async` instead) |
| `fix <review> --loop --start` | Agent takes over — runs fix in current session, loops in-session |

**The `--start` variant** is the most interesting for agents. The agent fixes issues in its own session (preserving context), then `--loop` handles the re-review and next fix cycle. Each fix iteration reuses the agent's session.

**Output:**

```
## Fix Loop
- **Review:** rvwnmnsmtlvtlsqtyllrtwkqvlrnopqr
- **Iteration:** 1 of 10
- **Issues:** 2

1. Potential null pointer dereference in auth handler
2. Missing error handling in API client

Fixing...

## Fix Loop
- **Review:** xnvprvxypulsxzqnznsxylrzkkqssytt
- **Iteration:** 2 of 10
- **Issues:** 1

1. Error message missing context

Fixing...

## Approved
- **Review:** qrsvtnmwxypulsxzqnznsxylrzkkqsmn
- Review approved - no issues found.
- **Iterations:** 3
```

### `aiki review --fix`

Sugar that creates a review with a fix-loop subtask. The review command stays a task factory — `--fix` just adds one more subtask to the DAG.

**Syntax:**

```
aiki review <task-id> --fix
```

**How it works:**

The `--fix` flag is passed through to the review template via the `options.*` data namespace. The review command sets `options.fix = "true"` in the template data map alongside the existing `scope.*` keys:

```
scope.kind              = "task"
scope.id                = "abc123"
scope.name              = "Task (abc123)"
options.fix             = "true"
```

The review template uses a conditional subtask reference to pull in `aiki/fix/loop`:

```markdown
{% subtask aiki/fix/loop if data.options.fix %}
```

The subtask template at `.aiki/templates/aiki/fix/loop.md` contains the instructions:

```markdown
# Fix Loop

Address all issues found during review and iterate until approved.

aiki fix {{parent.id}} --loop
```

**What it creates:**

```
Review Task (parent)
  ├── Digest         (subtask linked via subtask-of — from {% subtask aiki/review/<kind> %})
  ├── Review         (subtask linked via subtask-of — existing inline subtask)
  └── Fix Loop       (subtask linked via subtask-of — from {% subtask aiki/fix/loop %})
              instructions: aiki fix <parent-id> --loop
```

The fix-loop subtask is blocked by the review subtasks. When the review closes:
- If issues found → fix-loop subtask becomes ready, runs `fix --loop`
- If no issues → fix-loop subtask can detect "approved" and close itself as won't-do

**Composition with execution modes:**

| Flags | Behavior |
|-------|----------|
| `review <task> --fix` | Blocking — waits for review + entire fix loop |
| `review <task> --fix --async` | Returns immediately — review + fix loop run in background |
| `review <task> --fix --start` | Agent takes over the review; fix-loop subtask becomes ready after agent closes review |

The `--start --fix` case is natural: the agent does the review itself, closes it, and the fix-loop subtask appears in their ready queue. No special wiring — the task system handles sequencing.

**Without `--fix`**, the review command works exactly as today (no behavioral change).

### Depth Counting

`fix --loop` maintains a simple iteration counter (hardcoded max of 10). The source chain (`source: task:` links) also provides an audit trail — each review and fix task links to its predecessor, so `aiki task show` reveals the full iteration history.

---

## Layer 2: Hook Plugin

The hook-based approach automates the review loop after every agent turn. It triggers on `turn.completed` — reviewing tasks completed during the turn rather than firing per-task (which would be too noisy).

### Why `turn.completed` Instead of `task.closed`

Triggering on `task.closed` fires a review for every individual task closure — including subtasks, fix tasks, and other intermediate work. This creates noise: multiple reviews per turn, reviews of reviews, and unnecessary churn.

`turn.completed` fires once per agent turn and has native `autoreply:` support. No Event Gap — no engine changes needed for `autoreply:`.

### `turn.tasks.completed` Variable

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

### Turn Stamping on Task Events

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

### The Plugin

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

### Prerequisites

| Component | Status | Needed By |
|-----------|--------|-----------|
| `aiki review` command | Implemented | Both |
| `aiki fix` command | Implemented | Both |
| `aiki fix --loop` | **Not implemented** | CLI (Layer 1) |
| `aiki review --fix` | **Not implemented** | CLI (Layer 1) |
| `autoreply:` on `turn.completed` | Implemented | Hook (Layer 2) |
| Hook composition (`before:`/`after:`) | Implemented | Hook (Layer 2) |
| **Turn stamping on task events** | **Not implemented** | **Hook (Layer 2)** |
| **`turn.tasks.completed` event variable** | **Not implemented** | **Hook (Layer 2)** |
| **Built-in plugin registry** | **Not implemented** | **Hook (Layer 2)** |
| **Default hookfile scaffolding** (`aiki init` creates `.aiki/hooks.yml`) | **Not implemented** | **Hook (Layer 2)** |
| Built-in hook file | **Not created** | Hook (Layer 2) |

### Enabling / Disabling

Users add `aiki/review-loop` to their hookfile's `after:` list (created by `aiki init` — see `ops/now/default-hooks.md`):

```yaml
# .aiki/hooks.yml
name: hooks
version: "1"

after:
  - aiki/review-loop
```

Remove the line to disable. The default hookfile ships with `aiki/review-loop` already enabled.

### Customizing

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

## Iteration Lifecycle (CLI)

When a user or agent runs `aiki review <task> --fix`:

### Step 1: Review Created

```
aiki review <task-id> --fix
```

Creates the review task with digest, review, and fix-loop subtasks. The review runs (assigned to codex by default).

### Step 2: Codex Reviews

The codex agent:
1. Reads the task changes with `aiki task diff`
2. Reviews for bugs, quality, security, performance
3. Adds comments for each issue found
4. Closes the review task

### Step 3: Fix Loop Subtask Fires

The fix-loop subtask becomes ready. It runs `aiki fix <review-id> --loop`:

- If codex found issues → creates followup, runs fix, re-reviews
- If codex found no issues → prints "approved", exits

### Step 4: Loop Iterates

Each iteration:
1. Fix agent addresses the review comments
2. Fix agent closes the followup task
3. `fix --loop` creates a new review of the original task
4. Review runs to completion
5. If issues remain → loop continues
6. If clean → "approved", loop ends

### Step 5: Termination

The loop ends when:
- A review finds zero issues (natural termination)
- Max iterations reached (depth guard)
- User interrupts (Ctrl+C, `aiki task stop`)

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

## Loop Termination

### Natural Termination

The loop terminates when a review finds **zero issues**:

1. `fix --loop` receives the review task ID
2. Calls `fix()` — review has no comments
3. Prints "approved" and exits 0
4. Loop ends

### Depth Guard

`fix --loop` maintains a simple iteration counter. At the limit (default 10):

```
## Fix Loop — Max Iterations Reached
- Reached maximum of 10 iterations without full approval.
- Run `aiki review list` to see review history.
```

### Manual Termination

- **Stop the agent** — Ctrl+C
- **Stop the review task** — `aiki task stop <review-id>`
- **Close as won't-do** — `aiki task close <fix-id> --wont-do --summary "Acceptable as-is"`

---

## Variants

### Self-Review (No Codex)

For users who don't have codex:

```bash
# CLI: agent reviews its own work
aiki review <task-id> --fix --agent claude-code
```

The default hook plugin already does self-review (the agent reviews its own work via `--start`). To use codex as a separate reviewer instead, override the hook:

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
| Review task fails/errors | `fix --loop` logs error and stops |
| Codex is unavailable | Review creation fails; `fix --loop` exits with error |
| Multiple work tasks closed in same turn | All appear in `$event.turn.tasks.completed`; single review covers all |
| Agent closes fix task as won't-do | Won't-do outcome → excluded from `$event.turn.tasks.completed` |
| Network timeout during review | `fix --loop` blocks on review completion; fails on timeout |
| Depth limit reached | Warning message, loop exits |
| `fix --loop` called on non-review task | Error: "Task X is not a review task" (existing validation) |
| `review --fix` with `--start` | Agent does review; fix-loop subtask becomes ready after |
| Hook fires after review turn | `$event.turn.tasks.completed` empty (review task filtered) → hook skips |
| Hook fires after fix turn | `$event.turn.tasks.completed` empty (fix task has review in ancestor graph) → hook skips |
| No tasks closed this turn | `$event.turn.tasks.completed` empty → hook skips |
| Task closed as won't-do | Excluded from `$event.turn.tasks.completed` (outcome not "done") |

---

## Implementation Plan

### Phase 1: `fix --loop` (CLI primitive)

Add `--loop` flag to `aiki fix`:

1. Add clap arg to fix command in `cli/src/main.rs`
2. Implement loop logic in `cli/src/commands/fix.rs`:
   - Wrap existing `run_fix()` in a loop
   - After fix completes, create new review of original task
   - Run review to completion
   - Check for issues, iterate or exit
3. Track original task ID (extract from review's `scope.id`)
4. Hardcoded depth guard (max 10 iterations)
5. Add iteration output messages
6. Tests

### Phase 2: `review --fix` (CLI sugar)

Add `--fix` flag to `aiki review`:

1. Add clap arg to review command
2. Pass `options.fix` through `scope_data` to `create_review_task_from_template()`
3. ~~Add conditional subtask to `aiki/review` template~~ Done: `{% subtask aiki/fix/loop if data.options.fix %}` + `.aiki/templates/aiki/fix/loop.md`
4. Execution mode composes naturally (blocking/async/start)
5. Tests

### Phase 3: Turn stamping + `turn.tasks.completed` event variable

**3a: Turn stamping on task events**

Add `turn` and `turn_id` to task lifecycle events:

1. Add `turn: Option<u32>` and `turn_id: Option<String>` to `TaskEvent::Closed`, `Started`, `Stopped`, `CommentAdded` in `cli/src/tasks/types.rs`
2. Update serialization in `cli/src/tasks/storage.rs` — write `turn=N` / `turn_id=<uuid>` fields, parse on read (backward compatible: missing fields default to `None`)
3. In task commands (`cmd_close`, `cmd_start`, `cmd_stop`), load turn context:
   - `find_active_session(cwd)` → session UUID
   - `history::get_current_turn_info(&global_aiki_dir(), &session_uuid)` → turn number
   - `generate_turn_id(session_uuid, turn_number)` → turn_id
   - Graceful fallback: if session detection or turn lookup fails, `turn=None` / `turn_id=None`
4. Tests (unit: roundtrip serialization; integration: stamp correctness)

**3b: `turn.tasks.completed` lazy variable**

Add the lazy variable to `turn.completed` events in the flow engine:

1. Register `turn.tasks.completed` as a lazy variable on `turn.completed` events
2. Resolution: query task `Closed` events where `turn_id == $event.turn.id`, then filter:
   - Outcome is "done"
   - Task type is not "review"
   - No review-type task in ancestor graph (walk both `sourced-from` and `subtask-of` links)
3. Return space-separated task IDs (empty string if none)
4. Tests

### Phase 4: Hook plugin

1. **Embed plugin as built-in** — add `aiki/review-loop` to the built-in plugin registry (see `ops/now/default-hooks.md` Phase 2). The plugin is shipped inside the binary via `include_str!()`, resolved at the loader level when no user override exists on disk.
2. Tests

### Phase 5: Documentation

Update CLAUDE.md and relevant docs to cover:
- `fix --loop` usage
- `review --fix` usage
- Hook plugin setup (`after: aiki/review-loop`)

---

## Open Questions

1. **Default hook inclusion** — Should `aiki init` automatically include `aiki/review-loop` in the default hook's `after:` list? Or should users add it manually?

2. **Notification on completion** — Should there be a prominent visual signal when the loop terminates? Currently just the "approved" message from `aiki fix`.

3. **Configurable max iterations** — Hardcoded at 10 for now. Add `--max-iterations` flag later if needed.
