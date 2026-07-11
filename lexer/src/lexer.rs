//! Lexer (tokenizer) that converts raw input into a stream of tokens.

use aster_shell_core::{LexerError, Span};

use crate::token::{Token, TokenKind};

/// Tokenizer that converts a raw shell input string into tokens.
pub struct Lexer<'a> {
    input: &'a str,
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    /// Creates a new lexer for the given input string.
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    /// Tokenizes the entire input and returns all tokens including the final `Eof`.
    ///
    /// # Errors
    ///
    /// Returns [`LexerError`] on unterminated strings or escape sequences.
    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexerError> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace_and_comments(&mut tokens)?;
            if self.at_end() {
                tokens.push(self.make_token(TokenKind::Eof));
                break;
            }
            if let Some(token) = self.next_token()? {
                tokens.push(token);
            }
        }

        Ok(tokens)
    }

    /// Returns the original input string.
    #[must_use]
    pub const fn input(&self) -> &'a str {
        self.input
    }

    fn next_token(&mut self) -> Result<Option<Token>, LexerError> {
        self.skip_whitespace();

        if self.at_end() {
            return Ok(None);
        }

        let ch = self.peek().unwrap();
        let start_line = self.line;
        let start_col = self.column;
        let start_offset = self.pos;

        match ch {
            '\'' => {
                self.advance();
                self.read_single_quote(start_line, start_col, start_offset)
                    .map(Some)
            }
            '"' => {
                self.advance();
                self.read_double_quote(start_line, start_col, start_offset)
                    .map(Some)
            }
            '|' => {
                self.advance();
                if self.peek() == Some('|') {
                    self.advance();
                    Ok(Some(self.make_token_at(
                        TokenKind::PipePipe,
                        start_line,
                        start_col,
                        start_offset,
                    )))
                } else {
                    Ok(Some(self.make_token_at(
                        TokenKind::Pipe,
                        start_line,
                        start_col,
                        start_offset,
                    )))
                }
            }
            '&' => {
                self.advance();
                if self.peek() == Some('&') {
                    self.advance();
                    Ok(Some(self.make_token_at(
                        TokenKind::AmpAmp,
                        start_line,
                        start_col,
                        start_offset,
                    )))
                } else if self.peek() == Some('>') {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        Ok(Some(self.make_token_at(
                            TokenKind::AmpGreaterGreater,
                            start_line,
                            start_col,
                            start_offset,
                        )))
                    } else {
                        Ok(Some(self.make_token_at(
                            TokenKind::AmpGreater,
                            start_line,
                            start_col,
                            start_offset,
                        )))
                    }
                } else {
                    Ok(Some(self.make_token_at(
                        TokenKind::Amp,
                        start_line,
                        start_col,
                        start_offset,
                    )))
                }
            }
            ';' => {
                self.advance();
                Ok(Some(self.make_token_at(
                    TokenKind::Semicolon,
                    start_line,
                    start_col,
                    start_offset,
                )))
            }
            '<' => {
                self.advance();
                if self.peek() == Some('<') {
                    self.advance();
                    if self.peek() == Some('<') {
                        self.advance();
                        Ok(Some(self.make_token_at(
                            TokenKind::LessLessLess,
                            start_line,
                            start_col,
                            start_offset,
                        )))
                    } else {
                        Ok(Some(self.make_token_at(
                            TokenKind::LessLess,
                            start_line,
                            start_col,
                            start_offset,
                        )))
                    }
                } else if self.peek() == Some('&') {
                    self.advance();
                    Ok(Some(self.make_token_at(
                        TokenKind::LessAmp,
                        start_line,
                        start_col,
                        start_offset,
                    )))
                } else {
                    Ok(Some(self.make_token_at(
                        TokenKind::LessThan,
                        start_line,
                        start_col,
                        start_offset,
                    )))
                }
            }
            '>' => {
                self.advance();
                if self.peek() == Some('>') {
                    self.advance();
                    Ok(Some(self.make_token_at(
                        TokenKind::GreaterGreater,
                        start_line,
                        start_col,
                        start_offset,
                    )))
                } else if self.peek() == Some('&') {
                    self.advance();
                    Ok(Some(self.make_token_at(
                        TokenKind::GreaterAmp,
                        start_line,
                        start_col,
                        start_offset,
                    )))
                } else {
                    Ok(Some(self.make_token_at(
                        TokenKind::GreaterThan,
                        start_line,
                        start_col,
                        start_offset,
                    )))
                }
            }
            '(' => {
                self.advance();
                Ok(Some(self.make_token_at(
                    TokenKind::LeftParen,
                    start_line,
                    start_col,
                    start_offset,
                )))
            }
            ')' => {
                self.advance();
                Ok(Some(self.make_token_at(
                    TokenKind::RightParen,
                    start_line,
                    start_col,
                    start_offset,
                )))
            }
            '{' => {
                self.advance();
                Ok(Some(self.make_token_at(
                    TokenKind::OpenBrace,
                    start_line,
                    start_col,
                    start_offset,
                )))
            }
            '}' => {
                self.advance();
                Ok(Some(self.make_token_at(
                    TokenKind::CloseBrace,
                    start_line,
                    start_col,
                    start_offset,
                )))
            }
            '\\' => {
                self.advance();
                self.read_escaped_word(start_line, start_col, start_offset)
                    .map(Some)
            }
            _ => Ok(Some(self.read_word(start_line, start_col, start_offset))),
        }
    }

    fn read_word(&mut self, start_line: usize, start_col: usize, start_offset: usize) -> Token {
        let mut word = String::new();

        while let Some(ch) = self.peek() {
            match ch {
                '|' | '&' | ';' | '<' | '>' | '(' | ')' | '\'' | '"' | '{' | '}' => break,
                c if c.is_ascii_whitespace() => break,
                '\\' => {
                    self.advance();
                    if let Some(next) = self.peek() {
                        word.push(next);
                        self.advance();
                    }
                }
                _ => {
                    word.push(ch);
                    self.advance();
                }
            }
        }

        self.make_token_at(TokenKind::Word(word), start_line, start_col, start_offset)
    }

    fn read_single_quote(
        &mut self,
        start_line: usize,
        start_col: usize,
        start_offset: usize,
    ) -> Result<Token, LexerError> {
        let mut content = String::new();

        loop {
            if self.at_end() {
                return Err(LexerError::UnterminatedSingleQuote {
                    line: start_line,
                    column: start_col,
                });
            }
            let ch = self.peek().unwrap();
            if ch == '\'' {
                self.advance();
                break;
            }
            content.push(ch);
            self.advance();
        }

        Ok(self.make_token_at(
            TokenKind::SingleQuoted(content),
            start_line,
            start_col,
            start_offset,
        ))
    }

    fn read_double_quote(
        &mut self,
        start_line: usize,
        start_col: usize,
        start_offset: usize,
    ) -> Result<Token, LexerError> {
        let mut content = String::new();

        loop {
            if self.at_end() {
                return Err(LexerError::UnterminatedDoubleQuote {
                    line: start_line,
                    column: start_col,
                });
            }
            let ch = self.peek().unwrap();
            match ch {
                '"' => {
                    self.advance();
                    break;
                }
                '\\' => {
                    self.advance();
                    if let Some(escaped) = self.peek() {
                        match escaped {
                            'n' => content.push('\n'),
                            't' => content.push('\t'),
                            '\\' => content.push('\\'),
                            '"' => content.push('"'),
                            '$' => content.push('$'),
                            '`' => content.push('`'),
                            _ => {
                                content.push('\\');
                                content.push(escaped);
                            }
                        }
                        self.advance();
                    } else {
                        return Err(LexerError::UnterminatedEscape {
                            line: self.line,
                            column: self.column,
                        });
                    }
                }
                _ => {
                    content.push(ch);
                    self.advance();
                }
            }
        }

        Ok(self.make_token_at(
            TokenKind::DoubleQuoted(content),
            start_line,
            start_col,
            start_offset,
        ))
    }

    fn read_escaped_word(
        &mut self,
        start_line: usize,
        start_col: usize,
        start_offset: usize,
    ) -> Result<Token, LexerError> {
        if self.at_end() {
            return Err(LexerError::UnterminatedEscape {
                line: start_line,
                column: start_col,
            });
        }
        let ch = self.peek().unwrap();
        self.advance();
        Ok(self.make_token_at(
            TokenKind::Word(ch.to_string()),
            start_line,
            start_col,
            start_offset,
        ))
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_ascii_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_whitespace_and_comments(&mut self, tokens: &mut Vec<Token>) -> Result<(), LexerError> {
        loop {
            self.skip_whitespace();
            if self.peek() == Some('#') {
                let start_line = self.line;
                let start_col = self.column;
                let start_offset = self.pos;
                self.advance();
                let mut comment = String::new();
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    comment.push(ch);
                    self.advance();
                }
                tokens.push(self.make_token_at(
                    TokenKind::Comment(comment),
                    start_line,
                    start_col,
                    start_offset,
                ));
            } else {
                break;
            }
        }
        Ok(())
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    const fn make_token(&self, kind: TokenKind) -> Token {
        Token {
            kind,
            span: Span::new(self.line, self.column, self.pos, 0),
        }
    }

    const fn make_token_at(
        &self,
        kind: TokenKind,
        line: usize,
        column: usize,
        offset: usize,
    ) -> Token {
        Token {
            kind,
            span: Span::new(line, column, offset, self.pos.saturating_sub(offset)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize_all(input: &str) -> Vec<TokenKind> {
        Lexer::new(input)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn test_simple_command() {
        let tokens = tokenize_all("echo hello world");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("echo".into()),
                TokenKind::Word("hello".into()),
                TokenKind::Word("world".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_pipe() {
        let tokens = tokenize_all("ls | grep foo");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("ls".into()),
                TokenKind::Pipe,
                TokenKind::Word("grep".into()),
                TokenKind::Word("foo".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_operators() {
        let tokens = tokenize_all("a && b || c");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("a".into()),
                TokenKind::AmpAmp,
                TokenKind::Word("b".into()),
                TokenKind::PipePipe,
                TokenKind::Word("c".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_redirects() {
        let tokens = tokenize_all("echo > out >> append < inp");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("echo".into()),
                TokenKind::GreaterThan,
                TokenKind::Word("out".into()),
                TokenKind::GreaterGreater,
                TokenKind::Word("append".into()),
                TokenKind::LessThan,
                TokenKind::Word("inp".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_semicolon() {
        let tokens = tokenize_all("a ; b");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("a".into()),
                TokenKind::Semicolon,
                TokenKind::Word("b".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_parens() {
        let tokens = tokenize_all("(a | b)");
        assert_eq!(
            tokens,
            vec![
                TokenKind::LeftParen,
                TokenKind::Word("a".into()),
                TokenKind::Pipe,
                TokenKind::Word("b".into()),
                TokenKind::RightParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_braces() {
        let tokens = tokenize_all("{ a ; b }");
        assert_eq!(
            tokens,
            vec![
                TokenKind::OpenBrace,
                TokenKind::Word("a".into()),
                TokenKind::Semicolon,
                TokenKind::Word("b".into()),
                TokenKind::CloseBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_background() {
        let tokens = tokenize_all("sleep 10 &");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("sleep".into()),
                TokenKind::Word("10".into()),
                TokenKind::Amp,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_heredoc_tokens() {
        let tokens = tokenize_all("cat << EOF");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("cat".into()),
                TokenKind::LessLess,
                TokenKind::Word("EOF".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_here_string_tokens() {
        let tokens = tokenize_all("cat <<< hello");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("cat".into()),
                TokenKind::LessLessLess,
                TokenKind::Word("hello".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_fd_redirect_tokens() {
        let tokens = tokenize_all("echo >&2");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("echo".into()),
                TokenKind::GreaterAmp,
                TokenKind::Word("2".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_stderr_redirect_tokens() {
        let tokens = tokenize_all("cmd &> log.txt");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("cmd".into()),
                TokenKind::AmpGreater,
                TokenKind::Word("log.txt".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_single_quoted_string() {
        let tokens = tokenize_all("echo 'hello world'");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("echo".into()),
                TokenKind::SingleQuoted("hello world".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_double_quoted_string() {
        let tokens = tokenize_all(r#"echo "hello world""#);
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("echo".into()),
                TokenKind::DoubleQuoted("hello world".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_double_quoted_escapes() {
        let tokens = tokenize_all(r#"echo "line\n""#);
        assert_eq!(
            tokens,
            vec![
                TokenKind::Word("echo".into()),
                TokenKind::DoubleQuoted("line\n".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_unterminated_double_quote() {
        let result = Lexer::new("echo \"hello").tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_unterminated_single_quote() {
        let result = Lexer::new("echo 'hello").tokenize();
        assert!(result.is_err());
    }

    #[test]
    fn test_comment() {
        let tokens = tokenize_all("echo # this is a comment");
        assert!(tokens.contains(&TokenKind::Comment(" this is a comment".into())));
    }

    #[test]
    fn test_empty_input() {
        let tokens = tokenize_all("");
        assert_eq!(tokens, vec![TokenKind::Eof]);
    }

    #[test]
    fn test_token_kind_name() {
        assert_eq!(TokenKind::Pipe.name(), "|");
        assert_eq!(TokenKind::Word("x".into()).name(), "word");
        assert_eq!(TokenKind::Amp.name(), "&");
        assert_eq!(TokenKind::LessLess.name(), "<<");
        assert_eq!(TokenKind::OpenBrace.name(), "{");
        assert_eq!(TokenKind::CloseBrace.name(), "}");
        assert_eq!(TokenKind::Eof.name(), "end of input");
    }
}
