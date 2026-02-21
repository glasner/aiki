# Review-Fix Workflow: `fix`, `review --fix`, `build --fix`

**Date**: 2026-02-14
**Status**: Draft
**Priority**: P2
**Depends on**: 
- `ops/done/review-and-fix.md`
- `ops/now/spawn-tasks.md` (conditional spawning + loop implementation)
- `ops/now/autorun-unblocked-tasks.md` (autorun behavior for blocking links)

**Related Documents**:
- [Review and Fix Commands](../done/review-and-fix.md) - Core review/fix system (implemented)
- [Spawn Tasks](spawn-tasks.md) - Spawning and loop primitives (foundation for this workflow)
- [Review Loop Plugin](review-loop-plugin.md) - Hook-based automation (builds on these primitives)
- [Semantic Blocking Links](semantic-blockers.md) - Foundation for semantic links
- [Autorun Unblocked Tasks](autorun-unblocked-tasks.md) - Automatic task start on blocker completion

**Scope**:
This spec describes how the review-fix workflow USES the spawn and loop primitives defined in spawn-tasks.md. It does NOT implement spawning or loops — that's in spawn-tasks.md.

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

The fix workflow uses **semantic task links + nested loops** for automated review-fix cycles:

**Coordination layer:**
- Review **validates** original task (semantic link)
- Review **spawns** fix task conditionally (via `spawns:` frontmatter if not approved)
- Fix **spawned-by** review (semantic link with autorun)

**Iteration layer:**
- Nested loops via `loop:` frontmatter (inner: fix quality, outer: problem solved)
- `data.loop` metadata (automatic iteration tracking — see spawn-tasks.md)
- Customizable termination conditions
- Loop implementation via spawn mechanism (see spawn-tasks.md)

This creates a thorough, **customizable** review-fix workflow:

1. **`aiki review --fix`** — Creates review (validates original) and fix loop (follows up review)
2. **`aiki build --fix`** — Build → review → fix loops in a single command

**Task structure:**
```
Original Task
  ├── Review Task (validates original)
  └── Fix Task (follows-up review, autorun if issues found)
      ├── Iteration 1 (created from aiki/fix template)
      │   ├── Fix once (aiki/fix/once)
      │   ├── Quality loop (aiki/fix/quality)
      │   │   └── Inner iterations...
      │   └── Re-review original
      ├── Iteration 2 (spawned by loop:)
      └── Iteration 3...
```

**Benefits:**
- Fix stays as child of original task (preserves current hierarchy)
- Conditional creation: no fix spawned if review approved (via `spawns` mechanism)
- Semantic links: queryable, self-documenting task graph

---

## Nested Loop Rationale

**Why two loops instead of one?**

A single loop (fix → re-review original) can propagate low-quality fixes:
- Fix introduces new bugs
- Fix is incomplete or incorrect
- Fix has style/quality issues

The fix itself becomes part of the codebase, so it deserves independent review before we check if it solved the original problem.

**Two-gate approach:**

1. **Inner loop (fix quality)** — "Is this fix well-implemented?"
   - Reviews the fix task's changes in isolation
   - Catches bugs in the fix, incomplete implementations, poor approaches
   - Reviewer: Opposite of fixer (independent perspective)
   - Iterates until fix is clean

2. **Outer loop (effectiveness)** — "Did the fix solve the original problem?"
   - Reviews the original task after fix is applied
   - Verifies fix addressed all original issues
   - Catches cases where fix was clean but didn't solve the problem
   - Reviewer: Original reviewer (consistency)
   - Iterates until original task is clean

**Benefits:**
- Higher quality fixes (two independent reviews)
- Prevents cascading issues (bad fix caught before re-reviewing original)
- Clear separation of concerns (implementation quality vs. problem solving)
- Natural exit points (inner: fix clean, outer: problem solved)

**Cost:**
- More reviews per outer iteration (1 original review + N fix reviews)
- Slightly longer iteration times
- More complex implementation

The trade-off favors quality — better to catch issues early in the fix than to discover them later in the outer loop or in production.

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Coordination** | Semantic links (`validates`, `follows-up`) | Express domain relationships, queryable, self-documenting |
| Review link | `validates` | Review validates original task (semantic, not just dependency) |
| Fix link | `follows-up` with `autorun` | Fix follows up review, auto-runs when spawned |
| Fix task parent | Original task (not review) | Preserves existing hierarchy, fix is child of task being fixed |
| **Iteration** | `loop:` frontmatter in templates | Declarative, customizable via template overrides |
| Loop primitive | `loop: {until, data}` in frontmatter | Clean syntax, task system spawns next iteration |
| Loop metadata | Auto-generated `data.loop` | System provides index, iteration |
| Loop structure | Nested (outer + inner) | Two quality gates: fix must be clean (inner), and fix must solve problem (outer) |
| Default max outer | 10 iterations | Generous for complex fix chains; customizable via template data |
| Default max inner | 5 iterations per fix | Enough to refine a fix; customizable via template data |
| Inner loop reviewer | Opposite of fixer (in template) | Independent review of fix quality (e.g., if claude-code fixes, codex reviews fix) |
| Outer loop reviewer | Original reviewer (persisted in template data) | Consistency across iterations (if codex found original issues, codex verifies fixes) |
| Termination conditions | Defined in `loop.until` | Customizable per template (approved, score threshold, manual override) |
| Build review scope | Implementation review of the spec | Validates the whole result against the spec, not just individual diffs |
| Build `--fix` + `--async` | Allowed (task-based) | Task system handles async execution, no command blocking |

---

## Semantic Task Links

The review-fix workflow uses two new link types to coordinate tasks:

### `validates` Link

Review task validates another task:

```rust
add_link(review_task.id, LinkType::Validates {
    task_id: original_task.id,
    autorun: true,  // Start review automatically
});
```

**Semantics:**
- This review checks the quality/correctness of that task
- Creates queryable relationship: "What validates task X?"
- Bidirectional: task shows "validated by" reviews

**Usage:**
```bash
# Show what validates this task
aiki task show <task-id> --validators

# Create review that validates a task
aiki review <task-id>  # Automatically creates validates link
```

### `spawned-by` Link

Fix task spawned by a review:

```rust
// On the spawned task (created automatically)
add_link(fix_task.id, LinkType::SpawnedBy {
    task_id: review_task.id,
    autorun: true,  // Spawned task auto-starts
});
```

**Semantics:**
- This task was conditionally created by that task
- Links child back to spawning parent
- Provenance: answers "what created this task?"
- Automatically runs if `autorun: true`

**When review closes:**
1. Evaluate `spawns:` conditions in review's frontmatter
2. For each condition that is true → instantiate template
3. Create spawned task with `spawned-by` link
4. If `autorun: true` → start spawned task

**Usage:**
```bash
# Show what spawned this task
aiki task show <fix-id>  # Shows spawned-by link

# Show what a task spawned
aiki task show <review-id> --spawned

# Create review that conditionally spawns fix
aiki review <task> --fix  # Review template has spawns: config
```

### Link Types Summary

```rust
enum LinkType {
    Validates {
        task_id: TaskId,
        autorun: bool,
    },
    SpawnedBy {
        task_id: TaskId,  // The task that spawned this one
        autorun: bool,
    },
    // ... existing links (BlockedBy, Source, etc.)
}
```

**Note:** The `spawns:` configuration lives in task frontmatter (see [spawn-tasks.md](spawn-tasks.md)), not as a link type. Only the resulting `spawned-by` link is stored.

---

## Loop Templates

The fix workflow is implemented via **task templates** that declare loop behavior in frontmatter. Loops handle iteration, links handle coordination.

**Three templates:**
- **`aiki/fix`** — Main template with outer loop (looping fix workflow)
- **`aiki/fix/quality`** — Inner loop for fix quality checking
- **`aiki/fix/once`** — Single-pass fix (no loop)

**Runtime override:** Create task with `--once` parameter to prevent loop config from being copied (forces single-pass even from looping template).

### `.aiki/templates/aiki/fix.md`

Main fix template with outer loop (fix → review fix quality → re-review original).

```yaml
---
template: aiki/fix
loop:
  until: subtasks[2].approved or data.loop.iteration >= data.max_outer
  data: {}  # No manual iteration tracking - system provides data.loop
---

# Fix Iteration {{data.loop.iteration | default: 1}}/{{data.max_outer | default: 10}}

Fixing issues from review {{data.original_review}}.

## Instructions

1. **Fix the issues:**
   {% subtask aiki/fix/once with review={{data.original_review}} %}

2. **Ensure fix quality (inner loop):**
   {% subtask aiki/fix/quality with data={
     fix_task: "{{subtasks[0].followup_id}}",
     max_inner: {{data.max_inner | default: 5}}
   } %}

3. **Re-review original task:**
   {% subtask aiki/review with scope=task:{{data.original_review.scope.id}} %}

---

**Status:** Iteration {{data.loop.iteration | default: 1}} of {{data.max_outer | default: 10}}
```

**Loop semantics:**
- When task closes, evaluate `loop.until` condition
- If false → spawn next iteration with `loop.data` merged, auto-increment `data.loop` metadata
- If true → loop terminates
- Task system automatically provides `data.loop.{index, iteration}`

**Runtime override:**
- Create task with `once: true` parameter → `loop_config` not copied to task
- Runs once even though template has loop config
- Implementation: detect `loop_disabled` in template data, skip setting `task.loop_config`

### Loop Metadata (`data.loop`)

The task system automatically provides loop metadata to every iteration task:

```yaml
data:
  loop:
    index: 0           # Current iteration (0-indexed)
    iteration: 1       # Current iteration (1-indexed)
```

**Usage in templates:**
- `{{data.loop.iteration}}` — Display iteration number (1, 2, 3...)
- `data.loop.iteration >= data.max_outer` — Loop termination condition

**Benefits:**
- No manual iteration tracking in `loop.data`
- Consistent with Liquid/Jinja loop variables
- Automatic increment by task system
- Available in both template body and `loop.until` conditions

### `.aiki/templates/aiki/fix/quality.md`

Inner loop: Review fix quality and refine until clean.

```yaml
---
template: aiki/fix/quality
loop:
  until: subtasks[0].approved or data.loop.iteration >= data.max_inner
  data:
    fix_task: "{{subtasks[0].followup_id}}"
---

# Fix Quality Loop {{data.loop.iteration}}/{{data.max_inner}}

Reviewing fix task {{data.fix_task}} for quality.

## Instructions

1. **Review the fix:**
   {% subtask aiki/review with scope=task:{{data.fix_task}} %}

2. **Fix issues if found:**
   {% subtask aiki/fix/once with review={{subtasks[0].id}} if subtasks[0].has_comments %}

---

**Status:** Inner iteration {{data.loop.iteration}} of {{data.max_inner}}
```

**Loop semantics:**
- After review completes, check if approved
- If not approved and under max → spawn next iteration with updated `fix_task` and incremented `data.loop`
- If approved or max reached → exit inner loop

### `.aiki/templates/aiki/fix/once.md`

Single-pass fix (no loop) — agent reads review comments and creates fix subtasks.

```yaml
---
template: aiki/fix/once
# No loop: config - this template never loops
---

# Fix Issues from Review {{data.review}}

## Instructions

Read review comments and create subtasks to fix each issue.

[Agent instructions for addressing review findings...]
```

**Usage:**
- Called by `aiki/fix` template (outer loop needs single-pass fix per iteration)
- Called by `aiki/fix/quality` template (inner loop needs single-pass fix for fix-the-fix)
- Can be called directly: `aiki fix <review-id>` (command creates task with this template)

### Customization Examples

**Change iteration limits:**
```bash
aiki review <task> --fix --data '{"max_outer": 20, "max_inner": 3}'
```

**Custom termination condition:**

Override `.aiki/templates/aiki/fix/outer-iteration.md`:
```yaml
---
loop:
  until: subtasks[2].score >= 90 or data.loop.iteration >= data.max_outer
---
```

**No inner loop (single-pass fixes):**

Create custom template without inner-loop subtask:
```yaml
---
loop:
  until: subtasks[1].approved or data.loop.iteration >= data.max_outer
---
# Fix Iteration {{data.loop.iteration}}/{{data.max_outer}}

## Instructions
1. {% subtask aiki/fix with review={{data.original_review}}, once=true %}
2. {% subtask aiki/review with scope=task:{{data.original_review.scope.id}} %}
```

**Custom reviewer selection:**

Override template to use specific agent:
```yaml
{% subtask aiki/review with scope=task:{{data.fix_task}}, agent=codex-security %}
```

---

## `aiki review --fix`

Creates a review task (validates original) and a fix loop task (follows-up review).

**Syntax:**

```bash
aiki review <task-id>          # Just review (no auto-fix)
aiki review <task-id> --fix    # Review + fix loop
```

**What it creates:**

```rust
// 1. Create review task (validates original)
let review_task = create_review(scope, ...);
add_link(review_task.id, LinkType::Validates {
    task_id: original_task_id,
    autorun: true,
});

// 2. Create fix task (child of original, follows up review)
let fix_task = create_task_with_parent(
    original_task_id,  // Parent is the original task
    "aiki/fix",        // Template with loop: config
    data={
        original_review: review_task.id,
        max_outer: 10,
        max_inner: 5,
    },
    once: false,  // Copy loop config from template (default)
);
add_link(fix_task.id, LinkType::FollowsUp {
    task_id: review_task.id,
    autorun: true,  // Auto-start when spawned
});
```

**Task structure:**

```
Original Task
  ├── Review Task (validates original)
  └── Fix Task (follows-up review)
      ├── Iteration 1
      │   ├── Fix once (aiki/fix/once)
      │   ├── Quality loop (aiki/fix/quality)
      │   └── Re-review original
      ├── Iteration 2 (spawned by loop:)
      └── Iteration 3...
```

**When review closes:**
1. If review approved → fix loop auto-closes as won't-do
2. If review has issues → fix loop starts automatically

**Composition with execution modes:**

| Flags | Behavior |
|-------|----------|
| `review <task> --fix` | Blocking — waits for review + entire fix loop |
| `review <task> --fix --async` | Returns immediately — review + fix loop run in background |
| `review <task> --fix --start` | Agent does review; fix loop auto-starts when review closes |

**Without `--fix`**, the review command works exactly as today (no behavioral change).

### Loop Tracking

The task graph naturally tracks iteration history:
- Each outer iteration is a separate task (with `source: task:<prev-iteration>`)
- Each inner iteration is a separate task (with `source: task:<prev-inner>`)
- `aiki task tree` shows the full loop structure
- `aiki task show <iteration-id>` reveals iteration number and data

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
// --fix implies --review
let review_after = args.review || args.fix;
```

Note: `--fix` and `--async` are now compatible since loops are task-based.

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

## Iteration Lifecycle

When a user or agent runs `aiki review <task> --fix`:

### Step 1: Review Created

```bash
aiki review <task-id> --fix
```

Creates the review task with digest, review, and fix-loop subtasks. The review runs (assigned to codex by default).

### Step 2: Codex Reviews

The codex agent:
1. Reads the task changes with `aiki task diff`
2. Reviews for bugs, quality, security, performance
3. Adds comments for each issue found
4. Closes the review task

### Step 3: Fix Loop Starts

The fix-loop subtask becomes ready and spawns the first outer iteration task.

### Step 4: Outer Iteration Executes

The outer iteration task has three subtasks:
1. **Fix original issues** — `aiki fix <review-id> --once` creates fix task
2. **Inner loop** — Review and refine the fix until clean (see Step 5)
3. **Re-review original** — Verify fix solved the problem

### Step 5: Inner Loop Iterates

The inner loop task reviews the fix and spawns next iteration if needed:

1. Review the fix task for quality
2. If review approved → inner loop terminates
3. If review has issues:
   - Create fix task for the issues
   - Task closes, evaluates `loop.until` condition
   - If under max iterations → spawns next inner iteration with new fix_task
   - If at max → terminates with best-effort fix

### Step 6: Outer Loop Continues

After inner loop completes:
1. Original task is re-reviewed
2. Outer iteration task closes, evaluates `loop.until` condition
3. If original still has issues and under max → spawns next outer iteration
4. If original approved or at max → loop terminates

### Step 7: Termination

**Natural termination:** Original task review is approved  
**Depth guard:** Max iterations reached  
**Manual:** User stops task

---

## Loop Termination

### Natural Termination

**Outer loop** terminates when `loop.until` evaluates to true:

```yaml
loop:
  until: subtasks[2].approved or data.iteration >= data.max_outer
```

When original task review is approved or max iterations reached, no next iteration spawns.

**Inner loop** terminates when `loop.until` evaluates to true:

```yaml
loop:
  until: subtasks[0].approved or data.iteration >= data.max_inner
```

When fix review is approved or max iterations reached, no next iteration spawns.

### Depth Guards

Loop templates declare max iterations in `loop.until` condition:

**Outer:** `data.iteration >= data.max_outer` (default 10)  
**Inner:** `data.iteration >= data.max_inner` (default 5)

Users can customize via template data:
```bash
aiki review <task> --fix --data '{"max_outer": 20, "max_inner": 3}'
```

### Manual Termination

- **Stop iteration task** — `aiki task stop <iteration-id>`
- **Stop fix-loop task** — `aiki task stop <fix-loop-id>` (prevents new iterations)
- **Close as won't-do** — `aiki task close <iteration-id> --wont-do` (loop condition prevents spawn)

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
| Review task fails/errors | Fix-loop task cannot start (dependency failed) |
| Codex is unavailable | Review creation fails; fix loop never starts |
| Network timeout during review | Review subtask fails; outer iteration cannot complete |
| Outer depth limit reached | `loop.until` evaluates to true, no next iteration spawned |
| Inner depth limit reached | `loop.until` evaluates to true, inner loop exits, outer continues |
| Fix task review fails/errors | Inner loop cannot evaluate properly; depends on error handling |
| Review has no issues | Fix-loop task can check and close as won't-do, or subtask creation is conditional |
| `review --fix` with `--start` | Agent does review; fix-loop subtask becomes ready after |
| `build --fix` with `--async` | Allowed (task-based loops support async) |
| Initial review by specific agent | Template data can preserve reviewer, or use default logic |
| Fix reviewer unavailable | Inner loop review creation fails; iteration task errors |
| Fix is clean on first try | Inner loop `loop.until` true on first iteration, exits immediately |
| Fix introduces new issues in original | Outer loop spawns next iteration with new fix |
| User overrides max iterations to 0 | `loop.until` immediately true, no iterations spawn |
| User sets very high max (100+) | Loop continues until approved or manual termination |

---

## Files Changed

| File | Change | Status |
|------|--------|--------|
| **Core task system** | | |
| Link types | Add `Validates` and `FollowsUp` link types with `autorun` field | **New work** |
| Task close handler | Handle spawning and autorun triggers | **New work** |
| Template parser | Add `loop:` frontmatter support | **New work** |
| Loop handler | Evaluate `loop.until` conditions, spawn next iteration, inject `data.loop` metadata | **New work** |
| Condition evaluator | Simple expression evaluator for conditions (subtasks[N].approved, data comparisons) | **New work** |
| Task creation | Support `once` parameter to suppress loop config from template | **New work** |
| **Commands** | | |
| `cli/src/commands/review.rs` | When `--fix` flag: create fix task with `follows-up` link and `once` parameter | **Update existing** |
| `cli/src/commands/build.rs` | Add `--review` and `--fix` flags, `run_build_review()` | **New work** |
| **Templates** | | |
| `.aiki/templates/aiki/fix.md` | Outer loop template with `loop:` frontmatter (replaces fix/loop) | **Update existing** |
| `.aiki/templates/aiki/fix/quality.md` | Inner loop template with `loop:` frontmatter | **New file** |
| `.aiki/templates/aiki/fix/once.md` | Single-pass fix template (no loop) | **Rename from aiki/fix.md** |

---

## Prerequisites

- **Conditional task spawning + loops** (`spawn-tasks.md`): Implements `spawns:` frontmatter, `spawned-by` link type, and `loop:` syntactic sugar
- **Unified expression evaluation** (`rhai-for-conditionals.md`): Required for array indexing (`subtasks[2].approved`) and complex conditions in `loop.until` and spawn condition expressions

## Implementation Plan

**NOTE:** Phases 1-2 are implemented in `spawn-tasks.md`. This spec only describes Phase 3 (applying those primitives to the review-fix workflow).

### Phase 1: Fix loop templates

Create templates that use spawn + loop primitives from `spawn-tasks.md`:

1. **`.aiki/templates/aiki/fix.md`** — Main template with outer loop
   - Uses `loop:` frontmatter (implemented in spawn-tasks.md)
   - Spawns itself until `subtasks[2].approved` or max iterations

2. **`.aiki/templates/aiki/fix/quality.md`** — Inner loop template
   - Uses `loop:` frontmatter for quality iteration
   - Spawns itself until fix is approved

3. **`.aiki/templates/aiki/fix/once.md`** — Single-pass fix (rename from existing `aiki/fix.md`)
   - No loop, single iteration

4. **Update `.aiki/templates/aiki/review.md`:**
   - Add `spawns:` frontmatter section:
     ```yaml
     spawns:
       - template: aiki/fix
         condition: "!this.approved"
         autorun: true
     ```
   - Remove any old fix subtask creation logic

5. **Tests:**
   - End-to-end: `aiki review --fix` → review approves → no fix spawned
   - End-to-end: review has issues → fix spawns and starts, loops until clean
   - Unit: spawn condition evaluation (approved=false triggers spawn)
   - Integration: nested loops (outer + inner) work correctly

### Phase 2: `build --review` and `build --fix`

Add workflow flags to `aiki build`:

1. Add `--review` and `--fix` flags to `BuildArgs`
2. Implement `run_build_review()` using Phase 1+2 primitives
3. Tests

---

## What This Does NOT Change

- **Build template** (`.aiki/templates/aiki/build.md`) — untouched
- **Review template** (`.aiki/templates/aiki/review.md`) — gets `spawns:` config added, but core content unchanged
- **Fix command** — `aiki fix <review>` may become redundant (review templates spawn fixes automatically)

---

## Open Questions

1. **Condition expression language** — Use existing library (rhai, cel) or build simple parser? Need: boolean ops, comparisons, field access.

2. **Task property access in conditions** — How to expose task properties for `loop.until` evaluation?
   - `subtasks[N].approved` — needs task state inspection
   - `subtasks[N].has_comments` — needs comment counting
   - `subtasks[N].score` — if we add review scores later

3. **Loop data merge behavior** — When spawning next iteration:
   - Full merge: `next_data = task.data.merge(loop.data)` (keeps all fields, updates specified)
   - Replace only specified: only update fields in `loop.data`
   - Current design: full merge + auto-update `data.loop` metadata

4. **Error handling in loop conditions** — What if `loop.until` evaluation fails?
   - Subtask doesn't exist (index out of bounds)
   - Field doesn't exist on task
   - Type mismatch in comparison
   - Treat as "condition false" and continue loop? Or treat as "condition true" and stop?

5. **Conditional subtask creation** — Templates show `{% subtask ... if condition %}`. Does this already exist?
   - If yes: inner loop can conditionally create fix subtask only if review has issues
   - If no: need to implement or always create subtask (may be won't-do)

6. **Build review scope** — Implementation review of the spec vs. task review of the plan ID. Implementation review validates the whole result against the spec; task review checks individual diffs. Implementation review seems more useful post-build.

7. **Won't-do handling** — If iteration task closes as won't-do, does `loop.until` still evaluate? Or does task status override loop logic?

## Resolved Questions

4. ~~**Should we support `--review` without `--fix`?**~~ — Yes. `build --review` runs review once, `build --fix` runs review + fix loop.

5. ~~**Template naming for fix loop subtask**~~ — Use `aiki/fix/loop` (already wired in review.md:22). Don't touch `aiki/fix.md` (single-pass fix template with different contract).
