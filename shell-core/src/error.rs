use std::path::PathBuf;

use crate::span::Span;

/// Top-level error type for `AsterShell`.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    /// An error produced by the lexer.
    #[error(transparent)]
    Lexer(#[from] LexerError),
    /// An error produced by the parser.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// An error produced by the executor.
    #[error(transparent)]
    Exec(#[from] ExecError),
    /// An error from the configuration system.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// An error from the history subsystem.
    #[error(transparent)]
    History(#[from] HistoryError),
    /// An error from the plugin system.
    #[error(transparent)]
    Plugin(#[from] PluginError),
    /// An I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A break/continue was encountered outside a loop.
    #[error("break/continue outside loop at {0}")]
    BreakOutsideLoop(Span),
    /// A return was encountered outside a function.
    #[error("return outside function at {0}")]
    ReturnOutsideFunction(Span),
    /// Invalid function name.
    #[error("{0}")]
    InvalidFunctionName(String),
}

impl ShellError {
    /// Creates a `ShellError` with the given kind, message, and span.
    #[must_use]
    pub fn new(kind: ErrorKind, message: impl Into<String>, span: Span) -> Self {
        match kind {
            ErrorKind::InvalidFunctionName => Self::InvalidFunctionName(message.into()),
            ErrorKind::BreakOutsideLoop => Self::BreakOutsideLoop(span),
            ErrorKind::ReturnOutsideFunction => Self::ReturnOutsideFunction(span),
        }
    }
}

/// Generic error kind for `ShellError`.
#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    /// Invalid function name.
    InvalidFunctionName,
    /// Break/continue outside loop.
    BreakOutsideLoop,
    /// Return outside function.
    ReturnOutsideFunction,
}

/// Errors produced during tokenization.
#[derive(Debug, thiserror::Error)]
pub enum LexerError {
    /// An unexpected character was encountered.
    #[error("unexpected character '{ch}' at {line}:{column}")]
    UnexpectedChar {
        /// The unexpected character.
        ch: char,
        /// Line number.
        line: usize,
        /// Column number.
        column: usize,
    },
    /// An unterminated double-quoted string.
    #[error("unterminated double-quoted string at {line}:{column}")]
    UnterminatedDoubleQuote {
        /// Line where the quote started.
        line: usize,
        /// Column where the quote started.
        column: usize,
    },
    /// An unterminated single-quoted string.
    #[error("unterminated single-quoted string at {line}:{column}")]
    UnterminatedSingleQuote {
        /// Line where the quote started.
        line: usize,
        /// Column where the quote started.
        column: usize,
    },
    /// An unterminated escape sequence.
    #[error("unterminated escape sequence at {line}:{column}")]
    UnterminatedEscape {
        /// Line of the escape.
        line: usize,
        /// Column of the escape.
        column: usize,
    },
}

/// Errors produced during parsing.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// An unexpected token was encountered.
    #[error("unexpected token `{token}` at {line}:{column}")]
    UnexpectedToken {
        /// The unexpected token text.
        token: String,
        /// Line number.
        line: usize,
        /// Column number.
        column: usize,
    },
    /// Unexpected end of input.
    #[error("unexpected end of input, {expected}")]
    UnexpectedEof {
        /// What was expected.
        expected: String,
    },
    /// An unmatched parenthesis.
    #[error("unmatched parenthesis at {line}:{column}")]
    UnmatchedParen {
        /// Line number.
        line: usize,
        /// Column number.
        column: usize,
    },
    /// An empty pipeline component.
    #[error("empty command in pipeline at {line}:{column}")]
    EmptyPipeline {
        /// Line number.
        line: usize,
        /// Column number.
        column: usize,
    },
    /// Missing required keyword.
    #[error("expected `{expected}` at {line}:{column}")]
    ExpectedKeyword {
        /// What keyword was expected.
        expected: String,
        /// Line number.
        line: usize,
        /// Column number.
        column: usize,
    },
    /// Missing required delimiter.
    #[error("expected `{expected}` at {line}:{column}")]
    ExpectedDelimiter {
        /// What delimiter was expected.
        expected: String,
        /// Line number.
        line: usize,
        /// Column number.
        column: usize,
    },
}

/// Errors produced during command execution.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    /// The command was not found in PATH.
    #[error("command not found: {0}")]
    CommandNotFound(String),
    /// Permission denied when trying to execute.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    /// A required file was not found.
    #[error("no such file or directory: {0}")]
    FileNotFound(String),
    /// A redirect operation failed.
    #[error("redirect failed for '{target}': {reason}")]
    RedirectFailed {
        /// The target file of the redirect.
        target: String,
        /// The reason for failure.
        reason: String,
    },
    /// Failed to spawn a child process.
    #[error("failed to execute '{command}': {reason}")]
    SpawnFailed {
        /// The command that failed to spawn.
        command: String,
        /// The reason for failure.
        reason: String,
    },
    /// A directory operation failed.
    #[error("directory error: {0}")]
    DirError(String),
    /// The `cd` target directory is invalid.
    #[error("cd: {0}")]
    CdError(String),
    /// A function was not found.
    #[error("function not found: {0}")]
    FunctionNotFound(String),
    /// A variable expansion error.
    #[error("variable error: {0}")]
    VariableError(String),
    /// Break/continue outside loop.
    #[error("break/continue outside loop")]
    BreakOutsideLoop,
    /// Continue outside loop.
    #[error("continue outside loop")]
    ContinueOutsideLoop,
    /// Return outside function.
    #[error("return outside function")]
    ReturnOutsideFunction,
    /// An integer overflow.
    #[error("integer overflow")]
    IntegerOverflow,
    /// Arithmetic error.
    #[error("arithmetic error: {0}")]
    ArithmeticError(String),
}

impl ExecError {
    /// Returns the suggested exit code for this error.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::CommandNotFound(_) => 127,
            Self::PermissionDenied(_) => 126,
            Self::FunctionNotFound(_)
            | Self::VariableError(_)
            | Self::BreakOutsideLoop
            | Self::ContinueOutsideLoop
            | Self::ReturnOutsideFunction
            | Self::IntegerOverflow
            | Self::ArithmeticError(_)
            | Self::FileNotFound(_)
            | Self::RedirectFailed { .. }
            | Self::SpawnFailed { .. }
            | Self::DirError(_)
            | Self::CdError(_) => 1,
        }
    }
}

/// Errors from the configuration system.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read the config file.
    #[error("failed to read config at {path}: {source}")]
    Io {
        /// The config file path.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// Failed to parse the config file.
    #[error("failed to parse config: {0}")]
    Parse(String),
    /// The home directory could not be determined.
    #[error("could not determine home directory")]
    MissingHome,
    /// A config value is out of the allowed range.
    #[error("invalid config value for '{key}': {message}")]
    InvalidValue {
        /// The config key.
        key: String,
        /// Description of the invalidity.
        message: String,
    },
}

/// Errors from the plugin subsystem.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// The plugin directory could not be determined.
    #[error("could not determine plugin directory")]
    MissingPluginDir,
    /// Failed to read the plugin file.
    #[error("failed to read plugin file at {path}: {source}")]
    Io {
        /// The plugin file path.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// Failed to parse the plugin file.
    #[error("failed to parse plugin file '{path}': {reason}")]
    Parse {
        /// The plugin file path.
        path: PathBuf,
        /// The parse error description.
        reason: String,
    },
    /// The plugin was not found by name.
    #[error("plugin not found: {0}")]
    NotFound(String),
    /// A plugin with this name is already loaded.
    #[error("plugin already loaded: {0}")]
    AlreadyLoaded(String),
    /// The plugin file has an invalid name (empty or contains path separators).
    #[error("invalid plugin name '{0}'")]
    InvalidName(String),
    /// The plugin has a dependency on a missing plugin.
    #[error("plugin '{plugin}' depends on missing plugin '{dependency}'")]
    MissingDependency {
        /// The plugin that has the dependency.
        plugin: String,
        /// The missing dependency name.
        dependency: String,
    },
}

/// Errors from the history subsystem.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    /// Failed to read the history file.
    #[error("failed to read history file: {0}")]
    Io(#[source] std::io::Error),
    /// Failed to write the history file.
    #[error("failed to write history file: {0}")]
    WriteIo(#[source] std::io::Error),
    /// A history entry contained invalid data.
    #[error("invalid history entry: {0}")]
    InvalidEntry(String),
}

/// Checks if a string is a valid POSIX shell identifier.
#[must_use]
pub fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().expect("non-empty string");
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_error_from_lexer() {
        let err = ShellError::Lexer(LexerError::UnexpectedChar {
            ch: '@',
            line: 1,
            column: 5,
        });
        let msg = format!("{err}");
        assert!(msg.contains("unexpected character"));
        assert!(msg.contains("@"));
    }

    #[test]
    fn test_shell_error_from_parse() {
        let err = ShellError::Parse(ParseError::UnexpectedEof {
            expected: "command".into(),
        });
        let msg = format!("{err}");
        assert!(msg.contains("unexpected end of input"));
    }

    #[test]
    fn test_exec_error_exit_codes() {
        assert_eq!(ExecError::CommandNotFound("foo".into()).exit_code(), 127);
        assert_eq!(ExecError::PermissionDenied("bar".into()).exit_code(), 126);
        assert_eq!(ExecError::FileNotFound("baz".into()).exit_code(), 1);
        assert_eq!(ExecError::FunctionNotFound("f".into()).exit_code(), 1);
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::MissingHome;
        assert!(format!("{err}").contains("home directory"));
    }

    #[test]
    fn test_history_error_display() {
        let err = HistoryError::InvalidEntry("bad".into());
        assert!(format!("{err}").contains("bad"));
    }

    #[test]
    fn test_parse_error_display() {
        let err = ParseError::UnexpectedToken {
            token: "|".into(),
            line: 1,
            column: 3,
        };
        let msg = format!("{err}");
        assert!(msg.contains('|'));
        assert!(msg.contains("1:3"));
    }

    #[test]
    fn test_is_valid_identifier() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier("_bar"));
        assert!(is_valid_identifier("FOO_123"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("123foo"));
        assert!(!is_valid_identifier("foo-bar"));
    }
}
