use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use crate::agent_spawner::ProcessStatus;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed,
}

impl TaskState {
    pub fn from_process_status(status: ProcessStatus) -> Self {
        match status {
            ProcessStatus::Running => TaskState::Running,
            ProcessStatus::Completed => TaskState::Completed,
            ProcessStatus::Failed => TaskState::Failed,
            ProcessStatus::Terminated => TaskState::Failed,
        }
    }
}

pub struct ProgressUI {
    tasks: HashMap<String, TaskState>,
    progress_bar: Option<ProgressBar>,
}

impl ProgressUI {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            progress_bar: None,
        }
    }

    pub fn add_task(&mut self, task_id: String) {
        self.tasks.insert(task_id, TaskState::Pending);
    }

    pub fn update_task_status(&mut self, task_id: &str, state: TaskState) {
        if let Some(task_state) = self.tasks.get_mut(task_id) {
            *task_state = state;
        }
    }

    pub fn init_progress(&mut self, total: u64) {
        let pb = ProgressBar::new(total);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("##-"),
        );
        self.progress_bar = Some(pb);
    }

    pub fn update_progress(&self) {
        if let Some(pb) = &self.progress_bar {
            let completed = self
                .tasks
                .values()
                .filter(|s| **s == TaskState::Completed || **s == TaskState::Failed)
                .count() as u64;
            pb.set_position(completed);

            let summary = self.get_summary_string();
            pb.set_message(summary);
        }
    }

    pub fn finish(&self) {
        if let Some(pb) = &self.progress_bar {
            pb.finish_with_message("All tasks processed");
        }
    }

    fn get_summary_string(&self) -> String {
        let completed = self
            .tasks
            .values()
            .filter(|s| **s == TaskState::Completed)
            .count();
        let failed = self.tasks.values().filter(|s| **s == TaskState::Failed).count();
        let running = self
            .tasks
            .values()
            .filter(|s| **s == TaskState::Running)
            .count();

        format!(
            "Completed: {} | Failed: {} | Running: {}",
            completed, failed, running
        )
    }

    pub fn has_failures(&self) -> bool {
        self.tasks.values().any(|s| *s == TaskState::Failed)
    }
}
