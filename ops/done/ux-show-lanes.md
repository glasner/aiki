# Lane DAG Visualization

Compact dot DAG showing execution lanes during the implement stage, right-aligned against the stage section in the workflow view.

## Context

The workflow view (workflow.md) has two sections: epic tree (subtask list) and stage list. During implement, the subtask list shows what's running. But it can't show the *structure* — which tasks are parallel, which depend on each other, where fan-out/fan-in happens.

The lane DAG adds that structural information as a compact decoration on the stage lines. It draws from `aiki task lane` data (`LaneDecomposition` from `cli/src/tasks/lanes.rs`).

Inspired by the horizontal DAG in `ops/research/aiki-tui/mockups/v3.html`, but non-interactive and compact — the subtask list is the primary, the DAG is supplementary.

## Design

### Visibility rules

| Active stage | Lane DAG visible? | Why |
|-------------|-------------------|-----|
| Decompose | No | Don't know the DAG yet — subtasks still being created |
| Implement | **Yes** | Lanes are known, execution structure is interesting |
| Build done | No | DAG collapses with build |
| Review/Fix | No | Different work, different structure |

The DAG appears when implement starts and disappears when build completes or fails.

### Layout

The dot DAG renders right-aligned, spanning the same vertical rows as the stage lines (`build`, `decompose`, `implement`). The subtask list above it has the detail; the DAG has the shape.

```
 [luppzupt] Implement Stripe webhook event handling
 ⎿ ✓ Explore webhook requirements          cc    8s
 ⎿ ✓ Create implementation plan            cc    6s
 ⎿ ● Implement webhook route handler       cur
 ⎿ ● Verify Stripe signatures              cc
 ⎿ ○ Add idempotency key tracking
 ⎿ ○ Write integration tests

 ● build                    ●━●━┬━◉━○
    ✓ decompose  0:12           ├━◉
    ● implement  3/6  0:34      └━○━━┬━○
                                     └━○
 ○ review
```

### Dot semantics

One dot per **session** (not per task). A session is one or more tasks bound by `needs-context` — they run in a single agent invocation. This matches the parallelism unit: sessions within a lane are sequential, lanes are concurrent.

| Session state | Symbol | Color |
|--------------|--------|-------|
| Done | `●` | green |
| Active (in-progress) | `◉` | yellow, bold |
| Pending | `○` | dim |
| Failed | `✗` | red, bold |
| Stuck (blocked by failure) | `○` | dim (same as pending) |

Active sessions use `◉` (filled circle with ring) to distinguish from done `●` at a glance.

### Lane rendering

Each lane renders as a horizontal row of dots connected by `━` (horizontal line). Lanes stack vertically. Fork points (fan-out) use tree connectors:

```
━┬━     fan-out: one lane splits into multiple
 ├━     middle branch
 └━     last branch
```

Fan-in (multiple lanes converging) uses:

```
━┬━○    multiple lanes feed into a merge point
━╯
```

#### Horizontal positioning

- Sessions within a lane are left-to-right in execution order
- Forked lanes start at the fork point's column
- The main lane (lane 0, typically the longest) occupies the top row
- Dependent lanes appear below their parent, indented to the fork column

#### Right-alignment

The entire DAG block is right-aligned within the available width. The left edge of the DAG starts at a column computed as: `width - dag_render_width`. The stage text (`● build`, `✓ decompose`, `● implement`) occupies the left side, and the DAG floats right — they share the same rows but don't overlap.

If the terminal is too narrow (stage text + DAG would overlap), the DAG is hidden. No truncation — either show the whole thing or skip it.

### Connector characters

```
━   horizontal line connecting sessions (U+2501)
┬   fork down (U+252C)
├   branch continue (U+251C)
└   last branch (U+2514)
│   vertical connector between branches (U+2502)
╯   fan-in merge (U+256F, from v3 mockup)
```

All connectors use `dim` color. Only dots carry status color.

## Mockups

### Linear (no parallelism)

All tasks in one lane, no forks:

```
 ● build                    ●━●━◉━○━○
    ✓ decompose  0:12
    ● implement  2/5  0:34
 ○ review
```

### Fan-out (3 parallel lanes)

After session 2, work fans out into 3 parallel lanes:

```
 ● build                    ●━●━┬━◉━○
    ✓ decompose  0:12           ├━◉
    ● implement  3/6  0:34      └━○━━┬━○
                                     └━○
 ○ review
```

Reading: sessions 1-2 were sequential (explore, plan). Then fan-out: lane 1 continues with 2 more sessions, lane 2 has 1 session, lane 3 has 2 sessions with its own fan-out into 2 more.

### All done (DAG hidden)

Build collapsed, DAG gone:

```
 [luppzupt] Implement Stripe webhook event handling
 ⎿ ✓ 6 subtasks  2m28s

 ✓ build  6/6  2m40s
 ● review  0:14
    ...
```

### Fan-in convergence

Two lanes merge before a final session:

```
 ● build                    ●━┬━●━●━┬━◉
    ✓ decompose  0:12          └━●━━━╯
    ● implement  5/6  1:48
 ○ review
```

Lane 1: 3 sessions. Lane 2: 1 session. Both must complete before the final session (fan-in merge).

### Failure in a lane

```
 ● build                    ●━●━┬━◉━○
    ✓ decompose  0:12           ├━✗
    ● implement  3/6  0:52      └━○━○
 ○ review
```

Lane 2 failed (`✗`). Lane 1 continues. Lane 3 is blocked but shows as `○` (pending) — stuck status is only on the subtask list, not in the DAG. The DAG stays clean.

### Single session (no DAG)

If there's only one lane with one session, no DAG is shown — it would just be a single dot, which adds nothing:

```
 ● build
    ✓ decompose  0:08
    ● implement  1/1  0:22
 ○ review
```

Threshold: show DAG when there are 2+ lanes OR 3+ sessions in a single lane.

### Deep fan-out

```
 ● build                    ●━┬━●━◉━○
    ✓ decompose  0:12          ├━◉
    ● implement  4/8  1:14     ├━○━○
                               └━○━━┬━○
                                    └━○
 ○ review
```

4 lanes from a single fork point. The `┬`/`├`/`└` stack shows the fan-out clearly.

## Data model

### Input

```rust
use crate::tasks::lanes::{LaneDecomposition, Lane, LaneSession, LaneStatus};
use crate::tasks::graph::TaskGraph;

struct LaneDagInput<'a> {
    decomposition: &'a LaneDecomposition,
    graph: &'a TaskGraph,
}
```

The widget takes `LaneDecomposition` (already exists in `cli/src/tasks/lanes.rs`) and the `TaskGraph` (needed to look up session status).

### Session rendering state

```rust
enum SessionState {
    Done,
    Active,
    Pending,
    Failed,
}

struct RenderedSession {
    state: SessionState,
    col: u16,           // horizontal position in the DAG grid
}

struct RenderedLane {
    sessions: Vec<RenderedSession>,
    fork_col: Option<u16>,          // column where this lane forks from parent
    parent_lane_idx: Option<usize>, // which lane it forked from
}

struct DagLayout {
    lanes: Vec<RenderedLane>,
    width: u16,     // total columns needed
    height: u16,    // total rows needed (1 per lane + connector rows)
}
```

### Layout algorithm

1. **Topological ordering**: lanes are already in topo order from `derive_lanes`
2. **Column assignment**: walk lanes left-to-right, assign each session a column. Fork children start at the fork point's column.
3. **Row assignment**: lane 0 gets row 0. Each fork adds rows for its children below the fork point. Fan-in rows are on the merge lane's row.
4. **Connector generation**: for each fork, emit `┬`/`├`/`└` + `│` connectors. For each fan-in, emit `╯` connectors.
5. **Width/height calculation**: max column + 1 = width, max row + 1 = height.

### Minimum width

Each session is 1 char (the dot). Connectors between sessions are 1 char (`━`). Fork connectors add 1 char vertically. So minimum width for N sessions on the longest path = `2*N - 1` chars.

## Widget

New file: `cli/src/tui/widgets/lane_dag.rs`

```rust
pub struct LaneDag<'a> {
    layout: DagLayout,
    theme: &'a Theme,
}

impl<'a> Widget for LaneDag<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Render dots and connectors into the buffer area
        // Right-aligned: offset_x = area.width - self.layout.width
    }
}
```

Estimated: ~150 lines for layout + rendering.

### Integration with workflow view

The `LaneDag` widget is rendered by the workflow view composer (`views/workflow.rs`, planned in workflow.md) during the implement stage. The composer:

1. Checks if implement is the active sub-stage
2. Calls `derive_lanes(graph, epic_id)` to get the `LaneDecomposition`
3. Checks the visibility threshold (2+ lanes or 3+ sessions)
4. Computes the DAG layout
5. Renders `LaneDag` right-aligned on the stage section rows

The workflow view already has the `TaskGraph` available (needed for status lookups). The only new dependency is importing `derive_lanes` from the lanes module.

## Relationship to workflow.md

| workflow.md component | Impact |
|-----------------------|--------|
| `EpicTree` widget | No change — subtask list still renders normally during implement |
| `StageList` widget (planned) | Needs to share row space with `LaneDag` when active |
| `SubStageView` data model | Add optional `LaneDecomposition` field for implement |
| `views/workflow.rs` (planned) | Composes `LaneDag` during implement stage |

### Data model extension

Add to `SubStageView` in workflow.md:

```rust
struct SubStageView {
    name: String,
    state: StageState,
    progress: Option<String>,
    elapsed: Option<String>,
    lane_dag: Option<DagLayout>,    // ← NEW: present only for implement
}
```

## Key decisions

| Decision | Choice | Why |
|----------|--------|-----|
| One dot per session, not per task | Session is the parallelism unit | Tasks within a session run sequentially in one agent — the interesting structure is between sessions/lanes |
| Right-aligned against stage lines | DAG is supplementary, subtask list is primary | User reads subtask names on the left. The DAG shows structure on the right. No information competes. |
| Hidden when too narrow | No truncation — show or hide | A partial DAG is confusing. Better to omit entirely. |
| Appears only during implement | DAG data doesn't exist during decompose, isn't relevant after | Matches the natural lifecycle: lanes are derived at implement time |
| Threshold: 2+ lanes or 3+ sessions | Skip for trivial DAGs | A single dot or two dots in a line add no information |
| Connectors in dim, dots in status color | Visual hierarchy | Dots are the data, connectors are the structure. Status color on dots only. |
| `◉` for active, `●` for done | Need visual distinction between "completed" and "running now" | Without this, a row of green dots and one yellow dot is hard to scan |
| Stuck sessions show as `○` (not `✗`) | Only the failed session gets `✗` | Downstream stuck tasks are a consequence, not a state. The subtask list already shows "stuck" labels. |
| Fan-in uses `╯` | Consistent with v3 mockup | Familiar to anyone who's seen the v3 research |
