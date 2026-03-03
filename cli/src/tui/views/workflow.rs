//! Workflow view composer.
//!
//! Composes PathLine, EpicTree, and StageList into a single Buffer
//! representing the full workflow detail view.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::tui::theme::Theme;
use crate::tui::types::WorkflowView;
use crate::tui::widgets::epic_tree::EpicTree;
use crate::tui::widgets::lane_dag::{LaneDag, should_show_dag};
use crate::tui::widgets::path_line::PathLine;
use crate::tui::widgets::stage_list::StageList;

const WIDTH: u16 = 80;

/// Render the workflow view into a Buffer.
///
/// Composes three sections vertically:
/// - Row 0: PathLine (file path breadcrumb)
/// - Row 1: blank separator
/// - Rows 2..N: EpicTree (epic headline + subtask tree)
/// - Row N+1: blank separator
/// - Remaining rows: StageList (vertical stage lines)
pub fn render_workflow(view: &WorkflowView, theme: &Theme) -> Buffer {
    let epic_height = epic_height(&view.epic);
    let stage_list = StageList::new(&view.stages, theme);
    let stage_list_height = stage_list.height();

    // 1 (path) + 1 (blank) + epic_height + 1 (blank) + stage_list_height
    let height = 1 + 1 + epic_height + 1 + stage_list_height;
    let area = Rect::new(0, 0, WIDTH, height);
    let mut buf = Buffer::empty(area);

    // Row 0: PathLine
    let path_area = Rect::new(0, 0, WIDTH, 1);
    PathLine::new(&view.plan_path, theme).render(path_area, &mut buf);

    // Rows 2..N: EpicTree (row 1 is blank)
    let tree_area = Rect::new(0, 2, WIDTH, epic_height);
    EpicTree::new(&view.epic, theme).render(tree_area, &mut buf);

    // Remaining rows: StageList
    let stage_y = 2 + epic_height + 1;
    let stage_area = Rect::new(0, stage_y, WIDTH, stage_list_height);
    StageList::new(&view.stages, theme).render(stage_area, &mut buf);

    // Lane DAG: right-aligned overlay on the stage section rows.
    if let Some(ref dag_layout) = view.lane_dag {
        if should_show_dag(dag_layout) {
            let dag_width = dag_layout.width;
            let dag_height = dag_layout.height;
            // Skip if stage text + gap + DAG would overflow the width.
            const STAGE_TEXT_MAX: u16 = 35;
            const DAG_GAP: u16 = 2;
            if dag_width + STAGE_TEXT_MAX + DAG_GAP <= WIDTH && dag_height > 0 && stage_list_height > 0 {
                let dag_area = Rect::new(
                    stage_area.x,
                    stage_area.y,
                    stage_area.width,
                    dag_height.min(stage_list_height),
                );
                LaneDag::new(dag_layout.clone(), theme).render(dag_area, &mut buf);
            }
        }
    }

    buf
}

/// Calculate the height of the epic tree section.
fn epic_height(epic: &crate::tui::types::EpicView) -> u16 {
    if epic.collapsed {
        if epic.collapsed_summary.is_some() { 2 } else { 1 }
    } else {
        let error_lines = epic
            .subtasks
            .iter()
            .filter(|s| s.error.is_some())
            .count() as u16;
        1 + epic.subtasks.len() as u16 + error_lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::types::{
        EpicView, StageChild, StageState, StageView, SubStageView, SubtaskLine, SubtaskStatus,
    };

    fn test_theme() -> Theme {
        Theme::dark()
    }

    fn buf_text(buf: &Buffer) -> String {
        let area = buf.area();
        let mut result = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                if let Some(cell) = buf.cell((x, y)) {
                    result.push_str(cell.symbol());
                } else {
                    result.push(' ');
                }
            }
            result.push('\n');
        }
        result
    }

    fn buf_line(buf: &Buffer, y: u16) -> String {
        let width = buf.area().width;
        (0..width)
            .map(|x| {
                buf.cell((x, y))
                    .map(|c| c.symbol().to_string())
                    .unwrap_or_default()
            })
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn make_epic(name: &str, subtasks: Vec<SubtaskLine>) -> EpicView {
        EpicView {
            short_id: "luppzupt".to_string(),
            name: name.to_string(),
            subtasks,
            collapsed: false,
            collapsed_summary: None,
        }
    }

    fn make_subtask(name: &str, status: SubtaskStatus) -> SubtaskLine {
        SubtaskLine {
            name: name.to_string(),
            status,
            agent: None,
            elapsed: None,
            error: None,
        }
    }

    // ── Basic layout ─────────────────────────────────────────────

    #[test]
    fn basic_layout() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/webhooks.md".to_string(),
            epic: make_epic(
                "Epic name",
                vec![
                    make_subtask("subtask 1", SubtaskStatus::Done),
                    make_subtask("subtask 2", SubtaskStatus::Active),
                ],
            ),
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Active,
                    progress: Some("3/6".into()),
                    elapsed: Some("0:34".into()),
                    sub_stages: vec![
                        SubStageView {
                            name: "decompose".into(),
                            state: StageState::Done,
                            progress: None,
                            elapsed: Some("0:12".into()),
                        },
                        SubStageView {
                            name: "implement".into(),
                            state: StageState::Active,
                            progress: Some("3/6".into()),
                            elapsed: Some("0:34".into()),
                        },
                    ],
                    children: vec![],
                },
                StageView {
                    name: "review".into(),
                    state: StageState::Pending,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        let text = buf_text(&buf);

        // Row 0: PathLine
        let line0 = buf_line(&buf, 0);
        assert!(line0.contains("ops/now/"), "path dir: {}", line0);
        assert!(line0.contains("webhooks.md"), "path file: {}", line0);

        // Row 1: blank
        let line1 = buf_line(&buf, 1);
        assert!(line1.trim().is_empty(), "row 1 should be blank: '{}'", line1);

        // Row 2: EpicTree header
        let line2 = buf_line(&buf, 2);
        assert!(line2.contains("[luppzupt]"), "epic id: {}", line2);
        assert!(line2.contains("Epic name"), "epic name: {}", line2);

        // Row 3-4: subtasks
        assert!(text.contains("subtask 1"));
        assert!(text.contains("subtask 2"));

        // Blank row between epic tree and stage list
        let blank_y = 2 + 3; // epic_height = 1 header + 2 subtasks = 3
        let blank_line = buf_line(&buf, blank_y);
        assert!(blank_line.trim().is_empty(), "blank before stages: '{}'", blank_line);

        // Stage list
        assert!(text.contains("build"));
        assert!(text.contains("decompose"));
        assert!(text.contains("implement"));
        assert!(text.contains("review"));
    }

    #[test]
    fn height_calculation_basic() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/test.md".to_string(),
            epic: make_epic(
                "Test",
                vec![
                    make_subtask("s1", SubtaskStatus::Done),
                    make_subtask("s2", SubtaskStatus::Active),
                ],
            ),
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Pending,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        // 1 (path) + 1 (blank) + 3 (1 header + 2 subtasks) + 1 (blank) + 1 (build) = 7
        assert_eq!(buf.area().height, 7);
    }

    // ── Collapsed epic ───────────────────────────────────────────

    #[test]
    fn collapsed_epic() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/webhooks.md".to_string(),
            epic: EpicView {
                short_id: "luppzupt".to_string(),
                name: "Implement webhooks".to_string(),
                subtasks: vec![
                    make_subtask("s1", SubtaskStatus::Done),
                    make_subtask("s2", SubtaskStatus::Done),
                ],
                collapsed: true,
                collapsed_summary: Some("6 subtasks  2m28s".to_string()),
            },
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Done,
                    progress: Some("6/6".into()),
                    elapsed: Some("2m28s".into()),
                    sub_stages: vec![],
                    children: vec![],
                },
                StageView {
                    name: "review".into(),
                    state: StageState::Active,
                    progress: None,
                    elapsed: Some("0:14".into()),
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        let text = buf_text(&buf);

        // Collapsed epic: 2 lines (header + summary)
        // Total: 1 (path) + 1 (blank) + 2 (collapsed) + 1 (blank) + 2 (stages) = 7
        assert_eq!(buf.area().height, 7);

        // Epic header
        assert!(text.contains("[luppzupt]"));
        assert!(text.contains("Implement webhooks"));

        // Collapsed summary
        assert!(text.contains("6 subtasks  2m28s"));

        // Subtask names should NOT appear
        assert!(!text.contains("s1"));
        assert!(!text.contains("s2"));

        // Stages still rendered
        assert!(text.contains("build"));
        assert!(text.contains("review"));
    }

    #[test]
    fn collapsed_epic_no_summary() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/test.md".to_string(),
            epic: EpicView {
                short_id: "abcdefgh".to_string(),
                name: "Epic".to_string(),
                subtasks: vec![],
                collapsed: true,
                collapsed_summary: None,
            },
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Pending,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        // 1 (path) + 1 (blank) + 1 (collapsed, no summary) + 1 (blank) + 1 (build) = 5
        assert_eq!(buf.area().height, 5);
    }

    // ── Expanded stages ──────────────────────────────────────────

    #[test]
    fn expanded_stages() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/feature.md".to_string(),
            epic: make_epic("Feature", vec![]),
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Active,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![
                        SubStageView {
                            name: "decompose".into(),
                            state: StageState::Done,
                            progress: None,
                            elapsed: Some("0:12".into()),
                        },
                        SubStageView {
                            name: "implement".into(),
                            state: StageState::Active,
                            progress: Some("3/6".into()),
                            elapsed: Some("0:34".into()),
                        },
                    ],
                    children: vec![],
                },
                StageView {
                    name: "review".into(),
                    state: StageState::Pending,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
                StageView {
                    name: "fix".into(),
                    state: StageState::Pending,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        let text = buf_text(&buf);

        // build is expanded: 1 header + 2 sub-stages = 3 lines
        // review: 1 line, fix: 1 line => stage_list_height = 5
        // epic_height = 1 (no subtasks)
        // Total: 1 + 1 + 1 + 1 + 5 = 9
        assert_eq!(buf.area().height, 9);

        assert!(text.contains("decompose"));
        assert!(text.contains("0:12"));
        assert!(text.contains("implement"));
        assert!(text.contains("3/6"));
        assert!(text.contains("0:34"));
        assert!(text.contains("review"));
        assert!(text.contains("fix"));
    }

    #[test]
    fn stage_with_children() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/fix.md".to_string(),
            epic: make_epic("Fix issues", vec![]),
            stages: vec![
                StageView {
                    name: "fix".into(),
                    state: StageState::Active,
                    progress: Some("1/2".into()),
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![
                        StageChild::Subtask(SubtaskLine {
                            name: "Fix null check".into(),
                            status: SubtaskStatus::Done,
                            agent: Some("cc".into()),
                            elapsed: Some("12s".into()),
                            error: None,
                        }),
                        StageChild::Subtask(SubtaskLine {
                            name: "Fix error format".into(),
                            status: SubtaskStatus::Active,
                            agent: Some("cur".into()),
                            elapsed: None,
                            error: None,
                        }),
                    ],
                },
            ],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        let text = buf_text(&buf);

        assert!(text.contains("fix"));
        assert!(text.contains("1/2"));
        assert!(text.contains("Fix null check"));
        assert!(text.contains("Fix error format"));
        assert!(text.contains("cc"));
        assert!(text.contains("cur"));
    }

    // ── All-done state ───────────────────────────────────────────

    #[test]
    fn all_done_state() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/webhooks.md".to_string(),
            epic: EpicView {
                short_id: "luppzupt".to_string(),
                name: "Implement webhooks".to_string(),
                subtasks: vec![
                    make_subtask("s1", SubtaskStatus::Done),
                    make_subtask("s2", SubtaskStatus::Done),
                ],
                collapsed: true,
                collapsed_summary: Some("6 subtasks  2m28s".to_string()),
            },
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Done,
                    progress: Some("6/6".into()),
                    elapsed: Some("2m28s".into()),
                    sub_stages: vec![],
                    children: vec![],
                },
                StageView {
                    name: "review".into(),
                    state: StageState::Done,
                    progress: Some("0 issues".into()),
                    elapsed: Some("0:42".into()),
                    sub_stages: vec![],
                    children: vec![],
                },
                StageView {
                    name: "fix".into(),
                    state: StageState::Done,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        let text = buf_text(&buf);

        // All stages done, epic collapsed
        // 1 (path) + 1 (blank) + 2 (collapsed) + 1 (blank) + 3 (stages) = 8
        assert_eq!(buf.area().height, 8);

        assert!(text.contains("Implement webhooks"));
        assert!(text.contains("6 subtasks  2m28s"));
        assert!(text.contains("build"));
        assert!(text.contains("6/6"));
        assert!(text.contains("review"));
        assert!(text.contains("0 issues"));
        assert!(text.contains("fix"));
    }

    // ── Edge cases ───────────────────────────────────────────────

    #[test]
    fn no_stages() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/test.md".to_string(),
            epic: make_epic("Solo", vec![]),
            stages: vec![],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        // 1 (path) + 1 (blank) + 1 (epic header, no subtasks) + 1 (blank) + 0 (stages) = 4
        assert_eq!(buf.area().height, 4);

        let text = buf_text(&buf);
        assert!(text.contains("test.md"));
        assert!(text.contains("Solo"));
    }

    // ── Lane DAG integration ─────────────────────────────────────

    #[test]
    fn dag_visible_during_implement() {
        use crate::tui::widgets::lane_dag::{DagLayout, RenderedLane, RenderedSession, SessionState};

        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/webhooks.md".to_string(),
            epic: make_epic("Feature", vec![]),
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Active,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![
                        SubStageView {
                            name: "decompose".into(),
                            state: StageState::Done,
                            progress: None,
                            elapsed: Some("0:12".into()),
                        },
                        SubStageView {
                            name: "implement".into(),
                            state: StageState::Active,
                            progress: Some("3/6".into()),
                            elapsed: Some("0:34".into()),
                        },
                    ],
                    children: vec![],
                },
                StageView {
                    name: "review".into(),
                    state: StageState::Pending,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: Some(DagLayout {
                lanes: vec![
                    RenderedLane {
                        sessions: vec![
                            RenderedSession { state: SessionState::Done, col: 0 },
                            RenderedSession { state: SessionState::Done, col: 2 },
                            RenderedSession { state: SessionState::Active, col: 4 },
                        ],
                        fork_col: None,
                        merge_col: None,
                        parent_lane_idx: None,
                    },
                    RenderedLane {
                        sessions: vec![
                            RenderedSession { state: SessionState::Pending, col: 4 },
                        ],
                        fork_col: Some(2),
                        merge_col: None,
                        parent_lane_idx: Some(0),
                    },
                ],
                width: 5,
                height: 2,
            }),
        };

        let buf = render_workflow(&view, &theme);
        let text = buf_text(&buf);

        // DAG dots should appear in the rendered output
        assert!(text.contains('●'), "should contain done dots");
        assert!(text.contains('◉'), "should contain active dot");
        assert!(text.contains('○'), "should contain pending dot");
        assert!(text.contains('━'), "should contain horizontal connector");
    }

    #[test]
    fn dag_hidden_when_none() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/test.md".to_string(),
            epic: make_epic("Feature", vec![]),
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Active,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        let text = buf_text(&buf);

        // No DAG symbols should appear
        assert!(!text.contains('●'), "no done dot without DAG");
        assert!(!text.contains('◉'), "no active dot without DAG");
        assert!(!text.contains('━'), "no horizontal connector without DAG");
    }

    #[test]
    fn dag_hidden_when_too_simple() {
        use crate::tui::widgets::lane_dag::{DagLayout, RenderedLane, RenderedSession, SessionState};

        let theme = test_theme();
        // 1 lane, 2 sessions → should_show_dag returns false.
        let view = WorkflowView {
            plan_path: "ops/now/test.md".to_string(),
            epic: make_epic("Feature", vec![]),
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Active,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: Some(DagLayout {
                lanes: vec![RenderedLane {
                    sessions: vec![
                        RenderedSession { state: SessionState::Done, col: 0 },
                        RenderedSession { state: SessionState::Active, col: 2 },
                    ],
                    fork_col: None,
                    merge_col: None,
                    parent_lane_idx: None,
                }],
                width: 3,
                height: 1,
            }),
        };

        let buf = render_workflow(&view, &theme);
        let text = buf_text(&buf);

        // DAG is too simple (1 lane, 2 sessions) → should not render
        assert!(!text.contains('●'), "simple DAG should be hidden");
        assert!(!text.contains('◉'), "simple DAG should be hidden");
        assert!(!text.contains('━'), "simple DAG should be hidden");
    }

    #[test]
    fn epic_with_error_lines() {
        let theme = test_theme();
        let view = WorkflowView {
            plan_path: "ops/now/test.md".to_string(),
            epic: make_epic(
                "With errors",
                vec![
                    make_subtask("OK task", SubtaskStatus::Done),
                    SubtaskLine {
                        name: "Failed task".to_string(),
                        status: SubtaskStatus::Failed,
                        agent: None,
                        elapsed: None,
                        error: Some("Connection refused".to_string()),
                    },
                ],
            ),
            stages: vec![
                StageView {
                    name: "build".into(),
                    state: StageState::Failed,
                    progress: None,
                    elapsed: None,
                    sub_stages: vec![],
                    children: vec![],
                },
            ],
            lane_dag: None,
        };

        let buf = render_workflow(&view, &theme);
        let text = buf_text(&buf);

        // epic_height = 1 (header) + 2 (subtasks) + 1 (error line) = 4
        // Total: 1 + 1 + 4 + 1 + 1 = 8
        assert_eq!(buf.area().height, 8);

        assert!(text.contains("OK task"));
        assert!(text.contains("Failed task"));
        assert!(text.contains("Connection refused"));
        assert!(text.contains("build"));
    }
}
