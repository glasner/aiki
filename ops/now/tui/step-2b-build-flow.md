# Step 2b: `aiki build` Flow

**Date**: 2026-03-21
**Status**: Ready
**Priority**: P1
**Phase**: 2 — Pipeline flows
**Depends on**: 2a (validates infrastructure)

---

## Problem

The build flow is the most complex pipeline: plan → decompose → subtask table → loop → done. It exercises every shared component (phase headers, subtask table, lane blocks, progressive dimming). Currently handled by the 1240-line `chat_builder.rs` which maps TaskGraph into the generic Chat model.

---

## Fix: Build-specific pipeline builder

A single function that reads the TaskGraph and returns `Vec<Line>` for a build.

### Screen states (from [screen-states.md](screen-states.md), Flow 2)

States 2.0-2.9: loading → plan → decompose (starting/active/done) → subtask table → lanes assigned → agents active → mid-build → subtask fails → all done.

### Implementation

The view function uses components from `components.rs`:

```rust
pub fn view(graph: &TaskGraph, epic_id: &str, plan_path: &str, window: &WindowState) -> Vec<Line> {
    let mut lines = vec![];
    let mut group = 0;
    let subtasks = get_subtasks(graph, epic_id);

    // 1. Plan phase (always done — plan file exists)
    lines.extend(components::phase(group, "plan", Some("claude"), vec![
        ChildLine { text: format!("✓ {}", plan_path), style: ChildStyle::Done, meta: None },
    ]));
    group += 1;

    // 2. Section header
    lines.extend(components::section_header("Initial Build"));

    // 3. Decompose phase
    if let Some(decompose_task) = find_decompose_task(graph, epic_id) {
        let children = match decompose_task.status {
            TaskStatus::Open => vec![ChildLine::active("Reading task graph...")],
            TaskStatus::InProgress => vec![ChildLine::active_with_elapsed(
                &decompose_task.latest_heartbeat(), decompose_task.elapsed_str(),
            )],
            TaskStatus::Closed => vec![ChildLine::normal(
                &format!("{} subtasks created", subtasks.len()), decompose_task.elapsed_str(),
            )],
            _ => vec![],
        };
        lines.extend(components::phase(group, "decompose", decompose_task.agent_label(), children));
        group += 1;
    }

    // 4. Subtask table
    let loading = !subtasks.is_empty() || decompose_task_in_progress;
    let data: Vec<SubtaskData> = subtasks.iter().map(|s| s.into()).collect();
    lines.extend(components::subtask_table(group, &epic_short_id, &epic_name, &data, loading));

    // 5. Loop phase (if build has started)
    if let Some(lanes) = derive_lanes(graph, epic_id) {
        let lane_data: Vec<LaneData> = lanes.iter().map(|l| l.into()).collect();
        lines.extend(components::loop_block(group + 1, &lane_data));
    }

    // 6. Review phase (graph-driven — appears when review subtasks exist)
    if let Some(review) = find_build_review(graph, epic_id) {
        lines.extend(review_phase_lines(group, review));
        group += 1;
    }

    // 7. Fix iterations (graph-driven — appears when fix subtasks exist)
    // No separate BuildFix variant needed. When `aiki build -f` spawns
    // fix tasks, they appear in the graph and build::view() renders them.
    for iteration in 2..=current_iteration(graph, epic_id) {
        lines.extend(components::section_header(&format!("Iteration {}", iteration)));
        if let Some(fix_parent) = find_fix_parent(graph, epic_id, iteration) {
            lines.extend(fix::view(graph, &fix_parent, &review_id, window));
        }
    }

    // 8. Summary (if all done)
    if is_build_complete(graph, epic_id) {
        lines.extend(components::blank());
        // ... summary phase (includes max iterations warning if applicable)
    }

    lines
}
```

### Key behaviors

**Subtask table updates in place.** As subtasks transition (`○` → `◌` → `▸` → `✓`), the view function re-runs and emits updated lines. `Viewport::Inline` handles cursor-up to overwrite.

**Lane derivation.** Lanes are derived from task metadata (which agent session owns which subtasks). The existing `derive_lanes()` logic in `chat_builder.rs` can be extracted and simplified — it reads task `session_id` fields to group subtasks into lanes.

**Progressive dimming.** Handled by `apply_dimming()` scanning for the last active line. When the loop phase is active, earlier groups (plan, decompose) are dimmed.

**References, not clones.** View function takes `&TaskGraph` and uses `&str` references from task fields. No cloning task names into intermediate structures.

### Integration point

Wiring `build::view()` into `build.rs` happens in Phase 3 (cutover). The command will create a `Model` with `Screen::Build { epic_id, plan_path }` and call `tui::run()`. This step only creates the pure view function.

### Graph-driven rendering (absorbs step 2e)

The old plan had a separate `build_fix_lines()` in step 2e. This is no longer needed. `build::view()` is **graph-driven** — it renders whatever tasks exist in the graph:

- If the graph has review subtasks → review section appears
- If the graph has fix subtasks → fix/iteration sections appear
- If not → they don't

The `Screen::Build` variant no longer carries a `fix: bool` field. Flags like `--fix` affect what the *command orchestrates* (spawning fix/review tasks), not what the TUI renders. The TUI just renders what's in the graph.

**Iteration numbering:** The first build is "Initial Build" (no number). Fix cycles are "Iteration 2", "Iteration 3", etc.

**Max iterations:** `MAX_QUALITY_ITERATIONS = 10` (defined in the build command logic, not the TUI). When `iteration == MAX_QUALITY_ITERATIONS` and review still has issues, the summary includes a warning:
```
[80 cols]
 合 build completed — path                                ← bold+fg 合, bold+fg text
 ⎿ ⚠ Max iterations reached — 2 issues remain            ← dim ⎿, yellow ⚠, bold text
 ⎿ Total 2h15m — 8.2M tokens                             ← dim ⎿, bold text
```

### Files changed

| File | Change |
|------|--------|
| `cli/src/tui/screens/build.rs` | New: `view()` + lane derivation + graph-driven fix/iteration rendering (~300 lines) |

### Tests

- `insta` snapshots for key states: plan loaded, decompose active, subtasks arriving, mid-build, subtask failed, all done.
- Lane derivation unit tests: verify correct grouping from task metadata.
- Width-variant snapshots at 40, 80, 120 columns.
- `insta` snapshots for build-with-fix states (absorbed from step 2e): review approved, iteration 2 with fix, max iterations warning.
- **Cold-start reattachment test:** Construct a graph that's 70% done (plan done, decompose done, 3/5 subtasks done, 2 active), pass to `build::view()`, verify output has dimmed completed phases and active current phase. This validates `aiki build --attach` can reconstruct the full view from a mid-build graph.
