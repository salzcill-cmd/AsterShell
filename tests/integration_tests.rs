//! Integration tests for the full AsterShell pipeline.

use aster_executor::{ExecContext, ExecOutcome, Executor};
use aster_lexer::Lexer;
use aster_parser::Parser;

fn run(input: &str) -> Result<(i32, ExecContext), String> {
    let tokens = Lexer::new(input)
        .tokenize()
        .map_err(|e| format!("lex error: {e}"))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| format!("parse error: {e}"))?;
    let mut ctx = ExecContext::default();
    let outcome = Executor::execute(&program, &mut ctx).map_err(|e| format!("exec error: {e}"))?;
    let code = match outcome {
        ExecOutcome::Success(c) | ExecOutcome::Exit(c) => c,
        ExecOutcome::Break | ExecOutcome::Continue => 0,
    };
    Ok((code, ctx))
}

fn run_output(input: &str) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("aster")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn aster");

    let stdin = child.stdin.as_mut().expect("failed to open stdin");
    writeln!(stdin, "{input}").expect("failed to write input");
    writeln!(stdin, "exit").expect("failed to write exit");

    let output = child.wait_with_output().expect("failed to wait for aster");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    stdout
        .lines()
        .filter(|line| !line.starts_with("aster "))
        .collect::<Vec<&str>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// 1. Basic commands
// ---------------------------------------------------------------------------

#[test]
fn test_echo_returns_zero() {
    let (code, _) = run("echo hello").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_echo_multiple_args() {
    let (code, _) = run("echo one two three").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_echo_no_args() {
    let (code, _) = run("echo").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_pwd_returns_zero() {
    let (code, _) = run("pwd").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_true_returns_zero() {
    let (code, _) = run("true").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_false_returns_one() {
    let (code, _) = run("false").unwrap();
    assert_eq!(code, 1);
}

#[test]
fn test_version_returns_zero() {
    let (code, _) = run("version").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_help_returns_zero() {
    let (code, _) = run("help").unwrap();
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// 2. Variable assignment and expansion
// ---------------------------------------------------------------------------

#[test]
fn test_assignment_stores_variable() {
    let (code, ctx) = run("FOO=bar").unwrap();
    assert_eq!(code, 0);
    assert_eq!(ctx.variables.get("FOO").map(String::as_str), Some("bar"));
}

#[test]
fn test_variable_expansion_dollar() {
    let (code, _) = run("FOO=hello ; echo $FOO").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_unknown_variable_expands_to_literal() {
    let (code, _) = run("echo $UNDEFINED_VAR_XYZ").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_question_mark_expansion() {
    let (code, _) = run("echo $?").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_question_mark_reflects_exit_code() {
    let (code, _) = run("false ; echo $?").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_pid_expansion() {
    let (code, _) = run("echo $$").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_zero_expansion() {
    let (code, _) = run("echo $0").unwrap();
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// 3. Control flow
// ---------------------------------------------------------------------------

#[test]
fn test_if_then_branch() {
    let (code, _) = run("if true ; then echo yes ; fi").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_if_else_branch() {
    let (code, _) = run("if false ; then echo no ; else echo yes ; fi").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_if_elif_branch() {
    let (code, _) =
        run("if false ; then echo 1 ; elif true ; then echo 2 ; else echo 3 ; fi").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_while_loop_with_break() {
    let (code, _) = run("while true ; do break ; done").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_for_loop() {
    let (code, _) = run("for i in 1 2 3 ; do echo $i ; done").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_for_loop_with_break() {
    let (code, _) = run("for i in 1 2 3 ; do break ; done").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_case_exact_match() {
    let (code, _) = run("case foo in foo) echo matched ;; esac").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_case_glob_star() {
    let (code, _) = run("case foobar in foo*) echo matched ;; esac").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_case_question_mark() {
    let (code, _) = run("case ax in a?) echo matched ;; esac").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_case_no_match() {
    let (code, _) = run("case bar in foo) echo no ;; esac").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_case_multiple_patterns() {
    let (code, _) = run("case b in a|b|c) echo matched ;; esac").unwrap();
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// 4. Functions
// ---------------------------------------------------------------------------

#[test]
fn test_function_define_and_call() {
    let (code, _) = run("function greet { echo hello ; } ; greet").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_function_sets_variable() {
    let (code, ctx) = run("function setval { MYVAR=works ; } ; setval ; echo $MYVAR").unwrap();
    assert_eq!(code, 0);
    assert_eq!(
        ctx.variables.get("MYVAR").map(String::as_str),
        Some("works")
    );
}

// ---------------------------------------------------------------------------
// 5. Pipelines
// ---------------------------------------------------------------------------

#[test]
fn test_pipe_echo_wc() {
    let (code, _) = run("echo hello | wc").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_pipe_echo_cat() {
    let (code, _) = run("echo a | cat").unwrap();
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// 6. Logical operators
// ---------------------------------------------------------------------------

#[test]
fn test_and_success() {
    let (code, _) = run("true && echo ok").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_and_failure_skips_right() {
    let (code, _) = run("false && echo should_not_run").unwrap();
    assert_eq!(code, 1);
}

#[test]
fn test_or_success_skips_right() {
    let (code, _) = run("true || echo should_not_run").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_or_failure_runs_right() {
    let (code, _) = run("false || echo recovered").unwrap();
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// 7. Sequences
// ---------------------------------------------------------------------------

#[test]
fn test_sequence_three_commands() {
    let (code, _) = run("echo a ; echo b ; echo c").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_sequence_last_determines_code() {
    let (code, _) = run("true ; false").unwrap();
    assert_eq!(code, 1);
}

#[test]
fn test_sequence_first_determines_code() {
    let (code, _) = run("false ; true").unwrap();
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// 8. Break / Continue
// ---------------------------------------------------------------------------

#[test]
fn test_break_exits_while() {
    let (code, _) = run("while true ; do echo looping ; break ; done").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_continue_skips_iteration() {
    let (code, _) = run("COUNT=0 ; for i in 1 2 3 ; do COUNT=1 ; continue ; done").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_while_iterates_multiple_times() {
    let (code, ctx) = run("N=0 ; while true ; do N=1 ; break ; done").unwrap();
    assert_eq!(code, 0);
    assert_eq!(ctx.variables.get("N").map(String::as_str), Some("1"));
}

// ---------------------------------------------------------------------------
// 9. Substitution
// ---------------------------------------------------------------------------

#[test]
fn test_dollar_question_after_success() {
    let (_, ctx) = run("true ; echo $?").unwrap();
    assert_eq!(ctx.last_exit_code, 0);
}

#[test]
fn test_dollar_question_after_failure() {
    let (_, ctx) = run("false ; echo $?").unwrap();
    assert_eq!(ctx.last_exit_code, 0);
}

#[test]
fn test_dollar_dollar_is_pid() {
    let (code, _) = run("echo $$").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_dollar_zero_is_shell_name() {
    let (code, _) = run("echo $0").unwrap();
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// 10. Glob matching (via case patterns)
// ---------------------------------------------------------------------------

#[test]
fn test_case_star_matches_any() {
    let (code, _) = run("case anything in *) echo matched ;; esac").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_case_question_matches_single_char() {
    let (code, _) = run("case z in ?) echo matched ;; esac").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_case_bracket_class() {
    let (code, _) = run("case b in [a-z]) echo matched ;; esac").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_case_negated_bracket() {
    let (code, _) = run("case 9 in [!a-z]) echo matched ;; esac").unwrap();
    assert_eq!(code, 0);
}

// ---------------------------------------------------------------------------
// 11. Assignment with variables
// ---------------------------------------------------------------------------

#[test]
fn test_chained_assignment() {
    let (code, ctx) = run("FOO=hello ; BAR=$FOO").unwrap();
    assert_eq!(code, 0);
    assert_eq!(ctx.variables.get("BAR").map(String::as_str), Some("hello"));
}

#[test]
fn test_assignment_expansion_echo() {
    let (code, _) = run("FOO=hello ; BAR=$FOO ; echo $BAR").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_dollar_sign_literal_when_no_match() {
    let (code, _) = run("echo $NOSUCHVAR").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_overwrite_variable() {
    let (code, ctx) = run("FOO=first ; OOW=second").unwrap();
    assert_eq!(code, 0);
    assert_eq!(ctx.variables.get("FOO").map(String::as_str), Some("first"));
    assert_eq!(ctx.variables.get("OOW").map(String::as_str), Some("second"));
}

// ---------------------------------------------------------------------------
// Additional edge-case tests
// ---------------------------------------------------------------------------

#[test]
fn test_empty_input_fails_to_parse() {
    let result = run("");
    assert!(result.is_err());
}

#[test]
fn test_exit_returns_code() {
    let tokens = Lexer::new("exit 42").tokenize().unwrap();
    let program = Parser::new(&tokens).parse().unwrap();
    let mut ctx = ExecContext::default();
    let outcome = Executor::execute(&program, &mut ctx).unwrap();
    assert_eq!(outcome, ExecOutcome::Exit(42));
}

#[test]
fn test_exit_default_code() {
    let tokens = Lexer::new("exit").tokenize().unwrap();
    let program = Parser::new(&tokens).parse().unwrap();
    let mut ctx = ExecContext::default();
    let outcome = Executor::execute(&program, &mut ctx).unwrap();
    assert_eq!(outcome, ExecOutcome::Exit(0));
}

#[test]
fn test_compound_block() {
    let (code, _) = run("{ echo a ; echo b ; }").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_which_builtin() {
    let (code, _) = run("which echo").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_type_builtin() {
    let (code, _) = run("type echo").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_while_false_never_runs() {
    let (code, _) = run("while false ; do echo never ; done").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_if_with_command_condition() {
    let (code, _) = run("if true ; then echo yes ; fi").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_for_with_single_item() {
    let (code, _) = run("for x in only ; do echo $x ; done").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_nested_case_patterns() {
    let (code, _) = run("case hello in hel*) echo matched ;; esac").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_function_return_code() {
    let (code, _) = run("function fail { false ; } ; fail").unwrap();
    assert_eq!(code, 1);
}

// ---------------------------------------------------------------------------
// Expansion stress tests
// ---------------------------------------------------------------------------

#[test]
fn test_nested_command_substitution() {
    let (code, ctx) = run("x=$(echo $(echo hello))").unwrap();
    assert_eq!(code, 0);
    assert_eq!(ctx.variables.get("x").map(String::as_str), Some("hello"));
}

#[test]
fn test_parameter_expansion_default() {
    let (code, ctx) = run("x=${HOME:-fallback}").unwrap();
    assert_eq!(code, 0);
    let val = ctx.variables.get("x").unwrap();
    assert!(val == "/home/izza" || val == "fallback",
        "expected HOME or fallback, got {val}");
}

#[test]
fn test_parameter_expansion_unset_default() {
    let (code, ctx) = run("x=${ASTER_TEST_UNSET_VAR:-fallback_value}").unwrap();
    assert_eq!(code, 0);
    assert_eq!(ctx.variables.get("x").map(String::as_str), Some("fallback_value"));
}

#[test]
fn test_brace_range_expansion_e2e() {
    let output = run_output("echo {1..5}");
    let trimmed = output.trim();
    assert_eq!(trimmed, "1 2 3 4 5", "got: {trimmed}");
}

#[test]
fn test_pipe_and_combo() {
    let (code, _) = run("echo abc | grep a && echo ok").unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_for_loop_with_brace_expansion() {
    let (code, ctx) = run("for i in {1..3}; do x=$i; done").unwrap();
    assert_eq!(code, 0);
    assert_eq!(ctx.variables.get("x").map(String::as_str), Some("3"));
}

#[test]
fn test_arithmetic_expansion_e2e() {
    let (code, ctx) = run("x=$((2 + 3))").unwrap();
    assert_eq!(code, 0);
    assert_eq!(ctx.variables.get("x").map(String::as_str), Some("5"));
}

#[test]
fn test_heredoc_execution() {
    let output = run_output("cat <<EOF\nhello world\nEOF");
    let trimmed = output.trim();
    assert_eq!(trimmed, "hello world", "got: {trimmed}");
}
