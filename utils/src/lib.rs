//! Shared utilities for `AsterShell`.
//!
//! Provides path resolution, string helpers, and other
//! low-level utilities used across the shell crates.

use std::path::{Path, PathBuf};

/// Finds an executable by name using the `PATH` environment variable.
///
/// If `name` contains a path separator, it is treated as a relative or
/// absolute path and checked directly. Otherwise each directory in `PATH`
/// is searched.
///
/// # Errors
///
/// Returns `None` when the executable cannot be found.
#[must_use]
pub fn find_executable(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        if path.is_file() && is_executable(&path) {
            return Some(path);
        }
        return None;
    }

    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let full_path = PathBuf::from(dir).join(name);
        if full_path.is_file() && is_executable(&full_path) {
            return Some(full_path);
        }
    }

    None
}

/// Checks whether a file has any executable permission bits set.
#[must_use]
pub fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Abbreviates the user's home directory as `~`.
///
/// If `path` starts with `$HOME`, the home prefix is replaced with `~`.
/// The result preserves any trailing path components after `~`.
#[must_use]
pub fn abbreviate_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(relative) = path.strip_prefix(&home) {
            if relative.as_os_str().is_empty() {
                return "~".into();
            }
            return format!("~/{}", relative.display());
        }
    }
    path.display().to_string()
}

/// Splits a shell command string into words, respecting quotes and escapes.
///
/// This is a simple splitter for Phase 2 — it handles single quotes,
/// double quotes, and backslash escapes.
#[must_use]
pub fn split_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(&ch) = chars.peek() {
        match ch {
            '\'' if !in_double => {
                chars.next();
                in_single = !in_single;
            }
            '"' if !in_single => {
                chars.next();
                in_double = !in_double;
            }
            '\\' if !in_single => {
                chars.next();
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            c if c.is_ascii_whitespace() && !in_single && !in_double => {
                chars.next();
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => {
                chars.next();
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abbreviate_path_home() {
        if let Some(home) = dirs::home_dir() {
            let sub = home.join("Documents");
            assert_eq!(abbreviate_path(&sub), "~/Documents");
        }
    }

    #[test]
    fn test_abbreviate_path_root() {
        assert_eq!(abbreviate_path(Path::new("/usr/bin")), "/usr/bin");
    }

    #[test]
    fn test_split_words_simple() {
        let words = split_words("echo hello world");
        assert_eq!(words, vec!["echo", "hello", "world"]);
    }

    #[test]
    fn test_split_words_single_quotes() {
        let words = split_words("echo 'hello world'");
        assert_eq!(words, vec!["echo", "hello world"]);
    }

    #[test]
    fn test_split_words_double_quotes() {
        let words = split_words(r#"echo "hello world""#);
        assert_eq!(words, vec!["echo", "hello world"]);
    }

    #[test]
    fn test_split_words_escapes() {
        let words = split_words(r#"echo hello\ world"#);
        assert_eq!(words, vec!["echo", "hello world"]);
    }

    #[test]
    fn test_split_words_empty() {
        let words = split_words("");
        assert!(words.is_empty());
    }

    #[test]
    fn test_split_words_whitespace_only() {
        let words = split_words("   ");
        assert!(words.is_empty());
    }

    #[test]
    fn test_find_executable_path_separator() {
        // Test with an absolute path that should exist
        let result = find_executable("/bin/sh");
        assert!(result.is_some());
    }

    #[test]
    fn test_is_executable() {
        assert!(is_executable(Path::new("/bin/sh")));
        assert!(!is_executable(Path::new("/nonexistent")));
    }
}
