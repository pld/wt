use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pathdiff::diff_paths;
use std::path::{Path, PathBuf};
use std::process::Command;

use wt::worktree_manager::{
    check_not_in_worktree, ensure_worktrees_in_gitignore, get_current_worktree_name,
    WorktreeManager,
};

#[derive(Parser)]
#[command(name = "wt", version, about = "Parallel workspaces for agent sandboxes")]
struct Cli {
    /// Worktree directory
    #[arg(short = 'd', long, global = true, default_value = ".worktrees")]
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
    /// Print current worktree name (or "main" if in main worktree)
    Which,
}

fn get_repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to execute git rev-parse")?;

    if !output.status.success() {
        anyhow::bail!("Not a git repository");
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(path))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_path = get_repo_root()?;

    match cli.command {
        Commands::New { name, b } => cmd_new(&repo_path, &cli.dir, &name, &b),
        Commands::Ls => cmd_ls(&repo_path),
        Commands::Rm { name } => cmd_rm(&repo_path, &cli.dir, &name),
        Commands::Run { config, dry_run } => wt::run::execute(&config, dry_run),
        Commands::Which => cmd_which(&repo_path),
    }
}

fn cmd_new(repo_path: &PathBuf, worktree_dir: &PathBuf, name: &str, base: &str) -> Result<()> {
    check_not_in_worktree(repo_path)?;
    let manager = WorktreeManager::new(repo_path.clone())?;
    ensure_worktrees_in_gitignore(repo_path)?;
    std::fs::create_dir_all(worktree_dir)?;
    let path = manager.create_worktree(name, base, worktree_dir)?;
    let relative_path = diff_paths(&path, repo_path).unwrap_or(path);
    eprintln!("{}", name);
    println!("{}", relative_path.display());
    Ok(())
}

fn cmd_ls(repo_path: &PathBuf) -> Result<()> {
    let manager = WorktreeManager::new(repo_path.clone())?;
    let worktrees = manager.list_worktrees()?;

    for wt in &worktrees {
        if !wt.task_id.is_empty() {
            let relative_path = diff_paths(&wt.path, repo_path).unwrap_or(wt.path.clone());
            if wt.task_id != wt.branch {
                println!("{}\t{}\t(branch: {})", wt.task_id, relative_path.display(), wt.branch);
            } else {
                println!("{}\t{}", wt.task_id, relative_path.display());
            }
        }
    }
    Ok(())
}

fn cmd_rm(repo_path: &PathBuf, worktree_dir: &PathBuf, name: &str) -> Result<()> {
    let manager = WorktreeManager::new(repo_path.clone())?;
    manager.remove_worktree(name, worktree_dir)?;
    Ok(())
}

fn cmd_which(repo_path: &Path) -> Result<()> {
    let name = get_current_worktree_name(repo_path)?;
    println!("{}", name);
    Ok(())
}
