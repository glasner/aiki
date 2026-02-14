# Review-Fix Workflow: `fix`, `review --fix`, `build --fix`

**Date**: 2026-02-14
**Status**: Draft
**Priority**: P2
**Depends on**: `ops/done/review-and-fix.md`

**Related Documents**:
- [Review and Fix Commands](../done/review-and-fix.md) - Core review/fix system (implemented)
- [Review Loop Plugin](review-loop-plugin.md) - Hook-based automation (builds on these primitives)

---

## Problem

The review-fix cycle exists as individual commands (`aiki review`, `aiki fix`), but there's no integrated workflow that automates the iteration loop. Today a user must manually:

1. Run `aiki review` after work completes
2. Run `aiki fix` to address findings
3. Manually re-review to verify fixes
4. Repeat until clean

This is tedious and error-prone. The user has to remember to re-review, track which iteration they're on, and decide when to stop.

Similarly, after `aiki build` completes, the user often wants to review and fix the built code. There's no way to chain build → review-fix workflow in a single command.

---

## Summary

The `fix` command loops by default, iterating fix → re-review until clean or depth limit. Workflow flags (`--fix`, `--review`) chain commands together:

1. **`aiki fix <review>`** — Fix → re-review → fix until clean (DEFAULT behavior). Use `--once` to opt out of looping.
2. **`aiki review --fix`** — Review → fix loop (creates review with fix subtask).
3. **`aiki build --fix`** — Build → review → fix loop in a single command.

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Core primitive | `aiki fix` loops by default | "Fix" means "fix completely" — looping is the natural behavior |
| Opt-out flag | `fix --once` | Escape hatch for single-pass fixes (debugging, manual control) |
| Review sugar | `aiki review --fix` adds a fix subtask | Review command stays a task factory; task system handles sequencing |
| Build workflow | `aiki build --fix` | Clear workflow chain: build → review → fix (loop) |
| Max iterations | 10 | Generous enough for complex fix chains, tight enough to prevent runaway loops |
| Won't-do fix in loop | `fix` re-reviews after won't-do fix | Verifies the agent's decision to skip a fix was appropriate |
| Reviewer persistence | Re-reviews use original reviewer | If codex reviewed initially, codex re-reviews; maintains consistency |
| Build review scope | Implementation review of the spec | Validates the whole result against the spec, not just individual diffs |
| Build `--fix` + `--async` | Error | A fix loop requires blocking execution; async + fix is contradictory |
| Build review agent | Default (opposite of builder) | Follows existing `determine_reviewer()` convention |

---

## `aiki fix <review>`

The core fix command. **Loops by default** — iterates fix → re-review until the code is clean or the depth limit is reached.

**Syntax:**

```bash
aiki fix <review-id>          # Fix → re-review → fix until clean (DEFAULT)
aiki fix <review-id> --once   # Fix once, stop (opt-out of loop)
```

**Algorithm:**

```
fix(review_id, once=false, max_iterations=10):
  # Extract original task ID and reviewer from initial review
  original_task_id = review.scope.id
  original_reviewer = review.agent  # Persist the reviewer across iterations
  
  for iteration in 1..=max_iterations:
    # Step 1: Fix the review's findings
    result = fix_once(review_id)
    if result == approved:
      return approved  # No issues, loop ends

    followup_id = result.followup_id

    # Step 2: Wait for fix to complete
    wait(followup_id)  # Blocks until fix task closes

    # If --once flag set, stop here
    if once:
      return done

    # Step 3: Re-review the original task with the SAME reviewer
    new_review = create_review(original_task_id, agent=original_reviewer)
    run_to_completion(new_review)
    review_id = new_review.id

  return max_iterations_reached
```

**Behavior at each step:**

1. Extracts original task ID and reviewer from the initial review task
2. Calls `fix_once()` logic — if no comments, prints "approved" and exits
3. If comments found, creates followup task and runs it to completion (blocking)
4. If `--once` flag set, stops here (single pass)
5. Otherwise, creates a new review of the original task with the **same reviewer** as the initial review
6. Runs review to completion (blocking), then loops back to step 2
7. Depth guard: stops after N iterations (default 10) with a warning

**Execution modes:**

| Flags | Behavior |
|-------|----------|
| `fix <review>` | **Default** — loop until clean (blocking) |
| `fix <review> --once` | Single pass — fix once, stop |
| `fix <review> --async` | Error — async doesn't make sense for fix loop (use `review --fix --async` instead) |
| `fix <review> --start` | Agent takes over — runs fix in current session, loops in-session |

**The `--start` variant** is the most interesting for agents. The agent fixes issues in its own session (preserving context), and the loop handles re-review and next fix cycle. Each fix iteration reuses the agent's session.

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

---

## `aiki review --fix` (already implemented)

> **Status: Existing.** The `--fix` flag, data flow, and conditional subtask are already wired. The only fix needed is updating the loop template content (see Phase 2).

Creates a review with a fix-loop subtask. The review command stays a task factory — `--fix` just adds one more subtask to the DAG.

**Syntax:**

```bash
aiki review <task-id>          # Just review (no auto-fix)
aiki review <task-id> --fix    # Review → fix loop
```

**What already exists in code:**

- `ReviewArgs.fix: bool` — clap flag (`cli/src/commands/review.rs:167-169`)
- `CreateReviewParams.fix: bool` — passed through to template creation (`review.rs:220-229`)
- `options.fix = "true"` set in scope_data (`review.rs:408-409`)
- Review template conditional: `{% subtask aiki/fix/loop if data.options.fix %}` (`review.md:22`)
- Loop template stub: `.aiki/templates/aiki/fix/loop.md` (currently calls `aiki fix {{parent.id}} --loop` — stale flag)

**Template naming (important):**

The review template references `aiki/fix/loop`, **not** `aiki/fix`. This is correct — `aiki/fix.md` is the single-pass fix template (creates nested subtasks per issue). `aiki/fix/loop.md` is the loop subtask that invokes `aiki fix` in loop mode. These are different contracts:

```
.aiki/templates/aiki/fix.md       → Single-pass: agent reads comments, creates subtasks, fixes each
.aiki/templates/aiki/fix/loop.md  → Loop wrapper: calls `aiki fix <parent-id>` (loops by default)
```

**What it creates:**

```
Review Task (parent)
  ├── Digest         (subtask linked via subtask-of — from {% subtask aiki/review/<kind> %})
  ├── Review         (subtask linked via subtask-of — existing inline subtask)
  └── Fix Loop       (subtask linked via subtask-of — from {% subtask aiki/fix/loop %})
              instructions: aiki fix <parent-id>
```

The fix-loop subtask is blocked by the review subtasks. When the review closes:
- If issues found → fix-loop subtask becomes ready, runs `aiki fix <parent-id>` (loops by default)
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

`aiki fix` maintains a simple iteration counter (hardcoded max of 10). The source chain (`source: task:` links) also provides an audit trail — each review and fix task links to its predecessor, so `aiki task show` reveals the full iteration history.

---

## `aiki build --fix`

After `aiki build` completes, automatically runs `aiki review --fix` on the plan task. This is handled entirely in the build command's Rust code — the build template is not modified.

**Syntax:**

```bash
aiki build <spec>              # Just build (no review)
aiki build <spec> --review     # Build → review (no auto-fix)
aiki build <spec> --fix        # Build → review → fix loop
```

### Implementation

#### 1. Add `--fix` and `--review` flags to `BuildArgs`

In `cli/src/commands/build.rs`, add to the `BuildArgs` struct:

```rust
/// Run review after build completes
#[arg(long)]
pub review: bool,

/// Run review-fix loop after build completes (implies --review)
#[arg(long)]
pub fix: bool,
```

#### 2. Validate flag combinations

In `run()`, before dispatching:

```rust
if args.fix && args.run_async {
    return Err(AikiError::InvalidArgument(
        "--fix and --async are incompatible. --fix requires blocking execution.".to_string(),
    ));
}

// --fix implies --review
let review_after = args.review || args.fix;
```

#### 3. Thread the flags through to `run_build_spec` and `run_build_plan`

Both functions get new parameters: `review_after: bool, fix_after: bool`

#### 4. After sync build completes, run review (optionally with --fix)

In both `run_build_spec` and `run_build_plan`, after `task_run()` returns and the build completion output is printed, add the review step:

```rust
if review_after {
    run_build_review(cwd, spec_path, final_plan_id, fix_after)?;
}
```

The `run_build_review` function:

```rust
/// Run review (optionally with fix loop) after a build completes.
///
/// Creates a review scoped to the spec's implementation, optionally
/// including a fix subtask if `with_fix` is true.
fn run_build_review(cwd: &Path, spec_path: &str, plan_id: &str, with_fix: bool) -> Result<()> {
    use super::review::{create_review, CreateReviewParams, ReviewScope, ReviewScopeKind};

    // Create an implementation review scoped to the spec
    let scope = ReviewScope {
        kind: ReviewScopeKind::Implementation,
        id: spec_path.to_string(),
        task_ids: vec![],
    };

    let result = create_review(cwd, CreateReviewParams {
        scope,
        agent_override: None,
        template: None,
        fix: with_fix,  // includes fix subtask if true
    })?;

    // Run the review to completion (blocking)
    let options = TaskRunOptions::new();
    task_run(cwd, &result.review_task_id, options)?;

    // Output completion
    output_build_review_completed(&result.review_task_id, spec_path, with_fix)?;

    Ok(())
}
```

#### 5. Output

After the build output, show the review result:

```
## Build + Review Completed
- **Build ID:** <build-id>
- **Plan ID:** <plan-id>
- **Review ID:** <review-id>
```

Or with `--fix`:

```
## Build + Review + Fix Completed
- **Build ID:** <build-id>
- **Plan ID:** <plan-id>
- **Review ID:** <review-id>
```

---

## Iteration Lifecycle (CLI)

When a user or agent runs `aiki review <task> --fix`:

### Step 1: Review Created

```bash
aiki review <task-id> --fix
```

Creates the review task with digest, review, and fix subtasks. The review runs (assigned to codex by default).

### Step 2: Codex Reviews

The codex agent:
1. Reads the task changes with `aiki task diff`
2. Reviews for bugs, quality, security, performance
3. Adds comments for each issue found
4. Closes the review task

### Step 3: Fix Subtask Fires

The fix subtask becomes ready. It runs `aiki fix <review-id>` (loops by default):

- If codex found issues → creates followup, runs fix, re-reviews (loops)
- If codex found no issues → prints "approved", exits

### Step 4: Fix Loop Iterates

Each iteration:
1. Fix agent addresses the review comments
2. Fix agent closes the followup task
3. `aiki fix` creates a new review of the original task **using the same reviewer** (e.g., codex)
4. Review runs to completion
5. If issues remain → loop continues
6. If clean → "approved", loop ends

### Step 5: Termination

The loop ends when:
- A review finds zero issues (natural termination)
- Max iterations reached (depth guard)
- User interrupts (Ctrl+C, `aiki task stop`)

---

## Loop Termination

### Natural Termination

The loop terminates when a review finds **zero issues**:

1. `aiki fix` receives the review task ID
2. Calls `fix_once()` — review has no comments
3. Prints "approved" and exits 0
4. Loop ends

### Depth Guard

`aiki fix` maintains a simple iteration counter. At the limit (default 10):

```
## Fix Loop — Max Iterations Reached
- Reached maximum of 10 iterations without full approval.
- Run `aiki review list` to see review history.
```

### Manual Termination

- **Stop the agent** — Ctrl+C
- **Stop the fix task** — `aiki task stop <fix-id>`
- **Close as won't-do** — `aiki task close <fix-id> --wont-do --summary "Acceptable as-is"`

---

## Variants

### Self-Review (No Codex)

For users who don't have codex:

```bash
# CLI: agent reviews its own work
aiki review <task-id> --fix --agent claude-code
```

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Review task fails/errors | `aiki fix` logs error and stops |
| Codex is unavailable | Review creation fails; `aiki fix` exits with error |
| Network timeout during review | `aiki fix` blocks on review completion; fails on timeout |
| Depth limit reached | Warning message, loop exits |
| `fix` called on non-review task | Error: "Task X is not a review task" (existing validation) |
| `review --fix` with `--start` | Agent does review; fix subtask becomes ready after |
| `build --fix` with `--async` | Error: incompatible flags |
| `fix --once` on a review with no issues | Prints "approved", exits immediately (no loop) |
| Initial review by codex, re-review fails | Loop preserves codex as reviewer; if codex unavailable on re-review, loop fails |
| Review has no explicit agent set | Re-reviews use default reviewer logic (opposite of fixer) |

---

## Files Changed

| File | Change | Status |
|------|--------|--------|
| `cli/src/commands/fix.rs` | Make looping default behavior, add `--once` flag, loop logic, depth guard, iteration output | **New work** |
| `cli/src/main.rs` | Add `once: bool` to Fix command variant | **New work** |
| `cli/src/commands/build.rs` | Add `--review` and `--fix` flags, validation, `run_build_review()`, output helper | **New work** |
| `cli/src/commands/review.rs` | No changes needed — `--fix` flag and data flow already exist | **Already done** |
| `.aiki/templates/aiki/fix/loop.md` | Update: remove stale `--loop` flag (loop is now default behavior) | **Fix existing** |
| `.aiki/templates/aiki/fix.md` | No changes — single-pass fix template, separate from loop | **No change** |
| `.aiki/templates/aiki/review.md` | No changes — already references `aiki/fix/loop` correctly | **No change** |

---

## Implementation Plan

### Phase 1: `fix` loops by default

Make `aiki fix` loop by default with `--once` opt-out:

1. Add `--once` flag to `cli/src/main.rs` Fix variant (alongside existing `run_async`, `start`, `template`, `agent`)
2. Refactor `cli/src/commands/fix.rs`:
   - Rename current `run_fix()` → `fix_once()` (this is the existing single-pass logic)
   - New `run_fix()` wraps `fix_once()` in a loop:
     - Extract `scope.id` (original task) and reviewer from initial review
     - Call `fix_once()` — if approved, exit
     - If `--once`, stop after first fix
     - Otherwise: create new review of original task with **same reviewer**, run to completion, loop
   - Make `--async` error when looping (async only valid with `--once`)
   - `--start` works as today — agent takes over the fix subtask, loop re-reviews after agent closes
3. Track original task ID (extract from review's `data.scope.id`)
4. Track original reviewer (extract from review task's `assignee` field or `last_session_id`)
5. Hardcoded depth guard (max 10 iterations)
6. Add iteration output messages (iteration N of 10, issue count)
7. Tests (unit: loop termination, depth guard, --once opt-out; reviewer persistence)

### Phase 2: Fix existing `review --fix` wiring

> The `--fix` flag, data flow, and conditional subtask already exist. This phase just fixes the stale loop template.

1. Update `.aiki/templates/aiki/fix/loop.md`: change `aiki fix {{parent.id}} --loop` → `aiki fix {{parent.id}}` (loop is now default, `--loop` flag never existed)
2. Verify end-to-end: `aiki review <task> --fix` creates review with fix-loop subtask, subtask runs `aiki fix` which now loops by default
3. Tests (integration: review --fix creates correct subtask structure)

### Phase 3: `build --review` and `build --fix`

Add workflow flags to `aiki build`:

1. Add `review` and `fix` fields to `BuildArgs` with `#[arg(long)]`
2. Validate `--fix` + `--async` incompatibility
3. `--fix` implies `--review` (set `review_after = args.review || args.fix`)
4. Thread flags through `run_build_spec` and `run_build_plan`
5. Implement `run_build_review()` — creates implementation review with `fix: with_fix`, runs to completion
6. Add output helper (different messages for `--review` vs `--fix`)
7. Tests

---

## What This Does NOT Change

- **Build template** (`.aiki/templates/aiki/build.md`) — untouched
- **Review command internals** — `--fix` flag and data flow already work; `build --fix` uses existing `create_review()` public API
- **Single-pass fix template** (`.aiki/templates/aiki/fix.md`) — unchanged; this is the agent-facing fix instructions (nested subtasks per issue)
- **Review template** (`.aiki/templates/aiki/review.md`) — already has correct `{% subtask aiki/fix/loop %}` conditional
- **Fix command core logic** — The actual fix task creation and execution stays the same; we're just wrapping it in a loop
- **Hook/plugin system** — these are pure CLI primitives (see [Review Loop Plugin](review-loop-plugin.md) for the hook layer)

---

## Open Questions

1. **Configurable max iterations** — Hardcoded at 10 for now. Add `--max-iterations` flag later if needed.

2. **Build review scope** — Implementation review of the spec vs. task review of the plan ID. Implementation review validates the whole result against the spec; task review checks individual diffs. Implementation review seems more useful post-build.

3. **Build reviewer agent policy** — For `build --fix`, the reviewer defaults to `determine_reviewer()` (which picks the opposite of the worker). The existing review command supports `--agent` override. For build, we use the default reviewer and don't add `--review-agent` unless requested — keeps it simple. If the user needs a specific reviewer, they can run `aiki review` separately after build.

## Resolved Questions

4. ~~**Should we support `--review` without `--fix`?**~~ — Yes. `build --review` runs review once, `build --fix` runs review + fix loop.

5. ~~**Template naming for fix loop subtask**~~ — Use `aiki/fix/loop` (already wired in review.md:22). Don't touch `aiki/fix.md` (single-pass fix template with different contract).
