//! Token types produced by the lexer.

use aster_shell_core::Span;

/// A single lexical token with its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The kind of token.
    pub kind: TokenKind,
    /// Source location of this token.
    pub span: Span,
}

/// The type of a lexical token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// An unquoted word.
    Word(String),
    /// A single-quoted string (literal content, no escape processing).
    SingleQuoted(String),
    /// A double-quoted string (escape sequences are processed).
    DoubleQuoted(String),
    /// Pipe operator: `|`.
    Pipe,
    /// Logical AND operator: `&&`.
    AmpAmp,
    /// Logical OR operator: `||`.
    PipePipe,
    /// Semicolon: `;`.
    Semicolon,
    /// Input redirect: `<`.
    LessThan,
    /// Output redirect: `>`.
    GreaterThan,
    /// Append redirect: `>>`.
    GreaterGreater,
    /// Heredoc: `<<`.
    LessLess,
    /// Here-string: `<<<`.
    LessLessLess,
    /// FD input redirect: `<&`.
    LessAmp,
    /// FD output redirect: `>&`.
    GreaterAmp,
    /// Redirect stderr to stdout: `&>`.
    AmpGreater,
    /// Append stderr to stdout: `&>>`.
    AmpGreaterGreater,
    /// Background: `&`.
    Amp,
    /// Left parenthesis: `(`.
    LeftParen,
    /// Right parenthesis: `)`.
    RightParen,
    /// Open brace: `{` (compound command delimiter).
    OpenBrace,
    /// Close brace: `}` (compound command delimiter).
    CloseBrace,
    /// Comment (from `#` to end of line).
    Comment(String),
    /// Heredoc body content (collected by the lexer between `<<DELIM` and `DELIM`).
    HereDocBody(String),
    /// End of input.
    Eof,
}

impl TokenKind {
    /// Returns the raw text of this token.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Word(w) => w,
            Self::SingleQuoted(s) => s,
            Self::DoubleQuoted(s) => s,
            Self::Pipe => "|",
            Self::AmpAmp => "&&",
            Self::PipePipe => "||",
            Self::Semicolon => ";",
            Self::LessThan => "<",
            Self::GreaterThan => ">",
            Self::GreaterGreater => ">>",
            Self::LessLess => "<<",
            Self::LessLessLess => "<<<",
            Self::LessAmp => "<&",
            Self::GreaterAmp => ">&",
            Self::AmpGreater => "&>",
            Self::AmpGreaterGreater => "&>>",
            Self::Amp => "&",
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::OpenBrace => "{",
            Self::CloseBrace => "}",
            Self::Comment(c) => c,
            Self::HereDocBody(b) => b,
            Self::Eof => "",
        }
    }

    /// Returns a human-readable name for this token kind.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Word(_) => "word",
            Self::SingleQuoted(_) => "single-quoted string",
            Self::DoubleQuoted(_) => "double-quoted string",
            Self::Pipe => "|",
            Self::AmpAmp => "&&",
            Self::PipePipe => "||",
            Self::Semicolon => ";",
            Self::LessThan => "<",
            Self::GreaterThan => ">",
            Self::GreaterGreater => ">>",
            Self::LessLess => "<<",
            Self::LessLessLess => "<<<",
            Self::LessAmp => "<&",
            Self::GreaterAmp => ">&",
            Self::AmpGreater => "&>",
            Self::AmpGreaterGreater => "&>>",
            Self::Amp => "&",
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::OpenBrace => "{",
            Self::CloseBrace => "}",
            Self::Comment(_) => "comment",
            Self::HereDocBody(_) => "heredoc body",
            Self::Eof => "end of input",
        }
    }
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Word(w) => write!(f, "{w}"),
            Self::SingleQuoted(s) => write!(f, "'{s}'"),
            Self::DoubleQuoted(s) => write!(f, "\"{s}\""),
            Self::Pipe => write!(f, "|"),
            Self::AmpAmp => write!(f, "&&"),
            Self::PipePipe => write!(f, "||"),
            Self::Semicolon => write!(f, ";"),
            Self::LessThan => write!(f, "<"),
            Self::GreaterThan => write!(f, ">"),
            Self::GreaterGreater => write!(f, ">>"),
            Self::LessLess => write!(f, "<<"),
            Self::LessLessLess => write!(f, "<<<"),
            Self::LessAmp => write!(f, "<&"),
            Self::GreaterAmp => write!(f, ">&"),
            Self::AmpGreater => write!(f, "&>"),
            Self::AmpGreaterGreater => write!(f, "&>>"),
            Self::Amp => write!(f, "&"),
            Self::LeftParen => write!(f, "("),
            Self::RightParen => write!(f, ")"),
            Self::OpenBrace => write!(f, "{{"),
            Self::CloseBrace => write!(f, "}}"),
            Self::Comment(c) => write!(f, "#{c}"),
            Self::HereDocBody(b) => write!(f, "{b}"),
            Self::Eof => write!(f, "EOF"),
        }
    }
}

impl Token {
    /// Returns a display-friendly description of this token.
    #[must_use]
    pub fn display(&self) -> String {
        format!("`{}`", self.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_kind_name() {
        assert_eq!(TokenKind::Word("ls".into()).name(), "word");
        assert_eq!(TokenKind::Pipe.name(), "|");
        assert_eq!(TokenKind::AmpAmp.name(), "&&");
        assert_eq!(TokenKind::Amp.name(), "&");
        assert_eq!(TokenKind::LessLess.name(), "<<");
        assert_eq!(TokenKind::LessLessLess.name(), "<<<");
        assert_eq!(TokenKind::GreaterAmp.name(), ">&");
        assert_eq!(TokenKind::OpenBrace.name(), "{");
        assert_eq!(TokenKind::CloseBrace.name(), "}");
        assert_eq!(TokenKind::Eof.name(), "end of input");
    }

    #[test]
    fn test_token_kind_display() {
        assert_eq!(format!("{}", TokenKind::Word("echo".into())), "echo");
        assert_eq!(format!("{}", TokenKind::Pipe), "|");
        assert_eq!(format!("{}", TokenKind::GreaterGreater), ">>");
        assert_eq!(format!("{}", TokenKind::LessLess), "<<");
        assert_eq!(format!("{}", TokenKind::Amp), "&");
    }

    #[test]
    fn test_token_display() {
        let token = Token {
            kind: TokenKind::Word("ls".into()),
            span: Span::new(1, 1, 0, 2),
        };
        assert_eq!(token.display(), "`ls`");
    }
}
