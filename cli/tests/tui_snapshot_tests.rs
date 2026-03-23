use std::path::{Path, PathBuf};

use ab_glyph::{point, Font, FontRef, ScaleFont};
use image::RgbaImage;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use aiki::tui::theme::{
    Theme, SYM_CHECK, SYM_FAILED, SYM_PENDING, SYM_RUNNING,
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

// Tests for lane_dag, stage_list, and workflow views are pending
// implementation of the corresponding modules:
//   tui::widgets::lane_dag, tui::widgets::stage_list,
//   tui::views::workflow, tui::types::{StageState, WorkflowView, …}
