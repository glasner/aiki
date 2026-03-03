use std::path::{Path, PathBuf};

use ab_glyph::{point, Font, FontRef, ScaleFont};
use image::RgbaImage;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use aiki::tui::theme::{
    detect_mode, Theme, ThemeMode, SYM_CHECK, SYM_FAILED, SYM_PENDING,
    SYM_RUNNING,
};

// ── PNG renderer (inlined from render_png.rs since it's cfg(test)-gated) ─

const CELL_W: u32 = 10;
const CELL_H: u32 = 20;
const FONT_BYTES: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");

fn color_to_rgba(color: Color, default: Color) -> [u8; 4] {
    match color {
        Color::Rgb(r, g, b) => [r, g, b, 255],
        Color::Reset => {
            if let Color::Rgb(r, g, b) = default {
                [r, g, b, 255]
            } else {
                [0, 0, 0, 255]
            }
        }
        _ => [128, 128, 128, 255],
    }
}

fn buffer_to_png(buf: &Buffer, path: &Path, theme: &Theme) -> anyhow::Result<()> {
    let font = FontRef::try_from_slice(FONT_BYTES)?;
    let scale = ab_glyph::PxScale::from(CELL_H as f32);
    let scaled_font = font.as_scaled(scale);

    let area = buf.area();
    let img_w = area.width as u32 * CELL_W;
    let img_h = area.height as u32 * CELL_H;
    let mut img = RgbaImage::new(img_w, img_h);

    for row in 0..area.height {
        for col in 0..area.width {
            let cell = &buf[(col, row)];
            let px_x = col as u32 * CELL_W;
            let px_y = row as u32 * CELL_H;

            let bg = color_to_rgba(cell.bg, theme.bg);
            for y in px_y..px_y + CELL_H {
                for x in px_x..px_x + CELL_W {
                    img.put_pixel(x, y, image::Rgba(bg));
                }
            }

            let symbol = cell.symbol();
            let ch = match symbol.chars().next() {
                Some(c) if c > ' ' => c,
                _ => continue,
            };

            let mut fg = color_to_rgba(cell.fg, theme.text);

            let mods = cell.modifier;
            if mods.contains(Modifier::DIM) {
                fg[0] /= 2;
                fg[1] /= 2;
                fg[2] /= 2;
            }
            if mods.contains(Modifier::BOLD) {
                fg[0] = fg[0].saturating_add(40);
                fg[1] = fg[1].saturating_add(40);
                fg[2] = fg[2].saturating_add(40);
            }

            let glyph_id = font.glyph_id(ch);
            let glyph = glyph_id.with_scale_and_position(
                scale,
                point(px_x as f32, px_y as f32 + scaled_font.ascent()),
            );

            if let Some(outline) = font.outline_glyph(glyph) {
                outline.draw(|gx, gy, cov| {
                    let x = gx;
                    let y = gy;
                    if x < img_w && y < img_h {
                        let alpha = (cov * 255.0) as u8;
                        let bg_pixel = img.get_pixel(x, y).0;
                        let blend = |f: u8, b: u8, a: u8| -> u8 {
                            let fa = a as u16;
                            let ba = 255 - fa;
                            ((f as u16 * fa + b as u16 * ba) / 255) as u8
                        };
                        let blended = [
                            blend(fg[0], bg_pixel[0], alpha),
                            blend(fg[1], bg_pixel[1], alpha),
                            blend(fg[2], bg_pixel[2], alpha),
                            255,
                        ];
                        img.put_pixel(x, y, image::Rgba(blended));
                    }
                });
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    img.save(path)?;
    Ok(())
}

// ── Test helpers ─────────────────────────────────────────────────────

fn render_widget(widget: impl Widget, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);
    buf
}

fn buffer_to_text(buf: &Buffer) -> Vec<String> {
    let area = buf.area();
    (0..area.height)
        .map(|row| {
            (0..area.width)
                .map(|col| {
                    buf.cell((col, row))
                        .map(|c| c.symbol().to_string())
                        .unwrap_or_else(|| " ".to_string())
                })
                .collect::<String>()
        })
        .collect()
}

fn save_png(buf: &Buffer, name: &str, theme: &Theme) {
    let path = PathBuf::from(format!("tests/snapshots/{}.png", name));
    buffer_to_png(buf, &path, theme).expect("PNG save failed");
}

// ── Theme sampler tests ──────────────────────────────────────────────

fn render_theme_sampler(theme: &Theme) -> Buffer {
    let width = 60u16;
    let height = 10u16;
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);

    // Row 0: header
    buf.set_string(0, 0, "Theme Sampler", theme.hi_style());

    // Row 1-2: accent color labels and samples
    let accents: &[(&str, Color)] = &[
        ("green", theme.green),
        ("cyan", theme.cyan),
        ("yellow", theme.yellow),
        ("red", theme.red),
        ("magenta", theme.magenta),
    ];

    let mut x = 0u16;
    for (name, color) in accents {
        let style = Style::default().fg(*color);
        buf.set_string(x, 1, *name, style);
        buf.set_string(x, 2, "Sample", style);
        x += name.len() as u16 + 1;
    }

    // Row 4: structural colors header
    buf.set_string(0, 4, "Structural:", theme.text_style());

    // Row 5: structural colors
    let structural: &[(&str, Style)] = &[
        ("dim", theme.dim_style()),
        ("fg", theme.fg_style()),
        ("text", theme.text_style()),
        ("hi", theme.hi_style()),
    ];

    let mut x = 0u16;
    for (name, style) in structural {
        buf.set_string(x, 5, *name, *style);
        x += name.len() as u16 + 1;
    }

    // Row 7: symbols header
    buf.set_string(0, 7, "Symbols:", theme.text_style());

    // Row 8: all symbols with their associated colors
    let symbols: &[(&str, Color)] = &[
        (SYM_CHECK, theme.green),
        (SYM_RUNNING, theme.yellow),
        (SYM_PENDING, theme.fg),
        (SYM_FAILED, theme.red),
    ];

    let mut x = 0u16;
    for (sym, color) in symbols {
        let style = Style::default().fg(*color);
        buf.set_string(x, 8, *sym, style);
        x += 2;
    }

    buf
}

#[test]
fn snapshot_theme_sampler_dark() {
    let theme = Theme::dark();
    let buf = render_theme_sampler(&theme);
    let text = buffer_to_text(&buf);

    assert!(text[0].contains("Theme Sampler"));
    assert!(text[1].contains("green"));
    assert!(text[1].contains("red"));
    assert!(text[7].contains("Symbols:"));

    save_png(&buf, "theme_sampler_dark", &theme);
}

#[test]
fn snapshot_theme_sampler_light() {
    let theme = Theme::light();
    let buf = render_theme_sampler(&theme);
    let text = buffer_to_text(&buf);

    assert!(text[0].contains("Theme Sampler"));
    assert!(text[1].contains("green"));
    assert!(text[1].contains("red"));
    assert!(text[7].contains("Symbols:"));

    save_png(&buf, "theme_sampler_light", &theme);
}

// ── Lane DAG snapshot tests ──────────────────────────────────────────

use aiki::tui::widgets::lane_dag::{
    DagLayout, LaneDag, RenderedLane, RenderedSession, SessionState,
};

#[test]
fn snapshot_lane_dag_linear() {
    // Linear: ●━●━◉━○━○
    let layout = DagLayout {
        lanes: vec![RenderedLane {
            sessions: vec![
                RenderedSession { state: SessionState::Done, col: 0 },
                RenderedSession { state: SessionState::Done, col: 2 },
                RenderedSession { state: SessionState::Active, col: 4 },
                RenderedSession { state: SessionState::Pending, col: 6 },
                RenderedSession { state: SessionState::Pending, col: 8 },
            ],
            fork_col: None,
            merge_col: None,
            parent_lane_idx: None,
        }],
        width: 9,
        height: 1,
    };

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_widget(LaneDag::new(layout.clone(), &theme), 9, 1);
        let text = buffer_to_text(&buf);
        let line = text[0].trim_end();

        assert_eq!(line, "●━●━◉━○━○", "linear DAG: {}", line);

        save_png(&buf, &format!("lane_dag_linear_{}", suffix), &theme);
    }
}

#[test]
fn snapshot_lane_dag_fan_out() {
    // Fan-out: ●━┬━◉━○
    //            ├━◉
    //            └━○━○
    let layout = DagLayout {
        lanes: vec![
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Done, col: 0 },
                    RenderedSession { state: SessionState::Active, col: 2 },
                    RenderedSession { state: SessionState::Pending, col: 4 },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            },
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Active, col: 4 },
                ],
                fork_col: Some(2),
                merge_col: None,
                parent_lane_idx: Some(0),
            },
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Pending, col: 4 },
                    RenderedSession { state: SessionState::Pending, col: 6 },
                ],
                fork_col: Some(2),
                merge_col: None,
                parent_lane_idx: Some(0),
            },
        ],
        width: 7,
        height: 3,
    };

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_widget(LaneDag::new(layout.clone(), &theme), 7, 3);
        let text = buffer_to_text(&buf);

        // Row 0 should have ┬ fork
        assert!(text[0].contains('┬'), "fan-out row 0: {}", text[0]);
        // Row 1 should have ├ branch
        assert!(text[1].contains('├'), "fan-out row 1: {}", text[1]);
        // Row 2 should have └ last branch
        assert!(text[2].contains('└'), "fan-out row 2: {}", text[2]);

        save_png(&buf, &format!("lane_dag_fan_out_{}", suffix), &theme);
    }
}

#[test]
fn snapshot_lane_dag_fan_in() {
    // Fan-in: ●━┬━●━●━◉
    //            └━●━━━╯
    let layout = DagLayout {
        lanes: vec![
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Done, col: 0 },
                    RenderedSession { state: SessionState::Done, col: 2 },
                    RenderedSession { state: SessionState::Done, col: 4 },
                    RenderedSession { state: SessionState::Active, col: 6 },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            },
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Done, col: 4 },
                ],
                fork_col: Some(2),
                merge_col: Some(6),
                parent_lane_idx: Some(0),
            },
        ],
        width: 7,
        height: 2,
    };

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_widget(LaneDag::new(layout.clone(), &theme), 7, 2);
        let text = buffer_to_text(&buf);

        // Row 1 should have ╯ merge
        assert!(text[1].contains('╯'), "fan-in row 1: {}", text[1]);

        save_png(&buf, &format!("lane_dag_fan_in_{}", suffix), &theme);
    }
}

#[test]
fn snapshot_lane_dag_deep_fan_out() {
    // Deep fan-out: ●━┬━●━◉━○
    //                 ├━◉
    //                 ├━○━○
    //                 └━○
    let layout = DagLayout {
        lanes: vec![
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Done, col: 0 },
                    RenderedSession { state: SessionState::Done, col: 2 },
                    RenderedSession { state: SessionState::Active, col: 4 },
                    RenderedSession { state: SessionState::Pending, col: 6 },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            },
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Active, col: 4 },
                ],
                fork_col: Some(2),
                merge_col: None,
                parent_lane_idx: Some(0),
            },
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Pending, col: 4 },
                    RenderedSession { state: SessionState::Pending, col: 6 },
                ],
                fork_col: Some(2),
                merge_col: None,
                parent_lane_idx: Some(0),
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
        width: 7,
        height: 4,
    };

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_widget(LaneDag::new(layout.clone(), &theme), 7, 4);
        let text = buffer_to_text(&buf);

        assert!(text[0].contains('┬'), "deep row 0: {}", text[0]);
        assert!(text[1].contains('├'), "deep row 1: {}", text[1]);
        assert!(text[2].contains('├'), "deep row 2: {}", text[2]);
        assert!(text[3].contains('└'), "deep row 3: {}", text[3]);

        save_png(&buf, &format!("lane_dag_deep_fan_out_{}", suffix), &theme);
    }
}

#[test]
fn snapshot_lane_dag_failure() {
    // Failure in lane: ●━●━┬━◉━○
    //                      ├━✗
    //                      └━○━○
    let layout = DagLayout {
        lanes: vec![
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Done, col: 0 },
                    RenderedSession { state: SessionState::Done, col: 2 },
                    RenderedSession { state: SessionState::Active, col: 4 },
                    RenderedSession { state: SessionState::Pending, col: 6 },
                ],
                fork_col: None,
                merge_col: None,
                parent_lane_idx: None,
            },
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Failed, col: 4 },
                ],
                fork_col: Some(2),
                merge_col: None,
                parent_lane_idx: Some(0),
            },
            RenderedLane {
                sessions: vec![
                    RenderedSession { state: SessionState::Pending, col: 4 },
                    RenderedSession { state: SessionState::Pending, col: 6 },
                ],
                fork_col: Some(2),
                merge_col: None,
                parent_lane_idx: Some(0),
            },
        ],
        width: 7,
        height: 3,
    };

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_widget(LaneDag::new(layout.clone(), &theme), 7, 3);
        let text = buffer_to_text(&buf);

        // Row 1 should have ✗ (failed)
        assert!(text[1].contains('✗'), "failure row 1: {}", text[1]);

        save_png(&buf, &format!("lane_dag_failure_{}", suffix), &theme);
    }
}

#[test]
fn snapshot_workflow_with_dag() {
    use aiki::tui::types::{
        EpicView, StageState, StageView, SubStageView, SubtaskLine, SubtaskStatus,
        WorkflowView,
    };
    use aiki::tui::views::workflow::render_workflow;

    let view = WorkflowView {
        plan_path: "ops/now/webhooks.md".to_string(),
        epic: EpicView {
            short_id: "luppzupt".to_string(),
            name: "Implement Stripe webhook event handling".to_string(),
            subtasks: vec![
                SubtaskLine {
                    name: "Explore webhook requirements".into(),
                    status: SubtaskStatus::Done,
                    agent: Some("cc".into()),
                    elapsed: Some("8s".into()),
                    error: None,
                },
                SubtaskLine {
                    name: "Create implementation plan".into(),
                    status: SubtaskStatus::Done,
                    agent: Some("cc".into()),
                    elapsed: Some("6s".into()),
                    error: None,
                },
                SubtaskLine {
                    name: "Implement webhook route handler".into(),
                    status: SubtaskStatus::Active,
                    agent: Some("cur".into()),
                    elapsed: None,
                    error: None,
                },
                SubtaskLine {
                    name: "Verify Stripe signatures".into(),
                    status: SubtaskStatus::Active,
                    agent: Some("cc".into()),
                    elapsed: None,
                    error: None,
                },
                SubtaskLine {
                    name: "Add idempotency key tracking".into(),
                    status: SubtaskStatus::Pending,
                    agent: None,
                    elapsed: None,
                    error: None,
                },
                SubtaskLine {
                    name: "Write integration tests".into(),
                    status: SubtaskStatus::Pending,
                    agent: None,
                    elapsed: None,
                    error: None,
                },
            ],
            collapsed: false,
            collapsed_summary: None,
        },
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
        lane_dag: Some(DagLayout {
            lanes: vec![
                RenderedLane {
                    sessions: vec![
                        RenderedSession { state: SessionState::Done, col: 0 },
                        RenderedSession { state: SessionState::Done, col: 2 },
                        RenderedSession { state: SessionState::Active, col: 4 },
                        RenderedSession { state: SessionState::Pending, col: 6 },
                    ],
                    fork_col: None,
                    merge_col: None,
                    parent_lane_idx: None,
                },
                RenderedLane {
                    sessions: vec![
                        RenderedSession { state: SessionState::Active, col: 4 },
                    ],
                    fork_col: Some(2),
                    merge_col: None,
                    parent_lane_idx: Some(0),
                },
                RenderedLane {
                    sessions: vec![
                        RenderedSession { state: SessionState::Pending, col: 4 },
                        RenderedSession { state: SessionState::Pending, col: 6 },
                    ],
                    fork_col: Some(2),
                    merge_col: None,
                    parent_lane_idx: Some(0),
                },
            ],
            width: 7,
            height: 3,
        }),
    };

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_workflow(&view, &theme);
        let text = buffer_to_text(&buf);
        let flat = text.join("\n");

        // DAG symbols appear
        assert!(flat.contains('●'), "done dots: {}", suffix);
        assert!(flat.contains('◉'), "active dot: {}", suffix);
        assert!(flat.contains('┬'), "fork-down: {}", suffix);

        // Stage and epic text
        assert!(flat.contains("Implement Stripe webhook"), "epic name");
        assert!(flat.contains("build"), "stage build");
        assert!(flat.contains("3/6"), "progress");

        save_png(&buf, &format!("workflow_with_dag_{}", suffix), &theme);
    }
}

// ── Theme detection tests ────────────────────────────────────────────

#[test]
fn test_detect_mode_env_override() {
    // Test light override
    std::env::set_var("AIKI_THEME", "light");
    assert_eq!(detect_mode(), ThemeMode::Light);

    // Test dark override
    std::env::set_var("AIKI_THEME", "dark");
    assert_eq!(detect_mode(), ThemeMode::Dark);

    // Test case insensitivity
    std::env::set_var("AIKI_THEME", "Light");
    assert_eq!(detect_mode(), ThemeMode::Light);

    std::env::set_var("AIKI_THEME", "DARK");
    assert_eq!(detect_mode(), ThemeMode::Dark);

    // Clean up
    std::env::remove_var("AIKI_THEME");
}

// ── Epic show snapshot tests ────────────────────────────────────────

use aiki::tasks::types::{Task, TaskOutcome, TaskPriority, TaskStatus};
use aiki::tui::views::epic_show::render_epic_show;
use chrono::Utc;
use std::collections::HashMap;

fn make_mock_task(
    id: &str,
    name: &str,
    status: TaskStatus,
    outcome: Option<TaskOutcome>,
) -> Task {
    Task {
        id: format!("{:a<32}", id), // pad to 32 chars
        name: name.to_string(),
        slug: None,
        task_type: None,
        status,
        priority: TaskPriority::P2,
        assignee: Some("claude-code".to_string()),
        sources: vec![],
        template: None,
        instructions: None,
        data: HashMap::new(),
        created_at: Utc::now(),
        started_at: Some(Utc::now()),
        claimed_by_session: None,
        last_session_id: None,
        stopped_reason: None,
        closed_outcome: outcome,
        summary: None,
        turn_started: None,
        turn_closed: None,
        turn_stopped: None,
        comments: vec![],
    }
}

#[test]
fn snapshot_epic_show_build_complete() {
    let epic = make_mock_task("epic-complete", "Deploy webhook pipeline", TaskStatus::InProgress, None);
    let t1 = make_mock_task("sub1", "Write handler", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t2 = make_mock_task("sub2", "Add tests", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t3 = make_mock_task("sub3", "Setup CI", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t4 = make_mock_task("sub4", "Write docs", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t5 = make_mock_task("sub5", "Add monitoring", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t6 = make_mock_task("sub6", "Deploy to staging", TaskStatus::Closed, Some(TaskOutcome::Done));

    let subtasks: Vec<&Task> = vec![&t1, &t2, &t3, &t4, &t5, &t6];

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_epic_show(&epic, &subtasks, "ops/now/webhooks.md", &theme);
        let text = buffer_to_text(&buf);
        let flat = text.join("\n");

        // Epic name appears
        assert!(flat.contains("Deploy webhook pipeline"), "Epic name missing");

        // All subtask names appear
        assert!(flat.contains("Write handler"), "Subtask 1 missing");
        assert!(flat.contains("Add tests"), "Subtask 2 missing");
        assert!(flat.contains("Setup CI"), "Subtask 3 missing");
        assert!(flat.contains("Write docs"), "Subtask 4 missing");
        assert!(flat.contains("Add monitoring"), "Subtask 5 missing");
        assert!(flat.contains("Deploy to staging"), "Subtask 6 missing");

        // Check symbols for completed tasks
        assert!(flat.contains(SYM_CHECK), "✓ symbols missing for completed tasks");

        // Stage track shows build done with 6/6
        assert!(flat.contains("6/6"), "Stage track should show 6/6");
        assert!(flat.contains("build"), "Stage track should show build phase");

        save_png(&buf, &format!("epic_show_build_complete_{}", suffix), &theme);
    }
}

#[test]
fn snapshot_epic_show_build_failure() {
    let epic = make_mock_task("epic-fail", "Auth system migration", TaskStatus::InProgress, None);
    let t1 = make_mock_task("done1", "Migrate user model", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t2 = make_mock_task("done2", "Update login flow", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t3 = make_mock_task("done3", "Add OAuth provider", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t4 = make_mock_task("done4", "Write unit tests", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t5 = make_mock_task("done5", "Update API docs", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t6 = make_mock_task("done6", "Add rate limiting", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t7 = make_mock_task("done7", "Setup monitoring", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t8 = make_mock_task("done8", "Add audit logging", TaskStatus::Closed, Some(TaskOutcome::Done));
    let mut t9 = make_mock_task("fail1", "Integration tests", TaskStatus::Stopped, None);
    t9.stopped_reason = Some("Redis connection refused on CI".to_string());
    let t10 = make_mock_task("pend1", "Deploy to production", TaskStatus::Open, None);

    let subtasks: Vec<&Task> = vec![&t1, &t2, &t3, &t4, &t5, &t6, &t7, &t8, &t9, &t10];

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_epic_show(&epic, &subtasks, "ops/now/auth-migration.md", &theme);
        let text = buffer_to_text(&buf);
        let flat = text.join("\n");

        // ✗ appears for the failed task
        assert!(flat.contains(SYM_FAILED), "✗ symbol missing for failed task");

        // Error message appears below the failed task
        assert!(
            flat.contains("Redis connection refused on CI"),
            "Error message missing for failed task"
        );

        // Stage track shows failure
        assert!(flat.contains("1 failed"), "Stage track should show failure count");

        save_png(&buf, &format!("epic_show_build_failure_{}", suffix), &theme);
    }
}

#[test]
fn snapshot_epic_show_build_in_progress() {
    let epic = make_mock_task("epic-prog", "Feature rollout", TaskStatus::InProgress, None);
    let t1 = make_mock_task("comp1", "Design schema", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t2 = make_mock_task("comp2", "Create migrations", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t3 = make_mock_task("run1", "Implement API", TaskStatus::InProgress, None);
    let t4 = make_mock_task("run2", "Build frontend", TaskStatus::InProgress, None);
    let t5 = make_mock_task("wait1", "Write E2E tests", TaskStatus::Open, None);
    let t6 = make_mock_task("wait2", "Deploy canary", TaskStatus::Open, None);

    let subtasks: Vec<&Task> = vec![&t1, &t2, &t3, &t4, &t5, &t6];

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_epic_show(&epic, &subtasks, "ops/now/rollout.md", &theme);
        let text = buffer_to_text(&buf);
        let flat = text.join("\n");

        // ✓ for completed
        assert!(flat.contains(SYM_CHECK), "✓ missing for completed tasks");
        // ▸ for in-progress
        assert!(flat.contains(SYM_RUNNING), "▸ missing for in-progress tasks");
        // ○ for pending
        assert!(flat.contains(SYM_PENDING), "○ missing for pending tasks");

        // Stage track shows active build with progress 2/6
        assert!(flat.contains("2/6"), "Stage track should show 2/6 progress");

        save_png(&buf, &format!("epic_show_build_in_progress_{}", suffix), &theme);
    }
}

#[test]
fn snapshot_epic_show_mid_fix() {
    // Scenario: all build subtasks done, review done, fix phase active.
    // Since compute_phases currently derives only build from subtasks (review/fix
    // are always Pending), we test that build shows Done and the three phases appear.
    let epic = make_mock_task("epic-fix", "Refactor auth module", TaskStatus::InProgress, None);
    let t1 = make_mock_task("bld1", "Extract token validator", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t2 = make_mock_task("bld2", "Split middleware", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t3 = make_mock_task("bld3", "Update error types", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t4 = make_mock_task("bld4", "Add integration tests", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t5 = make_mock_task("bld5", "Update OpenAPI spec", TaskStatus::Closed, Some(TaskOutcome::Done));
    let t6 = make_mock_task("bld6", "Run full test suite", TaskStatus::Closed, Some(TaskOutcome::Done));

    let subtasks: Vec<&Task> = vec![&t1, &t2, &t3, &t4, &t5, &t6];

    for (mode, suffix) in [
        (ThemeMode::Dark, "dark"),
        (ThemeMode::Light, "light"),
    ] {
        let theme = Theme::from_mode(mode);
        let buf = render_epic_show(&epic, &subtasks, "ops/now/auth-refactor.md", &theme);
        let text = buffer_to_text(&buf);
        let flat = text.join("\n");

        // Stage track shows all three phase names
        assert!(flat.contains("build"), "Stage track should show build phase");
        assert!(flat.contains("review"), "Stage track should show review phase");
        assert!(flat.contains("fix"), "Stage track should show fix phase");

        // Build phase is done (all subtasks completed): ✓ build  6/6
        assert!(flat.contains(SYM_CHECK), "✓ should appear for completed build");
        assert!(flat.contains("6/6"), "Stage track should show 6/6 for build");

        save_png(&buf, &format!("epic_show_mid_fix_{}", suffix), &theme);
    }
}
