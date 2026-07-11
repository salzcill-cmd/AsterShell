//! Command history subsystem.
//!
//! Stores command history in memory with optional persistence to disk.
//! Each entry includes a timestamp and the command text.

use std::path::{Path, PathBuf};

use aster_shell_core::{HistoryError, ShellError};

/// A single history entry with timestamp.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// The command text.
    pub command: String,
    /// Timestamp when the command was recorded.
    pub timestamp: chrono::DateTime<chrono::Local>,
}

impl HistoryEntry {
    /// Creates a new history entry with the current time.
    #[must_use]
    pub fn new(command: String) -> Self {
        Self {
            command,
            timestamp: chrono::Local::now(),
        }
    }

    /// Formats the timestamp for display.
    #[must_use]
    pub fn format_timestamp(&self) -> String {
        self.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

impl std::fmt::Display for HistoryEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.command)
    }
}

/// Persistent command history with timestamps and search.
pub struct History {
    entries: Vec<HistoryEntry>,
    file_path: PathBuf,
    max_size: usize,
}

impl History {
    /// Creates a new history instance, loading entries from the default file.
    ///
    /// # Errors
    ///
    /// Returns an error if the history file path cannot be determined.
    pub fn new(max_size: usize) -> Result<Self, ShellError> {
        let file_path = aster_config::history_file_path()?;
        let mut history = Self {
            entries: Vec::new(),
            file_path,
            max_size,
        };
        history.load();
        Ok(history)
    }

    /// Creates a history instance backed by a specific file.
    #[must_use]
    pub fn with_file(file_path: PathBuf, max_size: usize) -> Self {
        let mut history = Self {
            entries: Vec::new(),
            file_path,
            max_size,
        };
        history.load();
        history
    }

    /// Adds a command to the history.
    ///
    /// Consecutive duplicate entries are skipped. If the history exceeds
    /// `max_size`, the oldest entries are removed.
    pub fn add(&mut self, entry: String) {
        if entry.trim().is_empty() {
            return;
        }
        if self.entries.last().is_some_and(|e| e.command == entry) {
            return;
        }
        self.entries.push(HistoryEntry::new(entry));
        while self.entries.len() > self.max_size {
            self.entries.remove(0);
        }
    }

    /// Returns a reference to all history entries.
    #[must_use]
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Returns the command strings for display purposes.
    #[must_use]
    pub fn commands(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.command.as_str()).collect()
    }

    /// Returns the number of stored entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the history is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clears all history entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Searches history for entries matching the given substring.
    ///
    /// Returns entries in reverse chronological order (newest first).
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .rev()
            .filter(|e| e.command.contains(query))
            .collect()
    }

    /// Returns the most recent entry matching the given prefix.
    #[must_use]
    pub fn search_prefix(&self, prefix: &str) -> Option<&HistoryEntry> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.command.starts_with(prefix))
    }

    /// Saves the history to the backing file.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError::WriteIo`] on I/O failure.
    pub fn save(&self) -> Result<(), ShellError> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent).map_err(HistoryError::Io)?;
        }
        let content: String = self
            .entries
            .iter()
            .map(|e| format!("{}\n", e.command))
            .collect();
        std::fs::write(&self.file_path, content).map_err(HistoryError::WriteIo)?;
        Ok(())
    }

    /// Loads history entries from the backing file.
    fn load(&mut self) {
        if !self.file_path.exists() {
            return;
        }
        if let Ok(content) = std::fs::read_to_string(&self.file_path) {
            for line in content.lines() {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    self.entries.push(HistoryEntry::new(trimmed));
                }
            }
            while self.entries.len() > self.max_size {
                self.entries.remove(0);
            }
        }
    }

    /// Returns the path to the history file.
    #[must_use]
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_history() -> (History, tempfile::NamedTempFile) {
        let f = tempfile::NamedTempFile::new().unwrap();
        let path = f.path().to_path_buf();
        let h = History::with_file(path, 100);
        (h, f)
    }

    #[test]
    fn test_add_and_get() {
        let (mut h, _f) = temp_history();
        h.add("echo hello".into());
        h.add("ls -la".into());
        assert_eq!(h.len(), 2);
        assert_eq!(h.entries()[0].command, "echo hello");
        assert_eq!(h.entries()[1].command, "ls -la");
    }

    #[test]
    fn test_duplicate_skipped() {
        let (mut h, _f) = temp_history();
        h.add("echo hello".into());
        h.add("echo hello".into());
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn test_empty_not_added() {
        let (mut h, _f) = temp_history();
        h.add("".into());
        h.add("   ".into());
        assert!(h.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let path = f.path().to_path_buf();

        {
            let mut h = History::with_file(path.clone(), 100);
            h.add("echo one".into());
            h.add("echo two".into());
            h.save().unwrap();
        }

        let h2 = History::with_file(path, 100);
        assert_eq!(h2.len(), 2);
        assert_eq!(h2.entries()[0].command, "echo one");
        assert_eq!(h2.entries()[1].command, "echo two");
    }

    #[test]
    fn test_clear() {
        let (mut h, _f) = temp_history();
        h.add("echo hello".into());
        h.clear();
        assert!(h.is_empty());
    }

    #[test]
    fn test_search() {
        let (mut h, _f) = temp_history();
        h.add("echo hello".into());
        h.add("ls -la".into());
        h.add("echo world".into());
        let results = h.search("echo");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_prefix() {
        let (mut h, _f) = temp_history();
        h.add("echo hello".into());
        h.add("ls -la".into());
        let entry = h.search_prefix("echo");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().command, "echo hello");
    }

    #[test]
    fn test_load_nonexistent_file() {
        let h = History::with_file(PathBuf::from("/tmp/aster_test_nonexistent"), 100);
        assert!(h.is_empty());
    }
}
