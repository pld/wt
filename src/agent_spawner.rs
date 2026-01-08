use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProcessStatus {
    Running,
    Completed,
    Failed,
    Terminated,
}

pub struct AgentProcess {
    pub task_id: String,
    pub status: ProcessStatus,
    pub exit_code: Option<i32>,
    child: Option<Child>,
}

pub struct AgentSpawner {
    processes: Arc<Mutex<HashMap<String, AgentProcess>>>,
}

impl AgentSpawner {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn spawn(&self, task_id: &str, command: &str, workspace: &Path) -> Result<()> {
        let child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let process = AgentProcess {
            task_id: task_id.to_string(),
            status: ProcessStatus::Running,
            exit_code: None,
            child: Some(child),
        };

        self.processes
            .lock()
            .unwrap()
            .insert(task_id.to_string(), process);

        Ok(())
    }

    pub fn get_status(&self, task_id: &str) -> Option<ProcessStatus> {
        let mut processes = self.processes.lock().unwrap();
        if let Some(process) = processes.get_mut(task_id) {
            if let Some(ref mut child) = process.child {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let exit_code = status.code().unwrap_or(-1);
                        process.exit_code = Some(exit_code);
                        process.status = if exit_code == 0 {
                            ProcessStatus::Completed
                        } else {
                            ProcessStatus::Failed
                        };
                        Some(process.status)
                    }
                    Ok(None) => Some(ProcessStatus::Running),
                    Err(_) => Some(ProcessStatus::Failed),
                }
            } else {
                Some(process.status)
            }
        } else {
            None
        }
    }

    pub fn all_completed(&self) -> bool {
        let processes = self.processes.lock().unwrap();
        if processes.is_empty() {
            return false;
        }
        processes.values().all(|p| {
            p.status == ProcessStatus::Completed || p.status == ProcessStatus::Failed
        })
    }

    pub fn has_failures(&self) -> bool {
        let processes = self.processes.lock().unwrap();
        processes.values().any(|p| p.status == ProcessStatus::Failed)
    }

    pub fn get_task_ids(&self) -> Vec<String> {
        let processes = self.processes.lock().unwrap();
        processes.keys().cloned().collect()
    }
}
