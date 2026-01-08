use anyhow::Result;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::agent_spawner::{AgentSpawner, ProcessStatus};
use crate::cleanup::{CleanupMode, CleanupOrchestrator};
use crate::merge_coordinator::{MergeCoordinator, MergeStrategy};
use crate::task_parser::TaskConfig;
use crate::ui::{ProgressUI, TaskState};
use crate::worktree_manager::WorktreeManager;

pub fn execute(config_path: &PathBuf, dry_run: bool) -> Result<()> {
    if !config_path.exists() {
        anyhow::bail!("Configuration file not found: {:?}", config_path);
    }

    let config = TaskConfig::from_file(config_path)?;

    println!("Loaded {} tasks from {:?}", config.tasks.len(), config_path);

    if dry_run {
        println!("\nDry run mode - showing tasks:");
        for task in &config.tasks {
            println!("  - {}: {}", task.id, task.prompt);
            println!("    Agent: {}", task.agent);
        }
        println!("\nWorktree directory: {:?}", config.worktree_dir);
        println!("Base branch: {}", config.base_branch);
        println!("Merge strategy: {}", config.merge_strategy);
        println!("Cleanup mode: {}", config.cleanup);
        return Ok(());
    }

    let repo_path = std::env::current_dir()?;
    let worktree_manager = WorktreeManager::new(repo_path.clone())?;
    let agent_spawner = AgentSpawner::new();
    let mut ui = ProgressUI::new();

    std::fs::create_dir_all(&config.worktree_dir)?;

    for task in &config.tasks {
        ui.add_task(task.id.clone());
    }

    println!("\nCreating worktrees...");
    for task in &config.tasks {
        match worktree_manager.create_worktree(&task.id, &config.base_branch, &config.worktree_dir) {
            Ok(path) => println!("  Created worktree for {} at {:?}", task.id, path),
            Err(e) => {
                eprintln!("  Failed to create worktree for {}: {}", task.id, e);
                ui.update_task_status(&task.id, TaskState::Failed);
            }
        }
    }

    println!("\nSpawning agents...");
    for task in &config.tasks {
        let worktree_path = config.worktree_dir.join(&task.id);
        if !worktree_path.exists() {
            continue;
        }

        match agent_spawner.spawn(&task.id, &task.agent, &worktree_path) {
            Ok(_) => {
                ui.update_task_status(&task.id, TaskState::Running);
                println!("  Started agent for {}", task.id);
            }
            Err(e) => {
                eprintln!("  Failed to start agent for {}: {}", task.id, e);
                ui.update_task_status(&task.id, TaskState::Failed);
            }
        }
    }

    println!("\nMonitoring progress...");
    ui.init_progress(config.tasks.len() as u64);

    while !agent_spawner.all_completed() {
        for task_id in agent_spawner.get_task_ids() {
            if let Some(status) = agent_spawner.get_status(&task_id) {
                let state = TaskState::from_process_status(status);
                ui.update_task_status(&task_id, state);
            }
        }
        ui.update_progress();
        thread::sleep(Duration::from_millis(500));
    }

    ui.finish();

    let merge_strategy = MergeStrategy::from_str(&config.merge_strategy)
        .ok_or_else(|| anyhow::anyhow!("Invalid merge strategy: {}", config.merge_strategy))?;

    if !matches!(merge_strategy, MergeStrategy::Manual) {
        println!("\nMerging completed tasks using {} strategy...", config.merge_strategy);
        let merge_coordinator = MergeCoordinator::new(repo_path.clone())?;

        for task in &config.tasks {
            if let Some(ProcessStatus::Completed) = agent_spawner.get_status(&task.id) {
                match merge_coordinator.merge(&task.id, &config.base_branch, merge_strategy) {
                    Ok(_) => println!("  Successfully merged {}", task.id),
                    Err(e) => eprintln!("  Failed to merge {}: {}", task.id, e),
                }
            }
        }
    }

    let cleanup_mode = CleanupMode::from_str(&config.cleanup)
        .ok_or_else(|| anyhow::anyhow!("Invalid cleanup mode: {}", config.cleanup))?;

    println!("\nCleaning up worktrees (mode: {})...", config.cleanup);
    let cleanup_orchestrator = CleanupOrchestrator::new(&worktree_manager);

    for task in &config.tasks {
        let task_failed = matches!(
            agent_spawner.get_status(&task.id),
            Some(ProcessStatus::Failed)
        );

        if let Err(e) = cleanup_orchestrator.cleanup_worktree(
            &task.id,
            &config.worktree_dir,
            cleanup_mode,
            task_failed,
        ) {
            eprintln!("  Failed to cleanup {}: {}", task.id, e);
        }
    }

    if ui.has_failures() || agent_spawner.has_failures() {
        eprintln!("\nSome tasks failed. Check the output above for details.");
        std::process::exit(1);
    } else {
        println!("\nAll tasks completed successfully!");
    }

    Ok(())
}
