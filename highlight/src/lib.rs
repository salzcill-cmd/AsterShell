//! Syntax highlighting for `AsterShell`.
//!
//! Applies ANSI color codes to shell input based on token types and
//! the active theme.

use aster_lexer::{Lexer, Token, TokenKind};
use aster_theme::{ColorRole, Theme};

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

        for token in &tokens {
            match &token.kind {
                TokenKind::Eof => break,
                _ => {}
            }

            let span = token.span;

            // Reconstruct whitespace/gap between tokens from spans
            if span.offset > prev_end {
                result.push_str(&input[prev_end..span.offset]);
            }
            prev_end = span.offset + span.length;

            match &token.kind {
                TokenKind::Eof => break,
                TokenKind::Comment(_) => {
                    result.push_str(&self.colorize_role(token, ColorRole::Comment, theme));
                }
                TokenKind::Word(w) => {
                    if w.starts_with('$') {
                        result.push_str(&self.colorize_role(token, ColorRole::Variable, theme));
                    } else if result.is_empty() || Self::is_after_pipe_or_semicolon(&result) {
                        result.push_str(&self.colorize_role(token, ColorRole::Command, theme));
                    } else {
                        result.push_str(&self.colorize_role(token, ColorRole::Path, theme));
                    }
                }
                TokenKind::SingleQuoted(_) | TokenKind::DoubleQuoted(_) => {
                    result.push_str(&self.colorize_role(token, ColorRole::String, theme));
                }
                TokenKind::Pipe | TokenKind::AmpAmp | TokenKind::PipePipe => {
                    result.push_str(&self.colorize_role(token, ColorRole::Operator, theme));
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
                }
            }
        }

        // Append any trailing content after last token
        if prev_end < input.len() {
            result.push_str(&input[prev_end..]);
        }

        result
    }

    fn colorize_role(&self, token: &Token, role: ColorRole, theme: &dyn Theme) -> String {
        if let Some(color) = theme.color(role) {
            format!(
                "\x1b[38;2;{};{};{}m{}\x1b[0m",
                color.r,
                color.g,
                color.b,
                token.kind.text(),
            )
        } else {
            token.kind.text().to_string()
        }
    }

    fn is_after_pipe_or_semicolon(s: &str) -> bool {
        let trimmed = s.trim_end();
        trimmed.ends_with('|')
            || trimmed.ends_with(';')
            || trimmed.ends_with("&&")
            || trimmed.ends_with("||")
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
}
