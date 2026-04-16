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
use crate::slide::Presentation;
use crate::theme::Theme;
use crate::transition::{Cell, Transition, TransitionDirection};

/// Tick interval for animation frames (~33ms ≈ 30fps).
const TICK_RATE: Duration = Duration::from_millis(33);

pub struct App {
    presentation: Presentation,
    theme: Theme,
    current_slide: usize,
    visible_chunks: usize,
    should_quit: bool,
    notes_server: Option<NotesServer>,
    transition: Option<Transition>,
}

impl App {
    pub fn new(presentation: Presentation, theme: Theme) -> Self {
        Self {
            presentation,
            theme,
            current_slide: 0,
            visible_chunks: 1,
            should_quit: false,
            notes_server: None,
            transition: None,
        }
    }

    pub fn set_notes_server(&mut self, server: NotesServer) {
        self.notes_server = Some(server);
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
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
            let ctx = LowerContext {
                window_width: content_area.width,
                window_height: content_area.height,
                theme: &self.theme,
            };

            let mut all_ops = Vec::new();
            for chunk in slide.chunks.iter().take(self.visible_chunks) {
                for element in &chunk.elements {
                    all_ops.extend(element.lower(&ctx));
                }
            }

            let mut engine = RenderEngine::new(content_area);
            engine.set_theme(&self.theme);
            engine.set_default_colors(fg_color, bg_color);
            engine.render(&all_ops, frame);
        }
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
                .any(|e| matches!(e, SlideElement::Chart { .. } | SlideElement::Diagram { .. }))
        } else {
            false
        }
    }

    fn start_transition(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) {
        let grid = self.capture_frame(terminal);
        let area = terminal.get_frame().area();
        let width = area.width as usize;
        let height = area.height as usize;
        let direction = if self.current_slide_has_visual() {
            TransitionDirection::BottomUp
        } else {
            TransitionDirection::Forward
        };
        self.transition = Some(Transition::new(grid, width, height, direction));
    }

    fn handle_action(
        &mut self,
        action: Action,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) {
        let total = self.presentation.slides.len();
        let prev_slide = self.current_slide;
        let prev_chunks = self.visible_chunks;

        match action {
            Action::NextSlide => {
                let chunk_count = self.current_slide_chunk_count();
                if self.visible_chunks < chunk_count {
                    self.visible_chunks += 1;
                } else if self.current_slide + 1 < total {
                    self.current_slide += 1;
                    self.visible_chunks = 1;
                }
            }
            Action::PrevSlide => {
                if self.visible_chunks > 1 {
                    self.visible_chunks -= 1;
                } else if self.current_slide > 0 {
                    self.current_slide -= 1;
                    self.visible_chunks = self.current_slide_chunk_count();
                }
            }
            Action::FirstSlide => {
                self.current_slide = 0;
                self.visible_chunks = 1;
            }
            Action::LastSlide => {
                self.current_slide = total.saturating_sub(1);
                self.visible_chunks = self.current_slide_chunk_count();
            }
            Action::Quit => {
                self.should_quit = true;
            }
        }

        // If content changed, start a transition
        let changed = self.current_slide != prev_slide || self.visible_chunks != prev_chunks;
        if changed && !self.should_quit {
            self.start_transition(terminal);
        }

        self.broadcast_notes();
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
}

/// Extract an Option<Color> from a ratatui color, treating Reset as None.
fn extract_color(color: Color) -> Option<Color> {
    match color {
        Color::Reset => None,
        c => Some(c),
    }
}
