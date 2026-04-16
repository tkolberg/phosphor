use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "phosphor", about = "Terminal slide deck presentations")]
pub struct Cli {
    /// Path to the markdown slide file
    pub file: PathBuf,

    /// Path to a YAML theme file
    #[arg(long)]
    pub theme: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Show presenter notes in a separate terminal
    Notes {
        /// Unix socket path for syncing with the presenter
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Render a single slide to stdout for debugging (no TUI)
    Test {
        /// Slide number (1-indexed)
        #[arg(long, default_value = "1")]
        slide: usize,

        /// Terminal widths to test (comma-separated, e.g. "40,80,120")
        #[arg(long, value_delimiter = ',', default_values_t = vec![40, 80, 120])]
        widths: Vec<u16>,

        /// Terminal height to use
        #[arg(long, default_value = "30")]
        height: u16,
    },
}
