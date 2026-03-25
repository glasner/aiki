<<<<<<< conflict 1 of 1
+++++++ xkvpuxvu 276ad1de (rebased revision)
%%%%%%% diff from: vyrrvpvq 5cc88f0f "[aiki]" (rebased revision)
\\\\\\\        to: ltqontsp 3e9b66f4 "[aiki]" (parents of rebased revision)
 # Review-Fix Workflow: `fix`, `review --fix`, `build --fix`
 
 **Date**: 2026-02-14
 **Status**: Done
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
 
 **Implementation Notes (2026-02-25)**:
 The implementation uses `spawns:` frontmatter config instead of the originally-designed `loop:` frontmatter sugar. This is simpler — the spawn mechanism handles both conditional task creation and loop iteration via self-referencing spawn configs. Key deviations from original design:
 - **No `loop:` frontmatter**: Outer/inner loops use `spawns:` with self-referencing templates and `data.loop_index` tracking instead of `data.loop.*` metadata
 - **No explicit `validates`/`follows-up` links**: The `scoped-to` (auto-materialized from `scope.id` data) and `spawned-by` (created by spawn engine) links provide equivalent queryability without the blocking-semantics mismatch
 - **Fix as subtask of review**: The spawned fix is a subtask of the review (via `subtask:` in spawns config), not a sibling under the original task. This is simpler and keeps the review→fix relationship explicit in the task hierarchy
 
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
 | Loop metadata | Auto-generated `data.loop` | System provides index, index1, first, last, length |
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
   until: subtasks[2].approved or data.loop.index1 >= data.max_outer
   data: {}  # No manual iteration tracking - system provides data.loop
 ---
 
 # Fix Iteration {{data.loop.index1 | default: 1}}/{{data.max_outer | default: 10}}
 
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
 
 **Status:** Iteration {{data.loop.index1 | default: 1}} of {{data.max_outer | default: 10}}
 {% if data.loop.first %}First attempt at fixing issues{% endif %}
 ```
 
 **Loop semantics:**
 - When task closes, evaluate `loop.until` condition
 - If false → spawn next iteration with `loop.data` merged, auto-increment `data.loop` metadata
 - If true → loop terminates
 - Task system automatically provides `data.loop.{index, index1, first, last, length}`
 
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
     index1: 1          # Current iteration (1-indexed)
     first: true        # Is this the first iteration?
     last: false        # Is this the last iteration? (unknown until termination)
     length: null       # Total iterations (unknown for dynamic loops)
 ```
 
 **Usage in templates:**
 - `{{data.loop.index1}}` — Display iteration number (1, 2, 3...)
 - `data.loop.index1 >= data.max_outer` — Loop termination condition
 - `{% if data.loop.first %}...{% endif %}` — Special behavior on first iteration
 
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
   until: subtasks[0].approved or data.loop.index1 >= data.max_inner
   data:
     fix_task: "{{subtasks[0].followup_id}}"
 ---
 
 # Fix Quality Loop {{data.loop.index1}}/{{data.max_inner}}
 
 Reviewing fix task {{data.fix_task}} for quality.
 
 ## Instructions
 
 1. **Review the fix:**
    {% subtask aiki/review with scope=task:{{data.fix_task}} %}
 
 2. **Fix issues if found:**
    {% subtask aiki/fix/once with review={{subtasks[0].id}} if subtasks[0].has_comments %}
 
 ---
 
 **Status:** Inner iteration {{data.loop.index1}} of {{data.max_inner}}
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
   until: subtasks[2].score >= 90 or data.loop.index1 >= data.max_outer
 ---
 ```
 
 **No inner loop (single-pass fixes):**
 
 Create custom template without inner-loop subtask:
 ```yaml
 ---
 loop:
   until: subtasks[1].approved or data.loop.index1 >= data.max_outer
 ---
 # Fix Iteration {{data.loop.index1}}/{{data.max_outer}}
 
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
-| Link types (`cli/src/tasks/graph.rs`) | `validates` and `follows-up` link types with blocking semantics | **Done** (existed via semantic-blockers spec) |
-| Spawn engine (`cli/src/tasks/spawner.rs`) | Evaluate `spawns:` conditions, build Rhai scope, execute spawn actions | **Done** |
-| Spawn config (`cli/src/tasks/templates/spawn_config.rs`) | `SpawnEntry`, `SpawnTaskConfig` types for `spawns:` frontmatter | **Done** |
-| Expression evaluator (`cli/src/expressions.rs`) | Rhai-based evaluator for conditions (subtasks.slug.approved, data comparisons) | **Done** |
-| Task close handler (`cli/src/commands/task.rs`) | `execute_spawn_action()` — creates tasks/subtasks on close, adds `spawned-by` links, depth guard | **Done** |
+| Link types | Add `Validates` and `FollowsUp` link types with `autorun` field | **New work** |
+| Task close handler | Handle spawning and autorun triggers | **New work** |
+| Template parser | Add `loop:` frontmatter support | **New work** |
+| Loop handler | Evaluate `loop.until` conditions, spawn next iteration, inject `data.loop` metadata | **New work** |
+| Condition evaluator | Simple expression evaluator for conditions (subtasks[N].approved, data comparisons) | **New work** |
+| Task creation | Support `once` parameter to suppress loop config from template | **New work** |
 | **Commands** | | |
-| `cli/src/commands/review.rs` | `--fix` flag: sets `data.options.fix` on review task, which triggers `spawns:` config | **Done** |
-| `cli/src/commands/build.rs` | `--review` and `--fix` flags, `run_build_review()` creates code review post-build | **Done** |
-| `cli/src/commands/fix.rs` | Creates fix tasks from review comments, supports task/spec/code scopes | **Done** |
+| `cli/src/commands/review.rs` | When `--fix` flag: create fix task with `follows-up` link and `once` parameter | **Update existing** |
+| `cli/src/commands/build.rs` | Add `--review` and `--fix` flags, `run_build_review()` | **New work** |
 | **Templates** | | |
-| `.aiki/templates/aiki/review.md` | Added `spawns:` config to conditionally spawn fix when `not approved and data.options.fix` | **Done** |
-| `.aiki/templates/aiki/fix.md` | Outer loop template with `spawns:` config (self-referencing iteration) | **Done** |
-| `.aiki/templates/aiki/fix/quality.md` | Inner loop template with `spawns:` config for fix quality review | **Done** (new file) |
-| `.aiki/templates/aiki/fix/once.md` | Single-pass fix template (no loop) | **Done** (new file) |
+| `.aiki/templates/aiki/fix.md` | Outer loop template with `loop:` frontmatter (replaces fix/loop) | **Update existing** |
+| `.aiki/templates/aiki/fix/quality.md` | Inner loop template with `loop:` frontmatter | **New file** |
+| `.aiki/templates/aiki/fix/once.md` | Single-pass fix template (no loop) | **Rename from aiki/fix.md** |
 
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
%%%%%%% diff from: ltqontsp 5d0a97e5 "[aiki]" (rebase destination)
\\\\\\\        to: zskxxztn 623c41f6 "[aiki]" (rebased revision)
 ---
 status: draft
 ---
 
 # Review-Fix Workflow: `fix`, `review --fix`, `build --fix`
 
 **Date**: 2026-02-14
 **Status**: Done
 **Priority**: P2
 **Depends on**:
 - `ops/done/review-and-fix.md`
 - `ops/done/spawn-tasks.md` (conditional spawning + loop implementation)
 - `ops/done/autorun-unblocked-tasks.md` (autorun behavior for blocking links)
 - `ops/now/review-status-helpers.md` (review data fields: `data.approved`, `data.issue_count`)
 
 **Related Documents**:
 - [Review and Fix Commands](../done/review-and-fix.md) - Core review/fix system (implemented)
 - [Spawn Tasks](../done/spawn-tasks.md) - Spawning and loop primitives (foundation for this workflow)
 - [Review Loop Plugin](review-loop-plugin.md) - Hook-based automation (builds on these primitives)
 - [Semantic Blocking Links](../done/semantic-blockers.md) - Foundation for semantic links
 - [Autorun Unblocked Tasks](../done/autorun-unblocked-tasks.md) - Automatic task start on blocker completion
 
 **Scope**:
 This spec describes how the review-fix workflow USES the spawn and loop primitives defined in ../done/spawn-tasks.md. It does NOT implement spawning or loops — that's in ../done/spawn-tasks.md.
 
 **Implementation Notes (2026-02-25)**:
 The implementation uses `spawns:` frontmatter config instead of the originally-designed `loop:` frontmatter sugar. This is simpler — the spawn mechanism handles both conditional task creation and loop iteration via self-referencing spawn configs. Key deviations from original design:
 - **No `loop:` frontmatter**: Outer/inner loops use `spawns:` with self-referencing templates and `data.loop_index` tracking instead of `data.loop.*` metadata
 - **No explicit `validates`/`follows-up` links**: The `scoped-to` (auto-materialized from `scope.id` data) and `spawned-by` (created by spawn engine) links provide equivalent queryability without the blocking-semantics mismatch
 - **Fix as subtask of review**: The spawned fix is a subtask of the review (via `subtask:` in spawns config), not a sibling under the original task. This is simpler and keeps the review→fix relationship explicit in the task hierarchy
 
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
 
 The fix workflow uses **semantic task links + looping** for automated review-fix cycles:
 
 **Coordination layer:**
 - Review **validates** original task (semantic link)
 - Review **spawns** fix task conditionally (via `spawns:` frontmatter if not approved)
 - Fix **spawned-by** review (semantic link with autorun)
 
 **Iteration layer:**
 - Single loop via `loop:` frontmatter (fix iterates until its own review approves)
 - `data.loop` metadata (automatic iteration tracking — see ../done/spawn-tasks.md)
 - Customizable termination conditions
 - Loop implementation via spawn mechanism (see ../done/spawn-tasks.md)
 
 This creates a **simple, customizable** review-fix workflow:
 
 1. **`aiki review --fix`** — Creates review (validates original) and fix task (follows up review)
 2. **`aiki build --fix`** — Build → review → fix loop in a single command
 
 **Task structure:**
 ```
 Original Task
   ├── Fix Task - Iteration 1 (follows-up review, autorun if issues found)
   │   ├── Do fix work
   │   ├── Review this fix → if fails, spawn iteration 2
   │   └── If approved → Review original task
   ├── Fix Task - Iteration 2 (spawned if iteration 1 fix review failed)
   │   ├── Do fix work (addresses issues from iteration 1's review)
   │   ├── Review this fix → if fails, spawn iteration 3
   │   └── If approved → Review original task
   └── Fix Task - Iteration N (continues until fix review approves)
       └── If original review fails → spawn new Fix Task - Iteration 1 (addresses new review)
 
 Review Task (validates original, sibling of Original Task)
 ```
 
 **Benefits:**
 - Fix is a subtask of original task (clean hierarchy)
 - Conditional creation: no fix spawned if review approved (via `spawns` mechanism)
 - Semantic links: queryable, self-documenting task graph
 - Single template handles both fix quality iteration and original validation
 - Each fix iteration addresses the most recent failing review
 ---
 
 ## Two-Stage Fix Workflow
 
 **Why two stages instead of one simple loop?**
 
 A single loop (fix → re-review original) can propagate low-quality fixes:
 - Fix introduces new bugs
 - Fix is incomplete or incorrect  
 - Fix has style/quality issues
 
 The fix itself becomes part of the codebase, so it deserves independent review before we check if it solved the original problem.
 
 **Two-stage approach:**
 
 1. **Stage 1: Fix quality loop** — "Is this fix well-implemented?"
    - Fix work → Review this fix
    - If issues found → Fix again (loop back with new review)
    - Iterates until fix review approves
 
 2. **Stage 2: Original validation** — "Did we break anything?"
    - Once fix review approves → Review original task
    - If original review fails → Spawn new fix addressing that review (back to Stage 1)
 
 **Benefits:**
 - Higher quality fixes (reviewed before checking against original)
 - Prevents cascading issues (bad fix caught in Stage 1)
 - Clear separation: implementation quality vs. regression testing
 - Each new fix iteration addresses the **most recent failing review**, not the original one
 
 **Key insight:** Each fix spawns with `review={{latest_failing_review}}`, ensuring issues are addressed incrementally rather than re-reading the same old review repeatedly
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
 | Termination conditions | Defined in `loop.until` | Customizable per template (`data.approved`, score threshold, manual override) |
 | Build review scope | Implementation review of the spec | Validates the whole result against the spec, not just individual diffs |
 | Build `--fix` + `--async` | Allowed (task-based) | Task system handles async execution, no command blocking |
 
 ---
 
 
 ## Loop Templates
 
 The fix workflow is implemented via **one task template** that declares loop behavior in frontmatter. The template creates a two-stage workflow:
 
 1. **Stage 1 (inner loop)**: Fix → review fix → if issues, loop back to fix
 2. **Stage 2 (outer trigger)**: When fix review passes → review original task → if fails, spawn new fix addressing the new review
 
 **One template:**
 - **`aiki/fix`** — Fix template with loop that reviews its own changes until clean, then triggers original review
 
 **Loop metadata:** The task system automatically provides loop metadata to every iteration task:
 
 ```yaml
 data:
   loop:
     index: 0           # Current iteration (0-indexed)
     iteration: 1       # Current iteration (1-indexed)
 ```
 
 **Usage in templates:**
 - `{{data.loop.iteration}}` — Display iteration number (1, 2, 3...)
 - `data.loop.iteration >= data.max_iterations` — Loop termination condition
 
 ---
 
 ### Template: `.aiki/templates/aiki/fix.md`
 
 Fix template with loop: keeps polishing the fix until review approves, then validates against original.
 
 ```yaml
 ---
 template: aiki/fix
 loop:
   until: subtasks.review_this_fix.data.approved or data.loop.iteration >= data.max_iterations
   data:
     latest_review: "{{subtasks.review_this_fix.id}}"  # Pass forward the latest failing review
 spawns:
   - template: aiki/review
     alias: review_original
     condition: subtasks.review_this_fix.data.approved
     data:
       scope: "task:{{data.original_task}}"
       fix: true  # If this review fails, spawn another fix
 ---
 
 # Fix Issues (Iteration {{data.loop.iteration}}/{{data.max_iterations | default: 10}})
 
 **Addressing review:** {{data.review_task}}
 
 ## Instructions
 
 1. **Read the review comments:**
    ```bash
    aiki task show {{data.review_task}} --with-source
    ```
 
 2. **Fix the issues found:**
    - Create subtasks for each distinct issue if helpful
    - Make the necessary code changes
    - Ensure changes are focused and don't introduce new issues
 
 3. **Review this fix:**
    {% subtask aiki/review as review_this_fix with scope=task:{{id}} %}
 
 ---
 
 **Loop behavior:**
 - If `review_this_fix` fails → spawns next iteration with `data.latest_review` set to the new failing review
 - If `review_this_fix` passes → spawns `review_original` to check we didn't break anything
 - If `review_original` fails → it spawns a new `aiki/fix` task addressing that new review (via `fix: true`)
 
 **Status:** Iteration {{data.loop.iteration}} of {{data.max_iterations | default: 10}}
 ```
 
 **Loop semantics:**
 - When task closes, evaluate `loop.until` condition
 - If false → spawn next iteration with `data.latest_review` updated, auto-increment `data.loop`
 - If true → exit loop, trigger `spawns` section (review original)
 
 **Spawns semantics:**
 - After loop exits (fix review approved), evaluate `spawns` conditions
 - `review_original` spawns with `fix: true` flag
 - If that review fails (not approved), it automatically spawns a new `aiki/fix` task
 
 ---
 
 ### Customization Examples
 
 **Change iteration limit:**
 ```bash
 aiki fix <review-id> --data '{"max_iterations": 20}'
 ```
 
 **Custom termination condition:**
 
 Override `.aiki/templates/aiki/fix.md`:
 ```yaml
 ---
 loop:
   until: subtasks.review_this_fix.data.score >= 90 or data.loop.iteration >= data.max_iterations
 ---
 ```
 
 **Skip fix review (direct to original review):**
 
 Create custom template without loop:
 ```yaml
 ---
 template: aiki/fix-direct
 spawns:
   - template: aiki/review
     alias: review_original
     data:
       scope: "task:{{data.original_task}}"
 ---
 
 # Fix Issues (Direct)
 
 1. Read review and fix issues
 2. Review spawns automatically when you close this task
 ```
 
 **Custom reviewer selection:**
 
 Override template to use specific agent:
 ```yaml
 {% subtask aiki/review as review_this_fix with scope=task:{{id}}, agent=codex-security %}
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
 
 ```rust // pseudocode
 // 1. Create review task (validates original)
 let review_task = create_review(scope, ...);
 add_link(review_task.id, LinkType::Validates {
     task_id: original_task_id,
     autorun: true,
 });
 
 // 2. Create fix task (child of original, follows up review)
 let fix_task = create_task_with_parent(
     original_task_id,  // Parent is the original task
     "aiki/fix",   // Template with loop: config
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
   ├── Fix Task - Iteration 1 (follows-up review)
   │   ├── Fix work
   │   ├── Review this fix
   │   └── Re-review original (if fix approved)
   ├── Fix Task - Iteration 2 (spawned by loop if iteration 1 failed)
   │   ├── Fix work
   │   ├── Review this fix
   │   └── Re-review original (if fix approved)
   └── Fix Task - Iteration N...
 Review Task (validates original, sibling of Original Task)
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
 - `aiki task show <iteration-id>` reveals iteration number and data
 
 ---
 
 ## `aiki build --fix`
 
 After `aiki build` completes, automatically runs `aiki review --fix` on the plan task. This is handled entirely in the build command's Rust code — the build template is not modified.
 
 **Syntax:**
 
 ```bash
 aiki build <plan>              # Just build (no review)
 aiki build <plan> --review     # Build → review (no auto-fix)
 aiki build <plan> --fix        # Build → review → fix loop
 ```
 
 ### Implementation
 
 #### 1. Add `--fix` and `--review` flags to `BuildArgs`
 
 In `cli/src/commands/build.rs`, add to the `BuildArgs` struct:
 
 ```rust // pseudocode
 /// Run review after build completes
 #[arg(long)]
 pub review: bool,
 
 /// Run review-fix loop after build completes (implies --review)
 #[arg(long)]
 pub fix: bool,
 ```
 
 #### 2. Validate flag combinations
 
 In `run()`, before dispatching:
 
 ```rust // pseudocode
 // --fix implies --review
 let review_after = args.review || args.fix;
 ```
 
 Note: `--fix` and `--async` are now compatible since loops are task-based.
 
 #### 3. Thread the flags through to `run_build_spec` and `run_build_plan`
 
 Both functions get new parameters: `review_after: bool, fix_after: bool`
 
 #### 4. After sync build completes, run review (optionally with --fix)
 
 In both `run_build_plan` and `run_build_epic`, after `task_run()` returns and the build completion output is printed, add the review step:
 
 ```rust // pseudocode
 if review_after {
     run_build_review(cwd, plan_path, final_epic_id, fix_after)?;
 }
 ```
 
 The `run_build_review` function:
 
 ```rust // pseudocode
 /// Run review (optionally with fix loop) after a build completes.
 ///
 /// Creates a review scoped to the plan's implementation, optionally
 /// including a fix subtask if `with_fix` is true.
 fn run_build_review(cwd: &Path, plan_path: &str, epic_id: &str, with_fix: bool) -> Result<()> {
     use super::review::{create_review, CreateReviewParams, ReviewScope, ReviewScopeKind};
 
     // Create an implementation review scoped to the plan
     let scope = ReviewScope {
         kind: ReviewScopeKind::Implementation,
         id: plan_path.to_string(),
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
     output_build_review_completed(&result.review_task_id, plan_path, with_fix)?;
 
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
 
 Creates the review task. The review runs (assigned to codex by default).
 
 ### Step 2: Codex Reviews
 
 The codex agent:
 1. Reads the task changes with `aiki task diff`
 2. Reviews for bugs, quality, security, performance
 3. Adds comments for each issue found
 4. Closes the review task
 
 ### Step 3: Fix Spawned (if review found issues)
 
 When the review closes with issues, it spawns the first fix task (iteration 1).
 
 ### Step 4: Fix Iteration Executes
 
 The fix task follows the template workflow:
 1. **Fix the issues** — Address the problems found in the review
 2. **Review this fix** — Inner loop: review the fix quality
 3. **Re-review original** — If fix review passes, check if original issues are resolved
 
 ### Step 5: Inner Loop (Fix Quality)
 
 The fix reviews itself and spawns next iteration if the fix has quality issues:
 
 1. Review the fix task for quality
 2. If review approved → inner loop terminates, proceeds to re-review original
 3. If review has issues:
    - Task closes, evaluates `loop.until` condition
    - If under max iterations → spawns next fix iteration
    - If at max → terminates with best-effort fix
 
 ### Step 6: Outer Loop (Original Validation)
 
 After fix review passes:
 1. Original task is re-reviewed
 2. If original review passes → loop complete
 3. If original still has issues:
    - Review spawns a new fix task (back to Step 3)
    - New fix addresses the latest review feedback
 ## Loop Termination
 
 ### Natural Termination
 
 The fix loop terminates when `loop.until` evaluates to true:
 
 ```yaml
 loop:
   until: subtasks.review_this_fix.data.approved or data.loop.iteration >= data.max_iterations
 ```
 
 When fix review is approved or max iterations reached, the loop exits and the `spawns` section triggers (spawning review of original task).
 ### Manual Termination
 
 - **Stop current iteration** — `aiki task stop <iteration-id>`
 - **Stop fix task** — `aiki task stop <fix-task-id>` (prevents new iterations)
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
-| Link types | Add `Validates` and `FollowsUp` link types with `autorun` field | **New work** |
-| Task close handler | Handle spawning and autorun triggers | **New work** |
-| Template parser | Add `loop:` and `spawns:` frontmatter support | **New work** |
-| Loop handler | Evaluate `loop.until` conditions, spawn next iteration, inject `data.loop` metadata | **New work** |
-| Condition evaluator | Simple expression evaluator for conditions (`subtasks.slug.data.approved`, data comparisons) | **New work** |
+| Link types (`cli/src/tasks/graph.rs`) | `validates` and `follows-up` link types with blocking semantics | **Done** (existed via semantic-blockers spec) |
+| Spawn engine (`cli/src/tasks/spawner.rs`) | Evaluate `spawns:` conditions, build Rhai scope, execute spawn actions | **Done** |
+| Spawn config (`cli/src/tasks/templates/spawn_config.rs`) | `SpawnEntry`, `SpawnTaskConfig` types for `spawns:` frontmatter | **Done** |
+| Expression evaluator (`cli/src/expressions.rs`) | Rhai-based evaluator for conditions (subtasks.slug.approved, data comparisons) | **Done** |
+| Task close handler (`cli/src/commands/task.rs`) | `execute_spawn_action()` — creates tasks/subtasks on close, adds `spawned-by` links, depth guard | **Done** |
 | **Commands** | | |
-| `cli/src/commands/review.rs` | When `--fix` flag: create fix task with `follows-up` link and autorun | **Update existing** |
-| `cli/src/commands/build.rs` | Add `--review` and `--fix` flags, `run_build_review()` | **New work** |
+| `cli/src/commands/review.rs` | `--fix` flag: sets `data.options.fix` on review task, which triggers `spawns:` config | **Done** |
+| `cli/src/commands/build.rs` | `--review` and `--fix` flags, `run_build_review()` creates code review post-build | **Done** |
+| `cli/src/commands/fix.rs` | Creates fix tasks from review comments, supports task/spec/code scopes | **Done** |
 | **Templates** | | |
-| `.aiki/templates/aiki/fix.md` | Add `loop:` and `spawns:` frontmatter for two-stage workflow | **Update existing** |
+| `.aiki/templates/aiki/review.md` | Added `spawns:` config to conditionally spawn fix when `not approved and data.options.fix` | **Done** |
+| `.aiki/templates/aiki/fix.md` | Outer loop template with `spawns:` config (self-referencing iteration) | **Done** |
+| `.aiki/templates/aiki/fix/quality.md` | Inner loop template with `spawns:` config for fix quality review | **Done** (new file) |
+| `.aiki/templates/aiki/fix/once.md` | Single-pass fix template (no loop) | **Done** (new file) |
 
 ---
 
 ## Prerequisites
 
 - **Conditional task spawning + loops** (../done/spawn-tasks.md): Implements `spawns:` frontmatter, `spawned-by` link type, and `loop:` syntactic sugar
 - **Unified expression evaluation** (`rhai-for-conditionals.md`): Required for property access (`subtasks.slug.data.approved`) and complex conditions in `loop.until` and spawn conditions
 
 ## Implementation Plan
 
 **NOTE:** Phases 1-2 are implemented in ../done/spawn-tasks.md. This spec only describes Phase 3 (applying those primitives to the review-fix workflow).
 
 ### Phase 1: Fix loop template
 
 Update the template to use spawn + loop primitives from ../done/spawn-tasks.md:
 
 1. **`.aiki/templates/aiki/fix.md`** — Update with loop and spawns
    - Uses `loop:` frontmatter (inner loop: polish fix until review approves)
    - Uses `spawns:` frontmatter (outer trigger: review original after fix approved)
    - Loop: iterates until `subtasks.review_this_fix.data.approved`
    - Spawn: triggers `aiki/review` of original task with `fix: true` flag
 
 2. **Update `.aiki/templates/aiki/review.md`:**
    - Add `spawns:` frontmatter section:
      ```yaml
      spawns:
        - template: aiki/fix
          condition: "not data.approved and data.fix"
          autorun: true
          data:
            review_task: "{{this.id}}"
            original_task: "{{data.scope}}"
      ```
 
 3. **Tests:**
    - End-to-end: `aiki review --fix` → review approves → no fix spawned
    - End-to-end: review has issues → fix spawns and starts, loops until clean
    - End-to-end: fix clean → original review spawns → if fails, new fix spawns
    - Unit: spawn condition evaluation (`data.approved == false` triggers spawn)
    - Integration: two-stage workflow completes correctly
 
 ### Phase 2: `build --review` and `build --fix`
 
 Add workflow flags to `aiki build`:
 
 1. Add `--review` and `--fix` flags to `BuildArgs`
 2. Implement `run_build_review()` using Phase 1 primitives
 3. Tests
 ---
 
 ## What This Does NOT Change
 
 - **Build template** (`.aiki/templates/aiki/build.md`) — untouched
 - **Review template** (`.aiki/templates/aiki/review.md`) — gets `spawns:` config added, but core content unchanged
 - **Fix command** — `aiki fix <review>` may become redundant (review templates spawn fixes automatically)
 
 ---
 
 ## Open Questions
 
 1. ~~**Condition expression language**~~ — Resolved: Using Rhai (already implemented). See `EXPRESSIONS.md`.
 
 2. **Task property access in conditions** — How to expose task properties for `loop.until` evaluation?
    - `subtasks.slug.data.approved` (or `subtasks[N].data.approved` for index-based) — needs task state inspection
    - `subtasks.slug.data.issue_count` (or `subtasks[N].data.issue_count`) — needs issue counting
    - `subtasks.slug.data.score` (or `subtasks[N].data.score`) — if we add review scores later
 
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
    - Existing review.md already uses this pattern (e.g., `{% subtask aiki/review/criteria/plan if data.scope.kind == "plan" %}`)
    - If the evaluator handles simple equality conditions but not property access like `subtasks.review_fix.data.issue_count`, the `fix/quality.md` template needs a simpler condition or a fallback (always create subtask, agent closes as won't-do)
 6. **Build review scope** — Implementation review of the plan vs. task review of the epic ID. Implementation review validates the whole result against the plan; task review checks individual diffs. Implementation review seems more useful post-build.
 
 7. **Won't-do handling** — If iteration task closes as won't-do, does `loop.until` still evaluate? Or does task status override loop logic?
 
 ## Resolved Questions
 
 4. ~~**Should we support `--review` without `--fix`?**~~ — Yes. `build --review` runs review once, `build --fix` runs review + fix loop.
 
 5. ~~**Template naming for fix loop subtask**~~ — Use `aiki/fix` (single template handles both stages). Spec body updated.
>>>>>>> conflict 1 of 1 ends
