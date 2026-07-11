use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Represents the state of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Running in foreground or background.
    Running,
    /// Suspended by SIGTSTP.
    Stopped,
    /// Exited.
    Completed,
}

/// A single process tracked by the job manager.
#[derive(Debug)]
pub struct ProcessInfo {
    /// OS process ID.
    pub pid: u32,
    /// The command name.
    pub command_name: String,
    /// Current state.
    state: Mutex<JobState>,
}

impl ProcessInfo {
    /// Creates a new `ProcessInfo`.
    #[must_use]
    pub fn new(pid: u32, command_name: impl Into<String>) -> Self {
        Self {
            pid,
            command_name: command_name.into(),
            state: Mutex::new(JobState::Running),
        }
    }

    /// Returns the current state.
    #[must_use]
    pub fn state(&self) -> JobState {
        *self.state.lock().expect("lock poisoned")
    }

    /// Sets the state.
    pub fn set_state(&self, state: JobState) {
        *self.state.lock().expect("lock poisoned") = state;
    }
}

/// A job is one or more processes forming a pipeline.
#[derive(Debug)]
pub struct Job {
    /// 1-based job ID.
    pub id: u32,
    /// Processes in this job (first = pipeline leader).
    pub processes: Vec<ProcessInfo>,
    /// Current aggregate state.
    state: Mutex<JobState>,
    /// The command text as typed by the user.
    pub command_string: String,
    /// Whether this is a background job (`cmd &`).
    pub background: bool,
}

impl Job {
    /// Creates a new `Job`.
    #[must_use]
    pub fn new(
        id: u32,
        processes: Vec<ProcessInfo>,
        command_string: impl Into<String>,
        background: bool,
    ) -> Self {
        Self {
            id,
            processes,
            state: Mutex::new(JobState::Running),
            command_string: command_string.into(),
            background,
        }
    }

    /// Returns the current aggregate state.
    #[must_use]
    pub fn state(&self) -> JobState {
        *self.state.lock().expect("lock poisoned")
    }

    /// Sets the aggregate state.
    pub fn set_state(&self, state: JobState) {
        *self.state.lock().expect("lock poisoned") = state;
    }

    /// Returns the process group leader PID.
    #[must_use]
    pub fn pgid(&self) -> Option<u32> {
        self.processes.first().map(|p| p.pid)
    }

    /// Returns a formatted status line for `jobs -l`.
    #[must_use]
    pub fn status_line(&self) -> String {
        let state_str = match self.state() {
            JobState::Running => "Running",
            JobState::Stopped => "Stopped",
            JobState::Completed => "Done",
        };
        let bg_marker = if self.background { " &" } else { "" };
        format!(
            "[{}] {} {state_str}{bg_marker}",
            self.id,
            self.pgid().unwrap_or(0)
        )
    }
}

/// Manages shell jobs for foreground/background control.
pub struct JobManager {
    jobs: Mutex<HashMap<u32, Arc<Job>>>,
    next_id: Mutex<u32>,
}

impl JobManager {
    /// Creates a new `JobManager`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }

    /// Returns the number of tracked jobs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.jobs.lock().expect("lock poisoned").len()
    }

    /// Returns true if no jobs are tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.jobs.lock().expect("lock poisoned").is_empty()
    }

    /// Adds a new job and returns its ID.
    pub fn add(&self, job: Job) -> u32 {
        let id = job.id;
        self.jobs
            .lock()
            .expect("lock poisoned")
            .insert(id, Arc::new(job));
        id
    }

    /// Allocates the next job ID without inserting.
    pub fn next_id(&self) -> u32 {
        let mut next = self.next_id.lock().expect("lock poisoned");
        let id = *next;
        *next += 1;
        id
    }

    /// Returns a job by ID.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<Arc<Job>> {
        self.jobs.lock().expect("lock poisoned").get(&id).cloned()
    }

    /// Returns all active jobs.
    #[must_use]
    pub fn list(&self) -> Vec<Arc<Job>> {
        self.jobs
            .lock()
            .expect("lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Removes a job by ID.
    pub fn remove(&self, id: u32) -> bool {
        self.jobs
            .lock()
            .expect("lock poisoned")
            .remove(&id)
            .is_some()
    }

    /// Cleans up completed jobs.
    pub fn cleanup(&self) {
        self.jobs
            .lock()
            .expect("lock poisoned")
            .retain(|_, job| job.state() != JobState::Completed);
    }
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_manager_new() {
        let mgr = JobManager::new();
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_add_and_get() {
        let mgr = JobManager::new();
        let job = Job {
            id: 1,
            processes: vec![ProcessInfo {
                pid: 100,
                command_name: "ls".into(),
                state: Mutex::new(JobState::Running),
            }],
            state: Mutex::new(JobState::Running),
            command_string: "ls".into(),
            background: false,
        };
        mgr.add(job);
        assert_eq!(mgr.len(), 1);
        assert!(mgr.get(1).is_some());
    }

    #[test]
    fn test_status_line() {
        let job = Job {
            id: 1,
            processes: vec![ProcessInfo {
                pid: 100,
                command_name: "sleep".into(),
                state: Mutex::new(JobState::Running),
            }],
            state: Mutex::new(JobState::Running),
            command_string: "sleep 10 &".into(),
            background: true,
        };
        let line = job.status_line();
        assert!(line.contains("[1]"));
        assert!(line.contains("Running"));
    }
}
