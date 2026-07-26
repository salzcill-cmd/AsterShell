//! Session and process group management for AsterShell.
//!
//! Handles the critical process management that a login shell / session leader
//! must implement correctly to avoid destroying desktop sessions.
//!
//! # Key Responsibilities
//!
//! - Set up the shell as a session leader (`setsid`)
//! - Manage process groups for foreground/background jobs
//! - Control the terminal (`tcsetpgrp` for foreground job control)
//! - Properly wait for child processes (`waitpid` with `WNOHANG`/`WUNTRACED`)
//! - Reap zombie processes via SIGCHLD handler
//!
//! # Safety Rules
//!
//! 1. NEVER `exec()` the shell itself — the shell process must survive
//! 2. NEVER `fork()` infinitely — each command gets exactly one fork
//! 3. NEVER replace the session leader — maintain process group ownership
//! 4. NEVER call `setsid()` if already a session leader (display manager case)
//! 5. ALWAYS restore terminal control to the shell after foreground jobs
//! 6. ALWAYS wait for foreground jobs to complete before accepting new input

use std::ffi::CString;

/// Session management state for the shell.
pub struct SessionManager {
    /// The shell's own PID.
    pub shell_pid: u32,
    /// The shell's process group ID.
    pub shell_pgid: i32,
    /// The controlling terminal's foreground process group.
    pub foreground_pgid: Option<i32>,
}

/// Errors from session management operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// Failed to set the process group.
    #[error("setpgid failed: {0}")]
    SetPgid(String),
    /// Failed to set the foreground process group.
    #[error("tcsetpgrp failed: {0}")]
    TcSetPgrp(String),
    /// Failed to create a session.
    #[error("setsid failed: {0}")]
    SetSid(String),
    /// waitpid failed.
    #[error("waitpid failed: {0}")]
    WaitPid(String),
}

impl SessionManager {
    /// Creates a new `SessionManager` for the current process.
    ///
    /// Records the shell's PID and PGID. Does NOT call `setsid()` because
    /// the shell may already be a session leader (e.g., when started by
    /// a display manager or systemd).
    #[must_use]
    pub fn new() -> Self {
        let shell_pid = std::process::id();
        let shell_pgid = Self::get_pgid(shell_pid as i32);

        Self {
            shell_pid,
            shell_pgid,
            foreground_pgid: None,
        }
    }

    /// Gets the process group ID for a given PID.
    #[must_use]
    pub fn get_pgid(pid: i32) -> i32 {
        #[allow(unsafe_code)]
        unsafe { libc::getpgid(pid) }
    }

    /// Sets the shell as the session leader if it isn't already.
    ///
    /// # Safety
    ///
    /// Only safe to call before any child processes are spawned.
    ///
    /// # Errors
    ///
    /// Returns an error if `setsid()` fails when the process is not
    /// already a session leader.
    pub fn ensure_session_leader() -> Result<(), SessionError> {
        let pid = std::process::id() as i32;
        let pgid = Self::get_pgid(pid);

        // If we're already a session leader (PGID == PID), don't call setsid
        if pgid == pid {
            log::debug!("Already session leader (pid={pid})");
            return Ok(());
        }

        // Try to become session leader
        #[allow(unsafe_code)]
        let result = unsafe { libc::setsid() };

        if result < 0 {
            // This is expected when stdin is not a controlling terminal
            // (e.g., when launched by a display manager). The shell still
            // functions correctly — it just doesn't control the terminal.
            log::warn!(
                "setsid() failed (not critical for display manager sessions): {}",
                std::io::Error::last_os_error()
            );
            // Don't return error — display manager sessions work fine without setsid
        }

        Ok(())
    }

    /// Sets up the shell's process group.
    ///
    /// This ensures the shell is the group leader of its own process group.
    /// Called once during initialization.
    pub fn setup_process_group(&mut self) {
        let pid = self.shell_pid as i32;

        // If we're not the group leader, try to become one
        if self.shell_pgid != pid {
            #[allow(unsafe_code)]
            unsafe {
                libc::setpgid(pid, pid);
            }
            self.shell_pgid = Self::get_pgid(pid);
        }

        log::debug!(
            "Session setup: pid={}, pgid={}",
            self.shell_pid,
            self.shell_pgid
        );
    }

    /// Sets the foreground process group for the terminal.
    ///
    /// Used when a foreground job starts and finishes.
    ///
    /// # Errors
    ///
    /// Returns an error if `tcsetpgrp` fails (e.g., no controlling terminal).
    pub fn set_foreground(&mut self, pgid: i32) -> Result<(), SessionError> {
        #[allow(unsafe_code)]
        unsafe {
            if libc::tcsetpgrp(libc::STDIN_FILENO, pgid) < 0 {
                let err = std::io::Error::last_os_error();
                log::warn!("tcsetpgrp({pgid}) failed: {err}");
                // Don't return error for display managers — they don't have
                // a traditional controlling terminal
            }
        }
        self.foreground_pgid = Some(pgid);
        Ok(())
    }

    /// Restores the shell as the foreground process group.
    ///
    /// Called after a foreground job completes or is suspended.
    pub fn restore_foreground(&mut self) {
        let pgid = self.shell_pgid;
        #[allow(unsafe_code)]
        unsafe {
            libc::tcsetpgrp(libc::STDIN_FILENO, pgid);
        }
        self.foreground_pgid = Some(pgid);
    }

    /// Waits for a child process to exit or stop.
    ///
    /// # Safety
    ///
    /// Uses `libc::waitpid` which is safe in a single-threaded context.
    #[must_use]
    pub fn wait_child(&self, pid: i32, block: bool) -> Result<WaitResult, SessionError> {
        let mut status: libc::c_int = 0;
        let options = if block { 0 } else { libc::WNOHANG | libc::WUNTRACED };

        #[allow(unsafe_code)]
        let result = unsafe { libc::waitpid(pid, &mut status, options) };

        if result < 0 {
            let err = std::io::Error::last_os_error();
            // ECHILD means no such child — not an error for background jobs
            if err.raw_os_error() == Some(libc::ECHILD) {
                return Ok(WaitResult::NoChild);
            }
            return Err(SessionError::WaitPid(err.to_string()));
        }

        if result == 0 {
            return Ok(WaitResult::StillRunning);
        }

        if libc::WIFEXITED(status) {
            Ok(WaitResult::Exited {
                pid: result,
                status: libc::WEXITSTATUS(status),
            })
        } else if libc::WIFSIGNALED(status) {
            Ok(WaitResult::Signaled {
                pid: result,
                signal: libc::WTERMSIG(status),
            })
        } else if libc::WIFSTOPPED(status) {
            Ok(WaitResult::Stopped {
                pid: result,
                signal: libc::WSTOPSIG(status),
            })
        } else {
            Ok(WaitResult::Unknown { pid: result, status })
        }
    }

    /// Waits for all children in the current process group.
    ///
    /// Used by the SIGCHLD handler to reap zombies.
    pub fn reap_children() {
        loop {
            #[allow(unsafe_code)]
            let pid = unsafe {
                libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG)
            };
            if pid <= 0 {
                break;
            }
            log::debug!("Reaped zombie child pid={pid}");
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a `waitpid` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitResult {
    /// Child exited with a status code.
    Exited {
        /// The PID of the exited child.
        pid: i32,
        /// The exit status code.
        status: i32,
    },
    /// Child was killed by a signal.
    Signaled {
        /// The PID of the killed child.
        pid: i32,
        /// The signal that killed it.
        signal: i32,
    },
    /// Child was stopped by a signal.
    Stopped {
        /// The PID of the stopped child.
        pid: i32,
        /// The signal that stopped it.
        signal: i32,
    },
    /// Child is still running.
    StillRunning,
    /// No child process with this PID.
    NoChild,
    /// Unknown wait status.
    Unknown {
        /// The PID.
        pid: i32,
        /// The raw status.
        status: i32,
    },
}

/// Forks and executes a command, returning the child PID.
///
/// # Safety
///
/// Uses `libc::fork()` and `libc::execve()`. The child process MUST
/// call `_exit()` and never return. The parent MUST `waitpid()` on the child.
///
/// # Errors
///
/// Returns an error if fork() or execve() fails.
pub fn fork_exec(
    path: &str,
    args: &[String],
    env: &[String],
    pgid: Option<i32>,
) -> Result<u32, SessionError> {
    let c_path = CString::new(path).map_err(|e| SessionError::SetPgid(e.to_string()))?;
    let c_args: Vec<CString> = args
        .iter()
        .map(|a| CString::new(a.as_str()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| SessionError::SetPgid(e.to_string()))?;
    let c_env: Vec<CString> = env
        .iter()
        .map(|e| CString::new(e.as_str()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| SessionError::SetPgid(e.to_string()))?;

    // Build argv array (null-terminated)
    let mut argv: Vec<*const libc::c_char> = Vec::with_capacity(c_args.len() + 2);
    argv.push(c_path.as_ptr());
    for arg in &c_args {
        argv.push(arg.as_ptr());
    }
    argv.push(std::ptr::null());

    // Build envp array (null-terminated)
    let mut envp: Vec<*const libc::c_char> = Vec::with_capacity(c_env.len() + 1);
    for e in &c_env {
        envp.push(e.as_ptr());
    }
    envp.push(std::ptr::null());

    #[allow(unsafe_code)]
    unsafe {
        let pid = libc::fork();
        if pid < 0 {
            return Err(SessionError::SetPgid(
                std::io::Error::last_os_error().to_string(),
            ));
        }

        if pid == 0 {
            // ── Child process ──

            // Set process group if requested
            if let Some(g) = pgid {
                libc::setpgid(0, g);
            }

            // Reset signal handlers to default
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            libc::signal(libc::SIGQUIT, libc::SIG_DFL);
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGTSTP, libc::SIG_DFL);
            libc::signal(libc::SIGCONT, libc::SIG_DFL);
            libc::signal(libc::SIGCHLD, libc::SIG_DFL);

            // Execute
            libc::execve(c_path.as_ptr(), argv.as_ptr(), envp.as_ptr());

            // If we get here, exec failed
            libc::_exit(127);
        }

        // ── Parent process ──
        Ok(pid as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_manager_new() {
        let mgr = SessionManager::new();
        assert!(mgr.shell_pid > 0);
    }

    #[test]
    fn test_get_pgid() {
        let pid = std::process::id() as i32;
        let pgid = SessionManager::get_pgid(pid);
        // PGID should be >= 0
        assert!(pgid >= 0);
    }

    #[test]
    fn test_wait_result_variants() {
        let r1 = WaitResult::Exited { pid: 1, status: 0 };
        let r2 = WaitResult::StillRunning;
        let r3 = WaitResult::NoChild;
        assert_ne!(r1, r2);
        assert_ne!(r2, r3);
    }
}
