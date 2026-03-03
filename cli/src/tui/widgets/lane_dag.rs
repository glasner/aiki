//! Lane DAG widget: compact dot-graph showing execution lanes.
//!
//! Renders `LaneDecomposition` as a horizontal dot DAG with fork/merge
//! connectors, right-aligned within the render area.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

use crate::tasks::graph::TaskGraph;
use crate::tasks::lanes::{LaneDecomposition, LaneSession};
use crate::tasks::types::{TaskOutcome, TaskStatus};
use crate::tui::theme::Theme;

// ── Dot / connector symbols ─────────────────────────────────────────

const SYM_DONE: &str = "●";
const SYM_ACTIVE: &str = "◉";
const SYM_PENDING: &str = "○";
const SYM_FAILED: &str = "✗";

const CONN_HORIZ: &str = "━";
const CONN_FORK_DOWN: &str = "┬";
const CONN_BRANCH: &str = "├";
const CONN_LAST_BRANCH: &str = "└";
const CONN_VERT: &str = "│";
const CONN_FANIN: &str = "╯";

// ── Data model ──────────────────────────────────────────────────────

/// State of a single session, derived from its tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Done,
    Active,
    Pending,
    Failed,
}

/// A session positioned in the DAG grid.
#[derive(Debug, Clone)]
pub struct RenderedSession {
    pub state: SessionState,
    pub col: u16,
}

/// A lane with its rendered sessions and structural metadata.
#[derive(Debug, Clone)]
pub struct RenderedLane {
    pub sessions: Vec<RenderedSession>,
    pub fork_col: Option<u16>,
    pub merge_col: Option<u16>,
    pub parent_lane_idx: Option<usize>,
}

/// Fully computed layout for the lane DAG.
#[derive(Debug, Clone)]
pub struct DagLayout {
    pub lanes: Vec<RenderedLane>,
    pub width: u16,
    pub height: u16,
}

// ── Session state resolution ────────────────────────────────────────

/// Determine the state of a session from its constituent tasks.
fn resolve_session_state(session: &LaneSession, graph: &TaskGraph) -> SessionState {
    let mut all_done = true;
    let mut any_active = false;
    let mut any_failed = false;

    for tid in &session.task_ids {
        if let Some(task) = graph.tasks.get(tid) {
            match task.status {
                TaskStatus::InProgress => {
                    any_active = true;
                }
                TaskStatus::Closed => {
                    if task.closed_outcome != Some(TaskOutcome::Done) {
                        any_failed = true;
                    }
                }
                TaskStatus::Stopped => {
                    any_failed = true;
                }
                TaskStatus::Open => {
                    all_done = false;
                }
            }
        } else {
            all_done = false;
        }
    }

    if any_active {
        SessionState::Active
    } else if all_done && !session.task_ids.is_empty() && !any_failed {
        SessionState::Done
    } else if any_failed {
        SessionState::Failed
    } else {
        SessionState::Pending
    }
}

// ── Layout algorithm ────────────────────────────────────────────────

/// Convert a `LaneDecomposition` + `TaskGraph` into a `DagLayout`.
///
/// Column assignment: sessions get columns 0, 2, 4, ... (dot + connector).
/// Forked lanes start at their fork point's column.
/// Row assignment: lane 0 → row 0, forked children get rows below.
pub fn compute_dag_layout(decomposition: &LaneDecomposition, graph: &TaskGraph) -> DagLayout {
    if decomposition.lanes.is_empty() {
        return DagLayout {
            lanes: Vec::new(),
            width: 0,
            height: 0,
        };
    }

    // Build a map from lane head_task_id → lane index for dependency lookup.
    let mut lane_idx_by_head: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, lane) in decomposition.lanes.iter().enumerate() {
        lane_idx_by_head.insert(&lane.head_task_id, i);
    }

    let mut rendered_lanes: Vec<RenderedLane> = Vec::with_capacity(decomposition.lanes.len());
    let mut max_col: u16 = 0;

    for lane in &decomposition.lanes {
        // Find parent lane and fork column.
        let (parent_lane_idx, start_col) = if lane.depends_on_lanes.is_empty() {
            (None, 0u16)
        } else {
            // Find the dependency lane that was already rendered.
            let mut best_parent: Option<usize> = None;
            let mut best_fork_col: u16 = 0;

            for dep_head in &lane.depends_on_lanes {
                if let Some(&dep_idx) = lane_idx_by_head.get(dep_head.as_str()) {
                    if dep_idx < rendered_lanes.len() {
                        let dep_lane = &rendered_lanes[dep_idx];
                        // Fork point is at the last session's column of the dep lane.
                        if let Some(last_session) = dep_lane.sessions.last() {
                            let fork_col = last_session.col;
                            if best_parent.is_none() || fork_col > best_fork_col {
                                best_parent = Some(dep_idx);
                                best_fork_col = fork_col;
                            }
                        }
                    }
                }
            }

            match best_parent {
                Some(pidx) => (Some(pidx), best_fork_col),
                None => (None, 0),
            }
        };

        let fork_col = if parent_lane_idx.is_some() {
            Some(start_col)
        } else {
            None
        };

        // Assign columns to sessions in this lane.
        let mut sessions: Vec<RenderedSession> = Vec::with_capacity(lane.sessions.len());
        let mut col = start_col;

        for (j, session) in lane.sessions.iter().enumerate() {
            if j > 0 || parent_lane_idx.is_some() {
                // Skip connector space: dot at col, connector at col+1, next dot at col+2
                if j > 0 {
                    col += 2;
                } else {
                    // First session of a forked lane: starts 2 cols after fork point
                    col += 2;
                }
            }

            let state = resolve_session_state(session, graph);
            sessions.push(RenderedSession { state, col });
        }

        // Track max column.
        if let Some(last) = sessions.last() {
            if last.col > max_col {
                max_col = last.col;
            }
        }

        // Detect merge: does any later lane depend on this lane?
        // (merge_col is set after all lanes are processed)
        rendered_lanes.push(RenderedLane {
            sessions,
            fork_col,
            merge_col: None,
            parent_lane_idx,
        });
    }

    // Detect fan-in merges: if a lane depends on multiple other lanes,
    // the non-primary dependencies are "merging in".
    // A lane with depends_on_lanes referencing this lane means this lane
    // merges into that lane.
    for (i, lane) in decomposition.lanes.iter().enumerate() {
        if lane.depends_on_lanes.len() > 1 {
            // This lane has multiple dependencies — the extra ones are fan-in merges
            // The primary parent is already set. Mark the other deps as merging.
            let primary_parent = rendered_lanes[i].parent_lane_idx;
            for dep_head in &lane.depends_on_lanes {
                if let Some(&dep_idx) = lane_idx_by_head.get(dep_head.as_str()) {
                    if Some(dep_idx) != primary_parent && dep_idx < rendered_lanes.len() {
                        // This dep lane merges into lane i
                        if let Some(first_session) = rendered_lanes[i].sessions.first() {
                            rendered_lanes[dep_idx].merge_col = Some(first_session.col);
                        }
                    }
                }
            }
        }
    }

    // Also detect fan-in when a lane's first session is depended upon by previous lanes.
    // Check: for each lane, if multiple earlier lanes' last sessions feed into it.
    for i in 0..decomposition.lanes.len() {
        let mut feeders: Vec<usize> = Vec::new();
        for (j, other_lane) in decomposition.lanes.iter().enumerate() {
            if j == i {
                continue;
            }
            // Check if lane i depends on lane j
            if decomposition.lanes[i]
                .depends_on_lanes
                .contains(&other_lane.head_task_id)
            {
                feeders.push(j);
            }
        }
        if feeders.len() > 1 {
            // Multiple lanes feed into this lane — fan-in
            if let Some(first_session) = rendered_lanes[i].sessions.first() {
                let merge_col = first_session.col;
                for &feeder_idx in &feeders {
                    if Some(feeder_idx) != rendered_lanes[i].parent_lane_idx {
                        rendered_lanes[feeder_idx].merge_col = Some(merge_col);
                    }
                }
            }
        }
    }

    let width = if max_col > 0 || !rendered_lanes.is_empty() {
        max_col + 1
    } else {
        0
    };
    let height = rendered_lanes.len() as u16;

    DagLayout {
        lanes: rendered_lanes,
        width,
        height,
    }
}

/// Whether the DAG is worth showing: 2+ lanes or 3+ sessions total.
pub fn should_show_dag(layout: &DagLayout) -> bool {
    if layout.lanes.len() >= 2 {
        return true;
    }
    let total_sessions: usize = layout.lanes.iter().map(|l| l.sessions.len()).sum();
    total_sessions >= 3
}

// ── Widget ──────────────────────────────────────────────────────────

/// The lane DAG widget — renders the dot DAG into a ratatui buffer area.
pub struct LaneDag<'a> {
    layout: DagLayout,
    theme: &'a Theme,
}

impl<'a> LaneDag<'a> {
    pub fn new(layout: DagLayout, theme: &'a Theme) -> Self {
        Self { layout, theme }
    }

    /// Width of the rendered DAG.
    #[allow(dead_code)]
    pub fn width(&self) -> u16 {
        self.layout.width
    }

    /// Height of the rendered DAG.
    #[allow(dead_code)]
    pub fn height(&self) -> u16 {
        self.layout.height
    }
}

impl Widget for LaneDag<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.layout.lanes.is_empty() {
            return;
        }

        // Right-align: compute offset so DAG is flush-right.
        let offset_x = area
            .x
            .saturating_add(area.width.saturating_sub(self.layout.width));
        let max_x = area.x.saturating_add(area.width);
        let max_y = area.y.saturating_add(area.height);

        let dim_style = self.theme.dim_style();

        for (row, lane) in self.layout.lanes.iter().enumerate() {
            let y = area.y + row as u16;
            if y >= max_y {
                break;
            }

            // Render fork connector (vertical branch from parent).
            if let Some(fork_col) = lane.fork_col {
                let x = offset_x.saturating_add(fork_col);
                if x < max_x {
                    // Determine if this is the last branch from parent or middle.
                    let is_last_branch = self.is_last_branch(row);
                    let connector = if is_last_branch {
                        CONN_LAST_BRANCH
                    } else {
                        CONN_BRANCH
                    };
                    buf.set_string(x, y, connector, dim_style);
                }

                // Draw horizontal connector from branch to first session.
                if let Some(first_session) = lane.sessions.first() {
                    let start = offset_x.saturating_add(fork_col + 1);
                    let end = offset_x.saturating_add(first_session.col);
                    for cx in start..end {
                        if cx < max_x {
                            buf.set_string(cx, y, CONN_HORIZ, dim_style);
                        }
                    }
                }
            }

            // Render sessions and horizontal connectors between them.
            for (j, session) in lane.sessions.iter().enumerate() {
                let x = offset_x.saturating_add(session.col);
                if x >= max_x {
                    break;
                }

                // Dot symbol and style.
                let (sym, style) = session_symbol_style(session.state, self.theme);
                buf.set_string(x, y, sym, style);

                // Horizontal connector to next session.
                if j + 1 < lane.sessions.len() {
                    let next_col = lane.sessions[j + 1].col;
                    let conn_start = session.col + 1;
                    for c in conn_start..next_col {
                        let cx = offset_x.saturating_add(c);
                        if cx < max_x {
                            buf.set_string(cx, y, CONN_HORIZ, dim_style);
                        }
                    }
                }
            }

            // Render fork-down connector at the last session if children fork from here.
            if let Some(last_session) = lane.sessions.last() {
                if self.has_children_forking_at(row, last_session.col) {
                    let x = offset_x.saturating_add(last_session.col);
                    if x < max_x {
                        // Overwrite the dot with fork connector? No — the fork connector
                        // replaces the horizontal connector AFTER the dot.
                        // Actually per the mockup, the ┬ appears at the dot position when
                        // the dot is also a fork point. Let's check:
                        // ●━●━┬━◉━○  — the ┬ replaces the connector between sessions.
                        // So ┬ goes at the connector position after the last pre-fork session.
                        // Wait, looking more carefully at mockups:
                        //   ●━●━┬━◉━○
                        //       ├━◉
                        //       └━○━○
                        // The ┬ is between session 2 and session 3 of lane 0.
                        // It's at the connector position (col of session 2 + 1).

                        // This is handled differently. The fork connector is placed
                        // at the fork_col of child lanes. Let me handle it via the
                        // fork column rendering.
                    }
                }
            }

            // Render fan-in merge connector.
            if let Some(merge_col) = lane.merge_col {
                // Draw ━╯ from the last session to the merge column.
                if let Some(last_session) = lane.sessions.last() {
                    let start = last_session.col + 1;
                    for c in start..merge_col {
                        let cx = offset_x.saturating_add(c);
                        if cx < max_x {
                            buf.set_string(cx, y, CONN_HORIZ, dim_style);
                        }
                    }
                    let mx = offset_x.saturating_add(merge_col);
                    if mx < max_x {
                        buf.set_string(mx, y, CONN_FANIN, dim_style);
                    }
                } else {
                    // No sessions, just put the merge connector.
                    let mx = offset_x.saturating_add(merge_col);
                    if mx < max_x {
                        buf.set_string(mx, y, CONN_FANIN, dim_style);
                    }
                }
            }
        }

        // Render vertical connectors between fork branches.
        // For each lane that has a fork_col, draw │ on rows between the parent's
        // fork-down connector and this lane's branch connector.
        for (row, lane) in self.layout.lanes.iter().enumerate() {
            if let (Some(fork_col), Some(parent_idx)) = (lane.fork_col, lane.parent_lane_idx) {
                let x = offset_x.saturating_add(fork_col);
                if x >= max_x {
                    continue;
                }

                // Draw ┬ on the parent row at fork_col (only for the first child).
                let parent_y = area.y + parent_idx as u16;
                if parent_y < max_y && self.is_first_child_of(parent_idx, row, fork_col) {
                    buf.set_string(x, parent_y, CONN_FORK_DOWN, dim_style);
                }

                // Draw │ on rows between parent+1 and this row (exclusive).
                for vy in (parent_idx + 1)..row {
                    let vy_abs = area.y + vy as u16;
                    if vy_abs < max_y {
                        // Only draw if this row's lane also forks from the same col
                        // (i.e., it's a sibling branch) — otherwise we'd overwrite content.
                        // Actually, we draw │ for any row between parent and last child.
                        let existing = buf
                            .cell((x, vy_abs))
                            .map(|c| c.symbol().to_string())
                            .unwrap_or_default();
                        if existing.trim().is_empty() || existing == " " {
                            buf.set_string(x, vy_abs, CONN_VERT, dim_style);
                        }
                    }
                }
            }
        }
    }
}

impl LaneDag<'_> {
    /// Check if the given row is the last child branch forking from a parent.
    fn is_last_branch(&self, row: usize) -> bool {
        let lane = &self.layout.lanes[row];
        let (fork_col, parent_idx) = match (lane.fork_col, lane.parent_lane_idx) {
            (Some(fc), Some(pi)) => (fc, pi),
            _ => return false,
        };

        // Check if any later lane also forks from the same parent at the same col.
        for later_row in (row + 1)..self.layout.lanes.len() {
            let later = &self.layout.lanes[later_row];
            if later.parent_lane_idx == Some(parent_idx) && later.fork_col == Some(fork_col) {
                return false;
            }
        }
        true
    }

    /// Check if any child lanes fork from the given row at the given column.
    fn has_children_forking_at(&self, _row: usize, _col: u16) -> bool {
        // Used for rendering ┬ — handled in the vertical connector pass instead.
        false
    }

    /// Check if the given row is the first child forking from parent_idx at fork_col.
    fn is_first_child_of(&self, parent_idx: usize, row: usize, fork_col: u16) -> bool {
        for earlier_row in (parent_idx + 1)..row {
            let earlier = &self.layout.lanes[earlier_row];
            if earlier.parent_lane_idx == Some(parent_idx) && earlier.fork_col == Some(fork_col) {
                return false;
            }
        }
        true
    }
}

/// Symbol and style for a session state.
fn session_symbol_style(state: SessionState, theme: &Theme) -> (&'static str, Style) {
    match state {
        SessionState::Done => (SYM_DONE, Style::default().fg(theme.green)),
        SessionState::Active => (
            SYM_ACTIVE,
            Style::default()
                .fg(theme.yellow)
                .add_modifier(Modifier::BOLD),
        ),
        SessionState::Pending => (SYM_PENDING, Style::default().fg(theme.dim)),
        SessionState::Failed => (
            SYM_FAILED,
            Style::default()
                .fg(theme.red)
                .add_modifier(Modifier::BOLD),
        ),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::lanes::{Lane, LaneDecomposition, LaneSession};
    use crate::tasks::types::{
        FastHashMap, Task, TaskOutcome, TaskPriority, TaskStatus,
    };
    use crate::tasks::graph::{EdgeStore, TaskGraph};
    use chrono::Utc;

    fn test_theme() -> Theme {
        Theme::dark()
    }

    /// Create a minimal task with the given status and outcome.
    fn make_task(id: &str, status: TaskStatus, outcome: Option<TaskOutcome>) -> Task {
        Task {
            id: id.to_string(),
            name: format!("Task {}", id),
            slug: None,
            task_type: None,
            status,
            priority: TaskPriority::P2,
            assignee: None,
            sources: Vec::new(),
            template: None,
            instructions: None,
            data: std::collections::HashMap::new(),
            created_at: Utc::now(),
            started_at: None,
            claimed_by_session: None,
            last_session_id: None,
            stopped_reason: None,
            closed_outcome: outcome,
            summary: None,
            turn_started: None,
            turn_closed: None,
            turn_stopped: None,
            comments: Vec::new(),
        }
    }

    fn make_graph(tasks: Vec<Task>) -> TaskGraph {
        let mut task_map = FastHashMap::default();
        for t in tasks {
            task_map.insert(t.id.clone(), t);
        }
        TaskGraph {
            tasks: task_map,
            edges: EdgeStore::new(),
            slug_index: FastHashMap::default(),
        }
    }

    fn make_session(ids: &[&str]) -> LaneSession {
        LaneSession {
            task_ids: ids.iter().map(|s| s.to_string()).collect(),
        }
    }

    // ── Linear layout ────────────────────────────────────────────

    #[test]
    fn linear_5_sessions() {
        let decomposition = LaneDecomposition {
            lanes: vec![Lane {
                head_task_id: "t1".into(),
                sessions: vec![
                    make_session(&["t1"]),
                    make_session(&["t2"]),
                    make_session(&["t3"]),
                    make_session(&["t4"]),
                    make_session(&["t5"]),
                ],
                depends_on_lanes: vec![],
            }],
        };
        let graph = make_graph(vec![
            make_task("t1", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t2", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t3", TaskStatus::InProgress, None),
            make_task("t4", TaskStatus::Open, None),
            make_task("t5", TaskStatus::Open, None),
        ]);

        let layout = compute_dag_layout(&decomposition, &graph);
        assert_eq!(layout.width, 9, "5 sessions → width = 2*5-1 = 9");
        assert_eq!(layout.height, 1, "1 lane → height = 1");

        // Verify column positions: 0, 2, 4, 6, 8
        assert_eq!(layout.lanes[0].sessions[0].col, 0);
        assert_eq!(layout.lanes[0].sessions[1].col, 2);
        assert_eq!(layout.lanes[0].sessions[2].col, 4);
        assert_eq!(layout.lanes[0].sessions[3].col, 6);
        assert_eq!(layout.lanes[0].sessions[4].col, 8);
    }

    // ── Fan-out ──────────────────────────────────────────────────

    #[test]
    fn fan_out_1_root_2_children() {
        // Lane 0: t1 → t2 (then fork)
        // Lane 1: depends on lane 0 → t3
        // Lane 2: depends on lane 0 → t4
        let decomposition = LaneDecomposition {
            lanes: vec![
                Lane {
                    head_task_id: "t1".into(),
                    sessions: vec![make_session(&["t1"]), make_session(&["t2"])],
                    depends_on_lanes: vec![],
                },
                Lane {
                    head_task_id: "t3".into(),
                    sessions: vec![make_session(&["t3"])],
                    depends_on_lanes: vec!["t1".into()],
                },
                Lane {
                    head_task_id: "t4".into(),
                    sessions: vec![make_session(&["t4"])],
                    depends_on_lanes: vec!["t1".into()],
                },
            ],
        };
        let graph = make_graph(vec![
            make_task("t1", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t2", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t3", TaskStatus::InProgress, None),
            make_task("t4", TaskStatus::Open, None),
        ]);

        let layout = compute_dag_layout(&decomposition, &graph);
        assert_eq!(layout.height, 3, "3 lanes → height = 3");

        // Lane 0: sessions at col 0, 2. Lane 1 forks from col 2.
        // Lane 1: fork_col = 2, session at col 4.
        // Lane 2: fork_col = 2, session at col 4.
        assert_eq!(layout.lanes[1].fork_col, Some(2));
        assert_eq!(layout.lanes[2].fork_col, Some(2));
        assert!(layout.lanes[1].parent_lane_idx == Some(0));
        assert!(layout.lanes[2].parent_lane_idx == Some(0));
    }

    // ── Fan-in ───────────────────────────────────────────────────

    #[test]
    fn fan_in_merge() {
        // Lane 0: t1 → t2
        // Lane 1: t3 (depends on nothing)
        // Lane 2: t4 (depends on lane 0 AND lane 1 → fan-in)
        let decomposition = LaneDecomposition {
            lanes: vec![
                Lane {
                    head_task_id: "t1".into(),
                    sessions: vec![make_session(&["t1"]), make_session(&["t2"])],
                    depends_on_lanes: vec![],
                },
                Lane {
                    head_task_id: "t3".into(),
                    sessions: vec![make_session(&["t3"])],
                    depends_on_lanes: vec![],
                },
                Lane {
                    head_task_id: "t4".into(),
                    sessions: vec![make_session(&["t4"])],
                    depends_on_lanes: vec!["t1".into(), "t3".into()],
                },
            ],
        };
        let graph = make_graph(vec![
            make_task("t1", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t2", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t3", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t4", TaskStatus::InProgress, None),
        ]);

        let layout = compute_dag_layout(&decomposition, &graph);

        // Lane 2 depends on both lane 0 and lane 1.
        // One of the non-primary deps should have merge_col set.
        let has_merge = layout
            .lanes
            .iter()
            .any(|l| l.merge_col.is_some());
        assert!(has_merge, "fan-in should produce a merge connector");
    }

    // ── Session state resolution ─────────────────────────────────

    #[test]
    fn session_state_in_progress() {
        let session = make_session(&["t1"]);
        let graph = make_graph(vec![make_task("t1", TaskStatus::InProgress, None)]);
        assert_eq!(resolve_session_state(&session, &graph), SessionState::Active);
    }

    #[test]
    fn session_state_all_done() {
        let session = make_session(&["t1", "t2"]);
        let graph = make_graph(vec![
            make_task("t1", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t2", TaskStatus::Closed, Some(TaskOutcome::Done)),
        ]);
        assert_eq!(resolve_session_state(&session, &graph), SessionState::Done);
    }

    #[test]
    fn session_state_failed_stopped() {
        let session = make_session(&["t1"]);
        let graph = make_graph(vec![make_task("t1", TaskStatus::Stopped, None)]);
        assert_eq!(resolve_session_state(&session, &graph), SessionState::Failed);
    }

    #[test]
    fn session_state_failed_wontdo() {
        let session = make_session(&["t1"]);
        let graph = make_graph(vec![make_task(
            "t1",
            TaskStatus::Closed,
            Some(TaskOutcome::WontDo),
        )]);
        assert_eq!(resolve_session_state(&session, &graph), SessionState::Failed);
    }

    #[test]
    fn session_state_pending() {
        let session = make_session(&["t1"]);
        let graph = make_graph(vec![make_task("t1", TaskStatus::Open, None)]);
        assert_eq!(
            resolve_session_state(&session, &graph),
            SessionState::Pending
        );
    }

    // ── Visibility threshold ─────────────────────────────────────

    #[test]
    fn should_show_dag_2_lanes() {
        let layout = DagLayout {
            lanes: vec![
                RenderedLane {
                    sessions: vec![RenderedSession {
                        state: SessionState::Done,
                        col: 0,
                    }],
                    fork_col: None,
                    merge_col: None,
                    parent_lane_idx: None,
                },
                RenderedLane {
                    sessions: vec![RenderedSession {
                        state: SessionState::Active,
                        col: 0,
                    }],
                    fork_col: None,
                    merge_col: None,
                    parent_lane_idx: None,
                },
            ],
            width: 1,
            height: 2,
        };
        assert!(should_show_dag(&layout));
    }

    #[test]
    fn should_show_dag_3_sessions_1_lane() {
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![
                    RenderedSession {
                        state: SessionState::Done,
                        col: 0,
                    },
                    RenderedSession {
                        state: SessionState::Done,
                        col: 2,
                    },
                    RenderedSession {
                        state: SessionState::Active,
                        col: 4,
                    },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 5,
            height: 1,
        };
        assert!(should_show_dag(&layout));
    }

    #[test]
    fn should_not_show_dag_1_lane_2_sessions() {
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![
                    RenderedSession {
                        state: SessionState::Done,
                        col: 0,
                    },
                    RenderedSession {
                        state: SessionState::Active,
                        col: 2,
                    },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 3,
            height: 1,
        };
        assert!(!should_show_dag(&layout));
    }

    #[test]
    fn should_not_show_dag_1_lane_1_session() {
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![RenderedSession {
                    state: SessionState::Done,
                    col: 0,
                }],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 1,
            height: 1,
        };
        assert!(!should_show_dag(&layout));
    }

    // ── Empty decomposition ──────────────────────────────────────

    #[test]
    fn empty_decomposition() {
        let decomposition = LaneDecomposition { lanes: vec![] };
        let graph = make_graph(vec![]);
        let layout = compute_dag_layout(&decomposition, &graph);
        assert_eq!(layout.width, 0);
        assert_eq!(layout.height, 0);
        assert!(layout.lanes.is_empty());
        assert!(!should_show_dag(&layout));
    }

    // ── Widget rendering ─────────────────────────────────────────

    fn buf_lines(buf: &Buffer, height: u16, width: u16) -> Vec<String> {
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| {
                        buf.cell((x, y))
                            .map(|c| {
                                let s = c.symbol();
                                if s.is_empty() {
                                    " ".to_string()
                                } else {
                                    s.to_string()
                                }
                            })
                            .unwrap_or_default()
                    })
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn render_linear_dag() {
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![
                    RenderedSession {
                        state: SessionState::Done,
                        col: 0,
                    },
                    RenderedSession {
                        state: SessionState::Active,
                        col: 2,
                    },
                    RenderedSession {
                        state: SessionState::Pending,
                        col: 4,
                    },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 5,
            height: 1,
        };

        let theme = test_theme();
        let area = Rect::new(0, 0, 5, 1);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);
        let lines = buf_lines(&buf, 1, 5);

        // Should be: ●━◉━○
        assert!(lines[0].contains('●'), "got: {}", lines[0]);
        assert!(lines[0].contains('◉'), "got: {}", lines[0]);
        assert!(lines[0].contains('○'), "got: {}", lines[0]);
        assert!(lines[0].contains('━'), "got: {}", lines[0]);
    }

    #[test]
    fn render_right_aligned() {
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![
                    RenderedSession {
                        state: SessionState::Done,
                        col: 0,
                    },
                    RenderedSession {
                        state: SessionState::Done,
                        col: 2,
                    },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 3,
            height: 1,
        };

        let theme = test_theme();
        // Render in a 10-wide area; DAG should be right-aligned.
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);
        let _lines = buf_lines(&buf, 1, 10);

        // DAG is 3 wide, area is 10 wide → offset_x = 7
        // So cols 7, 8, 9 should have the DAG.
        let cell7 = buf.cell((7, 0)).unwrap().symbol();
        assert_eq!(cell7, SYM_DONE, "col 7 should be done dot");
    }

    #[test]
    fn render_zero_area_no_panic() {
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![RenderedSession {
                    state: SessionState::Done,
                    col: 0,
                }],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 1,
            height: 1,
        };
        let theme = test_theme();
        let area = Rect::new(0, 0, 0, 0);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);
        // Should not panic.
    }

    // ── Snapshot: linear 5 sessions ─────────────────────────────

    #[test]
    fn render_linear_5_sessions_pattern() {
        // Verify the full ●━●━◉━○━○ pattern from the design doc.
        let decomposition = LaneDecomposition {
            lanes: vec![Lane {
                head_task_id: "t1".into(),
                sessions: vec![
                    make_session(&["t1"]),
                    make_session(&["t2"]),
                    make_session(&["t3"]),
                    make_session(&["t4"]),
                    make_session(&["t5"]),
                ],
                depends_on_lanes: vec![],
            }],
        };
        let graph = make_graph(vec![
            make_task("t1", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t2", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t3", TaskStatus::InProgress, None),
            make_task("t4", TaskStatus::Open, None),
            make_task("t5", TaskStatus::Open, None),
        ]);

        let layout = compute_dag_layout(&decomposition, &graph);
        let theme = test_theme();
        let area = Rect::new(0, 0, layout.width, layout.height);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);
        let lines = buf_lines(&buf, 1, 9);

        assert_eq!(lines[0], "●━●━◉━○━○", "linear 5 sessions: {}", lines[0]);
    }

    // ── Dot symbols ─────────────────────────────────────────────

    #[test]
    fn render_dot_symbols_all_states() {
        // Verify Done=●, Active=◉, Pending=○, Failed=✗
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Done, col: 0 },
                    RenderedSession { state: SessionState::Active, col: 2 },
                    RenderedSession { state: SessionState::Pending, col: 4 },
                    RenderedSession { state: SessionState::Failed, col: 6 },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 7,
            height: 1,
        };

        let theme = test_theme();
        let area = Rect::new(0, 0, 7, 1);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);

        assert_eq!(buf.cell((0, 0)).unwrap().symbol(), SYM_DONE);
        assert_eq!(buf.cell((2, 0)).unwrap().symbol(), SYM_ACTIVE);
        assert_eq!(buf.cell((4, 0)).unwrap().symbol(), SYM_PENDING);
        assert_eq!(buf.cell((6, 0)).unwrap().symbol(), SYM_FAILED);
    }

    // ── Dot colors ──────────────────────────────────────────────

    #[test]
    fn render_dot_colors() {
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Done, col: 0 },
                    RenderedSession { state: SessionState::Active, col: 2 },
                    RenderedSession { state: SessionState::Pending, col: 4 },
                    RenderedSession { state: SessionState::Failed, col: 6 },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 7,
            height: 1,
        };

        let theme = test_theme();
        let area = Rect::new(0, 0, 7, 1);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);

        // Done = green
        let done_cell = buf.cell((0, 0)).unwrap();
        assert_eq!(done_cell.style().fg, Some(theme.green));

        // Active = yellow + bold
        let active_cell = buf.cell((2, 0)).unwrap();
        assert_eq!(active_cell.style().fg, Some(theme.yellow));
        assert!(
            active_cell.style().add_modifier.contains(Modifier::BOLD),
            "active should be bold"
        );

        // Pending = dim
        let pending_cell = buf.cell((4, 0)).unwrap();
        assert_eq!(pending_cell.style().fg, Some(theme.dim));

        // Failed = red + bold
        let failed_cell = buf.cell((6, 0)).unwrap();
        assert_eq!(failed_cell.style().fg, Some(theme.red));
        assert!(
            failed_cell.style().add_modifier.contains(Modifier::BOLD),
            "failed should be bold"
        );
    }

    // ── Connector color ─────────────────────────────────────────

    #[test]
    fn render_connector_dim() {
        // All connectors (━) should use dim color.
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Done, col: 0 },
                    RenderedSession { state: SessionState::Done, col: 2 },
                    RenderedSession { state: SessionState::Done, col: 4 },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 5,
            height: 1,
        };

        let theme = test_theme();
        let area = Rect::new(0, 0, 5, 1);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);

        // Connectors at cols 1 and 3
        let conn1 = buf.cell((1, 0)).unwrap();
        assert_eq!(conn1.symbol(), CONN_HORIZ);
        assert_eq!(conn1.style().fg, Some(theme.dim), "connector should be dim");

        let conn3 = buf.cell((3, 0)).unwrap();
        assert_eq!(conn3.symbol(), CONN_HORIZ);
        assert_eq!(conn3.style().fg, Some(theme.dim), "connector should be dim");
    }

    // ── Deep fan-out (4 lanes) ──────────────────────────────────

    #[test]
    fn deep_fan_out_4_lanes() {
        // Lane 0: t1 (then fork into 4 children)
        // Lane 1: depends on lane 0 → t2, t3
        // Lane 2: depends on lane 0 → t4
        // Lane 3: depends on lane 0 → t5, t6
        // Lane 4: depends on lane 0 → t7
        let decomposition = LaneDecomposition {
            lanes: vec![
                Lane {
                    head_task_id: "t1".into(),
                    sessions: vec![make_session(&["t1"])],
                    depends_on_lanes: vec![],
                },
                Lane {
                    head_task_id: "t2".into(),
                    sessions: vec![make_session(&["t2"]), make_session(&["t3"])],
                    depends_on_lanes: vec!["t1".into()],
                },
                Lane {
                    head_task_id: "t4".into(),
                    sessions: vec![make_session(&["t4"])],
                    depends_on_lanes: vec!["t1".into()],
                },
                Lane {
                    head_task_id: "t5".into(),
                    sessions: vec![make_session(&["t5"]), make_session(&["t6"])],
                    depends_on_lanes: vec!["t1".into()],
                },
                Lane {
                    head_task_id: "t7".into(),
                    sessions: vec![make_session(&["t7"])],
                    depends_on_lanes: vec!["t1".into()],
                },
            ],
        };
        let graph = make_graph(vec![
            make_task("t1", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t2", TaskStatus::Closed, Some(TaskOutcome::Done)),
            make_task("t3", TaskStatus::InProgress, None),
            make_task("t4", TaskStatus::InProgress, None),
            make_task("t5", TaskStatus::Open, None),
            make_task("t6", TaskStatus::Open, None),
            make_task("t7", TaskStatus::Open, None),
        ]);

        let layout = compute_dag_layout(&decomposition, &graph);

        assert_eq!(layout.height, 5, "5 lanes → height = 5");

        // All child lanes fork from lane 0 at col 0
        for lane_idx in 1..5 {
            assert_eq!(
                layout.lanes[lane_idx].fork_col,
                Some(0),
                "lane {} should fork from col 0",
                lane_idx
            );
            assert_eq!(
                layout.lanes[lane_idx].parent_lane_idx,
                Some(0),
                "lane {} should have parent 0",
                lane_idx
            );
        }
    }

    #[test]
    fn render_deep_fan_out_fork_stack() {
        // Verify ┬/├/├/└ fork stack for 4 child lanes.
        let layout = DagLayout {
            lanes: vec![
                RenderedLane {
                    sessions: vec![
                        RenderedSession { state: SessionState::Done, col: 0 },
                        RenderedSession { state: SessionState::Active, col: 2 },
                    ],
                    fork_col: None,
                    merge_col: None,
                    parent_lane_idx: None,
                },
                RenderedLane {
                    sessions: vec![RenderedSession { state: SessionState::Active, col: 4 }],
                    fork_col: Some(2),
                    merge_col: None,
                    parent_lane_idx: Some(0),
                },
                RenderedLane {
                    sessions: vec![RenderedSession { state: SessionState::Pending, col: 4 }],
                    fork_col: Some(2),
                    merge_col: None,
                    parent_lane_idx: Some(0),
                },
                RenderedLane {
                    sessions: vec![RenderedSession { state: SessionState::Pending, col: 4 }],
                    fork_col: Some(2),
                    merge_col: None,
                    parent_lane_idx: Some(0),
                },
            ],
            width: 5,
            height: 4,
        };

        let theme = test_theme();
        let area = Rect::new(0, 0, 5, 4);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);
        let lines = buf_lines(&buf, 4, 5);

        // Row 0: ●━┬━◉  (fork-down at col 2)
        assert!(lines[0].contains('┬'), "row 0 fork-down: {}", lines[0]);

        // Row 1:   ├━◉  (middle branch)
        assert!(lines[1].contains('├'), "row 1 middle branch: {}", lines[1]);

        // Row 2:   ├━○  (middle branch)
        assert!(lines[2].contains('├'), "row 2 middle branch: {}", lines[2]);

        // Row 3:   └━○  (last branch)
        assert!(lines[3].contains('└'), "row 3 last branch: {}", lines[3]);
    }

    // ── Fan-in rendering ────────────────────────────────────────

    #[test]
    fn render_fan_in_merge_connector() {
        // Lane 0: sessions at col 0, 2
        // Lane 1: session at col 0, merge_col at 4 (merges into next lane)
        // Lane 2: session at col 4 (depends on both)
        let layout = DagLayout {
            lanes: vec![
                RenderedLane {
                    sessions: vec![
                        RenderedSession { state: SessionState::Done, col: 0 },
                        RenderedSession { state: SessionState::Done, col: 2 },
                    ],
                    fork_col: None,
                    merge_col: None,
                    parent_lane_idx: None,
                },
                RenderedLane {
                    sessions: vec![RenderedSession { state: SessionState::Done, col: 0 }],
                    fork_col: None,
                    merge_col: Some(4),
                    parent_lane_idx: None,
                },
                RenderedLane {
                    sessions: vec![RenderedSession { state: SessionState::Active, col: 4 }],
                    fork_col: Some(2),
                    merge_col: None,
                    parent_lane_idx: Some(0),
                },
            ],
            width: 5,
            height: 3,
        };

        let theme = test_theme();
        let area = Rect::new(0, 0, 5, 3);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);
        let lines = buf_lines(&buf, 3, 5);

        // Row 1 should have ╯ merge connector
        assert!(
            lines[1].contains('╯'),
            "row 1 should have fan-in merge: {}",
            lines[1]
        );

        // Verify ╯ is rendered in dim style
        let merge_x = (0..5u16).find(|&x| {
            buf.cell((x, 1)).map(|c| c.symbol()) == Some(CONN_FANIN)
        });
        assert!(merge_x.is_some(), "should find ╯ symbol");
        let merge_cell = buf.cell((merge_x.unwrap(), 1)).unwrap();
        assert_eq!(
            merge_cell.style().fg,
            Some(theme.dim),
            "merge connector should be dim"
        );
    }

    // ── Right alignment ─────────────────────────────────────────

    #[test]
    fn render_right_alignment_offset() {
        // DAG is 5 wide, area is 20 wide → dots start at col 15.
        let layout = DagLayout {
            lanes: vec![RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Done, col: 0 },
                    RenderedSession { state: SessionState::Active, col: 2 },
                    RenderedSession { state: SessionState::Pending, col: 4 },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            }],
            width: 5,
            height: 1,
        };

        let theme = test_theme();
        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);

        // offset_x = 20 - 5 = 15
        // Dot at col 0 → rendered at x=15
        assert_eq!(buf.cell((15, 0)).unwrap().symbol(), SYM_DONE);
        // Dot at col 2 → rendered at x=17
        assert_eq!(buf.cell((17, 0)).unwrap().symbol(), SYM_ACTIVE);
        // Dot at col 4 → rendered at x=19
        assert_eq!(buf.cell((19, 0)).unwrap().symbol(), SYM_PENDING);

        // Columns before offset should be empty
        for x in 0..15u16 {
            let sym = buf.cell((x, 0)).unwrap().symbol();
            assert!(
                sym == " " || sym.trim().is_empty(),
                "col {} should be empty before offset, got '{}'",
                x,
                sym
            );
        }
    }

    #[test]
    fn render_fork_connectors() {
        // Lane 0: col 0, 2 (with fork at col 2)
        // Lane 1: fork from col 2, session at col 4
        let layout = DagLayout {
            lanes: vec![
                RenderedLane {
                    sessions: vec![
                        RenderedSession {
                            state: SessionState::Done,
                            col: 0,
                        },
                        RenderedSession {
                            state: SessionState::Done,
                            col: 2,
                        },
                        RenderedSession {
                            state: SessionState::Active,
                            col: 4,
                        },
                    ],
                    fork_col: None,
                    merge_col: None,
                    parent_lane_idx: None,
                },
                RenderedLane {
                    sessions: vec![RenderedSession {
                        state: SessionState::Pending,
                        col: 4,
                    }],
                    fork_col: Some(2),
                    merge_col: None,
                    parent_lane_idx: Some(0),
                },
            ],
            width: 5,
            height: 2,
        };

        let theme = test_theme();
        let area = Rect::new(0, 0, 5, 2);
        let mut buf = Buffer::empty(area);
        LaneDag::new(layout, &theme).render(area, &mut buf);
        let lines = buf_lines(&buf, 2, 5);

        // Row 0: ●━┬━◉  (fork down at col 2)
        // Row 1:   └━○
        assert!(
            lines[0].contains('┬'),
            "row 0 should have fork-down: {}",
            lines[0]
        );
        assert!(
            lines[1].contains('└'),
            "row 1 should have last-branch: {}",
            lines[1]
        );
    }
}
