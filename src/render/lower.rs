use crate::chart;
use crate::elements::*;
use crate::render::ops::*;
use crate::theme::Theme;

pub struct LowerContext<'a> {
    pub window_width: u16,
    pub window_height: u16,
    pub theme: &'a Theme,
}

pub trait Lower {
    fn lower(&self, ctx: &LowerContext) -> Vec<RenderOp>;
}

impl Lower for SlideElement {
    fn lower(&self, ctx: &LowerContext) -> Vec<RenderOp> {
        match self {
            SlideElement::Heading { level, text } => lower_heading(*level, text, ctx),
            SlideElement::Paragraph { text } => lower_paragraph(text, ctx),
            SlideElement::Code { code, .. } => lower_code(code, ctx),
            SlideElement::List {
                items,
                ordered,
                start,
                ..
            } => lower_list(items, *ordered, *start, ctx),
            SlideElement::BlockQuote { text } => lower_blockquote(text, ctx),
            SlideElement::HorizontalRule => lower_horizontal_rule(ctx),
            SlideElement::Chart { spec, base_dir } => lower_chart(spec, base_dir, ctx),
            SlideElement::Table {
                headers,
                rows,
                alignments,
            } => lower_table(headers, rows, alignments, ctx),
            SlideElement::Image {
                path,
                alt,
                base_dir,
            } => lower_image(path, alt, base_dir, ctx),
            SlideElement::Diagram { source } => lower_diagram(source, ctx),
            SlideElement::Wireframe { source } => lower_wireframe(source, ctx),
            SlideElement::Spacer => vec![RenderOp::Spacer { lines: 1 }],
            SlideElement::ChunkBreak => vec![], // should never reach here
        }
    }
}

/// Word-wrap a StyledText into multiple lines that fit within max_width.
fn wrap_styled_text(text: &StyledText, max_width: usize) -> Vec<StyledText> {
    if max_width == 0 {
        return vec![text.clone()];
    }

    let total: usize = text.segments.iter().map(|s| s.text.chars().count()).sum();
    if total <= max_width {
        return vec![text.clone()];
    }

    // Flatten to (char, segment_index) pairs
    let mut chars: Vec<(char, usize)> = Vec::new();
    for (idx, seg) in text.segments.iter().enumerate() {
        for c in seg.text.chars() {
            chars.push((c, idx));
        }
    }

    let mut lines: Vec<Vec<(char, usize)>> = Vec::new();
    let mut pos = 0;

    while pos < chars.len() {
        // Skip leading spaces on continuation lines
        if !lines.is_empty() {
            while pos < chars.len() && chars[pos].0 == ' ' {
                pos += 1;
            }
        }
        if pos >= chars.len() {
            break;
        }

        let line_start = pos;
        let line_end = (pos + max_width).min(chars.len());

        if line_end >= chars.len() {
            lines.push(chars[line_start..].to_vec());
            break;
        }

        // Find last space within the line to break at
        let mut break_at = line_end;
        let mut found_space = false;
        for j in (line_start..line_end).rev() {
            if chars[j].0 == ' ' {
                break_at = j;
                found_space = true;
                break;
            }
        }

        if !found_space {
            break_at = line_end;
        }

        lines.push(chars[line_start..break_at].to_vec());
        pos = break_at;
        if found_space && pos < chars.len() && chars[pos].0 == ' ' {
            pos += 1;
        }
    }

    // Convert back to StyledText, coalescing adjacent chars with the same style
    lines
        .iter()
        .map(|line_chars| {
            let mut segments: Vec<TextSegment> = Vec::new();
            for &(c, style_idx) in line_chars {
                let style = &text.segments[style_idx].style;
                if let Some(last) = segments.last_mut() {
                    if &last.style == style {
                        last.text.push(c);
                        continue;
                    }
                }
                segments.push(TextSegment {
                    text: c.to_string(),
                    style: style.clone(),
                });
            }
            // Trim trailing spaces
            if let Some(last) = segments.last_mut() {
                let trimmed = last.text.trim_end().to_string();
                if trimmed.is_empty() {
                    segments.pop();
                } else {
                    last.text = trimmed;
                }
            }
            StyledText { segments }
        })
        .filter(|st| !st.segments.is_empty())
        .collect()
}

fn lower_heading(_level: u8, text: &StyledText, ctx: &LowerContext) -> Vec<RenderOp> {
    let mut bold_text = text.clone();
    for seg in &mut bold_text.segments {
        seg.style.bold = true;
    }

    let mut ops = Vec::new();
    ops.push(RenderOp::Spacer { lines: 1 });

    // Apply heading color from theme
    if let Some(ref style) = ctx.theme.styles.heading {
        let fg = style.fg.as_deref().and_then(|c| ctx.theme.resolve_color(c));
        if fg.is_some() {
            ops.push(RenderOp::SetColors { fg, bg: None });
        }
    }

    for line in wrap_styled_text(&bold_text, ctx.window_width as usize) {
        ops.push(RenderOp::RenderText {
            line,
            alignment: Alignment::Center,
        });
    }

    // Reset colors after heading
    if ctx.theme.styles.heading.as_ref().and_then(|s| s.fg.as_ref()).is_some() {
        ops.push(RenderOp::SetColors { fg: None, bg: None });
    }

    ops.push(RenderOp::Spacer { lines: 1 });
    ops
}

fn lower_paragraph(text: &StyledText, ctx: &LowerContext) -> Vec<RenderOp> {
    let mut ops = Vec::new();
    for line in wrap_styled_text(text, ctx.window_width as usize) {
        ops.push(RenderOp::RenderText {
            line,
            alignment: Alignment::Left,
        });
    }
    ops.push(RenderOp::Spacer { lines: 1 });
    ops
}

fn lower_code(code: &str, ctx: &LowerContext) -> Vec<RenderOp> {
    let (fg, bg, padding) = if let Some(ref style) = ctx.theme.styles.code {
        (
            style.fg.as_deref().and_then(|c| ctx.theme.resolve_color(c)),
            style.bg.as_deref().and_then(|c| ctx.theme.resolve_color(c)),
            style.padding.unwrap_or(2),
        )
    } else {
        (None, None, 2)
    };

    let mut ops = vec![
        RenderOp::PushWindowRect {
            margin: Margin {
                left: padding,
                right: padding,
                top: 0,
                bottom: 0,
            },
        },
        RenderOp::SetColors { fg, bg },
    ];

    for line in code.lines() {
        ops.push(RenderOp::RenderText {
            line: StyledText::plain(line),
            alignment: Alignment::Left,
        });
    }

    ops.push(RenderOp::SetColors { fg: None, bg: None });
    ops.push(RenderOp::PopWindowRect);
    ops.push(RenderOp::Spacer { lines: 1 });
    ops
}

fn lower_list(items: &[ListItem], ordered: bool, start: usize, ctx: &LowerContext) -> Vec<RenderOp> {
    let mut ops = Vec::new();
    let mut ordered_index = start;

    for item in items {
        let indent = "  ".repeat(item.depth as usize);
        let prefix = if ordered {
            let p = format!("{indent}{}. ", ordered_index);
            ordered_index += 1;
            p
        } else {
            format!("{indent}• ")
        };

        let prefix_len = prefix.chars().count();
        let mut line = item.text.clone();
        if let Some(first) = line.segments.first_mut() {
            first.text = format!("{prefix}{}", first.text);
        } else {
            line.segments.push(TextSegment {
                text: prefix,
                style: SegmentStyle::default(),
            });
        }

        let wrapped = wrap_styled_text(&line, ctx.window_width as usize);
        for (i, wline) in wrapped.into_iter().enumerate() {
            if i > 0 {
                // Hanging indent: continuation lines align with text after bullet
                let mut indented = StyledText::plain(" ".repeat(prefix_len));
                indented.segments.extend(wline.segments);
                ops.push(RenderOp::RenderText {
                    line: indented,
                    alignment: Alignment::Left,
                });
            } else {
                ops.push(RenderOp::RenderText {
                    line: wline,
                    alignment: Alignment::Left,
                });
            }
        }
    }
    ops.push(RenderOp::Spacer { lines: 1 });
    ops
}

fn lower_blockquote(text: &StyledText, ctx: &LowerContext) -> Vec<RenderOp> {
    let mut styled = text.clone();
    for seg in &mut styled.segments {
        seg.style.italic = true;
    }

    let mut ops = Vec::new();

    // Apply blockquote color from theme
    if let Some(ref style) = ctx.theme.styles.blockquote {
        let fg = style.fg.as_deref().and_then(|c| ctx.theme.resolve_color(c));
        if fg.is_some() {
            ops.push(RenderOp::SetColors { fg, bg: None });
        }
    }

    let bq_margin = 4u16;
    let wrap_width = ctx.window_width.saturating_sub(bq_margin * 2) as usize;

    ops.push(RenderOp::PushWindowRect {
        margin: Margin {
            left: bq_margin,
            right: bq_margin,
            top: 0,
            bottom: 0,
        },
    });
    for (i, wline) in wrap_styled_text(&styled, wrap_width).into_iter().enumerate() {
        // Prepend │ to each wrapped line
        let mut prefixed = StyledText::plain(if i == 0 { "│ " } else { "│ " });
        prefixed.segments.extend(wline.segments);
        ops.push(RenderOp::RenderText {
            line: prefixed,
            alignment: Alignment::Left,
        });
    }
    ops.push(RenderOp::PopWindowRect);

    if ctx.theme.styles.blockquote.as_ref().and_then(|s| s.fg.as_ref()).is_some() {
        ops.push(RenderOp::SetColors { fg: None, bg: None });
    }

    ops.push(RenderOp::Spacer { lines: 1 });
    ops
}

fn lower_horizontal_rule(ctx: &LowerContext) -> Vec<RenderOp> {
    let width = ctx.window_width.saturating_sub(4) as usize;
    vec![
        RenderOp::RenderText {
            line: StyledText::plain("─".repeat(width)),
            alignment: Alignment::Center,
        },
        RenderOp::Spacer { lines: 1 },
    ]
}

fn lower_chart(spec: &chart::ChartSpec, base_dir: &std::path::Path, ctx: &LowerContext) -> Vec<RenderOp> {
    match chart::load_chart_data(spec, base_dir) {
        Ok(data) => {
            let mut ops = Vec::new();
            if let Some(ref title) = spec.title {
                let mut title_text = StyledText::plain(title.as_str());
                title_text.segments[0].style.bold = true;
                ops.push(RenderOp::RenderText {
                    line: title_text,
                    alignment: Alignment::Center,
                });
                ops.push(RenderOp::Spacer { lines: 1 });
            }
            // Use ~60% of available height for chart
            let chart_height = (ctx.window_height as f32 * 0.6) as u16;
            let chart_height = chart_height.max(8);
            ops.push(RenderOp::RenderChart {
                spec: spec.clone(),
                data,
                height: chart_height,
            });
            ops.push(RenderOp::Spacer { lines: 1 });
            ops
        }
        Err(_) => {
            // Fallback: show error message
            vec![
                RenderOp::RenderText {
                    line: StyledText::plain(format!("[Chart error: could not load {}]", spec.file)),
                    alignment: Alignment::Center,
                },
                RenderOp::Spacer { lines: 1 },
            ]
        }
    }
}

fn lower_table(
    headers: &[StyledText],
    rows: &[Vec<StyledText>],
    alignments: &[TableAlignment],
    ctx: &LowerContext,
) -> Vec<RenderOp> {
    let num_cols = headers.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    if num_cols == 0 {
        return vec![RenderOp::Spacer { lines: 1 }];
    }

    // Compute column widths: max of header and all row cells (as plain text length)
    let cell_text_len = |st: &StyledText| -> usize {
        st.segments.iter().map(|s| s.text.len()).sum()
    };

    let mut col_widths: Vec<usize> = vec![0; num_cols];
    for (i, h) in headers.iter().enumerate() {
        col_widths[i] = col_widths[i].max(cell_text_len(h));
    }
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                col_widths[i] = col_widths[i].max(cell_text_len(cell));
            }
        }
    }

    // Clamp total table width to available space
    let padding = 2; // 1 space each side inside cell
    let border_chars = num_cols + 1; // │ between and at edges
    let total_inner: usize = col_widths.iter().map(|w| w + padding).sum::<usize>() + border_chars;
    let available = ctx.window_width as usize;

    // If table is too wide, shrink proportionally
    if total_inner > available && available > border_chars {
        let budget = available - border_chars;
        let total_content: usize = col_widths.iter().map(|w| w + padding).sum();
        for w in &mut col_widths {
            let cell_total = *w + padding;
            let scaled = (cell_total as f64 / total_content as f64 * budget as f64) as usize;
            *w = scaled.saturating_sub(padding).max(1);
        }
    }

    let format_cell = |text: &StyledText, col: usize| -> String {
        let content: String = text.segments.iter().map(|s| s.text.as_str()).collect();
        let w = col_widths[col];
        let align = alignments.get(col).copied().unwrap_or(TableAlignment::None);
        match align {
            TableAlignment::Center => format!(" {:^w$} ", content, w = w),
            TableAlignment::Right => format!(" {:>w$} ", content, w = w),
            _ => format!(" {:<w$} ", content, w = w),
        }
    };

    let separator = |left: char, mid: char, right: char, fill: char| -> String {
        let mut s = String::new();
        s.push(left);
        for (i, w) in col_widths.iter().enumerate() {
            let cell_w = w + padding;
            for _ in 0..cell_w {
                s.push(fill);
            }
            if i + 1 < num_cols {
                s.push(mid);
            }
        }
        s.push(right);
        s
    };

    let build_row = |cells: &[StyledText]| -> String {
        let mut s = String::new();
        s.push('│');
        for i in 0..num_cols {
            let empty = StyledText::plain("");
            let cell = cells.get(i).unwrap_or(&empty);
            s.push_str(&format_cell(cell, i));
            if i + 1 < num_cols {
                s.push('│');
            }
        }
        s.push('│');
        s
    };

    let mut ops = Vec::new();

    // Top border
    ops.push(RenderOp::RenderText {
        line: StyledText::plain(separator('┌', '┬', '┐', '─')),
        alignment: Alignment::Left,
    });

    // Header row
    ops.push(RenderOp::RenderText {
        line: StyledText::plain(build_row(headers)),
        alignment: Alignment::Left,
    });

    // Header/body separator
    ops.push(RenderOp::RenderText {
        line: StyledText::plain(separator('├', '┼', '┤', '─')),
        alignment: Alignment::Left,
    });

    // Body rows
    for row in rows {
        ops.push(RenderOp::RenderText {
            line: StyledText::plain(build_row(row)),
            alignment: Alignment::Left,
        });
    }

    // Bottom border
    ops.push(RenderOp::RenderText {
        line: StyledText::plain(separator('└', '┴', '┘', '─')),
        alignment: Alignment::Left,
    });

    ops.push(RenderOp::Spacer { lines: 1 });
    ops
}

fn lower_image(
    path: &str,
    alt: &str,
    base_dir: &std::path::Path,
    ctx: &LowerContext,
) -> Vec<RenderOp> {
    let image_path = base_dir.join(path);

    // Use most of the available terminal space for the image
    // Leave a few rows for the caption and slide chrome
    let max_height = ctx.window_height.saturating_sub(4);
    let max_width = ctx.window_width;

    match crate::halfblock::image_to_halfblock_lines(&image_path, max_width, max_height) {
        Ok(lines) => {
            if lines.is_empty() {
                return vec![RenderOp::Spacer { lines: 1 }];
            }
            let img_width = lines
                .first()
                .map(|l| l.width() as u16)
                .unwrap_or(0);
            let mut ops = Vec::new();

            // Show alt text as caption if non-empty
            if !alt.is_empty() {
                let mut caption = StyledText::plain(alt);
                caption.segments[0].style.italic = true;
                ops.push(RenderOp::RenderText {
                    line: caption,
                    alignment: Alignment::Center,
                });
                ops.push(RenderOp::Spacer { lines: 1 });
            }

            ops.push(RenderOp::RenderImage {
                lines,
                width: img_width,
            });
            ops.push(RenderOp::Spacer { lines: 1 });
            ops
        }
        Err(_) => {
            vec![
                RenderOp::RenderText {
                    line: StyledText::plain(format!("[Image error: {}]", path)),
                    alignment: Alignment::Center,
                },
                RenderOp::Spacer { lines: 1 },
            ]
        }
    }
}

fn lower_wireframe(source: &str, ctx: &LowerContext) -> Vec<RenderOp> {
    let spec = crate::wireframe::parse_wireframe_spec(source);
    // Use ~90% of available height for the wireframe
    let wf_rows = ((ctx.window_height as f32) * 0.9) as u16;
    let wf_rows = wf_rows.max(10);
    let wf_cols = ctx.window_width;

    let lines = crate::wireframe::render_wireframe(&spec, wf_cols, wf_rows);
    let width = wf_cols;

    let mut ops = Vec::new();
    ops.push(RenderOp::RenderImage {
        lines,
        width,
    });
    ops.push(RenderOp::Spacer { lines: 1 });
    ops
}

fn lower_diagram(source: &str, ctx: &LowerContext) -> Vec<RenderOp> {
    // Delegate to the diagram module for parsing, layout, and text rendering
    let lines = crate::diagram::render_diagram(source, ctx.window_width);
    let mut ops = Vec::new();
    for line in lines {
        ops.push(RenderOp::RenderText {
            line,
            alignment: Alignment::Left,
        });
    }
    ops.push(RenderOp::Spacer { lines: 1 });
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::loader::default_theme;

    fn test_ctx() -> LowerContext<'static> {
        // Leak a default theme for test convenience
        let theme = Box::leak(Box::new(default_theme()));
        LowerContext {
            window_width: 80,
            window_height: 40,
            theme,
        }
    }

    #[test]
    fn test_heading_lowers_centered_bold() {
        let element = SlideElement::Heading {
            level: 1,
            text: StyledText::plain("Hello"),
        };
        let ctx = test_ctx();
        let ops = element.lower(&ctx);

        // Find the RenderText op (may have SetColors before it)
        let text_op = ops.iter().find(|op| matches!(op, RenderOp::RenderText { .. }));
        match text_op {
            Some(RenderOp::RenderText { alignment, line }) => {
                assert!(matches!(alignment, Alignment::Center));
                assert!(line.segments[0].style.bold);
            }
            _ => panic!("Expected RenderText"),
        }
    }

    #[test]
    fn test_paragraph_lowers_left_aligned() {
        let element = SlideElement::Paragraph {
            text: StyledText::plain("Some text"),
        };
        let ctx = test_ctx();
        let ops = element.lower(&ctx);

        match &ops[0] {
            RenderOp::RenderText { alignment, .. } => {
                assert!(matches!(alignment, Alignment::Left));
            }
            _ => panic!("Expected RenderText"),
        }
    }

    #[test]
    fn test_list_lowers_with_bullets() {
        let items = vec![
            ListItem {
                depth: 0,
                text: StyledText::plain("First"),
            },
            ListItem {
                depth: 0,
                text: StyledText::plain("Second"),
            },
        ];
        let element = SlideElement::List {
            items,
            ordered: false,
            start: 1,
        };
        let ctx = test_ctx();
        let ops = element.lower(&ctx);

        match &ops[0] {
            RenderOp::RenderText { line, .. } => {
                assert!(line.segments[0].text.starts_with("• "));
            }
            _ => panic!("Expected RenderText"),
        }
    }
}
