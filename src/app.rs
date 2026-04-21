use std::io::Stdout;
use std::time::Duration;

use color_eyre::eyre::Result;
use crossterm::event::{self, Event};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::elements::SlideElement;
use crate::input::{self, Action};
use crate::notes::protocol::NoteMessage;
use crate::notes::server::NotesServer;
use crate::render::engine::RenderEngine;
use crate::render::lower::{Lower, LowerContext};
use crate::render::ops::RenderOp;
use crate::slide::Presentation;
use crate::theme::Theme;
use crate::transition::{Cell, Transition, TransitionDirection};

/// Tick interval for animation frames (~8ms ≈ 120fps).
const TICK_RATE: Duration = Duration::from_millis(8);

pub struct App {
    presentation: Presentation,
    theme: Theme,
    current_slide: usize,
    visible_chunks: usize,
    scroll_offset: u16,
    should_quit: bool,
    notes_server: Option<NotesServer>,
    transition: Option<Transition>,
    /// Whether we're running inside Ghostty (can change font size).
    in_ghostty: bool,
    /// Current font size state: true = small (wireframe), false = normal.
    small_font: bool,
    /// The normal font size to restore when leaving wireframe slides.
    normal_font_size: u16,
    /// The small font size for wireframe slides.
    wireframe_font_size: u16,
    /// Ghostty window ID for the presenter (captured at startup).
    ghostty_window_id: Option<String>,
}

impl App {
    pub fn new(presentation: Presentation, theme: Theme) -> Self {
        let in_ghostty = std::env::var("PHOSPHOR_IN_GHOSTTY").is_ok();
        Self {
            presentation,
            theme,
            current_slide: 0,
            visible_chunks: 1,
            scroll_offset: 0,
            should_quit: false,
            notes_server: None,
            transition: None,
            in_ghostty,
            small_font: false,
            normal_font_size: 24,
            wireframe_font_size: 8,
            ghostty_window_id: None,
        }
    }

    pub fn set_notes_server(&mut self, server: NotesServer) {
        self.notes_server = Some(server);
    }

    pub fn set_ghostty_window_id(&mut self, id: String) {
        self.ghostty_window_id = Some(id);
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        // Set font size for the initial slide
        self.sync_font_size();

        while !self.should_quit {
            // Accept any pending notes viewer connections
            if let Some(ref mut server) = self.notes_server {
                server.accept_pending();
            }

            // Tick the transition animation if active
            if let Some(ref mut t) = self.transition {
                t.tick();
                if t.is_done() {
                    self.transition = None;
                }
            }

            terminal.draw(|frame| self.draw(frame))?;

            // Poll with timeout so animation frames keep firing
            if event::poll(TICK_RATE)? {
                match event::read()? {
                    Event::Key(key) => {
                        if let Some(action) = input::map_key(key) {
                            self.handle_action(action, terminal);
                        }
                    }
                    Event::Resize(_, _) => {
                        // Redraw on next loop iteration
                    }
                    _ => {}
                }
            }
        }
        self.restore_font_size();
        Ok(())
    }

    fn draw(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        // Fill background from theme
        let bg_color = self
            .theme
            .slide
            .as_ref()
            .and_then(|s| s.bg.as_deref())
            .and_then(|c| self.theme.resolve_color(c));
        let fg_color = self
            .theme
            .slide
            .as_ref()
            .and_then(|s| s.fg.as_deref())
            .and_then(|c| self.theme.resolve_color(c));

        let mut bg_style = Style::default();
        if let Some(bg) = bg_color {
            bg_style = bg_style.bg(bg);
        }
        if let Some(fg) = fg_color {
            bg_style = bg_style.fg(fg);
        }
        let block = ratatui::widgets::Block::default().style(bg_style);
        frame.render_widget(block, area);

        if let Some(ref transition) = self.transition {
            // Render the scramble transition effect
            self.draw_transition(frame, area, transition);
        } else {
            // Normal render
            self.draw_slide_content(frame, area, fg_color, bg_color);
        }

        // Footer is always rendered normally (not scrambled)
        self.draw_footer(frame, area, bg_color);
    }

    fn draw_slide_content(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        fg_color: Option<Color>,
        bg_color: Option<Color>,
    ) {
        let content_area = self.content_area(area);

        if let Some(slide) = self.presentation.slides.get(self.current_slide) {
            // Check if this is a photo slide
            let has_photo = slide.chunks.iter().any(|c| {
                c.elements.iter().any(|e| matches!(e, SlideElement::Photo { .. }))
            });

            if has_photo {
                self.draw_photo_slide(frame, area, content_area, slide, fg_color);
                return;
            }

            let ctx = LowerContext {
                window_width: content_area.width,
                window_height: content_area.height,
                theme: &self.theme,
                visible_chunks: self.visible_chunks,
            };

            // Lower elements individually to find the split between fixed and scrollable content.
            // Fixed = everything up through the last visual element (heading, chart, diagram, wireframe, image).
            // Scrollable = everything after that.
            let mut element_ops: Vec<(bool, Vec<RenderOp>)> = Vec::new();
            for chunk in slide.chunks.iter().take(self.visible_chunks) {
                for element in &chunk.elements {
                    let is_visual = matches!(
                        element,
                        SlideElement::Heading { .. }
                            | SlideElement::Chart { .. }
                            | SlideElement::Diagram { .. }
                            | SlideElement::Wireframe { .. }
                            | SlideElement::Image { .. }
                            | SlideElement::Histogram { .. }
                    );
                    element_ops.push((is_visual, element.lower(&ctx)));
                }
            }

            let last_visual_idx = element_ops.iter().rposition(|(vis, _)| *vis);

            if self.scroll_offset == 0 || last_visual_idx.is_none() {
                // No scroll or no visual elements — render everything in one pass
                let all_ops: Vec<RenderOp> =
                    element_ops.into_iter().flat_map(|(_, ops)| ops).collect();

                let render_area = if slide.center {
                    let content_height = RenderEngine::measure_height(&all_ops);
                    let y_offset = content_area.height.saturating_sub(content_height) / 2;
                    Rect {
                        x: content_area.x,
                        y: content_area.y + y_offset,
                        width: content_area.width,
                        height: content_area.height.saturating_sub(y_offset),
                    }
                } else {
                    content_area
                };

                let mut engine = RenderEngine::new(render_area);
                engine.set_theme(&self.theme);
                engine.set_default_colors(fg_color, bg_color);
                engine.set_scroll_offset(self.scroll_offset);
                engine.render(&all_ops, frame);
            } else {
                let split = last_visual_idx.unwrap() + 1;
                let fixed_ops: Vec<RenderOp> = element_ops[..split]
                    .iter()
                    .flat_map(|(_, ops)| ops.clone())
                    .collect();
                let scroll_ops: Vec<RenderOp> = element_ops[split..]
                    .iter()
                    .flat_map(|(_, ops)| ops.clone())
                    .collect();

                // Render fixed header without scroll
                let mut engine = RenderEngine::new(content_area);
                engine.set_theme(&self.theme);
                engine.set_default_colors(fg_color, bg_color);
                engine.render(&fixed_ops, frame);
                let fixed_height = engine.cursor_row();

                // Render scrollable body in remaining space
                let scroll_area = Rect {
                    x: content_area.x,
                    y: content_area.y + fixed_height,
                    width: content_area.width,
                    height: content_area.height.saturating_sub(fixed_height),
                };
                let mut scroll_engine = RenderEngine::new(scroll_area);
                scroll_engine.set_theme(&self.theme);
                scroll_engine.set_default_colors(fg_color, bg_color);
                scroll_engine.set_scroll_offset(self.scroll_offset);
                scroll_engine.render(&scroll_ops, frame);
            }
        }
    }

    fn draw_photo_slide(
        &self,
        frame: &mut ratatui::Frame,
        full_area: Rect,
        content_area: Rect,
        slide: &crate::slide::Slide,
        fg_color: Option<Color>,
    ) {
        // Render photo background to the full frame area (no margins)
        let photo_ctx = LowerContext {
            window_width: full_area.width,
            window_height: full_area.height,
            theme: &self.theme,
            visible_chunks: self.visible_chunks,
        };

        // Lower only the photo element to get the background
        for chunk in slide.chunks.iter().take(self.visible_chunks) {
            for element in &chunk.elements {
                if matches!(element, SlideElement::Photo { .. }) {
                    let ops = element.lower(&photo_ctx);
                    let mut engine = RenderEngine::new(full_area);
                    engine.set_theme(&self.theme);
                    engine.render(&ops, frame);
                }
            }
        }

        // Lower all non-photo elements for text overlay
        let text_ctx = LowerContext {
            window_width: content_area.width,
            window_height: content_area.height,
            theme: &self.theme,
            visible_chunks: self.visible_chunks,
        };

        let mut text_ops: Vec<RenderOp> = Vec::new();
        for chunk in slide.chunks.iter().take(self.visible_chunks) {
            for element in &chunk.elements {
                if !matches!(element, SlideElement::Photo { .. }) {
                    text_ops.extend(element.lower(&text_ctx));
                }
            }
        }

        if text_ops.is_empty() {
            return;
        }

        // Measure text height and position the band
        let text_height = RenderEngine::measure_height(&text_ops);
        let padding = 1u16;
        let band_height = text_height + padding * 2;
        let band_y = if slide.center {
            // Center the text band vertically
            content_area.y + content_area.height.saturating_sub(band_height) / 2
        } else {
            // Pin text band to the bottom
            content_area.y + content_area.height.saturating_sub(band_height)
        };

        // Draw a dark semi-transparent band behind the text
        let band_color = Color::Rgb(0, 0, 0);
        for row in 0..band_height {
            let y = band_y + row;
            if y < full_area.y + full_area.height {
                let area = Rect {
                    x: full_area.x,
                    y,
                    width: full_area.width,
                    height: 1,
                };
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(
                        ratatui::text::Line::from(
                            ratatui::text::Span::styled(
                                " ".repeat(full_area.width as usize),
                                ratatui::style::Style::default().bg(band_color),
                            )
                        )
                    ),
                    area,
                );
            }
        }

        // Render text on top of the band
        let text_area = Rect {
            x: content_area.x,
            y: band_y + padding,
            width: content_area.width,
            height: text_height,
        };
        let mut engine = RenderEngine::new(text_area);
        engine.set_theme(&self.theme);
        engine.set_default_colors(fg_color, Some(band_color));
        engine.render(&text_ops, frame);
    }

    /// Returns (fixed_height, scrollable_height, available_for_scroll) for overflow calculation.
    fn measure_content(&self, area: Rect) -> (u16, u16, u16) {
        let content_area = self.content_area(area);
        if let Some(slide) = self.presentation.slides.get(self.current_slide) {
            let ctx = LowerContext {
                window_width: content_area.width,
                window_height: content_area.height,
                theme: &self.theme,
                visible_chunks: self.visible_chunks,
            };

            let mut element_ops: Vec<(bool, Vec<RenderOp>)> = Vec::new();
            for chunk in slide.chunks.iter().take(self.visible_chunks) {
                for element in &chunk.elements {
                    let is_visual = matches!(
                        element,
                        SlideElement::Heading { .. }
                            | SlideElement::Chart { .. }
                            | SlideElement::Diagram { .. }
                            | SlideElement::Wireframe { .. }
                            | SlideElement::Image { .. }
                            | SlideElement::Histogram { .. }
                    );
                    element_ops.push((is_visual, element.lower(&ctx)));
                }
            }

            let last_visual_idx = element_ops.iter().rposition(|(vis, _)| *vis);

            if let Some(split) = last_visual_idx {
                let split = split + 1;
                let fixed_ops: Vec<RenderOp> = element_ops[..split]
                    .iter()
                    .flat_map(|(_, ops)| ops.clone())
                    .collect();
                let scroll_ops: Vec<RenderOp> = element_ops[split..]
                    .iter()
                    .flat_map(|(_, ops)| ops.clone())
                    .collect();
                let fixed_h = RenderEngine::measure_height(&fixed_ops);
                let scroll_h = RenderEngine::measure_height(&scroll_ops);
                let available = content_area.height.saturating_sub(fixed_h);
                (fixed_h, scroll_h, available)
            } else {
                let all_ops: Vec<RenderOp> =
                    element_ops.into_iter().flat_map(|(_, ops)| ops).collect();
                let total = RenderEngine::measure_height(&all_ops);
                (0, total, content_area.height)
            }
        } else {
            (0, 0, content_area.height)
        }
    }

    fn max_scroll_offset(&self, area: Rect) -> u16 {
        let (_fixed_h, scroll_h, available) = self.measure_content(area);
        scroll_h.saturating_sub(available)
    }

    fn draw_transition(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        transition: &Transition,
    ) {
        let buf = frame.buffer_mut();
        let tw = transition.width().min(area.width as usize);
        let th = transition.height().min(area.height as usize);

        for y in 0..th {
            for x in 0..tw {
                let cell = transition.get_cell(x, y);
                let bx = area.x + x as u16;
                let by = area.y + y as u16;

                if bx < area.x + area.width && by < area.y + area.height {
                    let buf_cell = &mut buf[(bx, by)];
                    buf_cell.set_char(cell.ch);
                    buf_cell.set_style(Style::default());
                    if let Some(fg) = cell.fg {
                        buf_cell.set_fg(fg);
                    }
                    if let Some(bg) = cell.bg {
                        buf_cell.set_bg(bg);
                    }
                    if !cell.modifier.is_empty() {
                        buf_cell.set_style(Style::default().add_modifier(cell.modifier));
                        if let Some(fg) = cell.fg {
                            buf_cell.set_fg(fg);
                        }
                        if let Some(bg) = cell.bg {
                            buf_cell.set_bg(bg);
                        }
                    }
                }
            }
        }
    }

    fn content_area(&self, area: Rect) -> Rect {
        let (ml, mr, mt, mb) = self
            .theme
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

        Rect {
            x: area.x + ml,
            y: area.y + mt,
            width: area.width.saturating_sub(ml + mr),
            height: area.height.saturating_sub(mt + mb),
        }
    }

    fn draw_footer(&self, frame: &mut ratatui::Frame, area: Rect, slide_bg: Option<Color>) {
        let footer_y = area.y + area.height.saturating_sub(1);
        let total = self.presentation.slides.len();
        let current = self.current_slide + 1;

        let title = self
            .presentation
            .metadata
            .title
            .as_deref()
            .unwrap_or("phosphor");

        let left = format!(" {title}");
        let right = format!("{current}/{total} ");

        let padding = (area.width as usize)
            .saturating_sub(left.len() + right.len());

        let footer_fg = self
            .theme
            .footer
            .as_ref()
            .and_then(|f| f.fg.as_deref())
            .and_then(|c| self.theme.resolve_color(c))
            .unwrap_or(Color::DarkGray);

        let mut footer_style = Style::default().fg(footer_fg);
        if let Some(bg) = slide_bg {
            footer_style = footer_style.bg(bg);
        }

        let footer_line = Line::from(vec![
            Span::styled(left, footer_style),
            Span::styled(" ".repeat(padding), footer_style),
            Span::styled(right, footer_style),
        ]);

        let footer_area = Rect {
            x: area.x,
            y: footer_y,
            width: area.width,
            height: 1,
        };

        frame.render_widget(ratatui::widgets::Paragraph::new(footer_line), footer_area);
    }

    fn current_slide_chunk_count(&self) -> usize {
        self.presentation
            .slides
            .get(self.current_slide)
            .map(|s| s.chunks.len())
            .unwrap_or(1)
    }

    /// Capture the current frame into a grid of Cells for the transition.
    fn capture_frame(&self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Vec<Vec<Cell>> {
        let area = terminal.get_frame().area();
        let width = area.width as usize;
        let height = area.height as usize;

        // Render into a temporary hidden frame to capture the buffer
        let mut grid = vec![vec![Cell::default(); width]; height];

        // We need to render the slide to get the actual buffer content.
        // Do a draw pass and read back from the buffer.
        let _ = terminal.draw(|frame| {
            // Render full slide normally (no transition)
            let fa = frame.area();

            let bg_color = self
                .theme
                .slide
                .as_ref()
                .and_then(|s| s.bg.as_deref())
                .and_then(|c| self.theme.resolve_color(c));
            let fg_color = self
                .theme
                .slide
                .as_ref()
                .and_then(|s| s.fg.as_deref())
                .and_then(|c| self.theme.resolve_color(c));

            let mut bg_style = Style::default();
            if let Some(bg) = bg_color {
                bg_style = bg_style.bg(bg);
            }
            if let Some(fg) = fg_color {
                bg_style = bg_style.fg(fg);
            }
            let block = ratatui::widgets::Block::default().style(bg_style);
            frame.render_widget(block, fa);

            self.draw_slide_content(frame, fa, fg_color, bg_color);
            self.draw_footer(frame, fa, bg_color);

            // Read back from the buffer
            let buf = frame.buffer_mut();
            for y in 0..height.min(fa.height as usize) {
                for x in 0..width.min(fa.width as usize) {
                    let bc = &buf[(fa.x + x as u16, fa.y + y as u16)];
                    grid[y][x] = Cell {
                        ch: bc.symbol().chars().next().unwrap_or(' '),
                        fg: extract_color(bc.fg),
                        bg: extract_color(bc.bg),
                        modifier: bc.modifier,
                    };
                }
            }
        });

        grid
    }

    /// Check if the current slide (at the current chunk visibility) contains charts or diagrams.
    fn current_slide_has_visual(&self) -> bool {
        if let Some(slide) = self.presentation.slides.get(self.current_slide) {
            slide
                .chunks
                .iter()
                .take(self.visible_chunks)
                .flat_map(|c| &c.elements)
                .any(|e| matches!(e, SlideElement::Chart { .. } | SlideElement::Diagram { .. } | SlideElement::Wireframe { .. } | SlideElement::Histogram { .. } | SlideElement::Photo { .. }))
        } else {
            false
        }
    }

    fn start_transition(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        prev_frame: Option<Vec<Vec<Cell>>>,
    ) {
        let grid = self.capture_frame(terminal);
        let area = terminal.get_frame().area();
        let width = area.width as usize;
        let height = area.height as usize;
        let direction = if self.current_slide_has_visual() {
            TransitionDirection::BottomUp
        } else {
            TransitionDirection::Forward
        };
        self.transition = Some(Transition::new(grid, width, height, direction, prev_frame));
    }

    fn handle_action(
        &mut self,
        action: Action,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) {
        // If a transition is still playing, skip it immediately
        if self.transition.is_some() {
            self.transition = None;
        }

        let area = terminal.get_frame().area();
        let total = self.presentation.slides.len();
        let prev_slide = self.current_slide;
        let prev_chunks = self.visible_chunks;
        let prev_scroll = self.scroll_offset;

        // Capture before-frame for chunk reveals (same slide, adding content)
        let before_frame = match action {
            Action::NextSlide
                if self.visible_chunks < self.current_slide_chunk_count() =>
            {
                Some(self.capture_frame(terminal))
            }
            Action::PrevSlide if self.visible_chunks > 1 && self.scroll_offset == 0 => {
                Some(self.capture_frame(terminal))
            }
            _ => None,
        };

        match action {
            Action::NextSlide => {
                let chunk_count = self.current_slide_chunk_count();
                if self.visible_chunks < chunk_count {
                    self.visible_chunks += 1;
                    // Auto-scroll so the new chunk's content is visible
                    self.scroll_offset = self.max_scroll_offset(area);
                } else {
                    let max_scroll = self.max_scroll_offset(area);
                    if self.scroll_offset < max_scroll {
                        let (_, _, available) = self.measure_content(area);
                        let step = available.saturating_sub(2).max(1);
                        self.scroll_offset = (self.scroll_offset + step).min(max_scroll);
                    } else if self.current_slide + 1 < total {
                        self.current_slide += 1;
                        self.visible_chunks = 1;
                        self.scroll_offset = 0;
                    }
                }
            }
            Action::PrevSlide => {
                if self.scroll_offset > 0 {
                    let (_, _, available) = self.measure_content(area);
                    let step = available.saturating_sub(2).max(1);
                    self.scroll_offset = self.scroll_offset.saturating_sub(step);
                } else if self.visible_chunks > 1 {
                    self.visible_chunks -= 1;
                } else if self.current_slide > 0 {
                    self.current_slide -= 1;
                    self.visible_chunks = self.current_slide_chunk_count();
                    self.scroll_offset = 0;
                }
            }
            Action::FirstSlide => {
                self.current_slide = 0;
                self.visible_chunks = 1;
                self.scroll_offset = 0;
            }
            Action::LastSlide => {
                self.current_slide = total.saturating_sub(1);
                self.visible_chunks = self.current_slide_chunk_count();
                self.scroll_offset = 0;
            }
            Action::Quit => {
                self.should_quit = true;
            }
        }

        // If content changed, start a transition (but not for scroll-only changes)
        let changed = self.current_slide != prev_slide || self.visible_chunks != prev_chunks || self.scroll_offset != prev_scroll;
        let scroll_only = self.current_slide == prev_slide && self.visible_chunks == prev_chunks && self.scroll_offset != prev_scroll;
        if changed && !self.should_quit && !scroll_only {
            let prev = if self.current_slide == prev_slide {
                before_frame
            } else {
                None
            };
            self.start_transition(terminal, prev);
        }

        self.broadcast_notes();
        self.sync_font_size();
    }

    fn broadcast_notes(&mut self) {
        if let Some(ref mut server) = self.notes_server {
            server.accept_pending();
            server.broadcast(&NoteMessage::SlideChanged {
                index: self.current_slide,
                visible_chunks: self.visible_chunks,
            });
        }
    }

    /// Check if the current slide has a wireframe element.
    fn current_slide_has_wireframe(&self) -> bool {
        if let Some(slide) = self.presentation.slides.get(self.current_slide) {
            slide
                .chunks
                .iter()
                .flat_map(|c| &c.elements)
                .any(|e| matches!(e, SlideElement::Wireframe { .. }))
        } else {
            false
        }
    }

    /// Switch Ghostty font size (currently disabled — AppleScript targeting not yet solved).
    fn sync_font_size(&mut self) {
        // TODO: find a reliable way to target the presenter Ghostty window
    }

    /// Restore normal font size (call on quit).
    fn restore_font_size(&mut self) {
        // TODO: re-enable when sync_font_size is working
    }
}

/// Extract an Option<Color> from a ratatui color, treating Reset as None.
fn extract_color(color: Color) -> Option<Color> {
    match color {
        Color::Reset => None,
        c => Some(c),
    }
}

