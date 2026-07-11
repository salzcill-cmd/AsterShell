//! Environment variable management.
//!
//! Provides a [`ShellEnvironment`] that wraps process-level environment
//! variables with export tracking and helper methods.

use std::collections::HashMap;

/// Manages shell environment variables with export tracking.
#[derive(Debug, Clone)]
pub struct ShellEnvironment {
    vars: HashMap<String, String>,
    exported: HashMap<String, bool>,
}

impl ShellEnvironment {
    /// Creates a new environment seeded from the current process.
    #[must_use]
    pub fn from_process() -> Self {
        let vars: HashMap<String, String> = std::env::vars().collect();
        let exported: HashMap<String, bool> = vars.keys().map(|k| (k.clone(), true)).collect();
        Self { vars, exported }
    }

    /// Gets the value of a variable.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(std::string::String::as_str)
    }

    /// Gets the value of a variable, falling back to the process environment.
    #[must_use]
    pub fn get_or_process(&self, name: &str) -> Option<String> {
        self.vars
            .get(name)
            .cloned()
            .or_else(|| std::env::var(name).ok())
    }

    /// Sets a variable in the shell environment.
    pub fn set(&mut self, name: &str, value: &str) {
        self.vars.insert(name.to_string(), value.to_string());
    }

    /// Exports a variable to the process environment.
    ///
    /// # Safety
    ///
    /// Calls `std::env::set_var` which is unsafe in multi-threaded contexts.
    /// This is safe here because AsterShell is single-threaded during execution.
    pub fn export(&mut self, name: &str, value: &str) {
        self.vars.insert(name.to_string(), value.to_string());
        self.exported.insert(name.to_string(), true);
        // SAFETY: AsterShell is single-threaded during command execution.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var(name, value);
        }
    }

    /// Removes a variable.
    ///
    /// # Safety
    ///
    /// Calls `std::env::remove_var` which is unsafe in multi-threaded contexts.
    /// This is safe here because AsterShell is single-threaded during execution.
    pub fn unset(&mut self, name: &str) {
        self.vars.remove(name);
        self.exported.remove(name);
        // SAFETY: AsterShell is single-threaded during command execution.
        #[allow(unsafe_code)]
        unsafe {
            std::env::remove_var(name);
        }
    }

    /// Returns whether a variable is marked as exported.
    #[must_use]
    pub fn is_exported(&self, name: &str) -> bool {
        *self.exported.get(name).unwrap_or(&false)
    }

    /// Returns all variable names.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.vars.keys().map(std::string::String::as_str).collect()
    }

    /// Returns all exported variable name/value pairs.
    #[must_use]
    pub fn exported_vars(&self) -> Vec<(&str, &str)> {
        self.vars
            .iter()
            .filter(|(k, _)| self.is_exported(k))
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
}

impl Default for ShellEnvironment {
    fn default() -> Self {
        Self::from_process()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        let mut env = ShellEnvironment::from_process();
        env.set("ASTER_TEST_VAR", "hello");
        assert_eq!(env.get("ASTER_TEST_VAR"), Some("hello"));
    }

    #[test]
    fn test_unset() {
        let mut env = ShellEnvironment::from_process();
        env.set("ASTER_TEST_DEL", "val");
        env.unset("ASTER_TEST_DEL");
        assert!(env.get("ASTER_TEST_DEL").is_none());
    }

    #[test]
    fn test_names() {
        let mut env = ShellEnvironment::from_process();
        env.set("ASTER_TEST_X", "1");
        assert!(env.names().contains(&"ASTER_TEST_X"));
    }
}
