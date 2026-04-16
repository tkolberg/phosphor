use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub enum Action {
    NextSlide,
    PrevSlide,
    FirstSlide,
    LastSlide,
    Quit,
}

pub fn map_key(key: KeyEvent) -> Option<Action> {
    // Ctrl+C always quits
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }

    match key.code {
        // Navigation forward
        KeyCode::Right | KeyCode::Down | KeyCode::Char(' ') | KeyCode::Char('l')
        | KeyCode::Char('n') | KeyCode::Enter => Some(Action::NextSlide),

        // Navigation backward
        KeyCode::Left | KeyCode::Up | KeyCode::Char('h') | KeyCode::Char('p')
        | KeyCode::Backspace => Some(Action::PrevSlide),

        // Jump to start/end
        KeyCode::Home | KeyCode::Char('g') => Some(Action::FirstSlide),
        KeyCode::End | KeyCode::Char('G') => Some(Action::LastSlide),

        // Quit
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),

        _ => None,
    }
}
