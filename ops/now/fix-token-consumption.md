---
draft: false
---

# Fix Token Consumption

**Date**: 2026-03-23
**Status**: Draft
**Purpose**: Fix broken loop/orchestrator handoff that appears to leave parent finalization and loop tasks alive at the same time, causing extra agent churn and higher token consumption

**Related Files**:
- [cli/src/tasks/templates/core/loop.md](../../cli/src/tasks/templates/core/loop.md) - Loop template that captures async task-run output
- [cli/src/commands/task.rs](../../cli/src/commands/task.rs) - `task run --next-thread` and parent auto-start logic
- [cli/src/tasks/runner.rs](../../cli/src/tasks/runner.rs) - Async task-run output formatting
- [cli/src/commands/loop_cmd.rs](../../cli/src/commands/loop_cmd.rs) - Loop orchestration entry point
- [cli/src/commands/build.rs](../../cli/src/commands/build.rs) - Build orchestrator caller
- [cli/src/commands/fix.rs](../../cli/src/commands/fix.rs) - Fix orchestrator caller

---

## Executive Summary

The most likely cause of the recent token spike is broken orchestration at the tail end of loop-driven workflows.

The loop template currently does this:

```bash
last_id=$(aiki run {{data.target}} --next-thread --lane $lane --async)
```

That assumes `aiki run --async` prints a bare task ID to stdout. It does not. The current implementation emits formatted markdown output instead. As a result:

1. The loop task can capture invalid `last_id` values.
2. `aiki task wait "${wait_ids[@]}" --any` can wait on malformed identifiers.
3. Child sessions may still complete and close subtasks normally.
4. The parent can auto-start for final review/finalization when all subtasks close.
5. The loop/orchestrator task can remain `in_progress`, creating a split-brain state.

This matches observed transcripts where:

- the parent is auto-started for review/finalization
- the loop/orchestrator task is still `in_progress`
- the workflow appears to hang on the last subtask or verification step

That kind of broken handoff can cause extra retries, extra polling, more status chatter, and more Claude context rereads.

The cross-session validation is now strong:

- the stranded loop/orchestrator pattern appears across multiple dates from **2026-03-03** through **2026-03-20**
- it appears in multiple workflow families: `build`, `fix`, and test-plan execution
- it is therefore not a new bug introduced on 2026-03-18
- the March 18 usage spike is more likely an amplification point where an older orchestration bug became more active or more expensive

---

## Problem

### Broken contract between loop template and task run output

The loop template in `core/loop.md` expects a machine-readable task ID:

```bash
last_id=$(aiki run {{data.target}} --next-thread --lane $lane --async)
```

But `task run --async` currently routes through `run_task_async_with_output()` and prints a markdown block:

```md
## Run Started
- **Task:** <id>
- Task started asynchronously.
```

That output is not safe for shell capture into a single task ID variable.

### Why this is dangerous

The loop template uses the captured values as inputs to `aiki task wait ... --any`.
If those are not canonical IDs:

- waiting may fail
- waiting may return unexpectedly
- the loop may think there is nothing valid to wait on
- orchestration may exit or stall without cleanly closing the loop task

### Observed bad state

Real transcripts show this sequence:

1. Last subtask closes
2. Close output says `Parent task ... auto-started for review/finalization`
3. The parent is now `in_progress`
4. The loop task still appears as `Loop: ... [orchestrator]` in `In Progress`

That means parent finalization and loop orchestration are no longer synchronized.

### Cross-session validation

This pattern is not isolated to one epic or one day. It was validated in real transcripts across multiple sessions:

- **2026-03-03**: fix workflow where parent auto-starts and linked loop remains `in_progress`
- **2026-03-07**: multiple test-plan workflows where parent auto-starts and linked loop remains `in_progress`
- **2026-03-08**: test-plan workflow where child completion leaves both epic and loop alive with `Ready (0)`
- **2026-03-10**: fix workflows where parent auto-starts and linked loop remains `in_progress`
- **2026-03-19**: test-plan workflows where the parent auto-starts and the loop remains `in_progress`, including a case where the parent later closes while the loop still remains `in_progress`
- **2026-03-20**: test-plan workflow with the same pattern during final verify/finalization

The strongest repeated failure shapes are:

1. final subtask closes
2. close output says the parent auto-started for review/finalization
3. loop/orchestrator still appears in `In Progress`

and, in stronger cases:

1. parent later closes successfully
2. loop/orchestrator still remains `in_progress`

That second shape proves the loop can outlive even successful parent finalization.

### March 23 test swarm findings

The large March 23 test swarm was not just one runaway loop repeatedly spawning the same child. The transcript evidence shows two overlapping problems:

1. a legitimate large `test-plan.md` implementation epic fanned out into many real worker sessions
2. some of those worker sessions were not cleanly isolated to one task and were carrying additional test or validation work at the same time

Observed session shapes:

- dedicated section workers existed, for example:
  - `Implement task lifecycle tests (sections 1.1-1.5)`
  - `Implement workspace isolation tests (section 7)`
- mixed sessions also existed, for example:
  - `Implement error handling tests (section 9)` while the same session also had `delegation-test-probe` in progress and briefly started the parent `Epic: Test Plan`
  - broader validation/fix sessions that touched multiple generated test files in one transcript, including `test_subtasks.sh`, `test_errors.sh`, `test_lifecycle.sh`, `test_links.sh`, `test_reviews.sh`, and `test_workspace.sh`

This matters because it weakens the assumption that every spawned session contributes only one bounded unit of context. Even when the swarm starts from a valid plan decomposition, mixed worker sessions can accumulate extra task-list chatter, additional test context, and more transcript volume than the orchestration model intends.

### Root of the March 23 swarm

The strongest upstream source identified so far is the older task:

- `mrrposl` — `Test build --fix pipeline: task graph verification`

That task matches `cli/tests/prompts/test_build_review_fix.md`, which explicitly instructs the agent to create `test-plan.md`, run `aiki build test-plan.md`, and later run `aiki build test-plan.md --fix`.

By March 23, `test-plan.md` had already evolved into a large multi-section plan, so later `Decompose: test-plan.md` and `Epic: Test Plan` runs were operating on a much larger artifact than the original simple pipeline check likely intended.

Current read:

- the March 23 swarm was seeded by real `test-plan.md` build/decompose work, not by duplicate session IDs
- the swarm then became more expensive because some worker sessions were mixed with additional test tasks and because loop/orchestrator tail-state bugs kept extra orchestration context alive
- no March 23 session has yet been proven to explicitly restart `mrrposl`; it appears more likely that descendant `test-plan.md` work continued while older upstream validation state remained open in the task graph

---

## Likely Regression

The loop template has used shell capture for a long time. The probable regression is that `task run --async` stopped being machine-readable but the loop template was never updated.

Primary candidate regression point:

- `13ab524` — removed pipe-aware special output handling from `cli/src/output_utils.rs`
- `fee488c` — markdown task output replaced older output formatting

The current read is:

- the orchestration bug itself predates the March 18 spike
- the most plausible underlying regression is older pipe/output-contract drift
- `13ab524` is the strongest candidate for the contract break because it removed special handling for piped output
- `fee488c` may also have contributed by moving task output toward markdown formatting

If the async run command previously emitted a compact parseable ID, and later emitted normal human-facing markdown while the loop template still used shell capture, the loop template would silently become unreliable without any loop-specific code changing.

### Why March 3 is the strongest breakpoint

The first major high-usage regime in the Claude logs starts on **2026-03-02** and peaks on **2026-03-03** and **2026-03-04**.

The key Aiki changes on **2026-03-03** were:

- `3244758` — large workflow/TUI/foundation refactor that touched task handling, hooks, status monitoring, and workflow rendering
- `13ab524` — introduced the `loop` command path, refactored `build`/`fix`/`decompose`, and removed pipe-aware output behavior

The tightest root-cause story is:

1. `3244758` changed a large amount of orchestration-adjacent code and increased workflow complexity
2. `13ab524` introduced the new loop/orchestrator path
3. in the same commit, `output_utils` changed from TTY/non-TTY aware output to unconditional normal stdout output
4. the loop template still relied on shell capture of `task run --async`
5. the machine-readable contract was therefore broken at the same time the new loop path became central to build/fix orchestration

That combination makes **2026-03-03** the most plausible origin point for the stranded orchestrator bug and the first large token-usage spike.

### Before vs after `13ab524`

Before `13ab524`:

- `cli/src/output_utils.rs` had explicit TTY/non-TTY behavior
- machine-readable IDs were emitted to stdout when output was piped
- formatted markdown was sent to stderr in interactive contexts

After `13ab524`:

- `cli/src/output_utils.rs` became a simple unconditional `println!`
- normal human-facing markdown went to stdout by default
- `build` and `fix` switched into the new `run_loop()` orchestration path
- `loop_cmd.rs` created orchestrator tasks linked by `orchestrates`
- the loop template still captured `aiki run ... --async` into `last_id`

That means the same change set both:

- made loop orchestration a first-class runtime path
- and removed the output behavior that shell-driven orchestration depended on

---

## Desired Behavior

For loop/orchestrator workflows:

1. `aiki run <parent> --next-thread --lane <lane> --async` must return a bare canonical task ID in machine-readable mode.
2. The loop must wait on real task IDs only.
3. When all lane sessions are complete, the loop task must close itself exactly once.
4. Parent auto-start for finalization must not race with a still-live loop task.
5. After finalization, the parent and loop must both end in terminal states with no orphaned `in_progress` tasks.

---

## Proposed Fix

### Phase 1: Restore machine-readable async output for orchestrators

Make the loop template explicitly request ID output:

```bash
last_id=$(aiki run {{data.target}} --next-thread --lane $lane --async --output id)
```

If `task run` does not yet support `--output id`, add it.

Requirements:

- stdout: bare canonical task ID only
- human-readable progress: stderr only
- no markdown wrappers in machine-readable mode

Also consider restoring the older invariant more broadly:

- scripted/non-TTY paths should never have to parse human-facing markdown from stdout

### Phase 2: Harden the loop template

Add validation in the template before appending to `wait_ids`:

```bash
if echo "$last_id" | grep -Eq '^[a-z]{32}$'; then
  wait_ids+=("$last_id")
else
  echo "Invalid task id from task run: $last_id" >&2
  exit 1
fi
```

This prevents silent corruption of the wait set.

### Phase 3: Add orchestration invariants in Rust

Add checks around loop completion:

- if all subtasks are closed and the parent auto-starts, the loop must either:
  - close itself immediately, or
  - already be closed

At minimum, add tests that fail if both of these are true at once:

- parent is `in_progress` due to auto-finalization
- loop task is still `in_progress`

Also add a defensive reconciliation path so the loop cannot remain live after the parent reaches terminal completion.

Candidate behavior:

- when all subtasks close and the parent auto-starts, identify any linked `orchestrates` loop task and stop/close it if it is still `in_progress`
- when the parent itself closes successfully, ensure any linked loop/orchestrator cannot remain `in_progress`

### Phase 4: Reduce transcript noise from orchestration output

Even after the contract fix, keep machine-readable outputs minimal:

- IDs on stdout for script/orchestrator paths
- markdown/status banners on stderr or interactive-only paths

This reduces context bloat in Claude sessions that invoke orchestration commands repeatedly.

---

## Implementation Plan

### Phase 1: Add `--output id` support to `task run`

1. Extend `task run` args to accept `--output id`
2. Thread output mode into `run_run()`
3. In async mode, emit bare `handle.task_id` on stdout when `--output id` is requested
4. Keep current markdown output for human-facing default mode

### Phase 2: Update loop template to use machine-readable output

1. Change `core/loop.md` to call `aiki run ... --async --output id`
2. Add shell-side validation for returned IDs
3. Fail fast if the captured value is not a canonical task ID

### Phase 3: Add regression tests

1. Test that `aiki run <parent> --next-thread --async --output id` prints only a canonical ID
2. Test that loop orchestration can start lane sessions and wait on them successfully
3. Test that when the final subtask closes:
   - parent auto-starts for finalization
   - loop does not remain indefinitely `in_progress`
4. Test that no split-brain state remains after successful completion
5. Test that when the parent later closes successfully, any linked loop/orchestrator is no longer `in_progress`
6. Add fixture coverage for representative workflows:
   - build/test-plan epic
   - fix epic
   - review-driven remediation epic

### Phase 4: Audit build/fix callers

1. Confirm all loop-driven orchestrators use the updated template
2. Confirm `build` and `fix` do not introduce alternate non-machine-readable paths
3. Verify review/fix/build end states in real workflows
4. Compare current behavior against the pre-`13ab524` output contract to make sure no other shell-capture call sites were broken by the same change

---

## Validation

### Unit / integration tests

- `task run --next-thread --async --output id` returns exactly one 32-char lowercase ID
- loop template uses only valid IDs in `wait_ids`
- orchestrator closes after all lanes complete
- parent finalization does not overlap with a dangling loop task

### Manual reproduction

1. Create a parent task with 2 or more subtasks
2. Run the loop/orchestrator path
3. Let the final subtask close
4. Verify:
   - parent auto-starts exactly once
   - loop task closes exactly once
   - no extra `in_progress` loop task remains
   - no repeated finalization churn occurs

Repeat the same manual check for:

- a plain test-plan/build flow
- a fix/remediation flow
- a workflow with a final verify/test subtask

Also validate with a mixed-session reproduction:

- one lane worker that only performs a single test section
- one lane worker that also has another in-progress test task or probe
- verify the orchestration layer still converges cleanly and does not leave extra parent/loop/test state alive

### Token-usage smoke check

After the fix:

- run a loop-driven fix/build flow
- inspect Claude transcript volume near the last subtask
- confirm fewer repeated orchestration/status messages
- confirm no stuck finalization/orchestrator split state

Also compare against the earlier usage breakpoint:

- verify whether the fix reduces the March 3-style failure mode, not just the March 18 amplification regime

---

## Open Questions

1. Should `task run --async` default to machine-readable stdout and send human output to stderr, or should that only happen under `--output id`?
2. Should the loop task explicitly close itself when it detects `AllComplete`, even if the parent is auto-started separately?
3. Should `task close` also defensively reconcile linked `orchestrates` tasks, so successful parent finalization cannot leave a dangling loop?
4. Is there another independent tail-state bug in parent auto-start logic, beyond the output contract mismatch?
5. Why was long-lived upstream validation work like `mrrposl` still present in the task graph when later `test-plan.md` execution resumed?
6. Should test-pipeline prompts that create `test-plan.md` use a disposable path or a uniquely named plan file to avoid repeated decomposition of the same shared artifact?

---

## Recommendation

Start with the output contract fix, but do not stop there.

It is narrow, testable, and directly matches the observed failure mode:

- the loop template expects a bare ID
- the command returns formatted markdown

Then add defensive loop reconciliation in the parent close/finalization path.

The transcript evidence shows the loop can remain alive even after the parent closes successfully, so relying only on the loop agent to close itself is too fragile.

At this point, the working hypothesis should be:

- **origin point:** March 3 orchestration/output refactor, especially `13ab524`
- **amplification point:** March 18, likely due to either increased loop/orchestrator exercise or higher Claude sensitivity/cost to transcript churn
