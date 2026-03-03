// PNG rendering utilities for TUI snapshot testing

use std::path::Path;

use ab_glyph::{point, Font, FontRef, ScaleFont};
use anyhow::Result;
use image::RgbaImage;
use ratatui::style::{Color, Modifier};

use super::theme::Theme;

const CELL_W: u32 = 10;
const CELL_H: u32 = 20;

const FONT_BYTES: &[u8] = include_bytes!("../../assets/JetBrainsMono-Regular.ttf");

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

/// Render a ratatui buffer to a PNG file.
///
/// Each cell is rendered as a `CELL_W`×`CELL_H` pixel rectangle with the
/// cell's background color filled and the character glyph rasterized in the
/// foreground color. `Color::Reset` falls back to `theme.bg` / `theme.text`.
pub fn buffer_to_png(buf: &ratatui::buffer::Buffer, path: &Path, theme: &Theme) -> Result<()> {
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

            // Fill background
            let bg = color_to_rgba(cell.bg, theme.bg);
            for y in px_y..px_y + CELL_H {
                for x in px_x..px_x + CELL_W {
                    img.put_pixel(x, y, image::Rgba(bg));
                }
            }

            // Render foreground glyph
            let symbol = cell.symbol();
            let ch = match symbol.chars().next() {
                Some(c) if c > ' ' => c,
                _ => continue,
            };

            let mut fg = color_to_rgba(cell.fg, theme.text);

            // Apply modifiers
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
                        // Alpha-blend foreground over background
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
