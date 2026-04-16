mod app;
mod braille;
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
mod testfire;
mod theme;
mod transition;
mod wireframe;

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

    // Ghostty relaunch: CLI flag > front matter > skip.
    // PHOSPHOR_IN_GHOSTTY prevents infinite recursion (set by the relaunched instance).
    if std::env::var("PHOSPHOR_IN_GHOSTTY").is_err() {
        let ghostty_config = cli.ghostty_config.clone().or_else(|| {
            front_matter
                .as_ref()
                .and_then(|fm| fm.ghostty.as_ref())
                .map(|g| expand_tilde(g))
        });

        if let Some(ref config) = ghostty_config {
            let in_ghostty = std::env::var("TERM_PROGRAM")
                .map(|v| v.eq_ignore_ascii_case("ghostty"))
                .unwrap_or(false);
            return launch_in_ghostty(&cli, config, in_ghostty);
        }
    }

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
        Some(Command::Test {
            slide,
            widths,
            height,
        }) => {
            let theme = if let Some(theme_path) = &cli.theme {
                theme::load_theme(theme_path)?
            } else if let Some(theme_path) =
                front_matter.as_ref().and_then(|fm| fm.theme.as_ref())
            {
                let theme_file = cli
                    .file
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join(theme_path);
                theme::load_theme(&theme_file)?
            } else {
                default_theme()
            };
            testfire::run(&presentation, &theme, slide - 1, &widths, height)
        }
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

    // Capture the presenter's Ghostty window ID before launching the notes window
    let presenter_window_id = if std::env::var("PHOSPHOR_IN_GHOSTTY").is_ok() {
        std::process::Command::new("osascript")
            .args(["-e", r#"tell application "Ghostty" to get id of front window"#])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    } else {
        None
    };

    // Auto-launch the notes viewer in a new Ghostty window
    let exe = std::env::current_exe().unwrap_or_default();
    let file_abs = std::fs::canonicalize(&cli.file).unwrap_or_else(|_| cli.file.clone());
    let launcher_path = std::env::temp_dir().join("phosphor-notes-launch.sh");
    std::fs::write(
        &launcher_path,
        format!(
            "#!/bin/sh\nexec '{}' '{}' notes --socket '{}'\n",
            exe.display(),
            file_abs.display(),
            socket_path.display(),
        ),
    )
    .ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&launcher_path, std::fs::Permissions::from_mode(0o755)).ok();
    }
    let notes_config_path = std::env::temp_dir().join("phosphor-notes.conf");
    std::fs::write(
        &notes_config_path,
        format!(
            "window-width = 120\nwindow-height = 10\nwindow-save-state = never\ncommand = {}\n",
            launcher_path.display(),
        ),
    )
    .ok();
    if std::env::var("PHOSPHOR_IN_GHOSTTY").is_ok() {
        let ghostty_bin = "/Applications/Ghostty.app/Contents/MacOS/ghostty";
        let _ = std::process::Command::new(ghostty_bin)
            .arg(format!("--config-file={}", notes_config_path.display()))
            .spawn();
    } else {
        let _ = std::process::Command::new("open")
            .args(["-na", "Ghostty.app", "--args"])
            .arg(format!("--config-file={}", notes_config_path.display()))
            .spawn();
    }

    // Set up terminal
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(presentation, theme);
    app.set_notes_server(notes_server);
    if let Some(wid) = presenter_window_id {
        app.set_ghostty_window_id(wid);
    }
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

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

fn launch_in_ghostty(cli: &Cli, ghostty_config: &std::path::Path, already_in_ghostty: bool) -> Result<()> {
    let exe = std::env::current_exe().wrap_err("Failed to find phosphor executable")?;
    let file = std::fs::canonicalize(&cli.file)
        .wrap_err_with(|| format!("Failed to resolve {:?}", cli.file))?;

    let mut cmd = format!(
        "PHOSPHOR_IN_GHOSTTY=1 '{}' '{}'",
        exe.display(),
        file.display()
    );
    if let Some(ref theme) = cli.theme {
        let theme_abs = std::fs::canonicalize(theme).unwrap_or_else(|_| theme.clone());
        cmd.push_str(&format!(" --theme '{}'", theme_abs.display()));
    }

    let config_abs = std::fs::canonicalize(ghostty_config)
        .wrap_err_with(|| format!("Failed to resolve Ghostty config {:?}", ghostty_config))?;

    let shell_cmd = format!("/bin/zsh -c {}", shell_escape(&cmd));

    let status = if already_in_ghostty {
        // Use the Ghostty CLI directly — opens a new window in the existing app
        // without spawning a new instance (which would restore the entire session).
        let ghostty_bin = std::path::PathBuf::from("/Applications/Ghostty.app/Contents/MacOS/ghostty");
        std::process::Command::new(&ghostty_bin)
            .arg(format!("--config-file={}", config_abs.display()))
            .arg(format!("--command={}", shell_cmd))
            .spawn()
            .wrap_err("Failed to launch Ghostty window")?;
        // The ghostty CLI spawns asynchronously; the original terminal returns immediately.
        eprintln!("Launched Ghostty with config: {}", config_abs.display());
        return Ok(());
    } else {
        std::process::Command::new("open")
            .args(["-na", "Ghostty.app", "--args"])
            .arg(format!("--config-file={}", config_abs.display()))
            .arg(format!("--command={}", shell_cmd))
            .status()
            .wrap_err("Failed to launch Ghostty window")?
    };

    if !status.success() {
        return Err(color_eyre::eyre::eyre!("Ghostty window launch failed"));
    }

    eprintln!("Launched Ghostty with config: {}", config_abs.display());
    Ok(())
}
