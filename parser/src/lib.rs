//! Recursive-descent parser that converts a token stream into an AST.
//!
//! Grammar (precedence from lowest to highest):
//!
//! ```text
//! program     = statement (';' statement)*
//! statement   = compound_stmt
//!             | if_stmt | while_stmt | for_stmt | case_stmt
//!             | func_def | return_stmt | break_stmt | continue_stmt
//!             | assign | pipeline ('&&' pipeline | '||' pipeline)*
//! pipe        = atom ('|' atom)*
//! atom        = WORD+ redirect* | '(' program ')' | '{' program '}'
//! ```

use aster_shell_core::{
    AssignStmt, Atom, CaseArm, CaseStmt, ForStmt, FunctionDef, Group, IfStmt, PipeExpr, Redirect,
    RedirectKind, Statement, UntilStmt, WhileStmt,
};
use aster_shell_core::{ParseError, Program, ShellError, SimpleCommand, Span};

use aster_lexer::{Token, TokenKind};

const RETURN_WORDS: &[&str] = &["return"];
const IF_WORDS: &[&str] = &["if"];
const THEN_WORDS: &[&str] = &["then"];
const ELSE_WORDS: &[&str] = &["else"];
const ELIF_WORDS: &[&str] = &["elif"];
const FI_WORDS: &[&str] = &["fi"];
const WHILE_WORDS: &[&str] = &["while"];
const UNTIL_WORDS: &[&str] = &["until"];
const DO_WORDS: &[&str] = &["do"];
const DONE_WORDS: &[&str] = &["done"];
const FOR_WORDS: &[&str] = &["for"];
const IN_WORDS: &[&str] = &["in"];
const CASE_WORDS: &[&str] = &["case"];
const ESAC_WORDS: &[&str] = &["esac"];
const FUNCTION_WORDS: &[&str] = &["function"];

/// Recursive-descent parser for shell expressions.
pub struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    /// Creates a new parser over the given token slice.
    #[must_use]
    pub const fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parses the token stream and returns a complete [`Program`] AST.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] when the token stream is malformed.
    pub fn parse(&mut self) -> Result<Program, ShellError> {
        let program = self.parse_program()?;
        self.expect_eof()?;
        Ok(program)
    }

    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let start = self.current_span();
        let mut statements = vec![self.parse_statement()?];

        while self.peek_is(&TokenKind::Semicolon) {
            self.advance();
            self.skip_comments();
            if self.at_end() || self.peek_is(&TokenKind::Eof) {
                break;
            }
            statements.push(self.parse_statement()?);
        }

        let span = if let Some(last) = statements.last() {
            Span::merge(start, last.span())
        } else {
            start
        };

        Ok(Program { statements, span })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        self.skip_comments();

        // Check for reserved words first
        if let Some(kw) = self.peek_keyword() {
            match kw {
                "if" => return self.parse_if(),
                "while" => return self.parse_while(),
                "until" => return self.parse_until(),
                "for" => return self.parse_for(),
                "case" => return self.parse_case(),
                "function" => return self.parse_function_def(),
                "return" => return self.parse_return(),
                "break" => {
                    let span = self.current_span();
                    self.advance();
                    return Ok(Statement::Break(span));
                }
                "continue" => {
                    let span = self.current_span();
                    self.advance();
                    return Ok(Statement::Continue(span));
                }
                _ => {}
            }
        }

        // Check for compound command { ... }
        if self.peek_is(&TokenKind::OpenBrace) {
            return self.parse_compound();
        }

        // Check for POSIX function definition: name() { ... } or name () { ... }
        if let Some(func) = self.try_parse_posix_function_def()? {
            return Ok(func);
        }

        // Check for assignment: WORD=VALUE (must not have spaces around =)
        if let Some(assign) = self.try_parse_assign()? {
            return Ok(assign);
        }

        // Regular pipe with && / ||
        let mut left = Statement::Pipe(self.parse_pipe()?);

        loop {
            self.skip_comments();
            if self.peek_is(&TokenKind::AmpAmp) {
                self.advance();
                let right = Statement::Pipe(self.parse_pipe()?);
                left = Statement::And(Box::new(left), Box::new(right));
            } else if self.peek_is(&TokenKind::PipePipe) {
                self.advance();
                let right = Statement::Pipe(self.parse_pipe()?);
                left = Statement::Or(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn skip_optional_semicolon(&mut self) {
        self.skip_comments();
        if self.peek_is(&TokenKind::Semicolon) {
            self.advance();
            self.skip_comments();
        }
    }

    fn parse_if(&mut self) -> Result<Statement, ParseError> {
        let start = self.current_span();
        self.expect_keyword(IF_WORDS)?;

        // Parse condition as a full statement (allows && / || in conditions)
        let condition = Box::new(self.parse_condition_statement()?);
        self.skip_optional_semicolon();
        self.expect_keyword(THEN_WORDS)?;
        let body = self.parse_until_done(&["elif", "else", "fi"])?;

        let mut elif_branches = Vec::new();
        let mut else_body = None;

        loop {
            self.skip_comments();
            if self.peek_keyword_is(ELIF_WORDS) {
                self.advance();
                let cond = Box::new(self.parse_condition_statement()?);
                self.skip_optional_semicolon();
                self.expect_keyword(THEN_WORDS)?;
                let body = self.parse_until_done(&["elif", "else", "fi"])?;
                elif_branches.push((cond, body));
            } else if self.peek_keyword_is(ELSE_WORDS) {
                self.advance();
                else_body = Some(self.parse_until_done(&["fi"])?);
            } else if self.peek_keyword_is(FI_WORDS) {
                self.advance();
                break;
            } else {
                return Err(ParseError::ExpectedKeyword {
                    expected: "elif, else, or fi".into(),
                    line: self.current_span().line,
                    column: self.current_span().column,
                });
            }
        }

        let span = Span::merge(start, self.prev_span());
        Ok(Statement::If(IfStmt {
            condition,
            body,
            elif_branches,
            else_body,
            span,
        }))
    }

    fn parse_while(&mut self) -> Result<Statement, ParseError> {
        let start = self.current_span();
        self.expect_keyword(WHILE_WORDS)?;
        let condition = Box::new(self.parse_condition_statement()?);
        self.skip_optional_semicolon();
        self.expect_keyword(DO_WORDS)?;
        let body = self.parse_until_done(&["done"])?;
        self.expect_keyword(DONE_WORDS)?;

        let span = Span::merge(start, self.prev_span());
        Ok(Statement::While(WhileStmt {
            condition,
            body,
            span,
        }))
    }

    fn parse_until(&mut self) -> Result<Statement, ParseError> {
        let start = self.current_span();
        self.expect_keyword(UNTIL_WORDS)?;
        let condition = Box::new(self.parse_condition_statement()?);
        self.skip_optional_semicolon();
        self.expect_keyword(DO_WORDS)?;
        let body = self.parse_until_done(&["done"])?;
        self.expect_keyword(DONE_WORDS)?;

        let span = Span::merge(start, self.prev_span());
        Ok(Statement::Until(UntilStmt {
            condition,
            body,
            span,
        }))
    }

    fn parse_for(&mut self) -> Result<Statement, ParseError> {
        let start = self.current_span();
        self.expect_keyword(FOR_WORDS)?;
        let variable = self.expect_word_value()?;

        // Optional "in" keyword followed by words
        let words = if self.peek_keyword_is(IN_WORDS) {
            self.advance();
            let mut words = Vec::new();
            loop {
                self.skip_comments();
                if self.at_end()
                    || self.peek_is(&TokenKind::Eof)
                    || self.peek_keyword_is(DO_WORDS)
                    || self.peek_is(&TokenKind::Semicolon)
                {
                    break;
                }
                words.push(self.expect_word_value()?);
            }
            words
        } else {
            Vec::new()
        };

        self.skip_optional_semicolon();
        self.expect_keyword(DO_WORDS)?;
        let body = self.parse_until_done(&["done"])?;
        self.expect_keyword(DONE_WORDS)?;

        let span = Span::merge(start, self.prev_span());
        Ok(Statement::For(ForStmt {
            variable,
            words,
            body,
            span,
        }))
    }

    fn parse_case(&mut self) -> Result<Statement, ParseError> {
        let start = self.current_span();
        self.expect_keyword(CASE_WORDS)?;
        let word = self.expect_word_value()?;
        // optional "in"
        if self.peek_keyword_is(IN_WORDS) {
            self.advance();
        }

        let mut arms = Vec::new();
        loop {
            self.skip_comments();
            if self.peek_keyword_is(ESAC_WORDS) {
                self.advance();
                break;
            }

            // Parse patterns separated by '|'
            let mut patterns = Vec::new();
            patterns.push(self.expect_word_value()?);
            while self.peek_is(&TokenKind::Pipe) {
                self.advance();
                patterns.push(self.expect_word_value()?);
            }

            // Expect ')'
            self.expect(&TokenKind::RightParen)?;

            // Parse body until ';;' or 'esac'
            let body = self.parse_case_arm_body()?;
            let arm_span = Span::merge(
                patterns
                    .first()
                    .map_or(Span::dummy(), |_p| self.prev_span()),
                self.prev_span(),
            );
            arms.push(CaseArm {
                patterns,
                body,
                span: arm_span,
            });
        }

        let span = Span::merge(start, self.prev_span());
        Ok(Statement::Case(CaseStmt { word, arms, span }))
    }

    fn parse_case_arm_body(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();

        loop {
            self.skip_comments();
            if self.at_end() || self.peek_keyword_is(ESAC_WORDS) {
                break;
            }

            // Check for ;; terminator
            if self.peek_is(&TokenKind::Semicolon) {
                self.advance();
                if self.peek_is(&TokenKind::Semicolon) {
                    self.advance();
                    break;
                }
            }

            // Parse a statement, but stop if we hit esac
            if self.peek_keyword_is(ESAC_WORDS) {
                break;
            }
            statements.push(self.parse_statement()?);
        }

        Ok(statements)
    }

    fn parse_function_def(&mut self) -> Result<Statement, ParseError> {
        let start = self.current_span();
        self.expect_keyword(FUNCTION_WORDS)?;
        let name = self.expect_word_value()?;
        // Expect either { or ( for the body
        self.skip_comments();
        if self.peek_is(&TokenKind::OpenBrace) {
            self.advance();
            let body = self.parse_until_brace()?;
            self.expect(&TokenKind::CloseBrace)?;
            let span = Span::merge(start, self.prev_span());
            Ok(Statement::FunctionDef(FunctionDef { name, body, span }))
        } else {
            Err(ParseError::ExpectedDelimiter {
                expected: "{".into(),
                line: self.current_span().line,
                column: self.current_span().column,
            })
        }
    }

    fn parse_return(&mut self) -> Result<Statement, ParseError> {
        self.expect_keyword(RETURN_WORDS)?;
        self.skip_comments();
        let value = if self.at_end()
            || self.peek_is(&TokenKind::Eof)
            || self.peek_is(&TokenKind::Semicolon)
        {
            None
        } else {
            Some(self.expect_word_value()?)
        };
        Ok(Statement::Return(value))
    }

    fn parse_compound(&mut self) -> Result<Statement, ParseError> {
        self.expect(&TokenKind::OpenBrace)?;
        let body = self.parse_until_brace()?;
        self.expect(&TokenKind::CloseBrace)?;
        let span = self.prev_span();
        Ok(Statement::Compound(body, span))
    }

    fn parse_condition_statement(&mut self) -> Result<Statement, ParseError> {
        // A condition is a pipe optionally followed by && or ||
        let mut left = Statement::Pipe(self.parse_pipe()?);

        loop {
            self.skip_comments();
            if self.peek_is(&TokenKind::AmpAmp) {
                self.advance();
                let right = Statement::Pipe(self.parse_pipe()?);
                left = Statement::And(Box::new(left), Box::new(right));
            } else if self.peek_is(&TokenKind::PipePipe) {
                self.advance();
                let right = Statement::Pipe(self.parse_pipe()?);
                left = Statement::Or(Box::new(left), Box::new(right));
            } else {
                break;
            }
        }

        Ok(left)
    }

    fn parse_until_done(&mut self, terminators: &[&str]) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();

        loop {
            self.skip_comments();
            if self.at_end() || self.peek_is(&TokenKind::Eof) {
                return Err(ParseError::UnexpectedEof {
                    expected: format!("one of: {}", terminators.join(", ")),
                });
            }

            if let Some(kw) = self.peek_keyword() {
                if terminators.contains(&kw) {
                    break;
                }
            }

            // Also stop at } for compound commands
            if self.peek_is(&TokenKind::CloseBrace) {
                break;
            }

            statements.push(self.parse_statement()?);

            // Expect a semicolon separator if next token is not a terminator
            self.skip_comments();
            if !self.at_end()
                && !self.peek_is(&TokenKind::Eof)
                && !self.peek_is(&TokenKind::CloseBrace)
            {
                if self.peek_is(&TokenKind::Semicolon) {
                    self.advance();
                }
            }
        }

        Ok(statements)
    }

    fn parse_until_brace(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();

        loop {
            self.skip_comments();
            if self.at_end() || self.peek_is(&TokenKind::Eof) {
                return Err(ParseError::UnexpectedEof {
                    expected: "}".into(),
                });
            }
            if self.peek_is(&TokenKind::CloseBrace) {
                break;
            }

            statements.push(self.parse_statement()?);

            self.skip_comments();
            if !self.at_end()
                && !self.peek_is(&TokenKind::Eof)
                && !self.peek_is(&TokenKind::CloseBrace)
            {
                if self.peek_is(&TokenKind::Semicolon) {
                    self.advance();
                }
            }
        }

        Ok(statements)
    }

    fn try_parse_assign(&mut self) -> Result<Option<Statement>, ParseError> {
        // Single token: FOO=bar as one Word("FOO=bar") — no whitespace between name, =, value
        if let Some(TokenKind::Word(w)) = self.peek() {
            if let Some(eq_pos) = w.find('=') {
                if eq_pos > 0 {
                    let span = self.current_span();
                    let name = w[..eq_pos].to_string();
                    let value = w[eq_pos + 1..].to_string();
                    self.advance();
                    return Ok(Some(Statement::Assign(AssignStmt { name, value, span })));
                }
            }
        }
        // Two tokens: WORD '=' WORD (e.g. FOO = bar — separate tokens from lexer)
        if self.pos + 2 < self.tokens.len() {
            if let TokenKind::Word(name) = &self.tokens[self.pos].kind {
                if let TokenKind::Word(eq_or_value) = &self.tokens[self.pos + 1].kind {
                    if eq_or_value.starts_with('=') {
                        let span = self.tokens[self.pos].span;
                        let value = if eq_or_value.len() > 1 {
                            eq_or_value[1..].to_string()
                        } else {
                            if self.pos + 2 < self.tokens.len() {
                                if let TokenKind::Word(val) = &self.tokens[self.pos + 2].kind {
                                    self.pos += 3;
                                    return Ok(Some(Statement::Assign(AssignStmt {
                                        name: name.clone(),
                                        value: val.clone(),
                                        span,
                                    })));
                                }
                            }
                            return Ok(None);
                        };
                        self.pos += 2;
                        return Ok(Some(Statement::Assign(AssignStmt {
                            name: name.clone(),
                            value,
                            span,
                        })));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Tries to parse a POSIX function definition: `name() { ... }` or `name () { ... }`.
    fn try_parse_posix_function_def(&mut self) -> Result<Option<Statement>, ParseError> {
        if self.pos + 1 >= self.tokens.len() {
            return Ok(None);
        }
        if let TokenKind::Word(name) = &self.tokens[self.pos].kind {
            if let TokenKind::LeftParen = &self.tokens[self.pos + 1].kind {
                let start = self.tokens[self.pos].span;
                self.pos += 2; // skip name (
                // consume optional ) — some use `name()` without space
                if self.pos < self.tokens.len() {
                    if let TokenKind::RightParen = &self.tokens[self.pos].kind {
                        self.pos += 1;
                    }
                }
                self.skip_comments();
                if self.peek_is(&TokenKind::OpenBrace) {
                    self.advance();
                    let body = self.parse_until_brace()?;
                    self.expect(&TokenKind::CloseBrace)?;
                    let span = Span::merge(start, self.prev_span());
                    return Ok(Some(Statement::FunctionDef(FunctionDef {
                        name: name.clone(),
                        body,
                        span,
                    })));
                }
            }
        }
        Ok(None)
    }

    fn parse_pipe(&mut self) -> Result<PipeExpr, ParseError> {
        let start = self.current_span();
        let mut atoms = vec![self.parse_atom()?];

        while self.peek_is(&TokenKind::Pipe) {
            self.advance();
            atoms.push(self.parse_atom()?);
        }

        let span = if let Some(last) = atoms.last() {
            Span::merge(start, last.span())
        } else {
            start
        };

        Ok(PipeExpr { atoms, span })
    }

    fn parse_atom(&mut self) -> Result<Atom, ParseError> {
        self.skip_comments();

        if self.peek_is(&TokenKind::LeftParen) {
            return self.parse_group();
        }

        self.parse_command().map(Atom::Command)
    }

    fn parse_group(&mut self) -> Result<Atom, ParseError> {
        let open = self.expect(&TokenKind::LeftParen)?;
        let open_span = open.span;
        self.skip_comments();
        let program = self.parse_program()?;
        let close = self.expect(&TokenKind::RightParen)?;
        let span = Span::merge(open_span, close.span);

        Ok(Atom::Group(Group {
            body: program,
            span,
        }))
    }

    fn parse_command(&mut self) -> Result<SimpleCommand, ParseError> {
        self.skip_comments();
        let first_span = self.peek_token()?.span;
        let name = self.expect_word_value()?;
        let mut args = Vec::new();
        let mut redirects = Vec::new();

        loop {
            self.skip_comments();
            match self.peek() {
                Some(TokenKind::Word(w)) if Self::is_fd_prefix(w) => {
                    // Check if next token is a redirect: e.g. 2> or 2>>
                    if self.pos + 1 < self.tokens.len() {
                        let next = &self.tokens[self.pos + 1].kind;
                        if matches!(
                            next,
                            TokenKind::GreaterThan
                                | TokenKind::LessThan
                                | TokenKind::GreaterGreater
                                | TokenKind::GreaterAmp
                                | TokenKind::AmpGreater
                                | TokenKind::AmpGreaterGreater
                        ) {
                            let fd: u32 = w.parse().unwrap_or(1);
                            self.advance(); // skip the fd number
                            redirects.push(self.parse_redirect_with_fd(fd)?);
                            continue;
                        }
                    }
                    args.push(self.expect_word_value()?);
                }
                Some(TokenKind::Word(_)) => {
                    args.push(self.expect_word_value()?);
                }
                Some(TokenKind::SingleQuoted(s)) => {
                    args.push(s.clone());
                    self.advance();
                }
                Some(TokenKind::DoubleQuoted(s)) => {
                    args.push(s.clone());
                    self.advance();
                }
                Some(TokenKind::LessThan)
                | Some(TokenKind::GreaterThan)
                | Some(TokenKind::GreaterGreater)
                | Some(TokenKind::LessLess)
                | Some(TokenKind::LessLessLess)
                | Some(TokenKind::LessAmp)
                | Some(TokenKind::GreaterAmp)
                | Some(TokenKind::AmpGreater)
                | Some(TokenKind::AmpGreaterGreater) => {
                    redirects.push(self.parse_redirect()?);
                }
                _ => break,
            }
        }

        let span = Span::merge(first_span, self.prev_span());

        Ok(SimpleCommand {
            name,
            args,
            redirects,
            span,
        })
    }

    fn parse_redirect(&mut self) -> Result<Redirect, ParseError> {
        let token = self.peek_token()?;
        let span = token.span;
        let kind = match &token.kind {
            TokenKind::LessThan => RedirectKind::Input,
            TokenKind::GreaterThan => RedirectKind::Output,
            TokenKind::GreaterGreater => RedirectKind::Append,
            TokenKind::LessLess => RedirectKind::HereDoc,
            TokenKind::LessLessLess => RedirectKind::HereString,
            TokenKind::LessAmp => RedirectKind::FdInput,
            TokenKind::GreaterAmp => RedirectKind::FdOutput,
            TokenKind::AmpGreater => RedirectKind::FdOutput,
            TokenKind::AmpGreaterGreater => RedirectKind::FdAppend,
            _ => unreachable!(),
        };
        self.advance();
        self.skip_comments();
        let target = self.expect_word_value_with_quoted()?;

        let mut body = None;
        if kind == RedirectKind::HereDoc || kind == RedirectKind::HereString {
            if let Ok(next) = self.peek_token() {
                if let TokenKind::HereDocBody(content) = &next.kind {
                    body = Some(content.clone());
                    self.advance();
                }
            }
        }

        Ok(Redirect {
            fd: None,
            kind,
            target,
            body,
            span,
        })
    }

    fn parse_redirect_with_fd(&mut self, fd: u32) -> Result<Redirect, ParseError> {
        let token = self.peek_token()?;
        let span = token.span;
        let kind = match &token.kind {
            TokenKind::LessThan => RedirectKind::Input,
            TokenKind::GreaterThan => RedirectKind::Output,
            TokenKind::GreaterGreater => RedirectKind::Append,
            TokenKind::LessLess => RedirectKind::HereDoc,
            TokenKind::LessLessLess => RedirectKind::HereString,
            TokenKind::LessAmp => RedirectKind::FdInput,
            TokenKind::GreaterAmp => RedirectKind::FdOutput,
            TokenKind::AmpGreater => RedirectKind::FdOutput,
            TokenKind::AmpGreaterGreater => RedirectKind::FdAppend,
            _ => unreachable!(),
        };
        self.advance();
        self.skip_comments();
        let target = self.expect_word_value_with_quoted()?;

        let mut body = None;
        if kind == RedirectKind::HereDoc || kind == RedirectKind::HereString {
            if let Ok(next) = self.peek_token() {
                if let TokenKind::HereDocBody(content) = &next.kind {
                    body = Some(content.clone());
                    self.advance();
                }
            }
        }

        Ok(Redirect {
            fd: Some(fd),
            kind,
            target,
            body,
            span,
        })
    }

    fn is_fd_prefix(w: &str) -> bool {
        !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit())
    }

    fn expect_word_value(&mut self) -> Result<String, ParseError> {
        let token = self.peek_token()?;
        match &token.kind {
            TokenKind::Word(w) => {
                let val = w.clone();
                self.advance();
                Ok(val)
            }
            _ => Err(ParseError::UnexpectedToken {
                token: token.display(),
                line: token.span.line,
                column: token.span.column,
            }),
        }
    }

    fn expect_word_value_with_quoted(&mut self) -> Result<String, ParseError> {
        let token = self.peek_token()?;
        let val = match &token.kind {
            TokenKind::Word(w) => w.clone(),
            TokenKind::SingleQuoted(s) => s.clone(),
            TokenKind::DoubleQuoted(s) => s.clone(),
            _ => {
                return Err(ParseError::UnexpectedToken {
                    token: token.display(),
                    line: token.span.line,
                    column: token.span.column,
                });
            }
        };
        self.advance();
        Ok(val)
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<&Token, ParseError> {
        let token = self.peek_token()?;
        if std::mem::discriminant(&token.kind) == std::mem::discriminant(expected) {
            self.advance();
            Ok(&self.tokens[self.pos - 1])
        } else {
            Err(ParseError::UnexpectedToken {
                token: token.display(),
                line: token.span.line,
                column: token.span.column,
            })
        }
    }

    fn expect_keyword(&mut self, keywords: &[&str]) -> Result<(), ParseError> {
        let token = self.peek_token()?;
        if let TokenKind::Word(w) = &token.kind {
            if keywords.contains(&w.as_str()) {
                self.advance();
                return Ok(());
            }
        }
        Err(ParseError::ExpectedKeyword {
            expected: keywords.join(" or "),
            line: token.span.line,
            column: token.span.column,
        })
    }

    fn expect_eof(&self) -> Result<(), ParseError> {
        match self.peek() {
            Some(TokenKind::Eof) | None => Ok(()),
            Some(_) => {
                let t = &self.tokens[self.pos];
                Err(ParseError::UnexpectedToken {
                    token: t.display(),
                    line: t.span.line,
                    column: t.span.column,
                })
            }
        }
    }

    fn peek(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn peek_token(&self) -> Result<&Token, ParseError> {
        self.tokens
            .get(self.pos)
            .ok_or_else(|| ParseError::UnexpectedEof {
                expected: "token".into(),
            })
    }

    fn peek_is(&self, kind: &TokenKind) -> bool {
        self.peek()
            .is_some_and(|k| std::mem::discriminant(k) == std::mem::discriminant(kind))
    }

    fn peek_keyword(&self) -> Option<&str> {
        if let Some(TokenKind::Word(w)) = self.peek() {
            Some(w.as_str())
        } else {
            None
        }
    }

    fn peek_keyword_is(&self, keywords: &[&str]) -> bool {
        self.peek_keyword().is_some_and(|w| keywords.contains(&w))
    }

    const fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    const fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn skip_comments(&mut self) {
        while let Some(TokenKind::Comment(_)) = self.peek() {
            self.advance();
        }
    }

    fn current_span(&self) -> Span {
        self.tokens.get(self.pos).map_or(Span::dummy(), |t| t.span)
    }

    fn prev_span(&self) -> Span {
        if self.pos == 0 {
            return Span::dummy();
        }
        self.tokens[self.pos - 1].span
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_lexer::Lexer;

    fn parse_input(input: &str) -> Program {
        let tokens = Lexer::new(input).tokenize().unwrap();
        Parser::new(&tokens).parse().unwrap()
    }

    #[test]
    fn test_simple_command() {
        let prog = parse_input("echo hello world");
        assert_eq!(prog.statements.len(), 1);
        match &prog.statements[0] {
            Statement::Pipe(pipe) => {
                assert_eq!(pipe.atoms.len(), 1);
                match &pipe.atoms[0] {
                    Atom::Command(cmd) => {
                        assert_eq!(cmd.name, "echo");
                        assert_eq!(cmd.args, vec!["hello", "world"]);
                    }
                    _ => panic!("expected command"),
                }
            }
            _ => panic!("expected pipe"),
        }
    }

    #[test]
    fn test_pipe() {
        let prog = parse_input("ls | grep foo");
        match &prog.statements[0] {
            Statement::Pipe(pipe) => {
                assert_eq!(pipe.atoms.len(), 2);
            }
            _ => panic!("expected pipe"),
        }
    }

    #[test]
    fn test_and() {
        let prog = parse_input("true && echo hi");
        match &prog.statements[0] {
            Statement::And(_, _) => {}
            _ => panic!("expected and"),
        }
    }

    #[test]
    fn test_or() {
        let prog = parse_input("false || echo fail");
        match &prog.statements[0] {
            Statement::Or(_, _) => {}
            _ => panic!("expected or"),
        }
    }

    #[test]
    fn test_sequence() {
        let prog = parse_input("echo a ; echo b ; echo c");
        assert_eq!(prog.statements.len(), 3);
    }

    #[test]
    fn test_group() {
        let prog = parse_input("( echo a | echo b ) && echo c");
        match &prog.statements[0] {
            Statement::And(left, _) => match left.as_ref() {
                Statement::Pipe(pipe) => {
                    assert_eq!(pipe.atoms.len(), 1);
                    match &pipe.atoms[0] {
                        Atom::Group(g) => {
                            assert_eq!(g.body.statements.len(), 1);
                        }
                        _ => panic!("expected group"),
                    }
                }
                _ => panic!("expected pipe"),
            },
            _ => panic!("expected and"),
        }
    }

    #[test]
    fn test_redirect() {
        let prog = parse_input("echo hello > out.txt");
        match &prog.statements[0] {
            Statement::Pipe(pipe) => match &pipe.atoms[0] {
                Atom::Command(cmd) => {
                    assert_eq!(cmd.redirects.len(), 1);
                    assert_eq!(cmd.redirects[0].kind, RedirectKind::Output);
                    assert_eq!(cmd.redirects[0].target, "out.txt");
                }
                _ => panic!("expected command"),
            },
            _ => panic!("expected pipe"),
        }
    }

    #[test]
    fn test_append_redirect() {
        let prog = parse_input("echo hello >> out.txt");
        match &prog.statements[0] {
            Statement::Pipe(pipe) => match &pipe.atoms[0] {
                Atom::Command(cmd) => {
                    assert_eq!(cmd.redirects[0].kind, RedirectKind::Append);
                }
                _ => panic!("expected command"),
            },
            _ => panic!("expected pipe"),
        }
    }

    #[test]
    fn test_input_redirect() {
        let prog = parse_input("cat < input.txt");
        match &prog.statements[0] {
            Statement::Pipe(pipe) => match &pipe.atoms[0] {
                Atom::Command(cmd) => {
                    assert_eq!(cmd.redirects[0].kind, RedirectKind::Input);
                    assert_eq!(cmd.redirects[0].target, "input.txt");
                }
                _ => panic!("expected command"),
            },
            _ => panic!("expected pipe"),
        }
    }

    #[test]
    fn test_complex_pipeline() {
        let prog = parse_input("cat < in.txt | grep error | wc -l > count.txt");
        match &prog.statements[0] {
            Statement::Pipe(pipe) => {
                assert_eq!(pipe.atoms.len(), 3);
            }
            _ => panic!("expected pipe"),
        }
    }

    #[test]
    fn test_precedence() {
        let prog = parse_input("a ; b && c ; d");
        assert_eq!(prog.statements.len(), 3);
    }

    #[test]
    fn test_if_simple() {
        let prog = parse_input("if true ; then echo yes ; fi");
        match &prog.statements[0] {
            Statement::If(ifstmt) => {
                assert_eq!(ifstmt.body.len(), 1);
                assert!(ifstmt.elif_branches.is_empty());
                assert!(ifstmt.else_body.is_none());
            }
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn test_if_else() {
        let prog = parse_input("if false ; then echo no ; else echo yes ; fi");
        match &prog.statements[0] {
            Statement::If(ifstmt) => {
                assert!(ifstmt.else_body.is_some());
            }
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn test_if_elif() {
        let prog =
            parse_input("if false ; then echo 1 ; elif true ; then echo 2 ; else echo 3 ; fi");
        match &prog.statements[0] {
            Statement::If(ifstmt) => {
                assert_eq!(ifstmt.elif_branches.len(), 1);
                assert!(ifstmt.else_body.is_some());
            }
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn test_while_simple() {
        let prog = parse_input("while true ; do echo loop ; done");
        match &prog.statements[0] {
            Statement::While(ws) => {
                assert_eq!(ws.body.len(), 1);
            }
            _ => panic!("expected while"),
        }
    }

    #[test]
    fn test_for_simple() {
        let prog = parse_input("for i in 1 2 3 ; do echo $i ; done");
        match &prog.statements[0] {
            Statement::For(fs) => {
                assert_eq!(fs.variable, "i");
                assert_eq!(fs.words, vec!["1", "2", "3"]);
                assert_eq!(fs.body.len(), 1);
            }
            _ => panic!("expected for"),
        }
    }

    #[test]
    fn test_function_def() {
        let prog = parse_input("function greet { echo hello ; }");
        match &prog.statements[0] {
            Statement::FunctionDef(fd) => {
                assert_eq!(fd.name, "greet");
                assert_eq!(fd.body.len(), 1);
            }
            _ => panic!("expected function def"),
        }
    }

    #[test]
    fn test_return() {
        let prog = parse_input("return 0");
        match &prog.statements[0] {
            Statement::Return(v) => {
                assert_eq!(v.as_deref(), Some("0"));
            }
            _ => panic!("expected return"),
        }
    }

    #[test]
    fn test_break_continue() {
        let prog = parse_input("break ; continue");
        assert_eq!(prog.statements.len(), 2);
        assert!(matches!(prog.statements[0], Statement::Break(_)));
        assert!(matches!(prog.statements[1], Statement::Continue(_)));
    }

    #[test]
    fn test_compound() {
        let prog = parse_input("{ echo a ; echo b ; }");
        match &prog.statements[0] {
            Statement::Compound(stmts, _) => {
                assert_eq!(stmts.len(), 2);
            }
            _ => panic!("expected compound"),
        }
    }

    #[test]
    fn test_assignment() {
        let prog = parse_input("FOO=bar");
        match &prog.statements[0] {
            Statement::Assign(a) => {
                assert_eq!(a.name, "FOO");
                assert_eq!(a.value, "bar");
            }
            _ => panic!("expected assign"),
        }
    }

    #[test]
    fn test_heredoc_redirect() {
        let prog = parse_input("cat << EOF");
        match &prog.statements[0] {
            Statement::Pipe(pipe) => match &pipe.atoms[0] {
                Atom::Command(cmd) => {
                    assert_eq!(cmd.redirects[0].kind, RedirectKind::HereDoc);
                    assert_eq!(cmd.redirects[0].target, "EOF");
                }
                _ => panic!("expected command"),
            },
            _ => panic!("expected pipe"),
        }
    }

    #[test]
    fn test_function_def_simple() {
        let tokens = aster_lexer::Lexer::new("function f { true }")
            .tokenize()
            .unwrap();
        for t in &tokens {
            eprintln!("tok: {:?} at {}:{}", t.kind, t.span.line, t.span.column);
        }
        let result = Parser::new(&tokens).parse();
        match &result {
            Ok(p) => eprintln!("OK: {} statements", p.statements.len()),
            Err(e) => eprintln!("ERR: {e}"),
        }
        let prog = result.unwrap();
        match &prog.statements[0] {
            Statement::FunctionDef(fd) => {
                assert_eq!(fd.name, "f");
            }
            other => panic!("expected function def, got: {other:?}"),
        }
    }
}
