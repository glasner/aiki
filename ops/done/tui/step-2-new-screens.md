# Step 2: Screen View Functions

**Date**: 2026-03-23
**Status**: Ready
**Priority**: P1
**Phase**: 2 — View functions
**Depends on**: 1a (inline renderer), 1b (data model + Elm core), 1c (shared components + line renderer)
**Consolidates**: step-2a (task run), step-2b (build), step-2c (review), step-2d (fix), step-2e (static views)

---

## Problem

The `view()` stub in `app.rs` returns an empty `Vec<Line>`. Six screen types need view functions that read the `TaskGraph` and produce lines using the shared components from step 1c. These are all pure functions — no I/O, no terminal, no event loop.

---

## Architecture Recap

```
Model.screen ──→ view() dispatch ──→ screen-specific view fn ──→ Vec<Line>
                                          │
                                     components::phase()
                                     components::subtask_table()
                                     components::loop_block()
                                     components::issues()
                                     components::section_header()
```

View rendering is **graph-driven**: view functions adapt to whatever tasks exist in the graph. The `Screen` enum defines lifecycle/exit behavior (when to stop polling), not what to render. Flags like `--fix` affect what the command orchestrates (spawning tasks), and the TUI renders whatever appears in the graph.

**References, not clones.** View functions take `&TaskGraph` and use `&str` references from task fields. No cloning task names into intermediate structures.

---

## New Files

| File | Screen | Est. lines | Key components used |
|------|--------|-----------|-------------------|
| `cli/src/tui/screens/task_run.rs` | `TaskRun` | ~80 | `phase`, `subtask_table` |
| `cli/src/tui/screens/build.rs` | `Build` | ~300 | All components, lane derivation, graph-driven fix/iteration |
| `cli/src/tui/screens/review.rs` | `Review` | ~100 | `phase`, `issues` |
| `cli/src/tui/screens/fix.rs` | `Fix` | ~120 | Composes build + review helpers |
| `cli/src/tui/screens/epic_show.rs` | `EpicShow` | ~30 | `phase`, `subtask_table` |
| `cli/src/tui/screens/review_show.rs` | `ReviewShow` | ~30 | `phase`, `issues` |
| `cli/src/tui/screens/helpers.rs` | — | ~100 | Graph query helpers shared across views |
| **Total** | | **~760** | |

Plus: wire `view()` in `app.rs` to dispatch to screen-specific functions (replacing the stub).

---

## Task Method Requirements

View functions assume these methods exist on the `Task` type. Add if not present:

| Method | Returns | Purpose |
|--------|---------|---------|
| `agent_label()` | `Option<&str>` | Agent name for phase header, e.g. `"claude"` |
| `latest_heartbeat()` | `String` | Most recent heartbeat text from comments |
| `elapsed_str()` | `Option<String>` | Formatted elapsed time since `started_at`, e.g. `"1m 23s"` |
| `effective_summary()` | `String` | Summary or fallback to task name for done state |
| `stopped_reason()` | `String` | Reason task was stopped, or fallback |
| `short_id()` | `String` | First 6 chars of task ID for display |
| `status.is_terminal()` | `bool` | True for `Closed` or `Stopped` |

---

## ChildLine Convenience Constructors

Add to `components.rs` to reduce boilerplate in view functions:

```rust
impl ChildLine {
    pub fn active(text: &str) -> Self {
        Self { text: text.to_string(), meta: None, style: ChildStyle::Active }
    }
    pub fn active_with_elapsed(text: &str, elapsed: Option<String>) -> Self {
        Self { text: text.to_string(), meta: elapsed, style: ChildStyle::Active }
    }
    pub fn done(text: &str, elapsed: Option<String>) -> Self {
        Self { text: text.to_string(), meta: elapsed, style: ChildStyle::Done }
    }
    pub fn error(text: &str, elapsed: Option<String>) -> Self {
        Self { text: text.to_string(), meta: elapsed, style: ChildStyle::Error }
    }
    pub fn warning(text: &str) -> Self {
        Self { text: text.to_string(), meta: None, style: ChildStyle::Warning }
    }
    pub fn normal(text: &str, elapsed: Option<String>) -> Self {
        Self { text: text.to_string(), meta: elapsed, style: ChildStyle::Normal }
    }
    pub fn bold(text: &str) -> Self {
        Self { text: text.to_string(), meta: None, style: ChildStyle::Bold }
    }
}
```

---

## 2.1 — `task_run::view()`

Monitors a single task. If the task has subtasks (parent task), shows a subtask table.

### States (see [screen-states.md](screen-states.md), Flow 1)

| State | Description |
|-------|-------------|
| 1.0a–d | Loading → resolving agent → creating workspace → starting session |
| 1.1 | Active leaf — heartbeat + elapsed |
| 1.2 | Done leaf — summary + hint |
| 1.3 | Failed leaf — error + hint |
| 1.4 | Detached (Ctrl+C) |
| 1.5–1.7 | Parent task — subtask table with mixed statuses |
| 1.8 | Parent task — all done |
| 1.9 | Parent task — subtask failed |

### Implementation

```rust
// cli/src/tui/screens/task_run.rs

pub fn view(graph: &TaskGraph, task_id: &str, window: &WindowState) -> Vec<Line> {
    let task = match graph.tasks.get(task_id) {
        Some(t) => t,
        None => return loading_lines(),
    };
    let subtasks = get_subtasks(graph, task_id);
    let mut lines = vec![];

    // Task phase — heartbeat or result
    let active = !task.status.is_terminal();
    let children = match task.status {
        TaskStatus::Open => vec![
            ChildLine::active("starting session..."),
        ],
        TaskStatus::InProgress if subtasks.is_empty() => vec![
            ChildLine::active_with_elapsed(&task.latest_heartbeat(), task.elapsed_str()),
        ],
        TaskStatus::InProgress => vec![
            ChildLine::normal(&format_progress(&subtasks), task.elapsed_str()),
        ],
        TaskStatus::Closed => vec![
            ChildLine::done(&task.effective_summary(), task.elapsed_str()),
        ],
        TaskStatus::Stopped => vec![
            ChildLine::error(&task.stopped_reason(), task.elapsed_str()),
        ],
    };
    lines.extend(components::phase(0, "task", task.agent_label(), active, children));

    // Subtask table (if parent task)
    if !subtasks.is_empty() {
        let data: Vec<SubtaskData> = subtasks.iter().map(|s| s.into()).collect();
        lines.extend(components::subtask_table(0, &task.short_id(), &task.name, &data, false));
    }

    // Hint text when done
    if task.status.is_terminal() {
        lines.extend(components::blank());
        lines.push(Line {
            indent: 0,
            text: format!("Run `aiki task show {}` for details.", &task.short_id()),
            meta: None,
            style: LineStyle::Dim,
            group: 0,
            dimmed: false,
        });
    }

    lines
}
```

### Tests

- Snapshot: leaf task in Open, InProgress, Closed, Stopped states
- Snapshot: parent task with subtask table (mixed statuses)
- Snapshot: parent task all done
- Snapshot: parent task with failed subtask

---

## 2.2 — `build::view()`

The most complex view. Phases: plan → section header → decompose → subtask table → loop → (review → fix iterations) → summary.

### States (see [screen-states.md](screen-states.md), Flow 2 + Flow 3)

| State | Description |
|-------|-------------|
| 2.0–2.1 | Validating → plan loaded |
| 2.2a–2.4b | Decompose: reading task graph → finding epic → creating workspace → active → subtasks arriving |
| 2.5 | Decompose done, subtasks populated |
| 2.6–2.8 | Lanes assigned → agents active → mid-build |
| 2.9 | Subtask fails |
| 2.10 | All done (summary with per-agent breakdown) |
| 3.10–3.12 | Review starting → active → approved (build done) |
| 3.13–3.18 | Review finds issues → Iteration 2 (fix → decompose → loop → review) |
| 3.19–3.20 | Max iterations warning, multi-iteration complete |

### Implementation

```rust
// cli/src/tui/screens/build.rs

pub fn view(graph: &TaskGraph, epic_id: &str, plan_path: &str, window: &WindowState) -> Vec<Line> {
    let mut lines = vec![];
    let mut group: u16 = 0;

    // 1. Plan phase (always "done" — plan file exists before build starts)
    lines.extend(components::phase(group, "plan", Some("claude"), false, vec![
        ChildLine::done(&format!("✓ {}", plan_path), None),
    ]));
    group += 1;

    // 2. Section header
    lines.extend(components::section_header(group, "Initial Build"));

    // 3. Decompose phase
    if let Some(decompose) = find_decompose_task(graph, epic_id) {
        let active = !decompose.status.is_terminal();
        let children = match decompose.status {
            TaskStatus::Open => vec![ChildLine::active("Reading task graph...")],
            TaskStatus::InProgress => vec![ChildLine::active_with_elapsed(
                &decompose.latest_heartbeat(), decompose.elapsed_str(),
            )],
            TaskStatus::Closed => {
                let subtask_count = get_subtasks(graph, epic_id).len();
                vec![ChildLine::normal(
                    &format!("{} subtasks created", subtask_count), decompose.elapsed_str(),
                )]
            }
            _ => vec![],
        };
        lines.extend(components::phase(group, "decompose", decompose.agent_label(), active, children));
        group += 1;
    }

    // 4. Subtask table (show when subtasks exist or decompose is still running)
    let subtasks = get_subtasks(graph, epic_id);
    if !subtasks.is_empty() || decompose_in_progress(graph, epic_id) {
        let data: Vec<SubtaskData> = subtasks.iter().map(|s| s.into()).collect();
        let loading = subtasks.is_empty(); // shows "..." placeholder
        let epic = &graph.tasks[epic_id];
        lines.extend(components::subtask_table(group, &epic.short_id(), &epic.name, &data, loading));
    }

    // 5. Loop phase (when lanes have been assigned)
    if let Some(lanes) = derive_lanes(graph, epic_id) {
        let lane_data: Vec<LaneData> = lanes.iter().map(|l| l.into()).collect();
        lines.extend(components::loop_block(group + 1, &lane_data));
        group += 2;
    }

    // 6. Review phase (graph-driven — appears when review subtask exists)
    if let Some(review) = find_build_review(graph, epic_id) {
        lines.extend(review_phase_lines(group, &review, graph));
        group += 1;
    }

    // 7. Fix iterations (graph-driven — appears when fix subtasks exist)
    //    No separate BuildFix variant needed. When `aiki build -f` spawns
    //    fix tasks, they appear in the graph and this loop renders them.
    let review_id = review_id_for_epic(graph, epic_id);
    for iteration in 2..=current_iteration(graph, epic_id) {
        lines.extend(components::section_header(group, &format!("Iteration {}", iteration)));
        if let Some(fix_parent) = find_fix_parent(graph, epic_id, iteration) {
            lines.extend(fix::view(graph, &fix_parent, &review_id, window));
        }
    }

    // 8. Summary (when build is complete)
    if is_build_complete(graph, epic_id) {
        lines.extend(build_summary_lines(graph, epic_id, plan_path, group));
    }

    lines
}
```

### Build summary implementation

The summary renders after a `---` separator when all phases complete:

```rust
fn build_summary_lines(graph: &TaskGraph, epic_id: &str, plan_path: &str, group: u16) -> Vec<Line> {
    let mut lines = vec![];
    let mut children = vec![];

    // Max iterations warning (if applicable)
    if let Some(warning) = max_iterations_warning(graph, epic_id) {
        children.push(ChildLine::warning(&warning));
    }

    // Per-agent breakdown: aggregate sessions, time, and tokens by agent type.
    // Only show per-agent lines when multiple agent types were used.
    let agent_stats = aggregate_agent_stats(graph, epic_id);
    if agent_stats.len() > 1 {
        for stat in &agent_stats {
            children.push(ChildLine::normal(
                &format!("{}: {} session{} — {} — {} tokens",
                    stat.agent, stat.sessions,
                    if stat.sessions == 1 { "" } else { "s" },
                    stat.elapsed, stat.tokens),
                None,
            ));
        }
    }

    // Total line (always present, bold)
    let totals = sum_agent_stats(&agent_stats);
    children.push(ChildLine::bold(&format!(
        "Total: {} session{} — {} — {} tokens",
        totals.sessions,
        if totals.sessions == 1 { "" } else { "s" },
        totals.elapsed, totals.tokens,
    )));

    lines.extend(components::separator(group));
    lines.extend(components::blank());
    lines.extend(components::phase(group, &format!("build completed — {}", plan_path), None, false, children));
    lines.extend(components::blank());

    // Hint
    let epic = &graph.tasks[epic_id];
    lines.push(Line {
        indent: 0,
        text: format!("Run `aiki task diff {}` to see changes.", &epic.short_id()),
        meta: None,
        style: LineStyle::Dim,
        group,
        dimmed: false,
    });

    lines
}
```

### Review phase helper (shared with fix)

Renders a review phase + issue list within a build or fix pipeline:

```rust
fn review_phase_lines(group: u16, review: &Task, graph: &TaskGraph) -> Vec<Line> {
    let mut lines = vec![];
    let issues = extract_issues(review);
    let active = !review.status.is_terminal();

    let children = match review.status {
        TaskStatus::Open => vec![ChildLine::active("starting session...")],
        TaskStatus::InProgress => vec![ChildLine::active_with_elapsed(
            &review.latest_heartbeat(), review.elapsed_str(),
        )],
        TaskStatus::Closed if !issues.is_empty() => vec![
            ChildLine::normal(&format!("Found {} issues", issues.len()), review.elapsed_str()),
        ],
        TaskStatus::Closed => vec![
            ChildLine::done("✓ approved", review.elapsed_str()),
        ],
        _ => vec![],
    };

    lines.extend(components::phase(group, "review", review.agent_label(), active, children));

    // Inline issue list
    if !issues.is_empty() {
        lines.extend(components::blank());
        let issue_texts: Vec<String> = issues.iter().map(|i| i.title.clone()).collect();
        lines.extend(components::issues(group, &issue_texts));
    }

    lines
}
```

### Key behaviors

**Subtask table updates in place.** As subtasks transition (`○` → `◌` → `▸` → `✓`), the view function re-runs and emits updated lines. `Viewport::Inline` handles cursor-up to overwrite.

**Lane derivation.** Lanes are derived from task metadata — which agent session owns which subtasks. The existing `derive_lanes()` logic in `chat_builder.rs` reads task `session_id` fields to group subtasks into lanes. Extract and simplify.

**Graph-driven fix/iteration rendering.** `build::view()` scans the graph for review/fix subtasks and renders them when present. The `Screen::Build` variant doesn't carry a `fix: bool` field. Flags like `--fix` affect what the command orchestrates (spawning fix/review tasks), not what the TUI renders.

**Iteration numbering.** First build is "Initial Build" (no number). Fix cycles are "Iteration 2", "Iteration 3", etc.

**Max iterations.** `MAX_QUALITY_ITERATIONS = 10` (defined in the build command logic, not the TUI). When `iteration == MAX_QUALITY_ITERATIONS` and review still has issues, the summary shows a warning:
```
合 build completed — path
⎿ ⚠ Max iterations reached — 2 issues remain
⎿ Total: 10 sessions — 2h15m — 8.2M tokens
```

**Summary per-agent breakdown.** Aggregate sessions, time, and tokens by agent type. When only one agent type was used, skip per-agent lines — just show the total.

### Tests

- `insta` snapshots for: plan loaded, decompose active, subtasks arriving, mid-build, subtask failed, all done
- Lane derivation unit tests: verify correct grouping from task metadata
- Width-variant snapshots at 40, 80, 120 columns
- Build-with-fix states: review approved, iteration 2 with fix, max iterations warning
- **Cold-start reattachment test:** construct a graph 70% done (plan done, decompose done, 3/5 subtasks done, 2 active), pass to `build::view()`, verify output has dimmed completed phases and active current phase. Validates `aiki build --attach` reconstructing the full view from a mid-build graph.

---

## 2.3 — `review::view()`

Four review scopes (plan, code, task, session) share the same rendering structure. A single view function handles all.

### States (see [screen-states.md](screen-states.md), Flows 4-7)

All four scopes follow the same shape:

```
⠹ review (<agent>)
⎿ <target>
⎿ <heartbeat or result>

    1. <issue>
    2. <issue>
```

### Scope differences

| Scope | Active heartbeat | Done (issues) | Done (clean) |
|-------|------------------|---------------|--------------|
| Plan | `Reviewing ops/now/...` | `Found N issues` | `✓ approved` |
| Code | `Reviewing diff for ...` | `Found N issues` | `✓ approved` |
| Task | `Reviewing changes for "..."` | `Found N issues` | `✓ approved` |
| Session | `Reviewing N completed tasks...` | `Found N issues across M tasks` | `✓ approved` |

Session scope additionally prefixes issues with `[task name]` for grouping:
```
    1. [Add get_repo_root helper] get_repo_root shells out to `jj root` on every ca
    2. [Lock task writes] acquire_named_lock uses wrong error variant
```

### Implementation

```rust
// cli/src/tui/screens/review.rs

pub fn view(graph: &TaskGraph, review_id: &str, target: &str, window: &WindowState) -> Vec<Line> {
    let mut lines = vec![];
    let review_task = match graph.tasks.get(review_id) {
        Some(t) => t,
        None => return loading_lines(),
    };
    let issues = extract_issues(review_task);
    let active = !review_task.status.is_terminal();

    // Review phase — target as first child, heartbeat/result as second
    let mut children = vec![
        ChildLine::normal(target, None),  // e.g. "path.md", "path.md --code", "[id] name", "4 completed tasks"
    ];

    match review_task.status {
        TaskStatus::Open => children.push(ChildLine::active("creating isolated workspace...")),
        TaskStatus::InProgress => children.push(ChildLine::active_with_elapsed(
            &review_task.latest_heartbeat(), review_task.elapsed_str(),
        )),
        TaskStatus::Closed if !issues.is_empty() => {
            children.push(ChildLine::normal(&format!("Found {} issues", issues.len()), None));
        }
        TaskStatus::Closed => {
            children.push(ChildLine::done("✓ approved", None));
        }
        _ => {}
    }

    lines.extend(components::phase(0, "review", review_task.agent_label(), active, children));

    // Issue list (if any — never dimmed, user needs to read these)
    if !issues.is_empty() {
        lines.extend(components::blank());
        let issue_texts: Vec<String> = issues.iter().map(|i| i.title.clone()).collect();
        lines.extend(components::issues(0, &issue_texts));
    }

    lines
}
```

### Issue extraction

Issues are stored as task comments with `type: issue` in their data fields. The existing `extract_review_issues()` in `review.rs` already does this. Reuse it.

### Tests

- `insta` snapshots: plan scope (issues, approved), code scope, task scope, session scope (grouped issues)
- Session scope: verify `[task name]` prefix rendering

---

## 2.4 — `fix::view()`

Fix pipeline: fix-plan → decompose → subtask table → loop → review of fixes → regression review. Reuses build and review components but has its own orchestration (quality loop with up to 10 iterations).

### States (see [screen-states.md](screen-states.md), Flow 8)

| State | Description |
|-------|-------------|
| 8.0–8.1 | Fix starting → plan written |
| 8.2–8.5 | Decompose → followup table → loop active |
| 8.6–8.7 | Review of fixes → regression review |
| 8.8 | No actionable issues (short-circuit) |

**No-actionable-issues short-circuit:**
```
合 fix (claude)
⎿ ✓ approved — no actionable issues
```

### Implementation

```rust
// cli/src/tui/screens/fix.rs

pub fn view(graph: &TaskGraph, fix_parent_id: &str, review_id: &str, window: &WindowState) -> Vec<Line> {
    let mut lines = vec![];
    let mut group: u16 = 0;

    // Short-circuit: no actionable issues
    if no_actionable_issues(graph, review_id) {
        lines.extend(components::phase(group, "fix", Some("claude"), false, vec![
            ChildLine::done("✓ approved — no actionable issues", None),
        ]));
        return lines;
    }

    // Fix plan phase
    let fix_task = match graph.tasks.get(fix_parent_id) {
        Some(t) => t,
        None => return loading_lines(),
    };
    let fix_active = !fix_task.status.is_terminal();
    let fix_children = match fix_task.status {
        TaskStatus::Open => vec![ChildLine::active("starting session...")],
        TaskStatus::InProgress => vec![ChildLine::active_with_elapsed(
            &fix_task.latest_heartbeat(), fix_task.elapsed_str(),
        )],
        TaskStatus::Closed => vec![ChildLine::done(
            &fix_task.effective_summary(), fix_task.elapsed_str(),
        )],
        _ => vec![],
    };
    lines.extend(components::phase(group, "fix", fix_task.agent_label(), fix_active, fix_children));
    group += 1;

    // Decompose phase (reuses same pattern as build)
    if let Some(decompose) = find_decompose_task(graph, fix_parent_id) {
        let active = !decompose.status.is_terminal();
        let children = match decompose.status {
            TaskStatus::Open => vec![ChildLine::active("Reading task graph...")],
            TaskStatus::InProgress => vec![ChildLine::active_with_elapsed(
                &decompose.latest_heartbeat(), decompose.elapsed_str(),
            )],
            TaskStatus::Closed => {
                let count = get_subtasks(graph, fix_parent_id).len();
                vec![ChildLine::normal(&format!("{} subtasks created", count), decompose.elapsed_str())]
            }
            _ => vec![],
        };
        lines.extend(components::phase(group, "decompose", decompose.agent_label(), active, children));
        group += 1;
    }

    // Subtask table
    let subtasks = get_subtasks(graph, fix_parent_id);
    if !subtasks.is_empty() || decompose_in_progress(graph, fix_parent_id) {
        let data: Vec<SubtaskData> = subtasks.iter().map(|s| s.into()).collect();
        let loading = subtasks.is_empty();
        lines.extend(components::subtask_table(group, &fix_task.short_id(), "Followup", &data, loading));
    }

    // Loop phase
    if let Some(lanes) = derive_lanes(graph, fix_parent_id) {
        let lane_data: Vec<LaneData> = lanes.iter().map(|l| l.into()).collect();
        lines.extend(components::loop_block(group + 1, &lane_data));
        group += 2;
    }

    // Review of fixes
    if let Some(review) = find_fix_review(graph, fix_parent_id) {
        lines.extend(review_phase_lines(group, &review, graph));
        group += 1;
    }

    // Regression review (checks original review target for regressions)
    if let Some(regression) = find_regression_review(graph, review_id) {
        let active = !regression.status.is_terminal();
        let children = match regression.status {
            TaskStatus::Open => vec![ChildLine::active("starting session...")],
            TaskStatus::InProgress => vec![ChildLine::active_with_elapsed(
                &regression.latest_heartbeat(), regression.elapsed_str(),
            )],
            TaskStatus::Closed => {
                let issues = extract_issues(&regression);
                if issues.is_empty() {
                    vec![ChildLine::done("✓ no regressions", regression.elapsed_str())]
                } else {
                    vec![ChildLine::error(
                        &format!("Found {} regressions", issues.len()), regression.elapsed_str(),
                    )]
                }
            }
            _ => vec![],
        };
        lines.extend(components::phase(group, "review for regressions", regression.agent_label(), active, children));
    }

    lines
}
```

### Quality loop rendering

When the fix review finds more issues, a new iteration starts. `build::view()` (section 2.2) handles iteration headers — it calls `fix::view()` for each iteration:

```rust
// In build::view():
lines.extend(components::section_header(group, &format!("Iteration {}", iteration)));
lines.extend(fix::view(graph, fix_parent_id, review_id, window));
```

### Tests

- Snapshot: no-actionable-issues shortcut
- Snapshot: single iteration (fix → decompose → loop → review approved)
- Snapshot: multi-iteration with `Iteration 2` header
- Snapshot: max iterations warning

---

## 2.5 — Static Views (`epic_show::view()`, `review_show::view()`)

Static detail views — render once and exit, no live polling. Used by `aiki epic show <id>` and `aiki review show <id>`. The `Screen::EpicShow` and `Screen::ReviewShow` variants have different exit semantics from the live flows: `is_finished()` always returns true, so the Elm loop renders once and exits.

### `epic_show::view()`

```rust
// cli/src/tui/screens/epic_show.rs

pub fn view(graph: &TaskGraph, epic_id: &str, window: &WindowState) -> Vec<Line> {
    let epic = match graph.tasks.get(epic_id) {
        Some(t) => t,
        None => return vec![],
    };
    let mut lines = vec![];
    let subtasks = get_subtasks(graph, epic_id);

    // Epic header
    let status_child = match epic.status {
        TaskStatus::Closed => ChildLine::done(&epic.effective_summary(), epic.elapsed_str()),
        TaskStatus::InProgress => ChildLine::normal(
            &format_progress(&subtasks), epic.elapsed_str(),
        ),
        TaskStatus::Stopped => ChildLine::error(&epic.stopped_reason(), epic.elapsed_str()),
        _ => ChildLine::normal("pending", None),
    };
    lines.extend(components::phase(0, &epic.name, epic.agent_label(), false, vec![status_child]));

    // Subtask table
    let data: Vec<SubtaskData> = subtasks.iter().map(|s| s.into()).collect();
    lines.extend(components::subtask_table(1, &epic.short_id(), &epic.name, &data, false));

    lines
}
```

### `review_show::view()`

```rust
// cli/src/tui/screens/review_show.rs

pub fn view(graph: &TaskGraph, review_id: &str, window: &WindowState) -> Vec<Line> {
    let review = match graph.tasks.get(review_id) {
        Some(t) => t,
        None => return vec![],
    };
    let mut lines = vec![];
    let issues = extract_issues(review);

    // Review header
    let status_child = if !issues.is_empty() {
        ChildLine::normal(&format!("Found {} issues", issues.len()), review.elapsed_str())
    } else {
        ChildLine::done("✓ approved", review.elapsed_str())
    };
    lines.extend(components::phase(0, "review", review.agent_label(), false, vec![status_child]));

    // Issues
    if !issues.is_empty() {
        lines.extend(components::blank());
        let texts: Vec<String> = issues.iter().map(|i| i.title.clone()).collect();
        lines.extend(components::issues(1, &texts));
    }

    lines
}
```

### Tests

- Snapshot: epic with all subtasks done
- Snapshot: epic with mixed status subtasks
- Snapshot: review with issues
- Snapshot: review approved (no issues)

---

## Wiring `view()` in `app.rs`

Replace the stub:

```rust
// cli/src/tui/app.rs

pub fn view(model: &Model) -> Vec<Line> {
    match &model.screen {
        Screen::TaskRun { task_id } =>
            screens::task_run::view(&model.graph, task_id, &model.window),
        Screen::Build { epic_id, plan_path } =>
            screens::build::view(&model.graph, epic_id, plan_path, &model.window),
        Screen::Review { review_id, target } =>
            screens::review::view(&model.graph, review_id, target, &model.window),
        Screen::Fix { fix_parent_id, review_id } =>
            screens::fix::view(&model.graph, fix_parent_id, review_id, &model.window),
        Screen::EpicShow { epic_id } =>
            screens::epic_show::view(&model.graph, epic_id, &model.window),
        Screen::ReviewShow { review_id } =>
            screens::review_show::view(&model.graph, review_id, &model.window),
    }
}
```

---

## Helper Functions

Shared across view functions. Put in `cli/src/tui/screens/helpers.rs`.

### Graph query helpers

| Function | Used by | Purpose |
|----------|---------|---------|
| `get_subtasks(graph, parent_id) → Vec<&Task>` | task_run, build, fix, epic_show | Ordered subtasks for a parent (by creation time) |
| `derive_lanes(graph, epic_id) → Option<Vec<Lane>>` | build, fix | Group subtasks into lanes by agent session. Reads `claimed_by_session` on tasks. Returns `None` if no lanes assigned yet. |
| `extract_issues(task) → Vec<Issue>` | review, review_show, build, fix | Parse issue comments from task. Reuse existing `extract_review_issues()`. |
| `find_decompose_task(graph, epic_id) → Option<&Task>` | build, fix | Find the decompose subtask of an epic |
| `find_build_review(graph, epic_id) → Option<&Task>` | build | Find the review subtask in a build epic |
| `find_fix_review(graph, fix_parent) → Option<&Task>` | fix | Find the review subtask in a fix parent |
| `find_regression_review(graph, review_id) → Option<&Task>` | fix | Find the regression review task |
| `find_fix_parent(graph, epic_id, iteration) → Option<String>` | build | Find the fix parent task for a specific iteration |
| `review_id_for_epic(graph, epic_id) → String` | build | Get the review task ID associated with an epic |
| `current_iteration(graph, epic_id) → u16` | build | Count fix iterations in the graph |
| `is_build_complete(graph, epic_id) → bool` | build | Check if all phases (including fix iterations) are done |
| `decompose_in_progress(graph, parent_id) → bool` | build, fix | Whether decompose task exists and is not terminal |
| `no_actionable_issues(graph, review_id) → bool` | fix | Check if review found zero actionable issues |

### Rendering helpers

| Function | Used by | Purpose |
|----------|---------|---------|
| `review_phase_lines(group, review, graph) → Vec<Line>` | build, fix | Render a review phase + inline issue list |
| `build_summary_lines(graph, epic_id, plan_path, group) → Vec<Line>` | build | Summary with per-agent breakdown |
| `loading_lines() → Vec<Line>` | all | Standard spinner + "Reading task graph..." placeholder |
| `format_progress(subtasks) → String` | task_run, build | `"2/5 subtasks completed"` or `"2/5 subtasks completed, 1 failed"` |

### Stats helpers (for build summary)

| Function | Used by | Purpose |
|----------|---------|---------|
| `aggregate_agent_stats(graph, epic_id) → Vec<AgentStat>` | build summary | Aggregate sessions, time, tokens per agent type |
| `sum_agent_stats(stats) → AgentStat` | build summary | Sum all agent stats into a total |
| `max_iterations_warning(graph, epic_id) → Option<String>` | build summary | Warning text if max iterations reached with remaining issues |

Many of these exist in the current `chat_builder.rs` (1240 lines) and can be extracted.

---

## New Component: `separator()`

The build summary needs a standalone `---` separator (without being part of a subtask_table). Add to `components.rs`:

```rust
/// Standalone separator line.
pub fn separator(group: u16) -> Vec<Line> {
    vec![Line {
        indent: 0,
        text: String::new(),
        meta: None,
        style: LineStyle::Separator,
        group,
        dimmed: false,
    }]
}
```

---

## Integration Point

Wiring view functions into actual commands happens in **Phase 3** (step 3a: cutover). Each command creates a `Model` with the appropriate `Screen` and calls `tui::run()`. This step only creates the pure view functions.

---

## Files Changed

| File | Change |
|------|--------|
| `cli/src/tui/screens/mod.rs` | New: module declarations |
| `cli/src/tui/screens/helpers.rs` | New: shared graph query + rendering helpers (~150 lines) |
| `cli/src/tui/screens/task_run.rs` | New: ~80 lines |
| `cli/src/tui/screens/build.rs` | New: ~300 lines (includes `build_summary_lines`, `review_phase_lines`) |
| `cli/src/tui/screens/review.rs` | New: ~100 lines |
| `cli/src/tui/screens/fix.rs` | New: ~120 lines |
| `cli/src/tui/screens/epic_show.rs` | New: ~30 lines |
| `cli/src/tui/screens/review_show.rs` | New: ~30 lines |
| `cli/src/tui/app.rs` | Wire `view()` dispatch (~15 lines replacing stub) |
| `cli/src/tui/components.rs` | Add `ChildLine` constructors (~30 lines) + `separator()` (~10 lines) |
| `cli/src/tui/mod.rs` | Add `pub mod screens;` |

---

## Testing Strategy

Each view function ships with `insta` snapshots. Tests construct a `TaskGraph` with specific states and assert on the rendered `Vec<Line>`.

| View | Snapshots |
|------|-----------|
| task_run | leaf: Open, InProgress, Closed, Stopped; parent: mixed, all-done, failed |
| build | plan loaded, decompose active, subtasks arriving, mid-build, subtask failed, all done, review approved, iteration 2, max iterations |
| review | plan/code/task/session scopes × (issues + approved) |
| fix | no-actionable-issues, single iteration, multi-iteration, max iterations |
| epic_show | all done, mixed status |
| review_show | with issues, approved |
| **Width variants** | build at 40, 80, 120 cols |
| **Cold-start** | build 70% done — verify dimming and active phase |

**Total: ~31 snapshots + helper unit tests.**

Test pattern:
```rust
#[test]
fn build_mid_progress() {
    let graph = make_build_graph_at_state(BuildState::MidBuild);
    let lines = build::view(&graph, "epic1", "plan.md", &WindowState::new(80));
    insta::assert_debug_snapshot!(lines);
}
```

**Key principle from ratatui testing best practices:** state logic tests should be the majority and fastest. Never mix state assertions with rendering assertions in the same test. View function tests assert on `Vec<Line>` content and styles, not on Buffer cells.

---

## Implementation Order

All view functions are independent pure functions. They can be built in any order or in parallel.

**Recommended:** Start with `task_run` (simplest, validates the full vertical), then `review` and static views (small), then `build` (most complex), then `fix` (composes build + review).

```
task_run ──→ review ──→ epic_show ──→ build ──→ fix
             review_show
```
