# Review Screen Cleanup

## Problem

`aiki review <target>` output uses plain `CommandOutput` + `MdBuilder` markdown. It should use the same `WorkflowView` → `render_workflow()` TUI that `build show` uses. The existing WorkflowView already handles the build→review→fix pipeline — we just need to make review output use it, and ensure the view does the right thing when the active stage is "review".

### What already works

- `build_workflow_view()` collapses the epic tree when review/fix is active (builder.rs:139)
- `build_review_stage()` finds review tasks via `validates` edges and adds children
- StageList expands Active/Failed stages to show their sub-stages and children
- `render_issue_list()` renders issue lists in TUI (used by `review issue list`)

### What's broken

- Review command output functions (`output_review_started`, `output_review_completed`, etc.) use `CommandOutput` + `MdBuilder` instead of `render_workflow()`
- `show_review()` renders plain markdown instead of TUI
- Review subtasks (explore, criteria, record-issues) may not be showing as children of the review stage in the StageList

---

## Design: One View, Three Phases

The `WorkflowView` is the single screen for the entire build→review→fix lifecycle. What changes between phases is **what's collapsed and what's expanded**:

| Phase | Epic tree | Build stage | Review stage | Fix stage |
|-------|-----------|-------------|-------------|-----------|
| **build** | expanded (subtasks visible) | expanded (decompose/implement) | pending | pending |
| **review** | collapsed ("6 subtasks 2m28s") | collapsed (done line) | expanded (explore/criteria/record-issues) | pending |
| **fix** | collapsed | collapsed | collapsed | expanded (fix children) |
| **all done (approved)** | collapsed | collapsed | collapsed | skipped (`─`) |
| **all done (fixed)** | collapsed | collapsed | collapsed | collapsed |

This is how builds already work. Review just needs to follow the same pattern.

---

## Lifecycle Screens

All screens use `render_workflow()` — same view, different data states.

### 1. During Build (reference)

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ ✓ Write handler                    cc  45s  ← dim ⎿, green ✓, text name, cyan cc, dim 45s
 ⎿ ▸ Add validation                   cc 0:12  ← yellow ▸, cyan cc, dim 0:12
 ⎿ ○ Add tests                                 ← dim ○, dim name

 ▸ build  1/3  0:57                             ← yellow all
    ✓ decompose  12s                            ← green, 4-char indent
    ▸ implement  1/3  0:45                      ← yellow, 4-char indent
 ○ review                                       ← dim ○, dim text
 ○ fix                                          ← dim ○, dim text
```

### 2. Review Starting — Agent Spawning

First subtask starting, agent spawn in progress (0.5-2s transition state).

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ▸ review                                       ← yellow all
    ⧗ explore                                   ← yellow ⧗ (hourglass), yellow text
    ○ criteria                                  ← dim
    ○ record-issues                             ← dim
 ○ fix                                          ← dim ○, dim text
```

**Note**: The `⧗` (hourglass) indicates agent spawn in progress. See `ops/now/ux-agent-loading.md` for full design.

### 3. Review In-Progress — Exploring

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ▸ review  0:08                                 ← yellow all
    ▸ explore                          cc 0:08  ← yellow, cyan cc, dim 0:08
    ○ criteria                                  ← dim
    ○ record-issues                             ← dim
 ○ fix                                          ← dim ○, dim text
```

### 4. Review In-Progress — Starting Criteria

Explore completed, criteria agent spawning (0.5-2s transition).

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ▸ review  0:12                                 ← yellow all
    ✓ explore                          cc  12s  ← green, cyan cc, dim 12s
    ⧗ criteria                                  ← yellow ⧗ (hourglass), yellow text
    ○ record-issues                             ← dim
 ○ fix                                          ← dim ○, dim text
```

### 5. Review In-Progress — Recording Issues

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ▸ review  0:35                                 ← yellow all
    ✓ explore                          cc  12s  ← green, cyan cc, dim 12s
    ✓ criteria                         cc  18s  ← green, cyan cc, dim 18s
    ▸ record-issues                    cc 0:05  ← yellow, cyan cc, dim 0:05
 ○ fix                                          ← dim ○, dim text
```

### 6. Review Completed — Approved (0 issues)

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ✓ review  approved  0:42                       ← green all
 ─ fix                                          ← dim ─, dim text
```

**Notes:**
- Fix shows `─` (horizontal dash) instead of `✓` or `○` — it wasn't needed, not pending and not done
- New symbol: `SYM_SKIPPED = "─"` (U+2500), rendered in dim
- New state: `StageState::Skipped` — distinct from Pending (will happen) and Done (did happen)
- Trigger: when review completes with 0 issues (approved), fix transitions to Skipped

### 7. Review Completed — Issues Found

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ✓ review  3 issues  0:42                       ← green ✓, green text, yellow "3 issues"
 ○ fix                                          ← dim ○, dim text
```

### 8. Fix Starting — First Agent Spawning

Fix stage started, first fix task agent spawning (0.5-2s transition).

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ✓ review  3 issues  0:42                       ← green ✓, green text, yellow "3 issues"
 ▸ fix  0/3                                     ← yellow all
    ⧗ Fix: Missing null check                   ← yellow ⧗ (hourglass), yellow text
    ○ Fix: Error message format                 ← dim all
    ○ Fix: Trailing whitespace                  ← dim all
```

### 9. Fix In-Progress

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ✓ review  3 issues  0:42                       ← green ✓, green text, yellow "3 issues"
 ▸ fix  1/3  0:18                               ← yellow all
    ✓ Fix: Missing null check     cur  12s      ← green ✓, magenta cur, dim 12s
    ▸ Fix: Error message format    cc 0:06      ← yellow ▸, cyan cc, dim 0:06
    ○ Fix: Trailing whitespace                  ← dim all
```

### 10. Fix with Review-Fix Gate

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ✓ review  3 issues  0:42                       ← green ✓, green text, yellow "3 issues"
 ▸ fix  3/3  0:38                               ← yellow all
    ✓ Fix: Missing null check     cur  12s      ← green ✓, magenta cur, dim 12s
    ✓ Fix: Error message format    cc   8s      ← green ✓, cyan cc, dim 8s
    ✓ Fix: Trailing whitespace     cc   4s      ← green ✓, cyan cc, dim 4s
    ▸ review fix                   cc 0:05      ← yellow ▸, cyan cc, dim 0:05
```

### 11. All Done — With Fixes

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ✓ review  3 issues  0:42                       ← green ✓, green text, yellow "3 issues"
 ✓ fix  3/3  0:38                               ← green all
```

### 12. All Done — No Issues (approved)

```
[80 cols]
 ops/now/ feature.md                           ← dim dir, text filename

 [luppzupt] Implement webhooks                 ← dim brackets, hi+bold name
 ⎿ 3 subtasks  2m28s                           ← dim ⎿, dim summary

 ✓ build  3/3  2m28s                            ← green all
 ✓ review  approved  0:42                       ← green all
 ─ fix                                          ← dim ─, dim text
```

### 13. Review Show (standalone: `aiki review show <id>`)

Uses `render_issue_list()` (already exists):

```
[80 cols]
 [rrmqxnps] Review feature.md  3 issues        ← dim brackets, hi+bold name, yellow count

 ⎿ [high] Missing null check before token access        src/auth.rs:42
 ⎿ [medium] Error message does not include context       src/api.rs:88-92
 ⎿ [low] Trailing whitespace in template
```

Style annotations per issue row:
```
 ⎿ [high] text...                    location   ← dim ⎿, red badge+text, dim location right-aligned
 ⎿ [medium] text...                  location   ← dim ⎿, yellow badge+text, dim location right-aligned
 ⎿ [low] text...                                ← dim ⎿, dim badge+text
```

---

## Implementation Plan

### Phase 1: Make review output use WorkflowView

Replace review command's output functions with `render_workflow()`.

**Changes in `commands/review.rs`:**

1. `output_review_started()` — Find the epic being reviewed, build `WorkflowView` via `build_workflow_view()`, render with `render_workflow()` + `buffer_to_ansi()`.

2. `output_review_completed()` — Same: build `WorkflowView` from epic + subtasks + graph, render.

3. `output_review_async()` — Same as started.

4. `show_review()` — Use `render_issue_list()` for the detail view (already mostly works, just needs to call TUI renderer instead of markdown).

### Phase 2: Review subtasks as stage children

Verify `build_review_stage()` in `builder.rs` populates review subtask children correctly.

**What needs checking:**
- `graph.children_of(review_id)` finds explore/criteria/record-issues subtasks
- Children render as `StageChild::Subtask` items in the StageList (status symbol + name + agent + elapsed)
- Review stage expands when Active (already handled by `is_expanded()` in stage_list.rs)

### Phase 3: Clean up

- Remove `output_review_started()`, `output_review_completed()`, `output_review_async()` and the `CommandOutput` usage from review.rs
- Keep `output_nothing_to_review()` (no epic context to render a WorkflowView)
- Evaluate if `commands/output.rs` can be removed (check if fix.rs still needs it)

---

## Key Insight

No new view type needed. No new data model. The `WorkflowView` + `render_workflow()` already handles this — the builder just needs to populate the right data, and the review command needs to call it instead of `CommandOutput`.

The collapsed-epic + expanded-review-stage behavior is already built into `builder.rs` — it collapses the epic when `active_stage == "review"` and the StageList expands any Active stage that has children.
