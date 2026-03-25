# New Screens — Iteration 2: Fix Mockup Mismatches

**Date**: 2026-03-24
**Status**: Ready
**Priority**: P1
**Phase**: 2 — View functions (iteration 2)
**Depends on**: step-2-new-screens.md (all screen view functions exist)
**Reference**: screen-states.md (authoritative mockups)

---

## Problem

The screen view functions and shared components from step 2 produce output that diverges from the `screen-states.md` mockups in several visible ways. This plan addresses each mismatch with a targeted fix.

---

## Fixes

### Fix 1: Subtask table header — `合` → `[id] Title`

**Files**: `components.rs`, `render.rs`, `app.rs`

The subtask table header currently uses `LineStyle::PhaseHeader { active: false }`, which renders as `合 lkji3d Epic: Mutex for Task Writes`. The mockup expects `[lkji3d] Epic: Mutex for Task Writes` with dim brackets+id and fg title.

**Changes:**

1. Add `LineStyle::SubtaskHeader` variant to `app.rs`:
   ```rust
   SubtaskHeader,  // [short-id] title — dim brackets+id, fg title
   ```

2. Update `components::subtask_table` header line to use it:
   ```rust
   lines.push(Line {
       indent: 1,
       text: title.to_string(),
       meta: None,
       style: LineStyle::SubtaskHeader { short_id: short_id.to_string() },
       group,
       dimmed: false,
   });
   ```

   Alternative (simpler, no new enum data): keep `text` as `format!("[{}] {}", short_id, title)` and use a new `LineStyle::SubtaskHeader` that renders the text as-is with fg color (no `合` icon). The `[id]` portion gets dim treatment via inline ANSI or by splitting into two `set_string` calls.

   **Recommended approach**: New `LineStyle::SubtaskHeader` variant (no data). Pre-format the text as the full string. In `render_lines`, render the `[id]` prefix portion in dim and the rest in fg. Parse the `]` position to split.

3. Render in `render.rs`:
   ```rust
   LineStyle::SubtaskHeader => {
       // Text is "[short_id] title" — dim the bracketed prefix, fg the rest
       let style_id = if line.dimmed { dim_style } else { theme.dim_style() };
       let style_title = if line.dimmed { dim_style } else { theme.fg_style() };

       if let Some(bracket_end) = line.text.find("] ") {
           let (id_part, title_part) = line.text.split_at(bracket_end + 2);
           buf.set_string(x, y, id_part, style_id);
           buf.set_string(x + id_part.len() as u16, y, title_part, style_title);
       } else {
           buf.set_string(x, y, &line.text, style_title);
       }
   }
   ```

**Test**: Render a subtask table header, assert first cell is `[` (not `合`).

---

### Fix 2: Subtask text color varies by status

**File**: `render.rs`

Currently all subtask text uses `theme.text_style()`. The mockup specifies:

| Status | Icon color | Text color |
|--------|-----------|------------|
| `✔` done | green | **dim** |
| `▸` active | yellow | **fg** |
| `○` pending | dim | **dim** |
| `◌` assigned | dim | **dim** |
| `✘` failed | red | **red** |

**Change** in `render.rs` `LineStyle::Subtask { status }` arm — replace the single `text_style` with per-status:

```rust
let text_style = if line.dimmed {
    dim_style
} else {
    match status {
        SubtaskStatus::Active => Style::default().fg(theme.fg),
        SubtaskStatus::Failed => Style::default().fg(theme.red),
        // Done, Pending, Assigned — all dim
        _ => theme.dim_style(),
    }
};
```

**Test**: Render a `SubtaskStatus::Failed` line into a buffer, assert the text cell has `theme.red` fg. Render `SubtaskStatus::Done`, assert dim fg.

---

### Fix 3: Lane block structure — hierarchical with `⎿` children

**Files**: `components.rs`, `render.rs`

Current output:
```
⠹ loop
⎿ lane 1 — claude (1/3)
    2 failed
    thinking...
```

Mockup expects:
```
⠹ loop
⎿ Lane 1 (claude)
    ⎿ 1/2 subtasks completed
    ⎿ Writing lock fn...      28s
```

**Changes to `components::loop_block`:**

Rewrite to produce the hierarchical structure:

```rust
pub fn loop_block(group: u16, lanes: &[LaneData]) -> Vec<Line> {
    let mut lines = Vec::new();

    let active = lanes.iter().any(|l| !l.shutdown);
    lines.push(Line {
        indent: 0,
        text: "loop".to_string(),
        meta: None,
        style: LineStyle::PhaseHeader { active },
        group,
        dimmed: false,
    });

    for (i, lane) in lanes.iter().enumerate() {
        // Blank line between lanes (not before the first)
        if i > 0 {
            lines.extend(blank());
        }

        // Lane header: ⎿ Lane N (agent)
        let lane_label = match lane.agent.as_str() {
            a => format!("Lane {} ({})", lane.number, a),
        };
        let lane_style = if lane.shutdown {
            LineStyle::ChildDone
        } else {
            LineStyle::Child
        };
        lines.push(Line {
            indent: 1,
            text: lane_label,
            meta: None,
            style: lane_style,
            group,
            dimmed: false,
        });

        // Progress line: ⎿ x/y subtasks completed[, z failed]
        let progress = format_lane_progress(lane);
        lines.push(Line {
            indent: 2,
            text: progress,
            meta: None,
            style: LineStyle::Child,
            group,
            dimmed: false,
        });

        // Heartbeat/status line: ⎿ <heartbeat>  <elapsed>  OR  ⎿ Agent shutdown.
        if lane.shutdown {
            lines.push(Line {
                indent: 2,
                text: "Agent shutdown.".to_string(),
                meta: None,
                style: LineStyle::Child,
                group,
                dimmed: false,
            });
        } else if let Some(ref hb) = lane.heartbeat {
            lines.push(Line {
                indent: 2,
                text: hb.clone(),
                meta: lane.elapsed.clone(),
                style: LineStyle::ChildActive,
                group,
                dimmed: false,
            });
        } else {
            lines.push(Line {
                indent: 2,
                text: "starting session...".to_string(),
                meta: lane.elapsed.clone(),
                style: LineStyle::ChildActive,
                group,
                dimmed: false,
            });
        }

        // Error line if failures exist
        if lane.failed > 0 && !lane.shutdown {
            lines.push(Line {
                indent: 2,
                text: format!("Error: {} task{} failed", lane.failed, if lane.failed == 1 { "" } else { "s" }),
                meta: None,
                style: LineStyle::ChildError,
                group,
                dimmed: false,
            });
        }
    }

    lines
}

fn format_lane_progress(lane: &LaneData) -> String {
    if lane.failed > 0 {
        format!("{}/{} subtasks completed, {} failed", lane.completed, lane.total, lane.failed)
    } else {
        format!("{}/{} subtasks completed", lane.completed, lane.total)
    }
}
```

Key structural changes:
- Lane header is `LineStyle::Child` (indent 1) — renders as `⎿ Lane 1 (claude)`
- Sub-children are `LineStyle::Child`/`ChildActive` (indent 2) — renders as `    ⎿ x/y subtasks completed`
- Blank line between lanes
- "Lane" capitalized, agent in parentheses (not em-dash)
- Done lane header uses `ChildDone` (shows `✓` prefix — actually no, `ChildDone` renders `⎿ ✓` which is wrong for lane header). Actually: done lane header should just be `Child` style (dim), not `ChildDone`. `ChildDone` adds a `✓` icon.

Correction: done lane header should use `LineStyle::Child` (dim text, no icon), same as active lane header but the entire group will get dimmed by `apply_dimming` when the loop phase completes. No special style needed.

```rust
lines.push(Line {
    indent: 1,
    text: lane_label,
    meta: None,
    style: LineStyle::Child, // Always Child — dimming handles done state
    group,
    dimmed: false,
});
```

**Test**: `loop_block` with 2 lanes produces blank line between them, each lane has 3+ lines (header, progress, heartbeat/status).

---

### Fix 4: Add `◌` symbol and `SubtaskStatus::PendingUnassigned`

**Files**: `app.rs`, `theme.rs`, `render.rs`, `screens/helpers.rs`

The mockup distinguishes two pending states:
- `○` — pending, in an active lane (will be worked on by this lane's agent)
- `◌` — pending, in a non-started lane (waiting for a lane to claim it)

**Changes:**

1. Add symbol to `theme.rs`:
   ```rust
   pub const SYM_PENDING_UNASSIGNED: &str = "◌";
   ```

2. Add variant to `SubtaskStatus` in `app.rs`:
   ```rust
   pub enum SubtaskStatus {
       PendingUnassigned,  // ◌ — no lane has claimed this yet
       Pending,            // ○ — in active lane, not yet started
       Assigned,           // ○ — assigned to session, workspace creating
       Active,             // ▸
       Done,               // ✔
       Failed,             // ✘
   }
   ```

3. Render `PendingUnassigned` in `render.rs`:
   ```rust
   SubtaskStatus::PendingUnassigned => (
       theme::SYM_PENDING_UNASSIGNED,
       if line.dimmed { dim_style } else { theme.dim_style() },
   ),
   ```

4. Update `SubtaskData` conversion in `helpers.rs` — determine whether a pending task is in an active lane or not. This requires lane information at conversion time. Two options:

   **Option A (simple)**: Default all `Open` tasks to `PendingUnassigned`. Change to `Pending` only for tasks that belong to a lane with at least one `InProgress` task. This requires passing lane context to the `From` impl — likely better as an explicit function rather than a `From` trait.

   **Option B (simpler)**: When no lanes exist yet (pre-loop), all pending tasks show `○`. When lanes are assigned, tasks in a lane with an active session show `○`, others show `◌`. Implement by adding a `has_active_lane: bool` parameter to the subtask data builder:

   ```rust
   pub fn subtask_data_from_task(task: &Task, in_active_lane: bool) -> SubtaskData {
       let status = match task.status {
           TaskStatus::Open => {
               if task.claimed_by_session.is_some() {
                   SubtaskStatus::Assigned
               } else if in_active_lane {
                   SubtaskStatus::Pending
               } else {
                   SubtaskStatus::PendingUnassigned
               }
           }
           TaskStatus::InProgress => SubtaskStatus::Active,
           TaskStatus::Closed => SubtaskStatus::Done,
           TaskStatus::Stopped => SubtaskStatus::Failed,
       };
       SubtaskData {
           name: task.name.clone(),
           status,
           elapsed: task.elapsed_str(),
       }
   }
   ```

   Callers (`build.rs`, `fix.rs`, `task_run.rs`) query lane assignment to set the flag. Pre-decompose / pre-loop: `in_active_lane = false` → all show `○` (correct: no lanes yet). Post-loop-start: look up which lane the task is in.

   **Recommended**: Option B. Requires updating the `From<&&Task>` impl to a standalone function with the extra parameter, and updating call sites.

**Test**: Assert `PendingUnassigned` renders as `◌`, `Pending` as `○`.

---

### Fix 5: Subtask table blank lines between separators and content

**File**: `components.rs`

Add blank lines inside the subtask table boundaries.

**Change in `subtask_table`:**

```rust
pub fn subtask_table(/* ... */) -> Vec<Line> {
    let mut lines = Vec::new();

    // Opening separator
    lines.push(/* Separator */);
    // Blank line after separator
    lines.extend(blank());

    // Header + subtasks (unchanged)
    // ...

    // Blank line before closing separator
    lines.extend(blank());
    // Closing separator
    lines.push(/* Separator */);

    lines
}
```

**Test**: `subtask_table` output starts with Separator then Blank, ends with Blank then Separator.

---

### Fix 6: Symbol weight — thin → heavy

**File**: `theme.rs`

Update symbol constants to match mockup:

```rust
pub const SYM_CHECK: &str = "✔";    // was "✓" (U+2713 → U+2714)
pub const SYM_FAILED: &str = "✘";   // was "✗" (U+2717 → U+2718)
```

Verify no code compares against the literal character (search for `"✓"` and `"✗"` in non-theme files). Update any hardcoded `"\u{2713}"` or `"✓"` in view functions (e.g., `build.rs:40` uses `"\u{2713}"` for the plan done child, `review.rs:35` uses `"✓ approved"`).

**Files to update**:
- `theme.rs` — symbol constants
- `build.rs:40` — `"\u{2713} {}"` → `format!("{} {}", theme::SYM_CHECK, plan_path)` (or use the heavy literal)
- `review.rs:35` — `"✓ approved"` → `format!("{} approved", theme::SYM_CHECK)`
- `fix.rs:29` — `"✓ approved — no actionable issues"` → use `SYM_CHECK`
- `fix.rs:137` — `"✓ no regressions"` → use `SYM_CHECK`
- `helpers.rs:194` — `"\u{2713} approved"` → use `SYM_CHECK`
- Any other hardcoded check/fail symbols → use the constants

**Test**: existing render tests that assert `SYM_CHECK` should still pass (they reference the constant, not the literal).

---

### Fix 7: Failed subtask text color → red

**File**: `render.rs`

Already addressed by Fix 2 — the per-status text style change sets failed text to `theme.red`. No additional change needed beyond Fix 2.

---

## Non-mockup fixes (code quality)

### Fix 8: `sum_agent_stats` — track raw values

**File**: `build.rs`

Replace the format-then-reparse pattern with raw value tracking.

**Change**: Make `AgentStat` carry raw values alongside formatted strings:

```rust
struct AgentStat {
    agent: String,
    sessions: usize,
    total_secs: i64,
    total_tokens: u64,
    elapsed: String,   // formatted from total_secs
    tokens: String,    // formatted from total_tokens
}
```

`sum_agent_stats` sums `total_secs` and `total_tokens` directly. Delete `parse_duration` and `parse_tokens`.

---

### Fix 9: Filter non-work subtasks from subtask table

**File**: `build.rs`, `fix.rs`

`get_subtasks(graph, epic_id)` returns ALL children of the epic, including the decompose task, review task, and fix tasks. The subtask table should only show work subtasks.

**Change**: Add a `get_work_subtasks` helper to `helpers.rs`:

```rust
/// Get subtasks that represent actual work items (excludes decompose, review, fix, orchestrator).
pub fn get_work_subtasks<'a>(graph: &'a TaskGraph, parent_id: &str) -> Vec<&'a Task> {
    let mut children = graph.children_of(parent_id);
    children.retain(|t| {
        !matches!(
            t.task_type.as_deref(),
            Some("decompose") | Some("review") | Some("fix") | Some("orchestrator")
        )
    });
    children.sort_by_key(|t| t.created_at);
    children
}
```

Update `build.rs` and `fix.rs` to use `get_work_subtasks` for subtask table population, keeping `get_subtasks` for cases where all children are needed (e.g., counting fix iterations).

---

## Execution Order

Fixes are mostly independent but share these dependencies:

```
Fix 1 (subtask header)     — standalone
Fix 2 (subtask text color) — standalone
Fix 3 (lane structure)     — standalone
Fix 4 (◌ symbol)           — touches same code as Fix 2 (render.rs Subtask arm)
Fix 5 (blank lines)        — standalone
Fix 6 (symbol weight)      — standalone, but grep all files first
Fix 7                      — subsumed by Fix 2
Fix 8 (raw stats)          — standalone
Fix 9 (filter subtasks)    — standalone
```

**Recommended order**: 6 → 5 → 1 → 2+4 (together) → 3 → 9 → 8

Rationale: Fix 6 is a simple find-replace warmup. Fix 5 is a two-line change. Fix 1 adds a new LineStyle and renderer. Fixes 2+4 both touch the Subtask rendering arm. Fix 3 is the largest rewrite. Fixes 8-9 are code quality and can land last.

---

## Files Changed Summary

| File | Fixes |
|------|-------|
| `cli/src/tui/app.rs` | 1 (new LineStyle), 4 (new SubtaskStatus) |
| `cli/src/tui/theme.rs` | 4 (new symbol), 6 (update symbols) |
| `cli/src/tui/render.rs` | 1 (render SubtaskHeader), 2 (per-status text), 4 (render ◌) |
| `cli/src/tui/components.rs` | 3 (rewrite loop_block), 5 (blank lines in subtask_table) |
| `cli/src/tui/screens/helpers.rs` | 4 (subtask_data_from_task fn), 9 (get_work_subtasks) |
| `cli/src/tui/screens/build.rs` | 6 (symbol refs), 8 (raw stats), 9 (use get_work_subtasks) |
| `cli/src/tui/screens/fix.rs` | 6 (symbol refs), 9 (use get_work_subtasks) |
| `cli/src/tui/screens/review.rs` | 6 (symbol refs) |

---

## Tests

Each fix includes its own test. Summary of new/updated tests:

| Test | Fix | Location |
|------|-----|----------|
| Subtask header renders `[` not `合` | 1 | `render.rs` tests |
| Failed subtask text is red | 2 | `render.rs` tests |
| Done subtask text is dim | 2 | `render.rs` tests |
| Active subtask text is fg | 2 | `render.rs` tests |
| Loop block has hierarchical structure | 3 | `components.rs` tests |
| Loop block blank line between lanes | 3 | `components.rs` tests |
| `PendingUnassigned` renders `◌` | 4 | `render.rs` tests |
| `Pending` renders `○` | 4 | `render.rs` tests |
| Subtask table has blank after opening `---` | 5 | `components.rs` tests |
| Subtask table has blank before closing `---` | 5 | `components.rs` tests |
| `SYM_CHECK` is `✔` (heavy) | 6 | `theme.rs` tests |
| `sum_agent_stats` sums raw values correctly | 8 | `build.rs` tests |
| `get_work_subtasks` excludes decompose/review | 9 | `helpers.rs` tests |
