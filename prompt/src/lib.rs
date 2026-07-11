//! Prompt rendering for the interactive shell.
//!
//! Produces a multi-segment, ANSI-colored prompt showing username,
//! hostname, current directory, git branch, exit status, and more.

use std::path::PathBuf;

use nu_ansi_term::{Color as AnsiColor, Style};

/// Renders the shell prompt.
#[derive(Debug, Clone)]
pub struct Prompt {
    /// Whether to show the exit-status indicator.
    pub show_status: bool,
    /// The prompt symbol (e.g. `❯`).
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

    /// Renders the full prompt string for the given last exit code.
    #[must_use]
    pub fn render(&self, last_exit_code: i32) -> String {
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
                    if let Some(branch) = Self::git_branch() {
                        result.push_str(&self.render_git(&branch));
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
                _ => {}
            }
        }

        // If no segments produced anything, use legacy rendering
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

    fn render_git(&self, branch: &str) -> String {
        let style = Style::new().fg(AnsiColor::Magenta);
        format!(" {}", style.paint(format!("({branch})")))
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

    fn render_symbol(&self) -> String {
        let style = Style::new().fg(AnsiColor::Magenta).bold();
        format!("{} ", style.paint(&self.symbol))
    }

    fn render_legacy(&self, last_exit_code: i32) -> String {
        let dir = Self::cwd_display();

        let status = if self.show_status && last_exit_code != 0 {
            let style = Style::new().fg(AnsiColor::Red).bold();
            format!(" {}", style.paint("\u{2717}"))
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
            symbol: "\u{276f}".into(),
            segments: vec!["status".into(), "dir".into()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_render_success() {
        let p = Prompt::default();
        let rendered = p.render(0);
        assert!(rendered.contains('\u{276f}'));
    }

    #[test]
    fn test_prompt_render_failure() {
        let p = Prompt::default();
        let rendered = p.render(1);
        assert!(rendered.contains('\u{2717}') || rendered.contains("[1]"));
    }

    #[test]
    fn test_prompt_no_status() {
        let p = Prompt::new(false, "!".into(), vec!["dir".into()]);
        let rendered = p.render(1);
        assert!(rendered.contains('!'));
    }

    #[test]
    fn test_prompt_user_segment() {
        let p = Prompt::new(true, ">".into(), vec!["user".into()]);
        let rendered = p.render(0);
        assert!(rendered.contains('@'));
    }

    #[test]
    fn test_prompt_time_segment() {
        let p = Prompt::new(true, ">".into(), vec!["time".into()]);
        let rendered = p.render(0);
        assert!(rendered.contains(':'));
    }
}
