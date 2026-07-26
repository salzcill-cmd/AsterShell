//! Environment variable preservation.
//!
//! Snapshots the inherited process environment at startup and provides
//! methods to check whether critical variables are preserved. This prevents
//! the shell from accidentally destroying variables set by PAM, systemd,
//! or display managers.
//!
//! # Critical Variables
//!
//! The following variables MUST never be removed or overwritten by the shell
//! unless explicitly requested by the user via `export` or `unset`:
//!
//! - `DISPLAY`, `WAYLAND_DISPLAY` — graphical session
//! - `XDG_RUNTIME_DIR` — systemd user session
//! - `XDG_SESSION_TYPE`, `XDG_SESSION_CLASS` — session metadata
//! - `XDG_CURRENT_DESKTOP`, `XDG_SESSION_DESKTOP` — desktop identity
//! - `DBUS_SESSION_BUS_ADDRESS` — D-Bus session
//! - `HOME`, `USER`, `LOGNAME`, `SHELL` — user identity
//! - `PATH`, `TERM`, `COLORTERM` — terminal environment
//! - `LANG`, `LC_*`, `LANGUAGE` — locale
//! - `SSH_AUTH_SOCK`, `SSH_CONNECTION`, `SSH_CLIENT` — SSH
//! - `GTK_USE_PORTAL`, `QT_QPA_PLATFORM`, `MOZ_ENABLE_WAYLAND`, `GDK_BACKEND` — toolkit
//! - `LD_LIBRARY_PATH`, `PKG_CONFIG_PATH` — library paths
//! - `EDITOR`, `VISUAL`, `BROWSER` — user preferences

use std::collections::HashMap;
use std::env;

/// A snapshot of the inherited environment at process startup.
///
/// Used to verify that critical variables are never accidentally removed
/// or corrupted by shell operations.
#[derive(Debug, Clone)]
pub struct EnvPreserver {
    /// The original snapshot of environment variables.
    snapshot: HashMap<String, String>,
}

/// Critical environment variable categories.
const CRITICAL_GRAPHICAL: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_TYPE",
    "XDG_SESSION_CLASS",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_DESKTOP",
    "DBUS_SESSION_BUS_ADDRESS",
];

const CRITICAL_USER: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "PATH",
    "TERM",
    "LANG",
    "LC_ALL",
    "LANGUAGE",
    "PWD",
    "OLDPWD",
];

const CRITICAL_SSH: &[&str] = &[
    "SSH_AUTH_SOCK",
    "SSH_CONNECTION",
    "SSH_CLIENT",
    "SSH_TTY",
    "XAUTHORITY",
];

const CRITICAL_TOOLKIT: &[&str] = &[
    "GTK_USE_PORTAL",
    "QT_QPA_PLATFORM",
    "MOZ_ENABLE_WAYLAND",
    "GDK_BACKEND",
    "LIBGL_DRIVERS_PATH",
    "MESA_LOADER_DRIVER_OVERRIDE",
    "LD_LIBRARY_PATH",
    "PKG_CONFIG_PATH",
];

const CRITICAL_EDITOR: &[&str] = &["EDITOR", "VISUAL", "BROWSER"];

/// All critical variables combined.
fn all_critical() -> Vec<&'static str> {
    let mut vars = Vec::new();
    vars.extend(CRITICAL_GRAPHICAL);
    vars.extend(CRITICAL_USER);
    vars.extend(CRITICAL_SSH);
    vars.extend(CRITICAL_TOOLKIT);
    vars.extend(CRITICAL_EDITOR);
    vars
}

impl EnvPreserver {
    /// Takes a snapshot of the current process environment.
    ///
    /// This MUST be called before any environment modifications.
    #[must_use]
    pub fn snapshot() -> Self {
        let snapshot: HashMap<String, String> = env::vars().collect();
        Self { snapshot }
    }

    /// Gets a value from the original snapshot.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.snapshot.get(name).map(String::as_str)
    }

    /// Checks if a critical variable is still present in the live environment.
    ///
    /// Returns `true` if the variable exists in both the snapshot and the
    /// current environment (i.e., it was preserved).
    #[must_use]
    pub fn is_preserved(&self, name: &str) -> bool {
        if self.snapshot.contains_key(name) {
            env::var(name).is_ok()
        } else {
            // Wasn't in the snapshot, so preservation doesn't apply
            true
        }
    }

    /// Checks whether all critical graphical variables that were inherited
    /// are still present.
    ///
    /// Returns a list of missing variable names (empty = all preserved).
    #[must_use]
    pub fn check_graphical_preservation(&self) -> Vec<String> {
        self.check_category(CRITICAL_GRAPHICAL)
    }

    /// Checks whether all critical user identity variables that were inherited
    /// are still present.
    #[must_use]
    pub fn check_user_preservation(&self) -> Vec<String> {
        self.check_category(CRITICAL_USER)
    }

    /// Checks all critical variables at once.
    ///
    /// Returns a list of any critical variables that were in the original
    /// snapshot but are now missing from the live environment.
    #[must_use]
    pub fn check_all_preservation(&self) -> Vec<String> {
        self.check_category(&all_critical())
    }

    fn check_category(&self, category: &[&str]) -> Vec<String> {
        let mut missing = Vec::new();
        for name in category {
            if self.snapshot.contains_key(*name) && env::var(name).is_err() {
                missing.push(name.to_string());
            }
        }
        missing
    }

    /// Returns the number of variables in the original snapshot.
    #[must_use]
    pub fn snapshot_size(&self) -> usize {
        self.snapshot.len()
    }

    /// Returns all variable names in the original snapshot.
    #[must_use]
    pub fn snapshot_names(&self) -> Vec<&str> {
        self.snapshot.keys().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_takes_env() {
        let preserver = EnvPreserver::snapshot();
        // PATH should always be present
        assert!(preserver.get("PATH").is_some());
    }

    #[test]
    fn test_is_preserved_existing() {
        let preserver = EnvPreserver::snapshot();
        assert!(preserver.is_preserved("PATH"));
    }

    #[test]
    fn test_check_all_preservation_empty_missing() {
        let preserver = EnvPreserver::snapshot();
        let missing = preserver.check_all_preservation();
        // Nothing should be missing right after snapshot
        assert!(missing.is_empty());
    }

    #[test]
    fn test_snapshot_size() {
        let preserver = EnvPreserver::snapshot();
        assert!(preserver.snapshot_size() > 0);
    }
}
