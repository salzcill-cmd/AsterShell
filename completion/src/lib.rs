//! Tab completion for `AsterShell`.
//!
//! Provides file, command, and builtin completion by searching the filesystem
//! and the shell's builtin command list.

use std::env;
use std::path::Path;

/// The shell completion engine.
#[derive(Debug, Clone)]
pub struct Completion {
    /// The completed text to insert.
    pub text: String,
    /// Optional description shown alongside the completion.
    pub description: String,
}

/// The shell completion engine.
pub struct Completer;

impl Completer {
    /// Returns completions for the given input fragment.
    ///
    /// Analyzes the input to determine whether to complete files, commands,
    /// arguments, environment variables, or directories based on the
    /// preceding command word.
    #[must_use]
    pub fn complete(input: &str) -> Vec<Completion> {
        let trimmed = input.trim_start();
        if trimmed.is_empty() {
            return Self::complete_commands();
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        // First word: complete commands + files (unless the partial starts with $)
        if parts.len() <= 1 {
            let partial = parts.first().copied().unwrap_or("");
            if partial.starts_with('$') {
                return Self::complete_env_vars(partial);
            }
            let mut completions = Self::complete_commands();
            completions.extend(Self::complete_files(partial));
            return completions;
        }

        let partial = parts.last().copied().unwrap_or("");

        // $-prefixed word: environment variable names
        if partial.starts_with('$') {
            return Self::complete_env_vars(partial);
        }

        // Context-aware completion based on the preceding command
        let command = parts[parts.len() - 2];
        match command {
            "cd" | "pushd" | "rmdir" => Self::complete_directories(partial),
            "source" | "." => Self::complete_script_files(partial),
            "export" | "declare" | "local" => Self::complete_var_names(partial),
            "unset" | "readonly" => Self::complete_env_var_names(partial),
            _ => Self::complete_files(partial),
        }
    }

    /// Returns completions matching executable names in PATH and builtins.
    #[must_use]
    pub fn complete_commands() -> Vec<Completion> {
        let mut completions = Vec::new();

        // Built-in commands
        for (name, desc) in aster_builtins::builtin_list() {
            completions.push(Completion {
                text: (*name).to_string(),
                description: (*desc).to_string(),
            });
        }

        // Executables in PATH
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in path_var.split(':') {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && aster_utils::is_executable(&path) {
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if !completions.iter().any(|c| c.text == name) {
                                    completions.push(Completion {
                                        text: name.to_string(),
                                        description: path.display().to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        completions.sort_by(|a, b| a.text.cmp(&b.text));
        completions.dedup_by(|a, b| a.text == b.text);
        completions
    }

    /// Returns file completions matching the given partial path.
    ///
    /// Supports tilde expansion (`~` → home directory) and hides files
    /// starting with `.` unless the partial itself starts with `.`.
    #[must_use]
    pub fn complete_files(partial: &str) -> Vec<Completion> {
        let (dir, prefix, base) = if partial.starts_with("~/") {
            if let Ok(home) = env::var("HOME") {
                let rest = &partial[2..];
                let path = Path::new(&home).join(rest);
                let dir = path.parent().unwrap_or(Path::new(&home));
                let prefix = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                (dir.to_path_buf(), prefix, format!("{home}/"))
            } else {
                let path = Path::new(partial);
                let dir = path.parent().unwrap_or(Path::new("."));
                let prefix = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                (dir.to_path_buf(), prefix, String::new())
            }
        } else if partial.contains('/') {
            let path = Path::new(partial);
            let dir = path.parent().unwrap_or(Path::new("."));
            let prefix = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (dir.to_path_buf(), prefix, String::new())
        } else {
            (Path::new(".").to_path_buf(), partial.to_string(), String::new())
        };

        let show_hidden = prefix.starts_with('.');
        let mut completions = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                // Hidden file filtering
                if !show_hidden && name_str.starts_with('.') {
                    continue;
                }

                if name_str.starts_with(&prefix) {
                    let path = entry.path();
                    let display = if !base.is_empty() {
                        if path.is_dir() {
                            format!("{base}{name_str}/")
                        } else {
                            format!("{base}{name_str}")
                        }
                    } else if path.is_dir() {
                        format!("{name_str}/")
                    } else {
                        name_str.to_string()
                    };
                    completions.push(Completion {
                        text: display,
                        description: String::new(),
                    });
                }
            }
        }

        completions.sort_by(|a, b| a.text.cmp(&b.text));
        completions
    }

    /// Returns directory-only completions matching the given partial path.
    #[must_use]
    pub fn complete_directories(partial: &str) -> Vec<Completion> {
        let (dir, prefix, base) = if partial.starts_with("~/") {
            if let Ok(home) = env::var("HOME") {
                let rest = &partial[2..];
                let path = Path::new(&home).join(rest);
                let dir = path.parent().unwrap_or(Path::new(&home));
                let prefix = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                (dir.to_path_buf(), prefix, format!("{home}/"))
            } else {
                let path = Path::new(partial);
                let dir = path.parent().unwrap_or(Path::new("."));
                let prefix = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                (dir.to_path_buf(), prefix, String::new())
            }
        } else if partial.contains('/') {
            let path = Path::new(partial);
            let dir = path.parent().unwrap_or(Path::new("."));
            let prefix = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (dir.to_path_buf(), prefix, String::new())
        } else {
            (Path::new(".").to_path_buf(), partial.to_string(), String::new())
        };

        let show_hidden = prefix.starts_with('.');
        let mut completions = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();

                if !show_hidden && name_str.starts_with('.') {
                    continue;
                }

                if name_str.starts_with(&prefix) {
                    let path = entry.path();
                    if path.is_dir() {
                        let display = if !base.is_empty() {
                            format!("{base}{name_str}/")
                        } else {
                            format!("{name_str}/")
                        };
                        completions.push(Completion {
                            text: display,
                            description: String::new(),
                        });
                    }
                }
            }
        }

        completions.sort_by(|a, b| a.text.cmp(&b.text));
        completions
    }

    /// Returns completions for script files (.sh, .bash, .bashrc, etc.).
    #[must_use]
    fn complete_script_files(partial: &str) -> Vec<Completion> {
        let all = Self::complete_files(partial);
        all.into_iter()
            .filter(|c| {
                let lower = c.text.to_lowercase();
                lower.ends_with(".sh")
                    || lower.ends_with(".bash")
                    || lower.ends_with(".bashrc")
                    || lower.ends_with(".bash_profile")
                    || lower.ends_with(".zsh")
                    || !c.text.contains('/')
                        && c.text != "."
                        && c.text != ".."
                    || c.text.ends_with('/')
            })
            .collect()
    }

    /// Returns completions for variable names defined in the current process.
    /// Used after `export`, `declare`, `local` — suggests all env var names.
    #[must_use]
    fn complete_var_names(partial: &str) -> Vec<Completion> {
        Self::complete_env_var_names(partial)
    }

    /// Returns environment variable name completions (without `$` prefix).
    #[must_use]
    fn complete_env_var_names(partial: &str) -> Vec<Completion> {
        let search = if partial.starts_with('$') {
            &partial[1..]
        } else {
            partial
        };

        env::vars()
            .filter_map(|(name, _)| {
                if name.starts_with(search) {
                    Some(Completion {
                        text: name,
                        description: String::new(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns environment variable completions in `${VAR}` format.
    /// Used when the word being completed starts with `$`.
    #[must_use]
    pub fn complete_env_vars(partial: &str) -> Vec<Completion> {
        let search = if partial.starts_with("${") {
            &partial[2..]
        } else if partial.starts_with('$') {
            &partial[1..]
        } else {
            partial
        };

        env::vars()
            .filter_map(|(name, _)| {
                if name.starts_with(search) {
                    Some(Completion {
                        text: format!("${{{name}}}"),
                        description: String::new(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Formats completions for display to the user.
    #[must_use]
    pub fn format_completions(completions: &[Completion]) -> String {
        let max_width = completions.iter().map(|c| c.text.len()).max().unwrap_or(0);

        let mut result = String::new();
        for c in completions {
            let padded = format!("{:<width$}", c.text, width = max_width + 2);
            if c.description.is_empty() {
                result.push_str(&padded);
            } else {
                result.push_str(&format!("{padded}{}", c.description));
            }
            result.push('\n');
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_commands_nonempty() {
        let completions = Completer::complete_commands();
        assert!(!completions.is_empty());
        assert!(completions.iter().any(|c| c.text == "echo"));
    }

    #[test]
    fn test_complete_files_in_root() {
        let completions = Completer::complete_files("/");
        assert!(!completions.is_empty());
    }

    #[test]
    fn test_format_completions() {
        let comps = vec![
            Completion {
                text: "echo".into(),
                description: "Display text".into(),
            },
            Completion {
                text: "exit".into(),
                description: "Exit shell".into(),
            },
        ];
        let formatted = Completer::format_completions(&comps);
        assert!(formatted.contains("echo"));
        assert!(formatted.contains("exit"));
    }

    #[test]
    fn test_tilde_expansion() {
        let completions = Completer::complete_files("~/");
        // Should expand to home directory contents; at minimum, not panic
        let _ = completions;
    }

    #[test]
    fn test_hidden_files_not_shown_by_default() {
        let completions = Completer::complete_files("/");
        // Root should have hidden files like .dot; they should not appear
        // unless partial starts with '.'
        for c in &completions {
            let name = c.text.trim_end_matches('/');
            // When partial is "/", prefix is "", so hidden files are skipped
            assert!(!name.starts_with('.') || name == "." || name == "..");
        }
    }

    #[test]
    fn test_hidden_files_shown_when_prefix_starts_with_dot() {
        // Completing "/." should show hidden entries like /.bashrc or /.dot
        let completions = Completer::complete_files("/.");
        // At least check that we didn't crash; root usually has hidden dirs
        let _ = completions;
    }

    #[test]
    fn test_complete_directories_only() {
        let completions = Completer::complete_directories("/usr");
        for c in &completions {
            assert!(c.text.ends_with('/'), "expected directory, got: {}", c.text);
        }
    }

    #[test]
    fn test_complete_env_vars() {
        let completions = Completer::complete_env_vars("$PATH");
        assert!(completions.iter().any(|c| c.text == "${PATH}"));
    }

    #[test]
    fn test_complete_env_var_names() {
        let completions = Completer::complete_env_var_names("P");
        assert!(completions.iter().any(|c| c.text == "PATH"));
    }

    #[test]
    fn test_context_cd_returns_directories() {
        let completions = Completer::complete("cd /usr");
        for c in &completions {
            assert!(c.text.ends_with('/'), "expected dir for cd, got: {}", c.text);
        }
    }

    #[test]
    fn test_context_dollar_completes_env() {
        let completions = Completer::complete("$PAT");
        assert!(completions.iter().any(|c| c.text == "${PATH}"));
    }
}
