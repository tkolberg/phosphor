use std::path::Path;

use color_eyre::eyre::{Result, WrapErr};
use image::GenericImageView;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Render an image as halfblock lines.
///
/// Each terminal cell represents 2 vertical pixels using the upper-half-block
/// character (▀) with fg = top pixel, bg = bottom pixel.
///
/// Returns a vec of ratatui `Line`s ready to render, plus the width in columns.
pub fn image_to_halfblock_lines(
    path: &Path,
    max_width: u16,
    max_height: u16,
) -> Result<Vec<Line<'static>>> {
    let img = image::open(path)
        .wrap_err_with(|| format!("Failed to open image: {}", path.display()))?;

    let (orig_w, orig_h) = img.dimensions();
    if orig_w == 0 || orig_h == 0 {
        return Ok(vec![]);
    }

    // Each terminal row = 2 pixel rows, so max pixel height = max_height * 2
    let max_px_w = max_width as u32;
    let max_px_h = (max_height as u32) * 2;

    // Scale to fit, preserving aspect ratio
    let scale = f64::min(
        max_px_w as f64 / orig_w as f64,
        max_px_h as f64 / orig_h as f64,
    );

    let new_w = ((orig_w as f64 * scale) as u32).max(1);
    let new_h = ((orig_h as f64 * scale) as u32).max(1);

    let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Triangle);
    let rgba = resized.to_rgba8();

    // Build halfblock lines: pair up rows
    let mut lines = Vec::new();
    let mut y = 0u32;
    while y < new_h {
        let mut spans: Vec<Span<'static>> = Vec::new();

        for x in 0..new_w {
            let top = rgba.get_pixel(x, y);
            let bottom = if y + 1 < new_h {
                rgba.get_pixel(x, y + 1)
            } else {
                // Odd height: bottom pixel is transparent/black
                &image::Rgba([0, 0, 0, 0])
            };

            let fg = pixel_to_color(top);
            let bg = pixel_to_color(bottom);

            let style = Style::default().fg(fg).bg(bg);
            spans.push(Span::styled("▀", style));
        }

        lines.push(Line::from(spans));
        y += 2;
    }

    Ok(lines)
}

fn pixel_to_color(pixel: &image::Rgba<u8>) -> Color {
    let [r, g, b, a] = pixel.0;
    if a < 128 {
        // Transparent — use black (will blend with slide background)
        Color::Rgb(0, 0, 0)
    } else {
        Color::Rgb(r, g, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_to_color_opaque() {
        let px = image::Rgba([255, 128, 0, 255]);
        assert_eq!(pixel_to_color(&px), Color::Rgb(255, 128, 0));
    }

    #[test]
    fn test_pixel_to_color_transparent() {
        let px = image::Rgba([255, 128, 0, 50]);
        assert_eq!(pixel_to_color(&px), Color::Rgb(0, 0, 0));
    }
}
