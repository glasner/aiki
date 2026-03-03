# Workflow Progress View

## Design

Two visual elements, stacked:

1. **Epic tree** — the epic and its subtasks (the implementation work)
2. **Workflow stages** — build (decompose + implement), review, fix listed vertically below

The epic tree is the hero. The workflow stages show what operations have been / are being / will be performed on it.

### Stages

`aiki build` is a convenience that runs decompose then implement. The stage list reflects this:

| Stage | Scope | What it does |
|-------|-------|--------------|
| **build** | — | Group stage (decompose + implement) — expands to show sub-stages when active |
| **review** | epic | Full review of the epic: Explore Scope, Understand Criteria, Review |
| **fix** | review findings | Creates a fix task on the epic with remediation subtasks; includes internal `review fix` quality gate |

Build is a **parent stage** containing decompose and implement. When active, it expands to show which sub-stage is running. When done, it collapses to a summary line.

Review and fix alternate as iterations. The flow is:

```
build → review → (fix → review)* → approved
```

Each fix stage internally contains remediations + a `review fix` quality gate. If the quality gate finds more issues, the fix continues with more remediations + another quality gate. When the quality gate approves, the fix stage closes and the next epic-scoped review starts.

```
fix internals: (remediations → review fix)* → approved
```

Both review types use `aiki review` — the scope differs:
- **review** (top-level stage) runs against the epic/plan — full process: explore, criteria, review
- **review fix** (inside fix stage) runs against the fix task — narrower, just checking the fix

### Layout

```
 path line

 [id] Epic name
 ⎿ subtask 1
 ⎿ subtask 2
 ⎿ ...

 ● build
    ✓ decompose  12s
    ● implement  3/6  34s
 ○ review
```

After review finds issues, the iteration stages appear dynamically:

```
 ✓ build  6/6  2m40
 ✓ review  2 issues  42s
 ● fix  1/2  18s
    ...                                               ← remediations
    ○ review fix                                      ← quality gate (inside fix)
 ○ review                                             ← epic re-review (top-level)
```

### Epic subtask visibility

| Active stage | Epic subtasks |
|-------------|---------------|
| Decompose | Expanding — subtasks appear as agent creates them |
| Implement | Executing — subtasks progress ○ → ● → ✓ |
| Review | Collapsed — `✓ 6 subtasks` (all done, not interesting) |
| Fix | Collapsed — fix stage expands with remediations + review fix |
| All done | Collapsed |

### Stage expansion rules

| Stage | When active | When done |
|-------|------------|-----------|
| Build | Expanded — shows decompose + implement sub-stages | Collapsed — `✓ build  6/6  2m40` |
| Review | Expanded — shows its template subtasks | Collapsed — `✓ review  approved  42s` or `✓ review  2 issues  42s` |
| Fix | Expanded — shows remediations + `review fix` quality gate | Collapsed — `✓ fix  2/2  approved  1m14` |

### Review-fix iteration

The top-level stages alternate: `review → (fix → review)* → approved`.

Each **fix stage** internally contains remediations and a `review fix` quality gate. If the quality gate finds more issues, the fix continues with more remediations + another quality gate (max 10 rounds via `fix/quality.md`):

| Line type | Example | What it is |
|-----------|---------|------------|
| `Fix:` | `✓ Fix: Missing null check (auth.rs)` | Remediation subtask |
| `review fix` | `✓ review fix  1 issue` | Quality gate — reviews just the fix (narrower scope) |

The fix stage's children are interleaved: remediations, then review fix, then (if issues found) more remediations, then another review fix, etc.

**Review fix numbering:** Unnumbered when there's only one. Numbered when there are 2+ (`review fix #1`, `review fix #2`). The view redraws, so numbering appears retroactively.

**Progress counter on fix stage line:** Counts only remediations (not review fix checkpoints). Updates as quality gates discover new issues.

**Collapsed fix summary:**

```
 ✓ fix  2/2  approved  1m14                          ← quality gate approved first try
 ✓ fix  3/3  approved  2m08                           ← quality gate found issues, then approved
```

The `approved` comes from `review fix`. Without a quality gate, the fix stage collapses without it: `✓ fix  2/2  1m14`.

Each **review stage** (top-level) reviews the epic. After the first review finds issues and the fix is approved, a new top-level review checks the whole epic again. This catches issues the fix might have introduced.

## Mockups

### Decompose in progress

Build is active, decompose sub-stage running. Subtasks appearing in epic tree:

```
[80×12]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ○ Explore webhook requirements                         ← subtasks appearing
 ⎿ ○ Create implementation plan
 ⎿ ○ Implement webhook route handler

 ● build                                                   ← yellow, active group
    ● decompose  12s                                      ← yellow, active sub-stage
```

### Implement in progress

Build is active, implement sub-stage running. Epic subtasks executing:

```
[80×15]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ Explore webhook requirements          cc    8s        ← green ✓
 ⎿ ✓ Create implementation plan            cc    6s
 ⎿ ● Implement webhook route handler       cur             ← yellow ●
 ⎿ ○ Verify Stripe signatures                              ← dim ○
 ⎿ ○ Add idempotency key tracking
 ⎿ ○ Write integration tests

 ● build                                                   ← yellow, active group
    ✓ decompose  12s                                      ← green, done
    ● implement  3/6  34s                                 ← yellow, active
```

### Build completed, review in progress

Build collapses to summary. Epic subtasks collapse. Review expands to show its template subtasks:

```
[80×13]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ 6 subtasks  2m28                                    ← green, collapsed summary

 ✓ build  6/6  2m40                                       ← green, collapsed
 ● review  14s                                            ← yellow, active
    ✓ Explore scope                         cc   22s       ← review subtask done
    ✓ Understand criteria                   cc    8s
    ● Review                                cc             ← review subtask active
    ○ Fix loop                                             ← pending (only with --fix)
```

### Review complete with issues, fix in progress

Epic stays collapsed (like review). Fix stage expands to show its remediation subtasks:

```
[80×13]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ 6 subtasks  2m28                                    ← collapsed, same as during review

 ✓ build  6/6  2m40                                       ← green
 ✓ review  2 issues  42s                                  ← green, yellow "2 issues"
 ● fix  1/2  18s                                          ← yellow, active
    ✓ Fix: Missing null check (auth.rs)    cur  12s        ← remediation subtask done
    ● Fix: Error message format            cc              ← remediation subtask active
```

Consistent with review: when a post-build stage is active, the epic collapses and the stage expands to show its own work.

### Fix remediations done, review fix in progress

Initial fixes complete. Quality gate running inside the fix stage:

```
[80×14]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ 6 subtasks  2m28                                    ← collapsed

 ✓ build  6/6  2m40                                       ← green
 ✓ review  2 issues  42s                                  ← green
 ● fix  2/2  38s                                          ← yellow, all fixes done
    ✓ Fix: Missing null check (auth.rs)    cur  12s
    ✓ Fix: Error message format            cc   8s
    ● review fix                           cc              ← yellow, quality gate
 ○ review                                                  ← pending epic re-review
```

Progress shows `2/2` — all initial fixes are done. The quality gate is checking whether they're good enough.

### Review fix found issues, re-fixing

Quality gate found 1 more issue. New remediation running inside the same fix stage:

```
[80×15]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ 6 subtasks  2m28                                    ← collapsed

 ✓ build  6/6  2m40                                       ← green
 ✓ review  2 issues  42s                                  ← green
 ● fix  2/3  52s                                          ← total bumped to 3
    ✓ Fix: Missing null check (auth.rs)    cur  12s
    ✓ Fix: Error message format            cc   8s
    ✓ review fix  1 issue                  cc  14s         ← green ✓, yellow "1 issue"
    ● Fix: Missed edge case                cur             ← new remediation
 ○ review                                                  ← pending epic re-review
```

### Fix approved, epic re-review in progress

Quality gate approved the fix. Fix collapses. Epic-scoped re-review starts:

```
[80×13]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ 6 subtasks  2m28                                    ← collapsed

 ✓ build  6/6  2m40                                       ← green
 ✓ review  2 issues  42s                                  ← green
 ✓ fix  2/2  approved  1m14                               ← green, green "approved"
 ● review  8s                                            ← yellow, epic re-review
    ● Explore scope                        cc              ← full review process again
```

### Epic re-review found issues, second fix

The re-review caught something the fix introduced. Another fix stage starts:

```
[80×13]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ 6 subtasks  2m28                                    ← collapsed

 ✓ build  6/6  2m40                                       ← green
 ✓ review  2 issues  42s                                  ← green
 ✓ fix  2/2  approved  1m14                               ← green
 ✓ review  1 issue  18s                                   ← green, found 1 new issue
 ● fix  0/1  4s                                          ← yellow, new fix stage
    ● Fix: Race condition in handler       cur
 ○ review                                                  ← pending epic re-review
```

### Review approved, no fix needed

```
[80×8]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ 6 subtasks  2m28                                    ← collapsed

 ✓ build  6/6  2m40                                       ← green
 ✓ review  approved  42s                                  ← green, green "approved"
```

### Implement failed

Epic subtasks stay expanded. Build stays expanded to show which sub-stage failed:

```
[80×18]
 ops/now/realtime.md                                       ← dim dir, fg filename

 [xtuttnyv] Add real-time notifications to dashboard       ← dim id, hi+bold name
 ⎿ ✓ Explore notification requirements     cc   22s
 ⎿ ✓ Create implementation plan            cc    8s
 ⎿ ✓ Set up WebSocket server               cur  38s
 ⎿ ✓ Implement event broadcast             cur 1m02
 ⎿ ✓ Run schema migrations                 cc   22s
 ⎿ ✓ Build preference settings UI          cc   44s
 ⎿ ✓ Write integration tests               cur  28s
 ⎿ ✗ Add retry logic for deliveries        cc   36s        ← red ✗
    ⎿ Redis connection refused                              ← red error
 ⎿ ○ Wire up notification toggle                           ← dim, blocked
 ⎿ ○ Add notification sound prefs                          ← dim, blocked

 ✗ build                                                   ← red ✗, stays expanded
    ✓ decompose  12s                                      ← green
    ✗ implement  8/10  2m48  1 failed                     ← red ✗
```

### Build only (no review/fix), completed

```
[80×13]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ Explore webhook requirements          cc    8s
 ⎿ ✓ Create implementation plan            cc    6s
 ⎿ ✓ Implement webhook route handler       cur  48s
 ⎿ ✓ Verify Stripe webhook signatures      cc   22s
 ⎿ ✓ Add idempotency key tracking          cur  34s
 ⎿ ✓ Write integration tests               cc   18s

 ✓ build  6/6  2m40                                       ← collapsed (no review/fix)
```

### All done (single iteration)

Fix approved on first try, epic re-review passes:

```
[80×9]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ 7 subtasks                                           ← collapsed (6 original + 1 followup)

 ✓ build  6/6  2m40
 ✓ review  2 issues  42s
 ✓ fix  2/2  approved  1m14                               ← "approved" from review fix
 ✓ review  approved  12s                                  ← epic re-review passed
```

### All done (multiple iterations)

Fix introduced a new issue, caught by epic re-review. Second fix + re-review passes:

```
[80×11]
 ops/now/webhooks.md                                       ← dim dir, fg filename

 [luppzupt] Implement Stripe webhook event handling        ← dim id, hi+bold name
 ⎿ ✓ 7 subtasks                                           ← collapsed

 ✓ build  6/6  2m40
 ✓ review  2 issues  42s
 ✓ fix  2/2  approved  1m14
 ✓ review  1 issue  18s                                   ← epic re-review found 1 new issue
 ✓ fix  1/1  approved  22s
 ✓ review  approved  12s                                  ← final epic review: clean
```

## Shared conventions

### Status rendering

| Status | Symbol | Color | Notes |
|--------|--------|-------|-------|
| Done | ✓ | green | |
| In progress | ● | yellow | Blinks (ANSI SGR 5/6) |
| Pending | ○ | dim | |
| Failed | ✗ | red | |
| Stuck (blocked by failure) | ○ | dim | |

### Agent + time columns

Right-aligned on subtask lines. Only shown for active or completed subtasks.

**Agent abbreviation:** `cc` (claude-code) in cyan, `cur` (cursor) in magenta, others in fg. Omitted for pending.

**Time:** Seconds for < 60s (`8s`), minutes+seconds for >= 60s (`1m02`). Omitted when no data.

### Stage line format

```
 symbol  stage-name  [progress]  [elapsed]  [extra]
```

Progress is `completed/total` for implement and fix. Review shows "approved" or "N issues". Elapsed is total stage time.

### No task IDs on subtask lines

Epic ID `[luppzupt]` on the header. Subtask IDs omitted — `aiki task show` for details.

## Live progress view

Same layout, redrawn in place by StatusMonitor. Additions:

1. **Elapsed time updates live** on active stage line
2. **Active subtask comment** — latest agent comment below the active subtask:

```
 ⎿ ● Implement webhook route handler       cur
      └─ Setting up route middleware...                    ← dim, latest comment
```

3. **Footer:**
```
 [Ctrl+C to detach]                                        ← dim hint
```

### Monitor context

Pass `WorkflowContext` to StatusMonitor:

```rust
struct WorkflowContext {
    plan_path: String,
    epic: TaskSnapshot,
    completed_stages: Vec<StageSnapshot>,
    pending_stages: Vec<String>,
}
```

## `aiki build show` (static view)

Same layout, rendered from persisted task data:

1. Epic task → header + subtask list
2. Decompose + implement tasks → build stage (expanded or collapsed)
3. Review task + subtasks → review stage (expanded or collapsed)
4. Fix task → fix stage expands to show remediation subtasks

Content rules apply.

## Pipe behavior

- **stderr** (TTY): styled view
- **stdout** (piped): machine-readable IDs (epic, build, review, fix — one per line)

## Task graph → workflow view mapping

```
Task graph:                              Workflow view:

  Epic                                    [luppzupt] Epic name
    ├── subtask 1                          ⎿ ✓ subtask 1      ← epic tree
    ├── subtask 2                          ⎿ ● subtask 2
    ├── subtask 3                          ⎿ ○ subtask 3
    └── Followup: Review <id>             (epic subtask, shown under fix stage)
         ├── Fix: issue 1
         ├── Fix: issue 2
         └── Quality Check 1
              ├── Review Fix
              ├── Fix Issues → Fix task 2
              └── Quality Check 2 → ...

  Decompose task                         ● build               ← group stage
  Implement task (orchestrator)             ✓ decompose  12s  ← sub-stage
    └── spawns → Review                     ● implement  3/6   ← sub-stage

  Review task (epic scope)               ● review               ← top-level, expands
    ├── Explore Scope                       ✓ Explore scope
    ├── Criteria                            ● Review
    ├── Review
    └── Fix Loop
         └── Fix task (epic subtask)     ● fix  2/2             ← top-level, expands
              ├── Fix: issue 1              ✓ Fix: issue 1     ← remediation
              ├── Fix: issue 2              ✓ Fix: issue 2     ← remediation
              └── Quality Check 1           ● review fix        ← quality gate (inside fix)
                   ├── Review Fix        ○ review               ← next epic review (top-level)
                   └── Fix Issues
```

**Two levels of review scope:**
- `review` (top-level stage) = epic-scoped, full review process
- `review fix` (inside fix stage) = fix-scoped, quality gate from `fix/quality.md`

Quality check internals (Review Fix, Fix Issues, nested Quality Checks) are flattened into the fix stage as interleaved `Fix:` and `review fix` lines. The deeply nested task graph is hidden — `aiki task show` for the full tree.

**Epic-scoped re-reviews** appear as new top-level `review` stages when fix is approved and the epic needs rechecking.

## Relationship to build.md

| From build.md | Status |
|---------------|--------|
| Ratatui widgets → Buffer → output pipeline | Keep |
| `buffer_to_ansi()` converter | Keep |
| `PathLine` widget | Keep |
| `EpicTree` widget | **Keep concept** — renders epic header + subtask list |
| `StageTrack` widget | **Replace** with vertical `StageList` |
| `views/epic_show.rs` composer | **Replace** with `views/workflow.rs` |

### Widgets

| Widget | Purpose | Est. lines |
|--------|---------|-----------|
| `PathLine` | `ops/now/webhooks.md` with dimmed dir | ~30 (from build.md) |
| `EpicTree` | Epic header + `⎿` subtask children (or collapsed summary) | ~100 |
| `StageList` | Vertical stage lines, with sub-stages and optional expanded subtasks | ~100 |

### Data model

```rust
struct WorkflowView {
    plan_path: String,
    epic: EpicView,
    stages: Vec<StageView>,
}

struct EpicView {
    short_id: String,
    name: String,
    subtasks: Vec<SubtaskLine>,
    collapsed: bool,
    collapsed_summary: Option<String>,  // "6 subtasks  2m28"
}

struct StageView {
    name: String,                       // "build", "review", "fix"
    state: StageState,                  // Pending, Active, Done, Failed
    progress: Option<String>,           // "6/6", "2 issues", "approved"
    elapsed: Option<String>,            // "2m40"
    failed: Option<u32>,               // count of failed items, e.g. 1 → "1 failed"
    sub_stages: Vec<SubStageView>,      // decompose + implement (for build)
    children: Vec<FixChild>,            // fix: interleaved remediations + review fix gates
                                        // review: SubtaskLine children
}

struct SubStageView {
    name: String,                       // "decompose", "implement"
    state: StageState,
    progress: Option<String>,           // "3/6"
    elapsed: Option<String>,            // "34s"
    failed: Option<u32>,               // count of failed items, e.g. 1 → "1 failed"
}

struct SubtaskLine {
    name: String,
    status: Status,
    agent: Option<String>,
    elapsed: Option<String>,
    error: Option<String>,
}

enum FixChild {
    Subtask(SubtaskLine),               // "Fix: Missing null check (auth.rs)"
    ReviewFix {                         // "review fix  1 issue" or "review fix  approved"
        number: Option<u32>,            // None if only one, Some(1), Some(2) if multiple
        state: StageState,
        result: Option<String>,         // "1 issue", "approved", None if in progress
        agent: Option<String>,
        elapsed: Option<String>,
    },
}
```

## Key decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Build as group stage | decompose + implement are sub-stages of build | `aiki build` runs both — the user thinks "build" not "decompose then implement". Sub-stages give visibility into which part is running. |
| Build collapses when done | `✓ build 6/6 2m40` | Once both sub-stages are done, the internals aren't interesting. Summary is enough. |
| Build stays expanded on failure | Shows which sub-stage failed | User needs to see if decompose or implement broke. |
| Implement shows progress | `● implement 3/6 34s` | The implement sub-stage runs epic subtasks — its progress count matches the epic subtask progress. |
| Epic tree + stage list, not nested | Two sections | Epic subtasks belong to the epic. Stages are operations, not parents. |
| Fix expands like review | Remediation subtasks shown under fix stage, epic stays collapsed | Consistent pattern: post-build stages (review, fix) expand to show their own work. Epic only expands during decompose/build. |
| Review and fix expand with own subtasks | Both have their own work to show | Decompose/implement work is visible in epic tree. Review/fix have separate subtasks. |
| Epic collapses during review | `✓ N subtasks` summary | All-checkmark lists waste space. Review's own subtasks are the content. |
| Epic stays collapsed during fix | Same as review — consistent pattern | Epic only expands during decompose/build when the subtask list is the active work. |
| Two review scopes, two levels | `review` (top-level, epic) and `review fix` (inside fix, narrower) | Epic review is a full process. Fix review is a quality gate. Different scopes, different positions in the stage list. |
| `review fix` inside fix stage | Quality gate is a child of fix, not a top-level stage | The fix-scoped review is checking the fix's work — it belongs inside the fix stage. Only epic-scoped reviews are top-level. |
| Epic re-review as new top-level stage | `review → fix → review → fix → ...` alternating | Each epic re-review checks the whole thing again. Top-level stages only grow with epic-level iterations, not fix-internal quality loops. |
| `review fix` numbered only when 2+ | `review fix` vs `review fix #1`, `#2` | Single quality gate needs no number. Multiple rounds get retroactive numbering (view redraws). |
| Progress counter excludes review fix | `3/3` counts fixes only | Quality gates are checkpoints, not work items. Counting them would inflate the denominator. |
| `approved` on collapsed fix line | `✓ fix  2/2  approved  1m14` | Signals the quality gate passed. Without a quality gate, no `approved` — just `✓ fix  2/2  1m14`. |
