//! Line editor for `AsterShell`.
//!
//! Wraps `rustyline` to provide syntax highlighting, tab completion,
//! and autosuggestion support.

use std::borrow::Cow;

use rustyline::completion::Completer as RustyCompleter;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter as RustyHighlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Editor, Helper};

use aster_theme::Theme;

/// AsterShell's rustyline helper implementing highlighting, completion, and hints.
pub struct AsterHelper {
    /// The active theme for syntax highlighting.
    pub theme: Box<dyn Theme>,
    /// Cached history commands for efficient hint lookups.
    pub history_cache: Vec<String>,
}

impl Helper for AsterHelper {}

impl Validator for AsterHelper {}

impl RustyHighlighter for AsterHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Owned(aster_highlight::Highlighter::new().highlight(line, &*self.theme))
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}

impl RustyCompleter for AsterHelper {
    type Candidate = rustyline::completion::Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let input_before_cursor = &line[..pos];
        let completions = aster_completion::Completer::complete(input_before_cursor);
        let candidates: Vec<rustyline::completion::Pair> = completions
            .into_iter()
            .map(|c| rustyline::completion::Pair {
                display: c.text.clone(),
                replacement: c.text,
            })
            .collect();

        let word_start = input_before_cursor
            .rfind(|c: char| c.is_whitespace())
            .map_or(0, |i| i + 1);

        Ok((word_start, candidates))
    }
}

impl Hinter for AsterHelper {
    type Hint = String;

    fn hint(&self, line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<String> {
        if line.is_empty() {
            return None;
        }

        let trimmed = line.trim();
        if let Some(entry) = self
            .history_cache
            .iter()
            .rev()
            .find(|cmd| cmd.starts_with(trimmed))
        {
            if entry != trimmed {
                return Some(entry[trimmed.len()..].to_string());
            }
        }
        None
    }
}

/// The AsterShell line editor.
pub struct EditorWrapper {
    editor: Editor<AsterHelper, rustyline::history::FileHistory>,
}

impl EditorWrapper {
    /// Creates a new editor with the given theme.
    ///
    /// # Errors
    ///
    /// Returns an error if the editor fails to initialize.
    pub fn new(theme: Box<dyn Theme>) -> rustyline::Result<Self> {
        let config = rustyline::config::Config::builder()
            .history_ignore_space(true)
            .history_ignore_dups(true)?
            .build();

        let history_cache = Self::load_history_cache();

        let helper = AsterHelper {
            theme,
            history_cache,
        };

        let mut editor = Editor::with_config(config)?;
        editor.set_helper(Some(helper));

        if let Ok(hist_path) = aster_config::history_file_path() {
            if hist_path.exists() {
                let _ = editor.load_history(&hist_path);
            }
        }

        Ok(Self { editor })
    }

    fn load_history_cache() -> Vec<String> {
        if let Ok(hist) = aster_history::History::new(1000) {
            hist.entries().iter().map(|e| e.command.clone()).collect()
        } else {
            Vec::new()
        }
    }

    /// Checks whether the input so far is syntactically complete.
    ///
    /// Returns `false` if there are unclosed quotes, unbalanced braces/parens,
    /// or the line ends with a continuation operator.
    fn is_input_complete(input: &str) -> bool {
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut brace_depth = 0i32;
        let mut paren_depth = 0i32;
        let mut chars = input.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\\' if !in_single_quote => {
                    chars.next();
                }
                '\'' if !in_double_quote => in_single_quote = !in_single_quote,
                '"' if !in_single_quote => in_double_quote = !in_double_quote,
                '{' if !in_single_quote && !in_double_quote => brace_depth += 1,
                '}' if !in_single_quote && !in_double_quote => brace_depth -= 1,
                '(' if !in_single_quote && !in_double_quote => paren_depth += 1,
                ')' if !in_single_quote && !in_double_quote => paren_depth -= 1,
                _ => {}
            }
        }

        if in_single_quote || in_double_quote {
            return false;
        }
        if brace_depth > 0 || paren_depth > 0 {
            return false;
        }

        let trimmed = input.trim_end();
        if trimmed.ends_with('\\') {
            return false;
        }
        if trimmed.ends_with('|') || trimmed.ends_with('&') || trimmed.ends_with(';') {
            return false;
        }
        if trimmed.ends_with("&&") || trimmed.ends_with("||") {
            return false;
        }

        true
    }

    /// Reads a line of input from the user, supporting multi-line input.
    ///
    /// If the input is incomplete (unclosed quotes, unbalanced braces/parens,
    /// trailing continuation operators), a continuation prompt (`> `) is shown
    /// and additional lines are read until the input is complete.
    ///
    /// Returns `Ok(Some(line))` on success, `Ok(None)` on EOF/Ctrl-C.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    pub fn readline(&mut self, prompt: &str) -> rustyline::Result<Option<String>> {
        let first_line = match self.editor.readline(prompt) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                eprintln!("^C");
                return Ok(None);
            }
            Err(ReadlineError::Eof) => return Ok(None),
            Err(e) => return Err(e),
        };

        if Self::is_input_complete(&first_line) {
            return Ok(Some(first_line));
        }

        let mut input = first_line;
        loop {
            match self.editor.readline("> ") {
                Ok(line) => {
                    input.push('\n');
                    input.push_str(&line);
                    if Self::is_input_complete(&input) {
                        break;
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    eprintln!("^C");
                    return Ok(Some(String::new()));
                }
                Err(ReadlineError::Eof) => break,
                Err(e) => return Err(e),
            }
        }

        Ok(Some(input))
    }

    /// Adds a line to the editor's history.
    ///
    /// # Errors
    ///
    /// Returns an error if the history operation fails.
    pub fn add_history_entry(&mut self, line: &str) -> rustyline::Result<bool> {
        self.editor.add_history_entry(line)
    }

    /// Saves the editor's history to disk.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    pub fn save_history(&mut self) -> rustyline::Result<()> {
        if let Ok(hist_path) = aster_config::history_file_path() {
            if let Some(parent) = hist_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            self.editor.save_history(&hist_path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_helper_highlight() {
        let helper = AsterHelper {
            theme: Box::new(aster_theme::DefaultTheme),
            history_cache: Vec::new(),
        };
        let result = RustyHighlighter::highlight(&helper, "echo hello", 0);
        assert!(result.contains("echo"));
    }

    #[test]
    fn test_editor_helper_hint_empty() {
        let helper = AsterHelper {
            theme: Box::new(aster_theme::DefaultTheme),
            history_cache: Vec::new(),
        };
        assert!(
            Hinter::hint(
                &helper,
                "",
                0,
                &rustyline::Context::new(&rustyline::history::MemHistory::new())
            )
            .is_none()
        );
    }

    #[test]
    fn test_is_input_complete_simple() {
        assert!(EditorWrapper::is_input_complete("echo hello"));
        assert!(EditorWrapper::is_input_complete("ls -la"));
        assert!(EditorWrapper::is_input_complete(""));
    }

    #[test]
    fn test_is_input_complete_unclosed_quotes() {
        assert!(!EditorWrapper::is_input_complete("echo 'hello"));
        assert!(!EditorWrapper::is_input_complete("echo \"hello"));
        assert!(!EditorWrapper::is_input_complete("echo 'hello' \"world"));
    }

    #[test]
    fn test_is_input_complete_escaped_quotes() {
        assert!(EditorWrapper::is_input_complete("echo \\'hello\\'"));
        assert!(EditorWrapper::is_input_complete("echo \"hello\\\"world\""));
    }

    #[test]
    fn test_is_input_complete_unbalanced_braces() {
        assert!(!EditorWrapper::is_input_complete("echo {hello"));
        assert!(!EditorWrapper::is_input_complete("fn main() {"));
        assert!(!EditorWrapper::is_input_complete("echo (hello"));
    }

    #[test]
    fn test_is_input_complete_trailing_operators() {
        assert!(!EditorWrapper::is_input_complete("echo hello\\"));
        assert!(!EditorWrapper::is_input_complete("echo hello |"));
        assert!(!EditorWrapper::is_input_complete("echo hello &"));
        assert!(!EditorWrapper::is_input_complete("echo hello &&"));
        assert!(!EditorWrapper::is_input_complete("echo hello ||"));
        assert!(!EditorWrapper::is_input_complete("echo hello ;"));
    }

    #[test]
    fn test_is_input_complete_balanced() {
        assert!(EditorWrapper::is_input_complete("echo {hello}"));
        assert!(EditorWrapper::is_input_complete("echo (hello)"));
        assert!(EditorWrapper::is_input_complete("echo 'hello'"));
        assert!(EditorWrapper::is_input_complete("echo \"hello\""));
    }
}
