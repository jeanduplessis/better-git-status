mod app;
mod git;
mod types;
mod ui;
mod watcher;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "better-git-status")]
#[command(about = "Interactive git status with tree view and diff preview")]
struct Cli {
    /// Path to the git repository (default: current directory)
    #[arg(default_value = ".")]
    path: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    app::run(&cli.path)
}
