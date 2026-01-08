use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env;
use std::path::PathBuf;

use wt::worktree_manager::WorktreeManager;

#[derive(Parser)]
#[command(name = "wt", version, about = "Parallel workspaces for agent sandboxes")]
struct Cli {
    /// Worktree directory
    #[arg(short = 'd', long, global = true, default_value = "../wt-trees")]
    dir: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new workspace
    New {
        /// Name for the workspace
        name: String,
        /// Base branch to create from
        #[arg(short, default_value = "main")]
        b: String,
    },
    /// List all workspaces
    Ls,
    /// Remove a workspace
    Rm {
        /// Name of the workspace to remove
        name: String,
    },
    /// Run batch orchestration from config file
    Run {
        /// Path to tasks.yaml configuration
        config: PathBuf,
        /// Show what would be done without executing
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_path = env::current_dir()?;

    match cli.command {
        Commands::New { name, b } => cmd_new(&repo_path, &cli.dir, &name, &b),
        Commands::Ls => cmd_ls(&repo_path),
        Commands::Rm { name } => cmd_rm(&repo_path, &cli.dir, &name),
        Commands::Run { config, dry_run } => wt::run::execute(&config, dry_run),
    }
}

fn cmd_new(repo_path: &PathBuf, worktree_dir: &PathBuf, name: &str, base: &str) -> Result<()> {
    let manager = WorktreeManager::new(repo_path.clone())?;
    std::fs::create_dir_all(worktree_dir)?;
    manager.create_worktree(name, base, worktree_dir)?;
    println!("{}", name);
    Ok(())
}

fn cmd_ls(repo_path: &PathBuf) -> Result<()> {
    let manager = WorktreeManager::new(repo_path.clone())?;
    let worktrees = manager.list_worktrees()?;

    for wt in &worktrees {
        if !wt.task_id.is_empty() {
            println!("{}", wt.task_id);
        }
    }
    Ok(())
}

fn cmd_rm(repo_path: &PathBuf, worktree_dir: &PathBuf, name: &str) -> Result<()> {
    let manager = WorktreeManager::new(repo_path.clone())?;
    manager.remove_worktree(name, worktree_dir)?;
    Ok(())
}
