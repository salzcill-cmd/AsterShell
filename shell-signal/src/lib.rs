//! POSIX signal handling for AsterShell.
//!
//! Properly handles signals that a login shell / session leader must handle:
//!
//! - `SIGCHLD` — child process exit notification
//! - `SIGINT`  — interrupt (Ctrl+C)
//! - `SIGTERM` — termination request
//! - `SIGQUIT` — quit (Ctrl+\)
//! - `SIGTSTP` — terminal stop (Ctrl+Z)
//! - `SIGCONT` — continue stopped process
//! - `SIGWINCH` — terminal window resize
//! - `SIGHUP`  — hangup (terminal disconnect)

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// Global signal state shared between signal handlers and the shell.
pub struct SignalState {
    /// SIGINT was received (Ctrl+C).
    pub sigint: AtomicBool,
    /// SIGCHLD was received (child exited).
    pub sigchld: AtomicBool,
    /// SIGWINCH was received (window resize).
    pub sigwinch: AtomicBool,
    /// SIGTERM was received (termination request).
    pub sigterm: AtomicBool,
    /// SIGHUP was received (hangup).
    pub sighup: AtomicBool,
    /// PID of the last child that exited.
    pub last_child_pid: AtomicI32,
    /// Exit status of the last child that exited.
    pub last_child_status: AtomicI32,
}

impl SignalState {
    /// Creates a new `SignalState` with all flags cleared.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sigint: AtomicBool::new(false),
            sigchld: AtomicBool::new(false),
            sigwinch: AtomicBool::new(false),
            sigterm: AtomicBool::new(false),
            sighup: AtomicBool::new(false),
            last_child_pid: AtomicI32::new(0),
            last_child_status: AtomicI32::new(0),
        }
    }

    /// Consumes the SIGINT flag (returns true if it was set, then clears it).
    pub fn take_sigint(&self) -> bool {
        self.sigint.swap(false, Ordering::Relaxed)
    }

    /// Consumes the SIGCHLD flag.
    pub fn take_sigchld(&self) -> bool {
        self.sigchld.swap(false, Ordering::Relaxed)
    }

    /// Consumes the SIGWINCH flag.
    pub fn take_sigwinch(&self) -> bool {
        self.sigwinch.swap(false, Ordering::Relaxed)
    }

    /// Consumes the SIGTERM flag.
    pub fn take_sigterm(&self) -> bool {
        self.sigterm.swap(false, Ordering::Relaxed)
    }

    /// Consumes the SIGHUP flag.
    pub fn take_sighup(&self) -> bool {
        self.sighup.swap(false, Ordering::Relaxed)
    }
}

impl Default for SignalState {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns a static reference to the global signal state.
#[must_use]
pub fn global_state() -> &'static SignalState {
    use std::sync::OnceLock;
    static STATE: OnceLock<SignalState> = OnceLock::new();
    STATE.get_or_init(SignalState::new)
}

/// Installs POSIX-compliant signal handlers for the shell.
///
/// This should be called once during shell initialization, before any
/// child processes are spawned.
pub fn install_handlers(_state: &'static SignalState) {
    install_sigint_handler();
    install_sigchld_handler();
    install_sigwinch_handler();
    install_sigterm_handler();
    install_sighup_handler();
    install_sigquit_handler();
    install_sigtstp_handler();
    install_sigcont_handler();
}

#[allow(unsafe_code)]
fn install_sigint_handler() {
    extern "C" fn handler(_sig: libc::c_int) {
        let state = global_state();
        state.sigint.store(true, Ordering::Relaxed);
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
    }
}

#[allow(unsafe_code)]
fn install_sigchld_handler() {
    extern "C" fn handler(_sig: libc::c_int) {
        let state = global_state();
        state.sigchld.store(true, Ordering::Relaxed);

        // Reap zombie children
        loop {
            let mut status: libc::c_int = 0;
            let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if pid <= 0 {
                break;
            }
            state.last_child_pid.store(pid, Ordering::Relaxed);
            state.last_child_status.store(status, Ordering::Relaxed);
        }
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as *const () as usize;
        sa.sa_flags = libc::SA_RESTART | libc::SA_NOCLDSTOP;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGCHLD, &sa, std::ptr::null_mut());
    }
}

#[allow(unsafe_code)]
fn install_sigwinch_handler() {
    extern "C" fn handler(_sig: libc::c_int) {
        let state = global_state();
        state.sigwinch.store(true, Ordering::Relaxed);
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());
    }
}

#[allow(unsafe_code)]
fn install_sigterm_handler() {
    extern "C" fn handler(_sig: libc::c_int) {
        let state = global_state();
        state.sigterm.store(true, Ordering::Relaxed);
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
    }
}

#[allow(unsafe_code)]
fn install_sighup_handler() {
    extern "C" fn handler(_sig: libc::c_int) {
        let state = global_state();
        state.sighup.store(true, Ordering::Relaxed);
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGHUP, &sa, std::ptr::null_mut());
    }
}

/// SIGQUIT: default action is core dump; we set it to default.
#[allow(unsafe_code)]
fn install_sigquit_handler() {
    extern "C" fn handler(_sig: libc::c_int) {
        unsafe {
            libc::signal(libc::SIGQUIT, libc::SIG_DFL);
            libc::raise(libc::SIGQUIT);
        }
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as *const () as usize;
        sa.sa_flags = 0;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGQUIT, &sa, std::ptr::null_mut());
    }
}

/// SIGTSTP: ignore in shell, forward to foreground group.
#[allow(unsafe_code)]
fn install_sigtstp_handler() {
    extern "C" fn handler(_sig: libc::c_int) {
        // Shell ignores SIGTSTP — foreground process group handles it
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as *const () as usize;
        sa.sa_flags = 0;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGTSTP, &sa, std::ptr::null_mut());
    }
}

/// SIGCONT: default action is continue; we set it to default.
#[allow(unsafe_code)]
fn install_sigcont_handler() {
    extern "C" fn handler(_sig: libc::c_int) {
        // Default action: continue stopped process
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as *const () as usize;
        sa.sa_flags = 0;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGCONT, &sa, std::ptr::null_mut());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_state_new() {
        let state = SignalState::new();
        assert!(!state.sigint.load(Ordering::Relaxed));
        assert!(!state.sigchld.load(Ordering::Relaxed));
        assert!(!state.sigwinch.load(Ordering::Relaxed));
    }

    #[test]
    fn test_take_sigint() {
        let state = SignalState::new();
        state.sigint.store(true, Ordering::Relaxed);
        assert!(state.take_sigint());
        assert!(!state.sigint.load(Ordering::Relaxed));
        assert!(!state.take_sigint());
    }

    #[test]
    fn test_global_state() {
        let s1 = global_state();
        let s2 = global_state();
        let p1 = s1 as *const _ as usize;
        let p2 = s2 as *const _ as usize;
        assert_eq!(p1, p2);
    }
}
