//! Shell invocation detection.
//!
//! Determines whether AsterShell is being invoked as a login shell,
//! interactive shell, non-interactive shell, or script by examining:
//!
//! - `argv[0]` prefix (`-` indicates login shell)
//! - `isatty(STDIN_FILENO)` for interactive detection
//! - `SHELL` environment variable
//! - Parent process identity (SSH, systemd, display manager)
//! - `XDG_SESSION_TYPE` and `XDG_SESSION_CLASS`
//! - `SSH_CONNECTION`, `SSH_CLIENT`, `SSH_TTY`
//! - `TERM` variable

use std::env;
use std::io::IsTerminal;

use crate::cli::ShellMode;

/// The detected kind of shell invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    /// Login shell (argv[0] starts with `-`).
    Login,
    /// Interactive non-login shell (tty attached, no `-` prefix).
    Interactive,
    /// Non-interactive command execution (`-c` flag).
    Command,
    /// Non-interactive script execution (file argument).
    Script,
    /// Reading from stdin (`-s` flag or piped input).
    Stdin,
}

/// Complete information about how the shell was invoked.
#[derive(Debug, Clone)]
pub struct ShellInvocation {
    /// The kind of shell invocation.
    pub kind: ShellKind,
    /// Whether this is a login shell.
    pub is_login: bool,
    /// Whether this is an interactive shell.
    pub is_interactive: bool,
    /// Command from `-c` flag, if any.
    pub command: Option<String>,
    /// Script file path, if any.
    pub script_file: Option<String>,
    /// Positional arguments.
    pub positional_args: Vec<String>,
    /// Whether we're in an SSH session.
    pub is_ssh: bool,
    /// Whether we're in a graphical session context.
    pub is_graphical: bool,
    /// Parent process name (if detectable).
    pub parent_process: Option<String>,
}

impl ShellInvocation {
    /// Detects the shell invocation from process arguments and environment.
    #[must_use]
    pub fn detect() -> Self {
        let args = crate::cli::ShellArgs::parse();
        let mode = args.mode();

        // Detect login shell from argv[0] prefix
        let is_login_from_argv0 = Self::is_login_argv0(&args.argv0);
        let is_login = is_login_from_argv0 || args.login_flag;

        let is_interactive = Self::detect_interactive(mode);
        let is_ssh = Self::detect_ssh();
        let is_graphical = Self::detect_graphical();
        let parent_process = Self::detect_parent_process();

        let kind = match mode {
            ShellMode::Command => ShellKind::Command,
            ShellMode::Script => ShellKind::Script,
            ShellMode::Stdin => ShellKind::Stdin,
            ShellMode::Interactive => {
                if is_login {
                    ShellKind::Login
                } else {
                    ShellKind::Interactive
                }
            }
        };

        Self {
            kind,
            is_login,
            is_interactive,
            command: args.command,
            script_file: args.script_file,
            positional_args: args.positional_args,
            is_ssh,
            is_graphical,
            parent_process,
        }
    }

    /// Returns the effective shell mode for the REPL loop.
    #[must_use]
    pub fn mode(&self) -> ShellMode {
        match self.kind {
            ShellKind::Command => ShellMode::Command,
            ShellKind::Script => ShellMode::Script,
            ShellKind::Stdin => ShellMode::Stdin,
            ShellKind::Login | ShellKind::Interactive => ShellMode::Interactive,
        }
    }

    /// Checks if `argv[0]` starts with `-`, indicating a login shell.
    ///
    /// POSIX spec: When a shell is invoked as a login shell, the value of
    /// `argv[0]` shall be a hyphen character (`-`). Display managers and
    /// `login(1)` set this to `-bash`, `-zsh`, etc.
    fn is_login_argv0(argv0: &str) -> bool {
        // Check the basename of argv[0]
        let basename = std::path::Path::new(argv0)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(argv0);

        basename.starts_with('-')
    }

    /// Determines if the shell should be interactive.
    ///
    /// A shell is interactive when:
    /// 1. `-c` was NOT provided (no command to execute)
    /// 2. A script file was NOT provided
    /// 3. stdin is a terminal (isatty) OR `-s` was used
    ///
    /// Bash's rule: interactive = (no -c, no script) AND (isatty OR -s)
    fn detect_interactive(mode: ShellMode) -> bool {
        match mode {
            ShellMode::Interactive => {
                // True interactive only if stdin is a TTY or forced with -s
                std::io::stdin().is_terminal()
            }
            ShellMode::Stdin => true,
            ShellMode::Command | ShellMode::Script => false,
        }
    }

    /// Detects SSH session from environment variables.
    fn detect_ssh() -> bool {
        env::var("SSH_CONNECTION").is_ok()
            || env::var("SSH_CLIENT").is_ok()
            || env::var("SSH_TTY").is_ok()
    }

    /// Detects graphical session context from environment variables.
    fn detect_graphical() -> bool {
        env::var("DISPLAY").is_ok()
            || env::var("WAYLAND_DISPLAY").is_ok()
            || env::var("XDG_SESSION_TYPE")
                .map(|t| t == "x11" || t == "wayland")
                .unwrap_or(false)
    }

    /// Attempts to detect the parent process name.
    fn detect_parent_process() -> Option<String> {
        // Try reading /proc/self/status for PPid
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let ppid_line = status.lines().find(|l| l.starts_with("PPid:"))?;
        let ppid_str = ppid_line.split_whitespace().nth(1)?;
        let ppid: u32 = ppid_str.parse().ok()?;

        // Read parent's comm
        let comm_path = format!("/proc/{ppid}/comm");
        let comm = std::fs::read_to_string(&comm_path).ok()?;
        Some(comm.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_argv0_dash() {
        assert!(ShellInvocation::is_login_argv0("-bash"));
        assert!(ShellInvocation::is_login_argv0("-aster"));
        assert!(ShellInvocation::is_login_argv0("/bin/-bash"));
    }

    #[test]
    fn test_not_login_argv0() {
        assert!(!ShellInvocation::is_login_argv0("aster"));
        assert!(!ShellInvocation::is_login_argv0("/usr/bin/bash"));
        assert!(!ShellInvocation::is_login_argv0("bash"));
    }

    #[test]
    fn test_interactive_detection() {
        // -c mode → not interactive
        assert!(!ShellInvocation::detect_interactive(ShellMode::Command));
        // script mode → not interactive
        assert!(!ShellInvocation::detect_interactive(ShellMode::Script));
        // -s mode → interactive
        assert!(ShellInvocation::detect_interactive(ShellMode::Stdin));
    }
}
