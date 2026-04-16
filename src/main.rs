mod app;
mod chart;
mod cli;
mod diagram;
mod elements;
mod halfblock;
mod input;
mod metadata;
mod notes;
mod notes_app;
mod parse;
mod render;
mod slide;
mod theme;
mod transition;

use std::io;

use clap::Parser;
use cli::{Cli, Command};
use color_eyre::eyre::{Result, WrapErr};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use app::App;
use notes::client::NotesClient;
use notes::protocol::default_socket_path;
use notes::server::NotesServer;
use notes_app::NotesApp;
use theme::loader::default_theme;

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(io::stdout(), LeaveAlternateScreen).ok();
    }
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    let content = std::fs::read_to_string(&cli.file)
        .wrap_err_with(|| format!("Failed to read {:?}", cli.file))?;

    // Extract front matter before parsing
    let (front_matter, markdown) = metadata::extract_front_matter(&content);

    let base_dir = cli
        .file
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let mut presentation = parse::parse_presentation(markdown, &base_dir);

    // Front matter title overrides auto-detected title
    if let Some(ref fm) = front_matter {
        if fm.title.is_some() {
            presentation.metadata.title = fm.title.clone();
        }
    }

    match cli.command {
        Some(Command::Notes { socket }) => run_notes_viewer(presentation, socket),
        None => run_presenter(presentation, cli, front_matter),
    }
}

fn run_presenter(
    presentation: slide::Presentation,
    cli: Cli,
    front_matter: Option<metadata::FrontMatter>,
) -> Result<()> {
    // Theme priority: CLI flag > front matter > default
    let theme = if let Some(theme_path) = &cli.theme {
        theme::load_theme(theme_path)?
    } else if let Some(theme_path) = front_matter.as_ref().and_then(|fm| fm.theme.as_ref()) {
        let theme_file = cli.file.parent().unwrap_or(std::path::Path::new(".")).join(theme_path);
        theme::load_theme(&theme_file)?
    } else {
        default_theme()
    };

    // Start notes server
    let socket_path = default_socket_path();
    let notes_server = NotesServer::bind(&socket_path)?;

    // Write socket path to a well-known location so the notes viewer can find it
    let socket_ref_path = std::env::temp_dir().join("phosphor-notes.sock");
    std::fs::write(&socket_ref_path, socket_path.display().to_string()).ok();

    // Set up terminal
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(presentation, theme);
    app.set_notes_server(notes_server);
    app.run(&mut terminal)?;

    Ok(())
}

fn run_notes_viewer(
    presentation: slide::Presentation,
    socket: Option<std::path::PathBuf>,
) -> Result<()> {
    // Auto-discover socket: check well-known ref file, or use --socket flag
    let socket_path = if let Some(path) = socket {
        path
    } else {
        let ref_path = std::env::temp_dir().join("phosphor-notes.sock");
        if ref_path.exists() {
            let contents = std::fs::read_to_string(&ref_path)
                .wrap_err("Failed to read socket reference file")?;
            std::path::PathBuf::from(contents.trim())
        } else {
            return Err(color_eyre::eyre::eyre!(
                "No running presenter found. Start the presenter first, or use --socket <path>"
            ));
        }
    };

    let client = NotesClient::connect(&socket_path)?;

    // Set up terminal
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = NotesApp::new(presentation, client);
    app.run(&mut terminal)?;

    Ok(())
}
