//! Directory stack for `pushd` / `popd` / `dirs`.
//!
//! Maintains a stack of working directories that can be navigated
//! with `pushd` and `popd`.

use std::path::{Path, PathBuf};

/// A directory stack for directory navigation.
#[derive(Debug, Clone)]
pub struct DirectoryStack {
    stack: Vec<PathBuf>,
}

impl DirectoryStack {
    /// Creates a new directory stack seeded with the given current directory.
    #[must_use]
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self { stack: vec![cwd] }
    }

    /// Pushes a directory onto the stack and changes into it.
    ///
    /// Returns the previous working directory.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the directory change fails.
    pub fn pushd(&mut self, path: &Path) -> Result<PathBuf, std::io::Error> {
        let current = std::env::current_dir()?;
        self.stack.push(current);
        std::env::set_current_dir(path)?;
        Ok(self.stack.last().cloned().unwrap_or_default())
    }

    /// Pops the top directory off the stack and changes into it.
    ///
    /// The bottom entry (original directory) is preserved.
    ///
    /// # Errors
    ///
    /// Returns `Err` if there is only one entry or the directory change fails.
    pub fn popd(&mut self) -> Result<PathBuf, std::io::Error> {
        if self.stack.len() <= 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "popd: directory stack empty",
            ));
        }
        self.stack.pop();
        let target = self
            .stack
            .last()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        std::env::set_current_dir(&target)?;
        Ok(target)
    }

    /// Returns all directories in the stack.
    #[must_use]
    pub fn entries(&self) -> &[PathBuf] {
        &self.stack
    }

    /// Returns the number of directories in the stack.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Returns whether the stack is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

impl Default for DirectoryStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_has_cwd() {
        let ds = DirectoryStack::new();
        assert!(!ds.is_empty());
        assert_eq!(ds.len(), 1);
    }

    #[test]
    fn test_entries_are_paths() {
        let ds = DirectoryStack::new();
        let entries = ds.entries();
        assert!(!entries.is_empty());
    }
}
