//! Prompt rendering for the interactive shell.
//!
//! Produces a multi-segment, ANSI-colored prompt showing username,
//! hostname, current directory, git branch, exit status, and more.

use std::path::PathBuf;
use std::process::Command;

use nu_ansi_term::{Color as AnsiColor, Style};

/// Rich git status information for the prompt.
#[derive(Debug, Clone, Default)]
struct GitStatus {
    branch: String,
    dirty: bool,
    ahead: u32,
    behind: u32,
    staged: u32,
    untracked: u32,
}

/// Renders the shell prompt.
#[derive(Debug, Clone)]
pub struct Prompt {
    /// Whether to show the exit-status indicator.
    pub show_status: bool,
    /// The prompt symbol (e.g. `>`).
    pub symbol: String,
    /// Segments to render (e.g. `["user", "dir", "git"]`).
    pub segments: Vec<String>,
}

impl Prompt {
    /// Creates a prompt with the given settings.
    #[must_use]
    pub fn new(show_status: bool, symbol: String, segments: Vec<String>) -> Self {
        Self {
            show_status,
            symbol,
            segments,
        }
    }

    /// Returns the current working directory abbreviated with `~`.
    #[must_use]
    pub fn cwd_display() -> String {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        aster_utils::abbreviate_path(&cwd)
    }

    /// Returns the current git branch name, if inside a git repo.
    #[must_use]
    pub fn git_branch() -> Option<String> {
        let cwd = std::env::current_dir().ok()?;
        Self::find_git_branch(&cwd)
    }

    fn find_git_branch(start: &PathBuf) -> Option<String> {
        let mut dir = start.as_path();
        loop {
            let git_dir = dir.join(".git");
            if git_dir.is_dir() {
                let head_file = git_dir.join("HEAD");
                let head = std::fs::read_to_string(&head_file).ok()?;
                let head = head.trim();
                if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
                    return Some(branch.to_string());
                }
                return Some(head[..7].to_string());
            }
            dir = dir.parent()?;
        }
    }

    /// Returns rich git status if inside a git repo.
    fn git_status() -> Option<GitStatus> {
        let cwd = std::env::current_dir().ok()?;

        let output = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(&cwd)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let branch = Self::find_git_branch(&cwd)?;

        let mut status = GitStatus {
            branch,
            ..GitStatus::default()
        };

        if let Ok(output) = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&cwd)
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.len() < 2 {
                    continue;
                }
                let index_status = line.as_bytes()[0];
                let worktree_status = line.as_bytes()[1];

                if index_status != b' ' && index_status != b'?' {
                    status.staged += 1;
                    status.dirty = true;
                }
                if worktree_status != b' ' && worktree_status != b'?' {
                    status.dirty = true;
                }
                if index_status == b'?' && worktree_status == b'?' {
                    status.untracked += 1;
                    status.dirty = true;
                }
            }
        }

        if let Ok(output) = Command::new("git")
            .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
            .current_dir(&cwd)
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = stdout.trim().split('\t').collect();
                if parts.len() == 2 {
                    status.ahead = parts[0].parse().unwrap_or(0);
                    status.behind = parts[1].parse().unwrap_or(0);
                }
            }
        }

        Some(status)
    }

    /// Returns true if running inside an SSH session.
    fn is_ssh() -> bool {
        std::env::var("SSH_CONNECTION").is_ok() || std::env::var("SSH_CLIENT").is_ok()
    }

    /// Returns the active virtualenv name, if any.
    fn virtualenv() -> Option<String> {
        std::env::var("VIRTUAL_ENV")
            .ok()
            .and_then(|p| PathBuf::from(p).file_name().map(|n| n.to_string_lossy().into_owned()))
    }

    /// Returns the count of background jobs.
    fn job_count() -> u32 {
        std::env::var("ASTER_JOB_COUNT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    /// Renders the full prompt string for the given last exit code and command duration.
    #[must_use]
    pub fn render(&self, last_exit_code: i32, last_duration: std::time::Duration) -> String {
        let mut result = String::new();

        for segment in &self.segments {
            match segment.as_str() {
                "user" => {
                    result.push_str(&self.render_user());
                }
                "host" => {
                    result.push_str(&self.render_host());
                }
                "dir" => {
                    result.push_str(&self.render_dir());
                }
                "git" => {
                    if let Some(status) = Self::git_status() {
                        result.push_str(&self.render_git(&status));
                    }
                }
                "status" => {
                    if self.show_status && last_exit_code != 0 {
                        result.push_str(&self.render_status(last_exit_code));
                    }
                }
                "time" => {
                    result.push_str(&self.render_time());
                }
                "duration" => {
                    if !last_duration.is_zero() && last_duration.as_millis() > 100 {
                        result.push_str(&self.render_duration(last_duration));
                    }
                }
                "ssh" => {
                    if Self::is_ssh() {
                        result.push_str(&self.render_ssh());
                    }
                }
                "venv" => {
                    if let Some(name) = Self::virtualenv() {
                        result.push_str(&self.render_venv(&name));
                    }
                }
                "jobs" => {
                    let count = Self::job_count();
                    if count > 0 {
                        result.push_str(&self.render_jobs(count));
                    }
                }
                _ => {}
            }
        }

        if result.is_empty() {
            result = self.render_legacy(last_exit_code);
        }

        result.push('\n');
        result.push_str(&self.render_symbol());
        result
    }

    fn render_user(&self) -> String {
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
        let style = Style::new().fg(AnsiColor::Cyan).bold();
        format!("{}@{}", style.paint(&user), style.paint(&Self::hostname()))
    }

    fn render_host(&self) -> String {
        let style = Style::new().fg(AnsiColor::Cyan).bold();
        format!("{}", style.paint(&Self::hostname()))
    }

    fn render_dir(&self) -> String {
        let dir = Self::cwd_display();
        let style = Style::new().fg(AnsiColor::Green).bold();
        format!("{}", style.paint(dir))
    }

    fn render_git(&self, status: &GitStatus) -> String {
        let branch_style = Style::new().fg(AnsiColor::Magenta);
        let dirty_style = Style::new().fg(AnsiColor::Yellow);
        let ahead_style = Style::new().fg(AnsiColor::Green);
        let behind_style = Style::new().fg(AnsiColor::Red);

        let dirty_mark = if status.dirty {
            format!("{}", dirty_style.paint("*"))
        } else {
            String::new()
        };

        let mut sync_info = String::new();
        if status.ahead > 0 {
            sync_info.push_str(&format!(
                "{}",
                ahead_style.paint(format!("\u{2191}{}", status.ahead))
            ));
        }
        if status.behind > 0 {
            sync_info.push_str(&format!(
                "{}",
                behind_style.paint(format!("\u{2193}{}", status.behind))
            ));
        }

        let extra = if status.staged > 0 || status.untracked > 0 {
            let mut parts = Vec::new();
            if status.staged > 0 {
                parts.push(format!("+{}", status.staged));
            }
            if status.untracked > 0 {
                parts.push(format!("?{}", status.untracked));
            }
            format!(" {}", parts.join(" "))
        } else {
            String::new()
        };

        format!(
            " {}",
            branch_style.paint(format!(
                "({branch}{dirty}{sync}{extra})",
                branch = status.branch,
                dirty = dirty_mark,
                sync = sync_info,
                extra = extra
            ))
        )
    }

    fn render_status(&self, code: i32) -> String {
        let style = Style::new().fg(AnsiColor::Red).bold();
        format!(" {}", style.paint(format!("[{code}]")))
    }

    fn render_time(&self) -> String {
        let style = Style::new().fg(AnsiColor::DarkGray);
        let now = chrono::Local::now();
        format!(" {}", style.paint(now.format("%H:%M:%S").to_string()))
    }

    fn render_duration(&self, duration: std::time::Duration) -> String {
        let style = Style::new().fg(AnsiColor::DarkGray);
        let ms = duration.as_millis();
        let text = if ms < 1000 {
            format!("{ms}ms")
        } else if ms < 60_000 {
            format!("{:.1}s", duration.as_secs_f64())
        } else {
            format!("{:.0}m{:.0}s", ms / 60_000, (ms % 60_000) / 1000)
        };
        format!(" {}", style.paint(format!("[{text}]")))
    }

    fn render_symbol(&self) -> String {
        let style = Style::new().fg(AnsiColor::Magenta).bold();
        format!("{} ", style.paint(&self.symbol))
    }

    fn render_ssh(&self) -> String {
        let style = Style::new().fg(AnsiColor::Yellow).bold();
        let host = Self::hostname();
        format!(" {} ", style.paint(format!("\u{1F4BB}{host}")))
    }

    fn render_venv(&self, name: &str) -> String {
        let style = Style::new().fg(AnsiColor::Blue).bold();
        format!(" {} ", style.paint(format!("\u{1F40D}{name}")))
    }

    fn render_jobs(&self, count: u32) -> String {
        let style = Style::new().fg(AnsiColor::DarkGray).bold();
        format!(" {} ", style.paint(format!("\u{2699}{count}")))
    }

    fn render_legacy(&self, last_exit_code: i32) -> String {
        let dir = Self::cwd_display();

        let status = if self.show_status && last_exit_code != 0 {
            let style = Style::new().fg(AnsiColor::Red).bold();
            format!(" {}", style.paint("x"))
        } else {
            String::new()
        };

        let dir_style = Style::new().fg(AnsiColor::Green).bold();
        format!("{}{status}", dir_style.paint(dir))
    }

    fn hostname() -> String {
        std::env::var("HOSTNAME")
            .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()))
            .unwrap_or_else(|_| "localhost".into())
    }
}

impl Default for Prompt {
    fn default() -> Self {
        Self {
            show_status: true,
            symbol: ">".into(),
            segments: vec![
                "status".into(),
                "dir".into(),
                "git".into(),
                "ssh".into(),
                "venv".into(),
                "jobs".into(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_render_success() {
        let p = Prompt::default();
        let rendered = p.render(0, std::time::Duration::ZERO);
        assert!(rendered.contains('>'));
    }

    #[test]
    fn test_prompt_render_failure() {
        let p = Prompt::default();
        let rendered = p.render(1, std::time::Duration::ZERO);
        assert!(rendered.contains('x') || rendered.contains("[1]"));
    }

    #[test]
    fn test_prompt_no_status() {
        let p = Prompt::new(false, "!".into(), vec!["dir".into()]);
        let rendered = p.render(1, std::time::Duration::ZERO);
        assert!(rendered.contains('!'));
    }

    #[test]
    fn test_prompt_user_segment() {
        let p = Prompt::new(true, ">".into(), vec!["user".into()]);
        let rendered = p.render(0, std::time::Duration::ZERO);
        assert!(rendered.contains('@'));
    }

    #[test]
    fn test_prompt_time_segment() {
        let p = Prompt::new(true, ">".into(), vec!["time".into()]);
        let rendered = p.render(0, std::time::Duration::ZERO);
        assert!(rendered.contains(':'));
    }

    #[test]
    fn test_prompt_duration_shows_for_slow() {
        let p = Prompt::new(true, ">".into(), vec!["duration".into()]);
        let rendered = p.render(0, std::time::Duration::from_millis(1500));
        assert!(rendered.contains("1.5s"));
    }

    #[test]
    fn test_prompt_duration_hides_for_fast() {
        let p = Prompt::new(true, ">".into(), vec!["duration".into()]);
        let rendered = p.render(0, std::time::Duration::from_millis(50));
        assert!(!rendered.contains("50ms"));
    }

    #[test]
    fn test_git_branch_detection() {
        if let Some(branch) = Prompt::git_branch() {
            assert!(!branch.is_empty());
        }
    }

    #[test]
    fn test_default_segments_include_git_ssh_venv_jobs() {
        let p = Prompt::default();
        assert!(p.segments.contains(&"git".to_string()));
        assert!(p.segments.contains(&"ssh".to_string()));
        assert!(p.segments.contains(&"venv".to_string()));
        assert!(p.segments.contains(&"jobs".to_string()));
    }
}
