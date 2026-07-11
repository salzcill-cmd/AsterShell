//! Lexer crate: tokenizes shell input into a stream of tokens.

pub mod lexer;
pub mod token;

pub use lexer::Lexer;
pub use token::{Token, TokenKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reexports() {
        let mut lex = Lexer::new("echo");
        let tokens = lex.tokenize().unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Word("echo".into()));
    }
}
