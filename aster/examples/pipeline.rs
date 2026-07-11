//! Example: pipeline execution.
//!
//! Run with: `cargo run --example pipeline`

fn main() {
    use aster_executor::{ExecContext, ExecOutcome, Executor};
    use aster_lexer::Lexer;
    use aster_parser::Parser;

    let input = "echo -e 'hello\nworld\nfoo\nbar' | wc -l";
    let tokens = Lexer::new(input).tokenize().expect("lex error");
    let program = Parser::new(&tokens).parse().expect("parse error");
    let mut ctx = ExecContext::default();

    match Executor::execute(&program, &mut ctx).expect("exec error") {
        ExecOutcome::Success(code) => println!("exit code: {code}"),
        ExecOutcome::Exit(code) => println!("exit requested: {code}"),
        ExecOutcome::Break => println!("break encountered"),
        ExecOutcome::Continue => println!("continue encountered"),
    }
}
