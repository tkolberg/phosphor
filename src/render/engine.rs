use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Bar, BarChart, Block, Chart, Dataset, GraphType};

use crate::chart::{ChartData, ChartSpec};
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
    scroll_offset: u16,
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
            scroll_offset: 0,
            fg: None,
            bg: None,
            default_fg: None,
            default_bg: None,
            theme: None,
        }
    }

    pub fn set_scroll_offset(&mut self, offset: u16) {
        self.scroll_offset = offset;
    }

    pub fn set_theme(&mut self, theme: &'a Theme) {
        self.theme = Some(theme);
    }

    pub fn cursor_row(&self) -> u16 {
        self.cursor_row
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

    pub fn measure_height(ops: &[RenderOp]) -> u16 {
        let mut row: u16 = 0;
        for op in ops {
            match op {
                RenderOp::ClearRect => row = 0,
                RenderOp::JumpToRow { row: r } => row = *r,
                RenderOp::RenderText { .. } => row += 1,
                RenderOp::Spacer { lines } => row += lines,
                RenderOp::RenderChart { height, .. } => row += height,
                RenderOp::RenderImage { lines, .. } => row += lines.len() as u16,
                RenderOp::RenderPhotoBackground { .. } => {}
                RenderOp::PushWindowRect { .. }
                | RenderOp::PopWindowRect
                | RenderOp::SetColors { .. } => {}
            }
        }
        row
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
            RenderOp::RenderPhotoBackground { lines } => {
                self.render_photo_background(lines, frame);
            }
        }
    }

    fn screen_y(&self, rect: &WindowRect) -> Option<u16> {
        if self.cursor_row < self.scroll_offset {
            return None; // above viewport
        }
        let vis_row = self.cursor_row - self.scroll_offset;
        if vis_row >= rect.height {
            return None; // below viewport
        }
        Some(rect.y + vis_row)
    }

    fn render_text(&mut self, text: &StyledText, alignment: &Alignment, frame: &mut Frame) {
        let rect = self.current_rect().clone();

        let screen_y = self.screen_y(&rect);
        self.cursor_row += 1;

        let screen_y = match screen_y {
            Some(y) => y,
            None => return,
        };

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
            y: screen_y,
            width: rect.width.saturating_sub(x_offset),
            height: 1,
        };

        if self.bg.is_some() {
            let bg_style = Style::default().bg(self.bg.unwrap());
            let fill = " ".repeat(rect.width as usize);
            let fill_area = Rect {
                x: rect.x,
                y: screen_y,
                width: rect.width,
                height: 1,
            };
            frame.render_widget(
                ratatui::widgets::Paragraph::new(Line::from(fill)).style(bg_style),
                fill_area,
            );
        }

        frame.render_widget(ratatui::widgets::Paragraph::new(line), area);
    }

    fn resolve_chart_color(name: &str) -> Color {
        resolve_color_with_palette(name, &std::collections::HashMap::new())
            .unwrap_or(Color::Cyan)
    }

    fn render_chart(
        &mut self,
        spec: &ChartSpec,
        data: &ChartData,
        height: u16,
        frame: &mut Frame,
    ) {
        let rect = self.current_rect().clone();
        let vis_start = self.cursor_row.saturating_sub(self.scroll_offset);
        let available_height = rect.height.saturating_sub(vis_start);
        let chart_height = height.min(available_height);

        if chart_height < 3 || self.cursor_row + height <= self.scroll_offset {
            self.cursor_row += height;
            return;
        }

        let screen_y = rect.y + self.cursor_row.saturating_sub(self.scroll_offset);

        let chart_area = Rect {
            x: rect.x,
            y: screen_y,
            width: rect.width,
            height: chart_height,
        };

        let chart_style = Style::default();
        let chart_style = if let Some(bg) = self.bg {
            chart_style.bg(bg)
        } else {
            chart_style
        };

        match data {
            ChartData::Bar(bar_data) => {
                let color = spec
                    .color
                    .as_deref()
                    .map(Self::resolve_chart_color)
                    .unwrap_or(Color::Cyan);

                let max_val = bar_data
                    .values
                    .iter()
                    .copied()
                    .fold(0.0_f64, f64::max);
                let has_fractional = bar_data.values.iter().any(|v| v.fract() != 0.0);
                let scale = if has_fractional && max_val > 0.0 {
                    1000.0 / max_val
                } else {
                    1.0
                };

                let bars: Vec<Bar> = bar_data
                    .labels
                    .iter()
                    .zip(&bar_data.values)
                    .map(|(l, v)| {
                        let display = if has_fractional {
                            format!("{:.2}", v)
                        } else {
                            format!("{}", *v as u64)
                        };
                        Bar::default()
                            .label(l.as_str().into())
                            .value((v * scale) as u64)
                            .text_value(display)
                            .style(Style::default().fg(color))
                            .value_style(Style::default().fg(Color::White))
                    })
                    .collect();

                let n = bars.len() as u16;
                let (bar_width, bar_gap) = if n > 0 {
                    let available = chart_area.width.saturating_sub(2);
                    let gap = 1u16;
                    let bw = (available / n).saturating_sub(gap).max(1);
                    (bw, gap)
                } else {
                    (5, 1)
                };

                let widget = BarChart::default()
                    .data(ratatui::widgets::BarGroup::default().bars(&bars))
                    .bar_width(bar_width)
                    .bar_gap(bar_gap)
                    .label_style(Style::default().fg(Color::Gray))
                    .style(chart_style);

                frame.render_widget(widget, chart_area);
            }
            ChartData::MultiSeries(multi) => {
                if multi.series.is_empty() || multi.series.iter().all(|s| s.points.is_empty()) {
                    self.cursor_row += chart_height;
                    return;
                }

                let graph_type = match spec.chart_type {
                    crate::chart::ChartType::Scatter => GraphType::Scatter,
                    _ => GraphType::Line,
                };

                let (x_min, x_max, y_min, y_max) = self.compute_bounds(&multi.series);
                let x_min = spec.x_min.unwrap_or(x_min);
                let x_max = spec.x_max.unwrap_or(x_max);

                let y_pad = (y_max - y_min) * 0.1;
                let y_min = y_min - y_pad;
                let y_max = y_max + y_pad;

                let datasets: Vec<Dataset> = multi
                    .series
                    .iter()
                    .map(|s| {
                        let color = s
                            .color
                            .as_deref()
                            .map(Self::resolve_chart_color)
                            .unwrap_or(Color::Cyan);
                        let mut ds = Dataset::default()
                            .data(&s.points)
                            .graph_type(graph_type)
                            .marker(symbols::Marker::Braille)
                            .style(Style::default().fg(color));
                        ds = ds.name(s.name.clone());
                        ds
                    })
                    .collect();

                let x_axis = self.make_x_axis(x_min, x_max, spec);
                let y_axis = self.make_y_axis(y_min, y_max, spec);

                let mut widget = Chart::new(datasets)
                    .x_axis(x_axis)
                    .y_axis(y_axis)
                    .style(chart_style);

                if multi.series.len() > 1 {
                    widget = widget
                        .hidden_legend_constraints((
                            ratatui::layout::Constraint::Ratio(1, 2),
                            ratatui::layout::Constraint::Ratio(1, 2),
                        ))
                        .legend_position(Some(
                            ratatui::widgets::LegendPosition::TopRight,
                        ));
                }

                frame.render_widget(widget, chart_area);
            }
            ChartData::Histogram(histo) => {
                if histo.series.is_empty() {
                    self.cursor_row += chart_height;
                    return;
                }

                // Convert histogram bins to stepped line datasets
                let stepped: Vec<crate::chart::Series> = histo
                    .series
                    .iter()
                    .map(|hs| {
                        let mut points = Vec::new();
                        for (i, &count) in hs.counts.iter().enumerate() {
                            let lo = hs.bin_edges[i];
                            let hi = hs.bin_edges[i + 1];
                            points.push((lo, count));
                            points.push((hi, count));
                        }
                        crate::chart::Series {
                            name: hs.name.clone(),
                            color: hs.color.clone(),
                            points,
                        }
                    })
                    .collect();

                let (x_min, x_max, _, y_max_raw) = self.compute_bounds(&stepped);
                let x_min = spec.x_min.unwrap_or(x_min);
                let x_max = spec.x_max.unwrap_or(x_max);
                let y_min = 0.0;
                let y_max = y_max_raw * 1.1;

                let datasets: Vec<Dataset> = stepped
                    .iter()
                    .map(|s| {
                        let color = s
                            .color
                            .as_deref()
                            .map(Self::resolve_chart_color)
                            .unwrap_or(Color::Cyan);
                        let mut ds = Dataset::default()
                            .data(&s.points)
                            .graph_type(GraphType::Line)
                            .marker(symbols::Marker::Braille)
                            .style(Style::default().fg(color));
                        ds = ds.name(s.name.clone());
                        ds
                    })
                    .collect();

                let x_axis = self.make_x_axis(x_min, x_max, spec);
                let y_axis = self.make_y_axis(y_min, y_max, spec);

                let mut widget = Chart::new(datasets)
                    .x_axis(x_axis)
                    .y_axis(y_axis)
                    .style(chart_style);

                if histo.series.len() > 1 {
                    widget = widget
                        .hidden_legend_constraints((
                            ratatui::layout::Constraint::Ratio(1, 2),
                            ratatui::layout::Constraint::Ratio(1, 2),
                        ))
                        .legend_position(Some(
                        ratatui::widgets::LegendPosition::TopRight,
                    ));
                }

                frame.render_widget(widget, chart_area);
            }
        }

        self.cursor_row += height;
    }

    fn compute_bounds(&self, series: &[crate::chart::Series]) -> (f64, f64, f64, f64) {
        let all_points = series.iter().flat_map(|s| s.points.iter());
        let mut x_min = f64::INFINITY;
        let mut x_max = f64::NEG_INFINITY;
        let mut y_min = f64::INFINITY;
        let mut y_max = f64::NEG_INFINITY;
        for &(x, y) in all_points {
            x_min = x_min.min(x);
            x_max = x_max.max(x);
            y_min = y_min.min(y);
            y_max = y_max.max(y);
        }
        (x_min, x_max, y_min, y_max)
    }

    fn make_x_axis(&self, x_min: f64, x_max: f64, spec: &ChartSpec) -> Axis<'static> {
        let labels = vec![
            Span::raw(format_axis_label(x_min)),
            Span::raw(format_axis_label(x_max)),
        ];
        let mut axis = Axis::default()
            .bounds([x_min, x_max])
            .labels(labels)
            .style(Style::default().fg(Color::Gray));
        if let Some(ref label) = spec.x_label {
            axis = axis.title(Span::raw(label.clone()));
        }
        axis
    }

    fn make_y_axis(&self, y_min: f64, y_max: f64, spec: &ChartSpec) -> Axis<'static> {
        let labels = vec![
            Span::raw(format_axis_label(y_min)),
            Span::raw(format_axis_label(y_max)),
        ];
        let mut axis = Axis::default()
            .bounds([y_min, y_max])
            .labels(labels)
            .style(Style::default().fg(Color::Gray));
        if let Some(ref label) = spec.y_label {
            axis = axis.title(Span::raw(label.clone()));
        }
        axis
    }

    fn render_image(
        &mut self,
        lines: &[ratatui::text::Line<'static>],
        img_width: u16,
        frame: &mut Frame,
    ) {
        let rect = self.current_rect().clone();

        for line in lines {
            if let Some(screen_y) = self.screen_y(&rect) {
                let x_offset = rect.width.saturating_sub(img_width) / 2;

                let area = Rect {
                    x: rect.x + x_offset,
                    y: screen_y,
                    width: img_width.min(rect.width),
                    height: 1,
                };

                frame.render_widget(
                    ratatui::widgets::Paragraph::new(line.clone()),
                    area,
                );
            }

            self.cursor_row += 1;
        }
    }

    fn render_photo_background(
        &mut self,
        lines: &[ratatui::text::Line<'static>],
        frame: &mut Frame,
    ) {
        let rect = self.current_rect().clone();
        for (i, line) in lines.iter().enumerate() {
            let y = rect.y + i as u16;
            if y >= rect.y + rect.height {
                break;
            }
            let area = Rect {
                x: rect.x,
                y,
                width: rect.width,
                height: 1,
            };
            frame.render_widget(
                ratatui::widgets::Paragraph::new(line.clone()),
                area,
            );
        }
        // Don't advance cursor — text will overlay on top
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

fn format_axis_label(val: f64) -> String {
    let abs = val.abs();
    if abs == 0.0 {
        "0".into()
    } else if abs >= 1000.0 || abs < 0.01 {
        format!("{:.1e}", val)
    } else if abs >= 1.0 {
        format!("{:.1}", val)
    } else {
        format!("{:.3}", val)
    }
}
