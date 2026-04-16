use color_eyre::eyre::{Result, eyre};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::render::engine::RenderEngine;
use crate::render::lower::{Lower, LowerContext};
use crate::slide::Presentation;
use crate::theme::Theme;

pub fn run(
    presentation: &Presentation,
    theme: &Theme,
    slide_index: usize,
    widths: &[u16],
    height: u16,
) -> Result<()> {
    let total = presentation.slides.len();
    if slide_index >= total {
        return Err(eyre!(
            "Slide {} out of range (deck has {} slides)",
            slide_index + 1,
            total
        ));
    }

    let slide = &presentation.slides[slide_index];
    let num_chunks = slide.chunks.len();

    println!(
        "=== Test fire: slide {} of {} ({} chunks) ===\n",
        slide_index + 1,
        total,
        num_chunks
    );

    // Apply same margin logic as App::content_area
    let (ml, mr, mt, mb) = theme
        .slide
        .as_ref()
        .and_then(|s| s.margin.as_ref())
        .map(|m| {
            (
                m.left.unwrap_or(2),
                m.right.unwrap_or(2),
                m.top.unwrap_or(1),
                m.bottom.unwrap_or(2),
            )
        })
        .unwrap_or((2, 2, 1, 2));

    let bg_color = theme
        .slide
        .as_ref()
        .and_then(|s| s.bg.as_deref())
        .and_then(|c| theme.resolve_color(c));
    let fg_color = theme
        .slide
        .as_ref()
        .and_then(|s| s.fg.as_deref())
        .and_then(|c| theme.resolve_color(c));

    for &w in widths {
        let content_width = w.saturating_sub(ml + mr);
        let content_height = height.saturating_sub(mt + mb);
        let content_area = Rect::new(ml, mt, content_width, content_height);

        let ctx = LowerContext {
            window_width: content_width,
            window_height: content_height,
            theme,
            visible_chunks: slide.chunks.len(),
        };

        // Lower all chunks
        let mut all_ops = Vec::new();
        for chunk in &slide.chunks {
            for element in &chunk.elements {
                all_ops.extend(element.lower(&ctx));
            }
        }

        // Render into a test backend
        let backend = TestBackend::new(w, height);
        let mut terminal = Terminal::new(backend)?;

        terminal.draw(|frame| {
            let mut engine = RenderEngine::new(content_area);
            engine.set_theme(theme);
            engine.set_default_colors(fg_color, bg_color);
            engine.render(&all_ops, frame);
        })?;

        // Read the buffer and find the actual content height
        let buffer = terminal.backend().buffer();
        let mut last_nonempty_row: u16 = 0;
        for y in 0..height {
            for x in 0..w {
                let cell = &buffer[(x, y)];
                let s = cell.symbol();
                if s != " " && !s.is_empty() {
                    last_nonempty_row = y;
                    break;
                }
            }
        }

        let used_rows = if last_nonempty_row > 0 {
            last_nonempty_row.saturating_sub(mt) + 1
        } else {
            0
        };
        let overflow = used_rows > content_height;

        // Print header for this width
        println!(
            "--- {}x{} (content: {}x{}) {} ---",
            w,
            height,
            content_width,
            content_height,
            if overflow {
                format!("⚠ OVERFLOW ({}/{} rows)", used_rows, content_height)
            } else {
                format!("ok ({}/{} rows)", used_rows, content_height)
            }
        );

        // Dump the rendered buffer
        for y in 0..height {
            let mut line = String::new();
            for x in 0..w {
                let cell = &buffer[(x, y)];
                line.push_str(cell.symbol());
            }
            // Trim trailing spaces for readability
            let trimmed = line.trim_end();
            if !trimmed.is_empty() || y < mt + used_rows {
                println!("{}", trimmed);
            }
        }
        println!();
    }

    Ok(())
}
