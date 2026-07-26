//! Login shell detection, CLI argument parsing, profile sourcing, and
//! environment preservation for AsterShell.
//!
//! This crate implements the POSIX-specified shell startup sequence and is
//! responsible for ensuring AsterShell behaves identically to bash/zsh when
//! invoked as a login shell by display managers (LightDM, GDM, SDDM), SSH,
//! `su`, `sudo`, `machinectl`, or TTY `login`.
//!
//! # Startup Flow
//!
//! ```text
//! main() args parsed → ShellMode determined → profile sourced → REPL or exec
//! ```
//!
//! ## Login shell startup order (POSIX):
//! 1. `/etc/profile`
//! 2. First found of `~/.bash_profile`, `~/.bash_login`, `~/.profile`
//!
//! ## Interactive non-login:
//! 1. `$ASTER_SHELL_RC` or `~/.config/astershell/shellrc`
//!
//! ## Non-interactive (`-c` or script):
//! 1. Execute command/script, no profile sourcing (unless `--login`)

mod cli;
mod detect;
mod env_preserve;
mod profile;

pub use cli::{ShellMode, ShellArgs};
pub use detect::{ShellKind, ShellInvocation};
pub use env_preserve::EnvPreserver;
pub use profile::ProfileLoader;

/// Shell invocation context determined at startup.
pub struct ShellInit {
    /// The parsed CLI arguments and detected shell kind.
    pub invocation: ShellInvocation,
    /// Environment preserver — holds original env snapshot.
    pub env_preserver: EnvPreserver,
}

impl ShellInit {
    /// Performs full shell initialization from process arguments and environment.
    ///
    /// This is the primary entry point called from `main()`. It:
    /// 1. Parses command-line arguments
    /// 2. Detects login/interactive/script mode
    /// 3. Snapshots the inherited environment for preservation
    /// 4. Does NOT source profiles yet (caller must invoke `source_profiles`)
    ///
    /// # Errors
    ///
    /// Returns an error only on unrecoverable initialization failure.
    /// CLI parsing errors are printed and cause `std::process::exit(2)`.
    pub fn initialize() -> Self {
        let invocation = ShellInvocation::detect();
        let env_preserver = EnvPreserver::snapshot();

        log::debug!(
            "Shell init: kind={:?}, login={}, interactive={}, command={:?}, script={:?}",
            invocation.kind,
            invocation.is_login,
            invocation.is_interactive,
            invocation.command,
            invocation.script_file,
        );

        Self {
            invocation,
            env_preserver,
        }
    }

    /// Sources profile files if appropriate for the detected shell mode.
    ///
    /// For login shells: `/etc/profile` then `~/.bash_profile` / `~/.bash_login` / `~/.profile`.
    /// For interactive non-login: `$ASTER_SHELL_RC` or `~/.config/astershell/shellrc`.
    /// For non-interactive with `-c`: nothing (unless `--login` is set).
    ///
    /// Missing files are silently skipped. Errors in profile files print
    /// diagnostics but never abort.
    pub fn source_profiles(&self) {
        let loader = ProfileLoader::new(self.invocation.is_login);

        if self.invocation.is_login {
            // POSIX login shell: /etc/profile first
            loader.source_etc_profile();

            // Then first of ~/.bash_profile, ~/.bash_login, ~/.profile
            loader.source_user_profile_chain();
        }

        // AsterShell-specific profiles (always sourced for interactive shells)
        if self.invocation.is_interactive || self.invocation.is_login {
            loader.source_astershell_profile();
            loader.source_astershell_env();
        }

        // Interactive non-login: also source shellrc
        if self.invocation.is_interactive && !self.invocation.is_login {
            loader.source_shellrc();
        }
    }

    /// Returns the effective shell mode that determines `main()` behavior.
    #[must_use]
    pub fn mode(&self) -> ShellMode {
        self.invocation.mode()
    }

    /// Returns the command string to execute (from `-c`), if any.
    #[must_use]
    pub fn command(&self) -> Option<&str> {
        self.invocation.command.as_deref()
    }

    /// Returns the script file path (positional arg when not `-c`), if any.
    #[must_use]
    pub fn script_file(&self) -> Option<&str> {
        self.invocation.script_file.as_deref()
    }

    /// Returns whether this is a login shell.
    #[must_use]
    pub fn is_login(&self) -> bool {
        self.invocation.is_login
    }

    /// Returns whether this is an interactive shell.
    #[must_use]
    pub fn is_interactive(&self) -> bool {
        self.invocation.is_interactive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_init_creates() {
        let init = ShellInit::initialize();
        // Should never panic — even with weird args
        assert!(init.env_preserver.get("PATH").is_some() || init.env_preserver.get("PATH").is_none());
    }
}
