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
}
