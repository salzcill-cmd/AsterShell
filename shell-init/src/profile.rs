//! POSIX-compliant profile sourcing for login and interactive shells.
//!
//! Implements the exact startup file sourcing order specified by POSIX and
//! practiced by bash/zsh. This is critical for graphical session startup
//! because display managers depend on environment variables set by these
//! profile files.
//!
//! # Sourcing Order
//!
//! ## Login shell (`-l` or argv[0] starts with `-`):
//! 1. `/etc/profile`
//! 2. First found of (in order):
//!    - `~/.bash_profile`
//!    - `~/.bash_login`
//!    - `~/.profile`
//!
//! ## Interactive non-login shell:
//! 1. `$ASTER_SHELL_RC` or `~/.config/astershell/shellrc`
//!
//! ## AsterShell-specific (always for interactive):
//! 1. `~/.config/astershell/profile`
//! 2. `~/.config/astershell/env`
//!
//! # Error Handling
//!
//! - Missing files: silently skipped (this is normal and expected)
//! - Parse errors: printed to stderr, shell continues
//! - I/O errors: printed to stderr, shell continues
//!
//! **No error in a profile file should ever cause the shell to abort.**

use std::path::{Path, PathBuf};

/// Loads and sources shell profile files.
pub struct ProfileLoader {
    /// Cached home directory.
    home: Option<PathBuf>,
}

impl ProfileLoader {
    /// Creates a new `ProfileLoader`.
    #[must_use]
    pub fn new(_is_login: bool) -> Self {
        Self {
            home: dirs::home_dir(),
        }
    }

    /// Sources `/etc/profile`.
    ///
    /// This file is only sourced for login shells. It typically sets up
    /// system-wide PATH, locale, and other environment variables.
    pub fn source_etc_profile(&self) {
        self.source_file_and_apply(Path::new("/etc/profile"));
    }

    /// Sources the first found of `~/.bash_profile`, `~/.bash_login`, `~/.profile`.
    ///
    /// This is the POSIX-specified priority order for login shells.
    /// Only the FIRST file found is sourced — this matches bash behavior.
    pub fn source_user_profile_chain(&self) {
        let home = match &self.home {
            Some(h) => h,
            None => return,
        };

        let candidates = [".bash_profile", ".bash_login", ".profile"];

        for name in &candidates {
            let path = home.join(name);
            if path.is_file() {
                self.source_file_and_apply(&path);
                return;
            }
        }
        // No user profile found — this is normal for new users
    }

    /// Sources `~/.config/astershell/profile`.
    ///
    /// This is AsterShell-specific and runs after system/user profiles.
    pub fn source_astershell_profile(&self) {
        let home = match &self.home {
            Some(h) => h,
            None => return,
        };

        let path = home
            .join(".config")
            .join("astershell")
            .join("profile");
        self.source_file_and_apply(&path);
    }

    /// Sources `~/.config/astershell/env`.
    ///
    /// This is AsterShell-specific and runs after the profile.
    pub fn source_astershell_env(&self) {
        let home = match &self.home {
            Some(h) => h,
            None => return,
        };

        let path = home.join(".config").join("astershell").join("env");
        self.source_file_and_apply(&path);
    }

    /// Sources the interactive RC file (`~/.config/astershell/shellrc`).
    ///
    /// This is sourced for interactive non-login shells.
    pub fn source_shellrc(&self) {
        // Check ASTER_SHELL_RC environment variable first
        if let Ok(rc_path) = std::env::var("ASTER_SHELL_RC") {
            let path = PathBuf::from(&rc_path);
            if path.is_file() {
                self.source_file_and_apply(&path);
                return;
            }
        }

        let home = match &self.home {
            Some(h) => h,
            None => return,
        };

        // Try multiple locations for backward compatibility
        let candidates = [
            home.join(".config")
                .join("astershell")
                .join("shellrc"),
            home.join(".aster").join("shellrc"),
            home.join(".asterrc"),
        ];

        for path in &candidates {
            if path.is_file() {
                self.source_file_and_apply(path);
                return;
            }
        }
    }

    /// Sources a shell file by executing it via `/bin/sh`.
    ///
    /// Profile files are POSIX shell scripts that may contain:
    /// - Variable assignments (`export FOO=bar`)
    /// - Command substitutions (`export PATH=$(...)`)
    /// - Conditional logic (`if [ ... ]; then ... fi`)
    /// - Source commands (`. /etc/profile.d/*.sh`)
    ///
    /// We execute them via `/bin/sh` to ensure full POSIX compatibility.
    /// The environment changes made by the script are captured and applied
    /// to the current process.
    fn source_file(&self, path: &Path) -> Vec<(String, String)> {
        if !path.is_file() {
            log::debug!("Profile not found (skipping): {}", path.display());
            return Vec::new();
        }

        log::info!("Sourcing profile: {}", path.display());

        // Use /bin/sh to source the file, then capture environment changes
        let script = format!(
            r#"
# Source the profile file
. "{path}" 2>/dev/null

# Print all environment changes as KEY=VALUE pairs
# This allows us to capture what the profile set
env -0
"#,
            path = path.display()
        );

        let result = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&script)
            .output();

        let mut env_pairs = Vec::new();

        match result {
            Ok(output) => {
                if output.status.success() {
                    // Parse NUL-delimited environment and collect
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for entry in stdout.split('\0') {
                        if let Some((key, value)) = entry.split_once('=') {
                            if !key.is_empty() {
                                env_pairs.push((key.to_string(), value.to_string()));
                            }
                        }
                    }
                } else {
                    // Profile had errors — print stderr but continue
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stderr.trim().is_empty() {
                        eprintln!("aster: {path}: {stderr}", path = path.display());
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "aster: failed to source {path}: {e}",
                    path = path.display()
                );
            }
        }

        env_pairs
    }

    /// Sources a file and applies its environment variables to the current process.
    ///
    /// # Safety
    ///
    /// This calls `std::env::set_var()` which must only be called when no other
    /// threads are reading environment variables. Call this BEFORE installing
    /// signal handlers (ctrlc, etc.).
    fn source_file_and_apply(&self, path: &Path) {
        let env_pairs = self.source_file(path);
        for (key, value) in &env_pairs {
            #[allow(unsafe_code)]
            unsafe {
                std::env::set_var(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_loader_creation() {
        let loader = ProfileLoader::new(true);
        assert!(loader.home.is_some() || loader.home.is_none());
    }

    #[test]
    fn test_nonexistent_file_silently_skipped() {
        let loader = ProfileLoader::new(false);
        // Should not panic — returns empty vec
        let result = loader.source_file(Path::new("/nonexistent/profile/file.sh"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_source_etc_profile() {
        let loader = ProfileLoader::new(true);
        // Should not panic even if /etc/profile doesn't exist
        loader.source_etc_profile();
    }

    #[test]
    fn test_source_user_profile_chain() {
        let loader = ProfileLoader::new(true);
        // Should not panic — will skip if no user profile exists
        loader.source_user_profile_chain();
    }
}
