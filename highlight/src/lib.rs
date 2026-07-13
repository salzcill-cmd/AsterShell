//! Syntax highlighting for `AsterShell`.
//!
//! Applies ANSI color codes to shell input based on token types and
//! the active theme.

use aster_lexer::{Lexer, Token, TokenKind};
use aster_theme::{ColorRole, Theme};

const SHELL_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done",
    "case", "esac", "in", "function", "select", "return", "exit", "local",
    "export", "readonly", "declare", "typeset", "unset", "shift", "source",
    ".", "trap", "exec", "eval", "set", "unset", "getopts", "wait", "kill",
    "break", "continue", "true", "false", "test", "[", "[[", "]]",
    "time", "coproc", "async", "await",
];

/// Applies syntax highlighting to shell input lines.
pub struct Highlighter;

impl Highlighter {
    /// Creates a new highlighter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Highlights a line of input, returning ANSI-escaped text.
    pub fn highlight(&self, input: &str, theme: &dyn Theme) -> String {
        let tokens = match Lexer::new(input).tokenize() {
            Ok(tokens) => tokens,
            Err(_) => return input.to_string(),
        };

        let mut result = String::with_capacity(input.len() * 2);
        let mut prev_end = 0;
        let mut expect_command = false;

        for token in &tokens {
            let span = token.span;

            if span.offset > prev_end {
                result.push_str(&input[prev_end..span.offset]);
            }
            prev_end = span.offset + span.length;

            match &token.kind {
                TokenKind::Eof => break,
                TokenKind::Comment(_) | TokenKind::HereDocBody(_) => {
                    result.push_str(&self.colorize_role(token, ColorRole::Comment, theme));
                }
                TokenKind::Word(w) => {
                    if w.starts_with('$') {
                        result.push_str(&self.colorize_role(token, ColorRole::Variable, theme));
                    } else if Self::is_number(w) {
                        result.push_str(&self.colorize_role(token, ColorRole::Number, theme));
                    } else if Self::is_keyword(w) {
                        result.push_str(&self.colorize_role(token, ColorRole::Keyword, theme));
                    } else if result.is_empty() || expect_command {
                        result.push_str(&self.colorize_role(token, ColorRole::Command, theme));
                        expect_command = false;
                    } else {
                        result.push_str(&self.colorize_role(token, ColorRole::Path, theme));
                    }
                }
                TokenKind::SingleQuoted(_) | TokenKind::DoubleQuoted(_) => {
                    result.push_str(&self.colorize_role(token, ColorRole::String, theme));
                }
                TokenKind::Pipe | TokenKind::AmpAmp | TokenKind::PipePipe => {
                    result.push_str(&self.colorize_role(token, ColorRole::Operator, theme));
                    expect_command = true;
                }
                TokenKind::GreaterThan
                | TokenKind::LessThan
                | TokenKind::GreaterGreater
                | TokenKind::LessLess
                | TokenKind::LessLessLess
                | TokenKind::LessAmp
                | TokenKind::GreaterAmp
                | TokenKind::AmpGreater
                | TokenKind::AmpGreaterGreater => {
                    result.push_str(&self.colorize_role(token, ColorRole::Redirect, theme));
                }
                TokenKind::Semicolon
                | TokenKind::LeftParen
                | TokenKind::RightParen
                | TokenKind::OpenBrace
                | TokenKind::CloseBrace
                | TokenKind::Amp => {
                    result.push_str(&self.colorize_role(token, ColorRole::Operator, theme));
                    expect_command = true;
                }
            }
        }

        if prev_end < input.len() {
            result.push_str(&input[prev_end..]);
        }

        result
    }

    fn colorize_role(&self, token: &Token, role: ColorRole, theme: &dyn Theme) -> String {
        let text = format!("{}", token.kind);
        if let Some(color) = theme.color(role) {
            format!(
                "\x1b[38;2;{};{};{}m{}\x1b[0m",
                color.r,
                color.g,
                color.b,
                text,
            )
        } else {
            text
        }
    }

    fn is_keyword(word: &str) -> bool {
        SHELL_KEYWORDS.contains(&word)
    }

    fn is_number(word: &str) -> bool {
        if word.is_empty() {
            return false;
        }
        let bytes = word.as_bytes();
        bytes.iter().all(|b| b.is_ascii_digit())
            || (bytes.len() > 2
                && bytes[0] == b'0'
                && (bytes[1] == b'x' || bytes[1] == b'X')
                && bytes[2..].iter().all(|b| b.is_ascii_hexdigit()))
    }


}

impl Default for Highlighter {
    fn default() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strip ANSI escape sequences and return only visible characters.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::new();
        let mut esc_seq = false;
        for c in s.chars() {
            if c == '\x1b' {
                esc_seq = true;
                continue;
            }
            if esc_seq {
                if c.is_ascii_alphabetic() {
                    esc_seq = false;
                }
                continue;
            }
            result.push(c);
        }
        result
    }

    #[test]
    fn test_highlighter_passthrough() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let result = h.highlight("echo hello", &theme);
        assert!(result.contains("echo"));
        assert!(result.contains("hello"));
    }

    #[test]
    fn test_highlighter_quotes() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let result = h.highlight("echo 'hello world'", &theme);
        assert!(result.contains("hello world"));
    }

    #[test]
    fn test_highlighter_pipe() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let result = h.highlight("ls | grep foo", &theme);
        assert!(result.contains('|'));
    }

    #[test]
    fn test_highlight_preserves_visible_width_simple() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let inputs = [
            "echo hello",
            "ls -la /tmp",
            "echo 'hello world'",
            r#"echo "hello world""#,
            "echo 'it'\"'s\"",
            "cat <<< hello",
            "echo >&2",
            "a && b || c",
            "echo 42 0xFF",
        ];
        for input in inputs {
            let highlighted = h.highlight(input, &theme);
            let visible = strip_ansi(&highlighted);
            assert_eq!(
                visible, input,
                "visible width mismatch for input: {input:?}\n  highlighted: {highlighted:?}\n  visible: {visible:?}"
            );
        }
    }

    #[test]
    fn test_highlight_preserves_width_quoted_strings() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;

        let input = "echo 'hello'";
        let highlighted = h.highlight(input, &theme);
        let visible = strip_ansi(&highlighted);
        assert_eq!(visible, input, "single-quoted string width mismatch");

        let input = r#"echo "hello""#;
        let highlighted = h.highlight(input, &theme);
        let visible = strip_ansi(&highlighted);
        assert_eq!(visible, input, "double-quoted string width mismatch");
    }

    #[test]
    fn test_highlight_preserves_width_empty_and_single_char() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        assert_eq!(strip_ansi(&h.highlight("", &theme)), "");
        assert_eq!(strip_ansi(&h.highlight("x", &theme)), "x");
    }

    #[test]
    fn test_highlight_preserves_width_operators() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let input = "echo a > b && cat c | grep d";
        let highlighted = h.highlight(input, &theme);
        let visible = strip_ansi(&highlighted);
        assert_eq!(visible, input);
    }

    #[test]
    fn test_highlight_preserves_width_redirects() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let input = "echo >&2 < in.txt > out.txt";
        let highlighted = h.highlight(input, &theme);
        let visible = strip_ansi(&highlighted);
        assert_eq!(visible, input);
    }

    #[test]
    fn test_highlight_preserves_width_comment() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let input = "echo # comment";
        let highlighted = h.highlight(input, &theme);
        let visible = strip_ansi(&highlighted);
        assert_eq!(visible, input);
    }

    #[test]
    fn test_highlight_preserves_width_numbers() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let input = "echo 42 0xFF 0";
        let highlighted = h.highlight(input, &theme);
        let visible = strip_ansi(&highlighted);
        assert_eq!(visible, input);
    }

    #[test]
    fn test_highlight_preserves_width_heredoc() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let input = "cat << EOF";
        let highlighted = h.highlight(input, &theme);
        let visible = strip_ansi(&highlighted);
        assert_eq!(visible, input);
    }

    #[test]
    fn test_command_after_pipe_colored_as_command() {
        let h = Highlighter::default();
        let theme = aster_theme::DefaultTheme;
        let result = h.highlight("echo a | grep b", &theme);
        // The result should contain ANSI codes for both 'echo' and 'grep'
        // as commands (both should be colorized, not just the first one)
        assert!(result.contains("echo"), "echo should be in result");
        assert!(result.contains("grep"), "grep should be in result");
    }

    #[test]
    fn test_is_keyword() {
        assert!(Highlighter::is_keyword("if"));
        assert!(Highlighter::is_keyword("then"));
        assert!(Highlighter::is_keyword("fi"));
        assert!(Highlighter::is_keyword("for"));
        assert!(Highlighter::is_keyword("done"));
        assert!(!Highlighter::is_keyword("echo"));
        assert!(!Highlighter::is_keyword("ls"));
    }

    #[test]
    fn test_is_number() {
        assert!(Highlighter::is_number("42"));
        assert!(Highlighter::is_number("0"));
        assert!(Highlighter::is_number("0xFF"));
        assert!(Highlighter::is_number("0XAB"));
        assert!(!Highlighter::is_number("hello"));
        assert!(!Highlighter::is_number(""));
        assert!(!Highlighter::is_number("12abc"));
    }
}
