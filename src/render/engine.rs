use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, BarChart, Block, Chart, Dataset, GraphType};

use crate::chart::{ChartData, ChartSpec, ChartType};
use crate::elements::{SegmentStyle, StyledText};
use crate::render::ops::*;
use crate::theme::Theme;
use crate::theme::types::resolve_color_with_palette;

#[derive(Debug, Clone)]
pub struct WindowRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl WindowRect {
    pub fn from_rect(rect: Rect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

pub struct RenderEngine<'a> {
    rects: Vec<WindowRect>,
    cursor_row: u16,
    fg: Option<Color>,
    bg: Option<Color>,
    default_fg: Option<Color>,
    default_bg: Option<Color>,
    theme: Option<&'a Theme>,
}

impl<'a> RenderEngine<'a> {
    pub fn new(area: Rect) -> Self {
        Self {
            rects: vec![WindowRect::from_rect(area)],
            cursor_row: 0,
            fg: None,
            bg: None,
            default_fg: None,
            default_bg: None,
            theme: None,
        }
    }

    pub fn set_theme(&mut self, theme: &'a Theme) {
        self.theme = Some(theme);
    }

    pub fn set_default_colors(&mut self, fg: Option<Color>, bg: Option<Color>) {
        self.default_fg = fg;
        self.default_bg = bg;
        self.fg = fg;
        self.bg = bg;
    }

    pub fn render(&mut self, ops: &[RenderOp], frame: &mut Frame) {
        for op in ops {
            self.render_one(op, frame);
        }
    }

    fn current_rect(&self) -> &WindowRect {
        self.rects.last().expect("rect stack should never be empty")
    }

    fn render_one(&mut self, op: &RenderOp, frame: &mut Frame) {
        match op {
            RenderOp::ClearRect => {
                self.cursor_row = 0;
            }
            RenderOp::JumpToRow { row } => {
                self.cursor_row = *row;
            }
            RenderOp::RenderText { line, alignment } => {
                self.render_text(line, alignment, frame);
            }
            RenderOp::Spacer { lines } => {
                self.cursor_row += lines;
            }
            RenderOp::PushWindowRect { margin } => {
                let current = self.current_rect().clone();
                let new_rect = WindowRect {
                    x: current.x + margin.left,
                    y: current.y + margin.top,
                    width: current
                        .width
                        .saturating_sub(margin.left + margin.right),
                    height: current
                        .height
                        .saturating_sub(margin.top + margin.bottom),
                };
                self.rects.push(new_rect);
            }
            RenderOp::PopWindowRect => {
                if self.rects.len() > 1 {
                    self.rects.pop();
                }
            }
            RenderOp::SetColors { fg, bg } => {
                // None means "reset to default", not "no color"
                self.fg = fg.or(self.default_fg);
                self.bg = bg.or(self.default_bg);
            }
            RenderOp::RenderChart { spec, data, height } => {
                self.render_chart(spec, data, *height, frame);
            }
            RenderOp::RenderImage { lines, width } => {
                self.render_image(lines, *width, frame);
            }
        }
    }

    fn render_text(&mut self, text: &StyledText, alignment: &Alignment, frame: &mut Frame) {
        let rect = self.current_rect();
        let abs_y = rect.y + self.cursor_row;

        // Don't render outside the window
        if abs_y >= rect.y + rect.height {
            self.cursor_row += 1;
            return;
        }

        let spans: Vec<Span> = text
            .segments
            .iter()
            .map(|seg| {
                let style = self.segment_to_style(&seg.style);
                Span::styled(seg.text.clone(), style)
            })
            .collect();

        let line = Line::from(spans);
        let text_width = line.width() as u16;

        let x_offset = match alignment {
            Alignment::Left => 0,
            Alignment::Center => rect.width.saturating_sub(text_width) / 2,
            Alignment::Right => rect.width.saturating_sub(text_width),
        };

        let area = Rect {
            x: rect.x + x_offset,
            y: abs_y,
            width: rect.width.saturating_sub(x_offset),
            height: 1,
        };

        // If we have a background color, fill the full width first
        if self.bg.is_some() {
            let bg_style = Style::default().bg(self.bg.unwrap());
            let fill = " ".repeat(rect.width as usize);
            let fill_area = Rect {
                x: rect.x,
                y: abs_y,
                width: rect.width,
                height: 1,
            };
            frame.render_widget(
                ratatui::widgets::Paragraph::new(Line::from(fill)).style(bg_style),
                fill_area,
            );
        }

        frame.render_widget(ratatui::widgets::Paragraph::new(line), area);

        self.cursor_row += 1;
    }

    fn render_chart(
        &mut self,
        spec: &ChartSpec,
        data: &ChartData,
        height: u16,
        frame: &mut Frame,
    ) {
        let rect = self.current_rect();
        let abs_y = rect.y + self.cursor_row;
        let available_height = rect.height.saturating_sub(self.cursor_row);
        let chart_height = height.min(available_height);

        if chart_height < 3 {
            self.cursor_row += chart_height;
            return;
        }

        let chart_area = Rect {
            x: rect.x,
            y: abs_y,
            width: rect.width,
            height: chart_height,
        };

        let color = spec
            .color
            .as_deref()
            .and_then(|c| resolve_color_with_palette(c, &std::collections::HashMap::new()))
            .unwrap_or(Color::Cyan);

        let chart_style = Style::default();
        let chart_style = if let Some(bg) = self.bg {
            chart_style.bg(bg)
        } else {
            chart_style
        };

        match data {
            ChartData::Bar(bar_data) => {
                let data_items: Vec<(&str, u64)> = bar_data
                    .labels
                    .iter()
                    .zip(&bar_data.values)
                    .map(|(l, v)| (l.as_str(), *v as u64))
                    .collect();

                let (bar_width, bar_gap) = if !data_items.is_empty() {
                    let n = data_items.len() as u16;
                    let available = chart_area.width.saturating_sub(2);
                    // Each bar takes bar_width + gap columns. Solve: n * (bw + gap) <= available
                    // Try gap=1 first, increase bar_width to fill space
                    let gap = 1u16;
                    let bw = (available / n).saturating_sub(gap).max(1);
                    (bw, gap)
                } else {
                    (5, 1)
                };

                let widget = BarChart::default()
                    .data(&data_items)
                    .bar_width(bar_width)
                    .bar_gap(bar_gap)
                    .bar_style(Style::default().fg(color))
                    .value_style(Style::default().fg(Color::White))
                    .label_style(Style::default().fg(Color::Gray))
                    .style(chart_style);

                frame.render_widget(widget, chart_area);
            }
            ChartData::Line(line_data) => {
                if line_data.points.is_empty() {
                    self.cursor_row += chart_height;
                    return;
                }

                let x_min = line_data
                    .points
                    .iter()
                    .map(|(x, _)| *x)
                    .fold(f64::INFINITY, f64::min);
                let x_max = line_data
                    .points
                    .iter()
                    .map(|(x, _)| *x)
                    .fold(f64::NEG_INFINITY, f64::max);
                let y_min = line_data
                    .points
                    .iter()
                    .map(|(_, y)| *y)
                    .fold(f64::INFINITY, f64::min);
                let y_max = line_data
                    .points
                    .iter()
                    .map(|(_, y)| *y)
                    .fold(f64::NEG_INFINITY, f64::max);

                // Add a little padding to y range
                let y_pad = (y_max - y_min) * 0.1;
                let y_min = y_min - y_pad;
                let y_max = y_max + y_pad;

                let x_labels = vec![
                    Span::raw(format!("{:.0}", x_min)),
                    Span::raw(format!("{:.0}", x_max)),
                ];
                let y_labels = vec![
                    Span::raw(format!("{:.1}", y_min)),
                    Span::raw(format!("{:.1}", y_max)),
                ];

                let dataset = Dataset::default()
                    .data(&line_data.points)
                    .graph_type(GraphType::Line)
                    .marker(symbols::Marker::Braille)
                    .style(Style::default().fg(color));

                let x_axis = Axis::default()
                    .bounds([x_min, x_max])
                    .labels(x_labels)
                    .style(Style::default().fg(Color::Gray));

                let y_axis = Axis::default()
                    .bounds([y_min, y_max])
                    .labels(y_labels)
                    .style(Style::default().fg(Color::Gray));

                let mut x_axis = x_axis;
                let mut y_axis = y_axis;
                if let Some(ref label) = spec.x_label {
                    x_axis = x_axis.title(Span::raw(label.clone()));
                }
                if let Some(ref label) = spec.y_label {
                    y_axis = y_axis.title(Span::raw(label.clone()));
                }

                let widget = Chart::new(vec![dataset])
                    .x_axis(x_axis)
                    .y_axis(y_axis)
                    .style(chart_style);

                frame.render_widget(widget, chart_area);
            }
        }

        self.cursor_row += chart_height;
    }

    fn render_image(
        &mut self,
        lines: &[ratatui::text::Line<'static>],
        img_width: u16,
        frame: &mut Frame,
    ) {
        let rect = self.current_rect().clone();

        for line in lines {
            let abs_y = rect.y + self.cursor_row;
            if abs_y >= rect.y + rect.height {
                break;
            }

            // Center the image horizontally
            let x_offset = rect.width.saturating_sub(img_width) / 2;

            let area = Rect {
                x: rect.x + x_offset,
                y: abs_y,
                width: img_width.min(rect.width),
                height: 1,
            };

            frame.render_widget(
                ratatui::widgets::Paragraph::new(line.clone()),
                area,
            );

            self.cursor_row += 1;
        }
    }

    fn segment_to_style(&self, seg_style: &SegmentStyle) -> Style {
        let mut style = Style::default();

        if let Some(fg) = self.fg {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg {
            style = style.bg(bg);
        }

        if seg_style.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if seg_style.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if seg_style.code {
            style = style.fg(Color::Cyan);
        }

        // Apply semantic highlight from theme
        if let Some(ref highlight_class) = seg_style.highlight {
            if let Some(theme) = self.theme {
                if let Some(hl) = theme.highlights.get(highlight_class) {
                    if let Some(fg) = hl.fg.as_deref().and_then(|c| theme.resolve_color(c)) {
                        style = style.fg(fg);
                    }
                    if let Some(bg) = hl.bg.as_deref().and_then(|c| theme.resolve_color(c)) {
                        style = style.bg(bg);
                    }
                    if hl.bold == Some(true) {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    if hl.italic == Some(true) {
                        style = style.add_modifier(Modifier::ITALIC);
                    }
                }
            }
        }

        style
    }
}
