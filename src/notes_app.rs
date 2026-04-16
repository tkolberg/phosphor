use std::io::Stdout;
use std::time::Instant;

use color_eyre::eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::notes::client::NotesClient;
use crate::notes::protocol::NoteMessage;
use crate::slide::Presentation;

pub struct NotesApp {
    presentation: Presentation,
    client: NotesClient,
    current_slide: usize,
    visible_chunks: usize,
    started_at: Instant,
    should_quit: bool,
}

impl NotesApp {
    pub fn new(presentation: Presentation, client: NotesClient) -> Self {
        Self {
            presentation,
            client,
            current_slide: 0,
            visible_chunks: 1,
            started_at: Instant::now(),
            should_quit: false,
        }
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        // Use a short poll timeout so we can check for both key events and socket messages
        while !self.should_quit {
            terminal.draw(|frame| self.draw(frame))?;

            // Poll for keyboard input with a short timeout
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q')
                        || key.code == KeyCode::Esc
                        || (key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('c'))
                    {
                        self.should_quit = true;
                        continue;
                    }
                }
            }

            // Try to receive a message from the presenter (non-blocking via short timeout)
            // The client recv is blocking, so we set a read timeout on the underlying stream
            self.try_recv();
        }
        Ok(())
    }

    fn try_recv(&mut self) {
        match self.client.recv() {
            Some(NoteMessage::SlideChanged {
                index,
                visible_chunks,
            }) => {
                self.current_slide = index;
                self.visible_chunks = visible_chunks;
            }
            Some(NoteMessage::Quit) => {
                self.should_quit = true;
            }
            None => {
                // Timeout or connection lost — will retry next loop
            }
        }
    }

    fn draw(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        // Dark background
        let bg_style = Style::default().bg(Color::Black).fg(Color::White);
        let block = ratatui::widgets::Block::default().style(bg_style);
        frame.render_widget(block, area);

        // Header: slide indicator
        let total = self.presentation.slides.len();
        let current = self.current_slide + 1;
        let header_text = format!(" Slide {current}/{total}");
        let header = Line::from(Span::styled(
            header_text,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        let header_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(header), header_area);

        // Separator
        let sep = Line::from("─".repeat(area.width as usize));
        let sep_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(sep).style(Style::default().fg(Color::DarkGray)),
            sep_area,
        );

        // Notes content
        let notes_text = self
            .presentation
            .slides
            .get(self.current_slide)
            .and_then(|s| s.notes.as_deref())
            .unwrap_or("(no notes for this slide)");

        let content_area = Rect {
            x: area.x + 2,
            y: area.y + 3,
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(5),
        };

        let notes_style = if notes_text == "(no notes for this slide)" {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        let notes = Paragraph::new(notes_text)
            .style(notes_style)
            .wrap(Wrap { trim: false });
        frame.render_widget(notes, content_area);

        // Footer: elapsed timer
        let elapsed = self.started_at.elapsed();
        let mins = elapsed.as_secs() / 60;
        let secs = elapsed.as_secs() % 60;
        let timer = format!(" Elapsed: {mins:02}:{secs:02}");

        let footer_area = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        let footer = Paragraph::new(Line::from(Span::styled(
            timer,
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(footer, footer_area);
    }
}
