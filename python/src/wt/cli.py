import argparse
import sys
import time
from pathlib import Path
from rich.console import Console
from wt.task_parser import parse_tasks
from wt.worktree_manager import WorktreeManager
from wt.agent_spawner import AgentSpawner
from wt.ui import ProgressUI, TaskState
from wt.merge_coordinator import MergeCoordinator, MergeStrategy
from wt.cleanup import CleanupOrchestrator, CleanupMode


def main():
    parser = argparse.ArgumentParser(
        description="Git worktree orchestrator for parallel agent sessions"
    )
    parser.add_argument(
        "config",
        type=Path,
        help="Path to tasks.yaml configuration file"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be done without executing"
    )

    args = parser.parse_args()

    console = Console()

    if not args.config.exists():
        console.print(f"[red]Error: Configuration file not found: {args.config}[/red]")
        sys.exit(1)

    try:
        config = parse_tasks(args.config)
    except Exception as e:
        console.print(f"[red]Error parsing configuration: {e}[/red]")
        sys.exit(1)

    console.print(f"[cyan]Loaded {len(config.tasks)} tasks from {args.config}[/cyan]")

    if args.dry_run:
        console.print("\n[yellow]Dry run mode - showing tasks:[/yellow]")
        for task in config.tasks:
            console.print(f"  - {task.id}: {task.prompt}")
            console.print(f"    Agent: {task.agent}")
        console.print(f"\nWorktree directory: {config.worktree_dir}")
        console.print(f"Base branch: {config.base_branch}")
        console.print(f"Merge strategy: {config.merge_strategy}")
        console.print(f"Cleanup mode: {config.cleanup}")
        return

    repo_path = Path.cwd()

    try:
        worktree_manager = WorktreeManager(repo_path)
        agent_spawner = AgentSpawner()
        ui = ProgressUI()
        merge_coordinator = MergeCoordinator(repo_path)
        cleanup_orchestrator = CleanupOrchestrator(worktree_manager)

        config.worktree_dir.mkdir(parents=True, exist_ok=True)

        for task in config.tasks:
            ui.add_task(task.id, task.prompt)

        console.print("\n[cyan]Creating worktrees...[/cyan]")
        for task in config.tasks:
            try:
                worktree_path = worktree_manager.create_worktree(
                    task.id,
                    config.base_branch,
                    config.worktree_dir
                )
                console.print(f"  Created worktree for {task.id} at {worktree_path}")
            except Exception as e:
                console.print(f"[red]  Failed to create worktree for {task.id}: {e}[/red]")
                ui.update_task_status(task.id, TaskState.FAILED)

        console.print("\n[cyan]Spawning agents...[/cyan]")
        processes = {}
        for task in config.tasks:
            worktree_path = config.worktree_dir / task.id
            if not worktree_path.exists():
                continue

            try:
                process = agent_spawner.spawn(
                    task.id,
                    task.agent,
                    worktree_path
                )
                processes[task.id] = process
                ui.update_task_status(task.id, TaskState.RUNNING)
                console.print(f"  Started agent for {task.id}")
            except Exception as e:
                console.print(f"[red]  Failed to start agent for {task.id}: {e}[/red]")
                ui.update_task_status(task.id, TaskState.FAILED)

        console.print("\n[cyan]Monitoring progress...[/cyan]")
        ui.display()

        while not ui.all_tasks_completed():
            for task_id, process in processes.items():
                status = agent_spawner.get_status(process)
                current_state = TaskState.from_process_status(status)
                ui.update_task_status(task_id, current_state)

            time.sleep(1)

        ui.display()
        ui.display_summary()

        if config.merge_strategy != "manual":
            console.print(f"\n[cyan]Merging completed tasks using {config.merge_strategy} strategy...[/cyan]")
            merge_strategy = MergeStrategy(config.merge_strategy)

            for task in config.tasks:
                task_info = ui.get_tasks()
                task_dict = {t["task_id"]: t for t in task_info}

                if task_dict[task.id]["state"] == TaskState.COMPLETED:
                    try:
                        result = merge_coordinator.merge(
                            task.id,
                            config.base_branch,
                            merge_strategy
                        )
                        console.print(f"  {result.message}")
                    except Exception as e:
                        console.print(f"[red]  Failed to merge {task.id}: {e}[/red]")

        cleanup_mode = CleanupMode(config.cleanup)
        console.print(f"\n[cyan]Cleaning up worktrees (mode: {cleanup_mode.value})...[/cyan]")

        task_info = ui.get_tasks()
        task_statuses = {
            t["task_id"]: t["state"] == TaskState.FAILED
            for t in task_info
        }

        cleanup_orchestrator.cleanup_all(
            config.worktree_dir,
            cleanup_mode,
            task_statuses
        )

        if ui.has_failed_tasks():
            console.print("\n[red]Some tasks failed. Check the output above for details.[/red]")
            sys.exit(1)
        else:
            console.print("\n[green]All tasks completed successfully![/green]")

    except KeyboardInterrupt:
        console.print("\n[yellow]Interrupted by user. Cleaning up...[/yellow]")
        sys.exit(130)
    except Exception as e:
        console.print(f"\n[red]Fatal error: {e}[/red]")
        sys.exit(1)


if __name__ == "__main__":
    main()
