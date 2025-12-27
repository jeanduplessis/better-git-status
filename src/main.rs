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
struct Cli {}

fn main() -> Result<()> {
    app::run(".")
}
