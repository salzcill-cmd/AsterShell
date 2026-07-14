//! Command execution engine.
//!
//! Walks the AST produced by the parser and executes commands, pipelines,
//! logical operators, sequences, grouped expressions, control flow (if/while/for),
//! functions, job control, and variable assignment.

use aster_lexer::Lexer;
use aster_parser::Parser;
use aster_shell_core::{
    AliasMap, Atom, CaseStmt, ExecError, ForStmt, FunctionDef, Group, IfStmt, PipeExpr, Program,
    Redirect, RedirectKind, SelectStmt, ShellError, SimpleCommand, Statement, UntilStmt, WhileStmt,
};
use std::env;
use std::io::{Read, Write};
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
#[allow(unsafe_code)]
use std::os::unix::process::CommandExt;

/// Result of executing a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecOutcome {
    /// Execution succeeded with the given exit code.
    Success(i32),
    /// The shell should exit with the given code.
    Exit(i32),
    /// A `break` was encountered inside a loop.
    Break,
    /// A `continue` was encountered inside a loop.
    Continue,
}

/// Mutable context passed through execution.
pub struct ExecContext {
    /// Exit code of the last executed command.
    pub last_exit_code: i32,
    /// Previous working directory (for `cd -`).
    pub prev_dir: Option<PathBuf>,
    /// The shell's alias map.
    pub aliases: AliasMap,
    /// Abbreviation map (fish-style: expand inline before execution).
    pub abbreviations: std::collections::HashMap<String, String>,
    /// Local variables (set by `name=value` assignments).
    pub variables: std::collections::HashMap<String, String>,
    /// Defined functions.
    pub functions: std::collections::HashMap<String, Vec<Statement>>,
    /// Whether we are currently inside a function body.
    pub in_function: bool,
    /// Whether we are currently inside a loop.
    pub in_loop: bool,
    /// Positional arguments ($1, $2, ...).
    pub positional_args: Vec<String>,
    /// Job manager for background/foreground job control.
    pub jobs: aster_shell_core::jobs::JobManager,
    /// Current line number for $LINENO.
    pub current_line: u64,
    /// Exit codes of individual commands in the last pipeline ($PIPESTATUS).
    pub pipeline_exit_codes: Vec<i32>,
    /// Shell start time for $SECONDS.
    pub start_time: std::time::Instant,
    /// Signal handlers registered via `trap`.
    pub trap_handlers: std::collections::HashMap<i32, String>,
    /// The shell's own PID (for terminal control).
    pub shell_pgid: i32,
    /// Foreground process group ID (for terminal control).
    pub foreground_pgid: Option<i32>,
    /// PID of the most recently backgrounded process ($!).
    pub last_background_pid: u32,
    /// Current shell option flags ($-).
    pub last_shell_options: String,
    /// Whether Ctrl+C was pressed (set by signal handler).
    pub interrupted: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Default for ExecContext {
    fn default() -> Self {
        Self {
            last_exit_code: 0,
            prev_dir: None,
            aliases: AliasMap::new(),
            abbreviations: std::collections::HashMap::new(),
            variables: std::collections::HashMap::new(),
            functions: std::collections::HashMap::new(),
            in_function: false,
            in_loop: false,
            positional_args: Vec::new(),
            jobs: aster_shell_core::jobs::JobManager::new(),
            current_line: 1,
            pipeline_exit_codes: Vec::new(),
            start_time: std::time::Instant::now(),
            trap_handlers: std::collections::HashMap::new(),
            shell_pgid: std::process::id() as i32,
            foreground_pgid: None,
            last_background_pid: 0,
            last_shell_options: "i".into(),
            interrupted: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

/// The command executor.
pub struct Executor;

impl Executor {
    /// Executes a parsed program and returns the outcome.
    ///
    /// # Errors
    ///
    /// Returns [`ShellError`] on execution failure.
    pub fn execute(program: &Program, ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        let mut last_code = 0;

        for stmt in &program.statements {
            match Self::execute_statement(stmt, ctx)? {
                ExecOutcome::Success(code) => {
                    last_code = code;
                    ctx.last_exit_code = code;
                }
                ExecOutcome::Exit(code) => {
                    ctx.last_exit_code = code;
                    return Ok(ExecOutcome::Exit(code));
                }
                ExecOutcome::Break | ExecOutcome::Continue => {
                    last_code = 0;
                }
            }
        }

        Ok(ExecOutcome::Success(last_code))
    }

    fn execute_statement(
        stmt: &Statement,
        ctx: &mut ExecContext,
    ) -> Result<ExecOutcome, ShellError> {
        match stmt {
            Statement::Pipe(pipe) => Self::execute_pipe(pipe, ctx),
            Statement::And(left, right) => {
                let outcome = Self::execute_statement(left, ctx)?;
                match outcome {
                    ExecOutcome::Success(0) => Self::execute_statement(right, ctx),
                    other => Ok(other),
                }
            }
            Statement::Or(left, right) => {
                let outcome = Self::execute_statement(left, ctx)?;
                match outcome {
                    ExecOutcome::Success(0) => Ok(outcome),
                    ExecOutcome::Success(_) | ExecOutcome::Exit(_) => {
                        Self::execute_statement(right, ctx)
                    }
                    other => Ok(other),
                }
            }
            Statement::If(if_stmt) => Self::execute_if(if_stmt, ctx),
            Statement::While(while_stmt) => Self::execute_while(while_stmt, ctx),
            Statement::Until(until_stmt) => Self::execute_until(until_stmt, ctx),
            Statement::For(for_stmt) => Self::execute_for(for_stmt, ctx),
            Statement::Select(select_stmt) => Self::execute_select(select_stmt, ctx),
            Statement::Case(case_stmt) => Self::execute_case(case_stmt, ctx),
            Statement::FunctionDef(func_def) => Self::define_function(func_def, ctx),
            Statement::Return(value) => {
                let code = value
                    .as_ref()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(ctx.last_exit_code);
                if ctx.in_function {
                    Ok(ExecOutcome::Success(code))
                } else {
                    Err(ShellError::Exec(ExecError::ReturnOutsideFunction))
                }
            }
            Statement::Break(_) => {
                if ctx.in_loop {
                    Ok(ExecOutcome::Break)
                } else {
                    Err(ShellError::Exec(ExecError::BreakOutsideLoop))
                }
            }
            Statement::Continue(_) => {
                if ctx.in_loop {
                    Ok(ExecOutcome::Continue)
                } else {
                    Err(ShellError::Exec(ExecError::ContinueOutsideLoop))
                }
            }
            Statement::Compound(stmts, _span) => {
                let mut last = 0;
                for s in stmts {
                    match Self::execute_statement(s, ctx)? {
                        ExecOutcome::Success(code) => last = code,
                        ExecOutcome::Exit(code) => return Ok(ExecOutcome::Exit(code)),
                        ExecOutcome::Break => return Ok(ExecOutcome::Break),
                        ExecOutcome::Continue => return Ok(ExecOutcome::Continue),
                    }
                }
                Ok(ExecOutcome::Success(last))
            }
            Statement::Assign(assign) => {
                let value = Self::expand_variables(&assign.value, ctx);
                ctx.variables.insert(assign.name.clone(), value);
                Ok(ExecOutcome::Success(0))
            }
            Statement::DoubleBracket(args, _span) => {
                let result = Self::eval_double_bracket(args, ctx)?;
                Ok(ExecOutcome::Success(i32::from(!result)))
            }
            Statement::Background(inner) => Self::execute_background(inner, ctx),
        }
    }

    fn execute_if(if_stmt: &IfStmt, ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        if Self::eval_condition(&if_stmt.condition, ctx)? {
            return Self::execute_body(&if_stmt.body, ctx);
        }

        for (cond, body) in &if_stmt.elif_branches {
            if Self::eval_condition(cond, ctx)? {
                return Self::execute_body(body, ctx);
            }
        }

        if let Some(else_body) = &if_stmt.else_body {
            return Self::execute_body(else_body, ctx);
        }

        Ok(ExecOutcome::Success(0))
    }

    fn execute_while(
        while_stmt: &WhileStmt,
        ctx: &mut ExecContext,
    ) -> Result<ExecOutcome, ShellError> {
        let was_in_loop = ctx.in_loop;
        ctx.in_loop = true;
        let mut last = 0;

        loop {
            if !Self::eval_condition(&while_stmt.condition, ctx)? {
                break;
            }

            match Self::execute_body(&while_stmt.body, ctx)? {
                ExecOutcome::Success(code) => last = code,
                ExecOutcome::Exit(code) => {
                    ctx.in_loop = was_in_loop;
                    return Ok(ExecOutcome::Exit(code));
                }
                ExecOutcome::Break => break,
                ExecOutcome::Continue => continue,
            }
        }

        ctx.in_loop = was_in_loop;
        Ok(ExecOutcome::Success(last))
    }

    fn execute_until(
        until_stmt: &UntilStmt,
        ctx: &mut ExecContext,
    ) -> Result<ExecOutcome, ShellError> {
        let was_in_loop = ctx.in_loop;
        ctx.in_loop = true;
        let mut last = 0;

        loop {
            if Self::eval_condition(&until_stmt.condition, ctx)? {
                break;
            }

            match Self::execute_body(&until_stmt.body, ctx)? {
                ExecOutcome::Success(code) => last = code,
                ExecOutcome::Exit(code) => {
                    ctx.in_loop = was_in_loop;
                    return Ok(ExecOutcome::Exit(code));
                }
                ExecOutcome::Break => break,
                ExecOutcome::Continue => continue,
            }
        }

        ctx.in_loop = was_in_loop;
        Ok(ExecOutcome::Success(last))
    }

    fn execute_for(for_stmt: &ForStmt, ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        let was_in_loop = ctx.in_loop;
        ctx.in_loop = true;
        let mut last = 0;

        let mut all_words = Vec::new();
        for word in &for_stmt.words {
            let expanded = Self::expand_variables(word, ctx);
            let braced = expand_braces(&[expanded]);
            all_words.extend(braced);
        }

        for val in &all_words {
            ctx.variables.insert(for_stmt.variable.clone(), val.clone());

            match Self::execute_body(&for_stmt.body, ctx)? {
                ExecOutcome::Success(code) => last = code,
                ExecOutcome::Exit(code) => {
                    ctx.in_loop = was_in_loop;
                    return Ok(ExecOutcome::Exit(code));
                }
                ExecOutcome::Break => break,
                ExecOutcome::Continue => continue,
            }
        }

        ctx.in_loop = was_in_loop;
        Ok(ExecOutcome::Success(last))
    }

    fn execute_select(
        select_stmt: &SelectStmt,
        ctx: &mut ExecContext,
    ) -> Result<ExecOutcome, ShellError> {
        let was_in_loop = ctx.in_loop;
        ctx.in_loop = true;
        let mut last = 0;

        let mut all_words = Vec::new();
        for word in &select_stmt.words {
            let expanded = Self::expand_variables(word, ctx);
            let braced = expand_braces(&[expanded]);
            all_words.extend(braced);
        }

        let prompt = "#? ";

        loop {
            // Print menu
            for (i, word) in all_words.iter().enumerate() {
                println!(" {}) {word}", i + 1);
            }

            // Print prompt
            print!("{prompt}");
            let _ = std::io::stdout().flush();

            // Read user input
            let mut input = String::new();
            match std::io::stdin().read_line(&mut input) {
                Ok(0) => break, // EOF
                Ok(_) => {}
                Err(_) => break,
            }

            let input = input.trim();

            // Empty input — redisplay menu
            if input.is_empty() {
                continue;
            }

            // Check if input is a number
            if let Ok(idx) = input.parse::<usize>() {
                if idx >= 1 && idx <= all_words.len() {
                    ctx.variables.insert(
                        select_stmt.variable.clone(),
                        all_words[idx - 1].clone(),
                    );
                } else {
                    ctx.variables
                        .insert(select_stmt.variable.clone(), String::new());
                }
            } else {
                // Not a number — treat as the value itself
                ctx.variables
                    .insert(select_stmt.variable.clone(), input.to_string());
            }

            match Self::execute_body(&select_stmt.body, ctx)? {
                ExecOutcome::Success(code) => last = code,
                ExecOutcome::Exit(code) => {
                    ctx.in_loop = was_in_loop;
                    return Ok(ExecOutcome::Exit(code));
                }
                ExecOutcome::Break => break,
                ExecOutcome::Continue => continue,
            }
        }

        ctx.in_loop = was_in_loop;
        Ok(ExecOutcome::Success(last))
    }

    fn execute_case(
        case_stmt: &CaseStmt,
        ctx: &mut ExecContext,
    ) -> Result<ExecOutcome, ShellError> {
        let word = Self::expand_variables(&case_stmt.word, ctx);

        for arm in &case_stmt.arms {
            let matched = arm.patterns.iter().any(|pattern| {
                let pattern = Self::expand_variables(pattern, ctx);
                simple_glob_match(&pattern, &word)
            });

            if matched {
                let mut last = 0;
                for stmt in &arm.body {
                    match Self::execute_statement(stmt, ctx)? {
                        ExecOutcome::Success(code) => last = code,
                        ExecOutcome::Exit(code) => return Ok(ExecOutcome::Exit(code)),
                        ExecOutcome::Break => return Ok(ExecOutcome::Break),
                        ExecOutcome::Continue => return Ok(ExecOutcome::Continue),
                    }
                }
                return Ok(ExecOutcome::Success(last));
            }
        }

        Ok(ExecOutcome::Success(0))
    }

    fn define_function(
        func_def: &FunctionDef,
        ctx: &mut ExecContext,
    ) -> Result<ExecOutcome, ShellError> {
        ctx.functions
            .insert(func_def.name.clone(), func_def.body.clone());
        Ok(ExecOutcome::Success(0))
    }

    fn execute_body(body: &[Statement], ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        let mut last = 0;
        for stmt in body {
            match Self::execute_statement(stmt, ctx)? {
                ExecOutcome::Success(code) => last = code,
                ExecOutcome::Exit(code) => return Ok(ExecOutcome::Exit(code)),
                ExecOutcome::Break => return Ok(ExecOutcome::Break),
                ExecOutcome::Continue => return Ok(ExecOutcome::Continue),
            }
        }
        Ok(ExecOutcome::Success(last))
    }

    fn eval_condition(cond: &Statement, ctx: &mut ExecContext) -> Result<bool, ShellError> {
        match Self::execute_statement(cond, ctx)? {
            ExecOutcome::Success(code) | ExecOutcome::Exit(code) => Ok(code == 0),
            ExecOutcome::Break | ExecOutcome::Continue => Ok(false),
        }
    }

    /// Evaluates a `[[ ... ]]` double-bracket expression.
    #[allow(clippy::too_many_lines)]
    fn eval_double_bracket(args: &[String], ctx: &mut ExecContext) -> Result<bool, ShellError> {
        if args.is_empty() {
            return Ok(false);
        }

        // Handle negation: [[ ! ... ]]
        if args[0] == "!" {
            return Self::eval_double_bracket(&args[1..], ctx).map(|r| !r);
        }

        // Handle grouping with parentheses: [[ ( ... ) ]]
        if args[0] == "(" {
            // Find matching )
            let mut depth = 1;
            let mut end = args.len();
            for (i, a) in args[1..].iter().enumerate() {
                if a == "(" { depth += 1; }
                if a == ")" { depth -= 1; }
                if depth == 0 { end = i + 1; break; }
            }
            let inner = &args[1..end];
            let rest = if end < args.len() { &args[end + 1..] } else { &[] };
            let inner_result = Self::eval_double_bracket(inner, ctx)?;
            if rest.is_empty() {
                return Ok(inner_result);
            }
            // Handle && and || after grouping
            if rest[0] == "&&" {
                return if inner_result {
                    Self::eval_double_bracket(&rest[1..], ctx)
                } else {
                    Ok(false)
                };
            }
            if rest[0] == "||" {
                return if inner_result {
                    Ok(true)
                } else {
                    Self::eval_double_bracket(&rest[1..], ctx)
                };
            }
            return Ok(inner_result);
        }

        // Handle && and || between expressions
        if let Some(pos) = args.iter().position(|a| a == "&&" || a == "||") {
            let left = Self::eval_double_bracket(&args[..pos], ctx)?;
            let op = &args[pos];
            let right_args = &args[pos + 1..];
            if op == "&&" {
                return if left {
                    Self::eval_double_bracket(right_args, ctx)
                } else {
                    Ok(false)
                };
            } else {
                return if left {
                    Ok(true)
                } else {
                    Self::eval_double_bracket(right_args, ctx)
                };
            }
        }

        // Handle unary test operators
        if args.len() == 2 {
            let op = &args[0];
            let val = Self::expand_variables(&args[1], ctx);
            return match op.as_str() {
                "-z" => Ok(val.is_empty()),
                "-n" => Ok(!val.is_empty()),
                "-f" => Ok(std::path::Path::new(val.as_str()).is_file()),
                "-d" => Ok(std::path::Path::new(val.as_str()).is_dir()),
                "-e" | "-r" | "-w" | "-x" => Ok(std::path::Path::new(val.as_str()).exists()),
                "-s" => Ok(std::fs::metadata(val.as_str()).map(|m| m.len() > 0).unwrap_or(false)),
                "-L" => Ok(std::path::Path::new(val.as_str()).is_symlink()),
                _ => Ok(false),
            };
        }

        // Handle binary comparison operators
        if args.len() == 3 {
            let left = Self::expand_variables(&args[0], ctx);
            let op = &args[1];
            let right = Self::expand_variables(&args[2], ctx);
            return match op.as_str() {
                "==" | "=" => {
                    // Glob pattern matching: [[ *.txt == file.txt ]]
                    if right.contains('*') || right.contains('?') || right.contains('[') {
                        Ok(simple_glob_match(&right, &left))
                    } else {
                        Ok(left == right)
                    }
                }
                "!=" => {
                    if right.contains('*') || right.contains('?') || right.contains('[') {
                        Ok(!simple_glob_match(&right, &left))
                    } else {
                        Ok(left != right)
                    }
                }
                "=~" => {
                    // Regex matching
                    if let Ok(re) = regex::Regex::new(&right) {
                        Ok(re.is_match(&left))
                    } else {
                        Ok(false)
                    }
                }
                "-eq" => Ok(left.trim().parse::<i64>().unwrap_or(0) == right.trim().parse::<i64>().unwrap_or(0)),
                "-ne" => Ok(left.trim().parse::<i64>().unwrap_or(0) != right.trim().parse::<i64>().unwrap_or(0)),
                "-lt" => Ok(left.trim().parse::<i64>().unwrap_or(0) < right.trim().parse::<i64>().unwrap_or(0)),
                "-le" => Ok(left.trim().parse::<i64>().unwrap_or(0) <= right.trim().parse::<i64>().unwrap_or(0)),
                "-gt" => Ok(left.trim().parse::<i64>().unwrap_or(0) > right.trim().parse::<i64>().unwrap_or(0)),
                "-ge" => Ok(left.trim().parse::<i64>().unwrap_or(0) >= right.trim().parse::<i64>().unwrap_or(0)),
                _ => Ok(false),
            };
        }

        // Fallback: treat as a string test (non-empty = true)
        let expanded = Self::expand_variables(&args.join(" "), ctx);
        Ok(!expanded.is_empty())
    }

    /// Expands variables in a string (e.g. `$HOME`, `$?`, `$1`),
    /// command substitutions `$(cmd)`, arithmetic expansions `$((expr))`,
    /// and parameter expansion `${var:...}`.
    pub fn expand_variables(input: &str, ctx: &mut ExecContext) -> String {
        let chars: Vec<char> = input.chars().collect();
        let mut result = String::with_capacity(input.len());
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '$' {
                i += 1;
                if i >= chars.len() {
                    result.push('$');
                    break;
                }
                match chars[i] {
                    '?' => {
                        i += 1;
                        result.push_str(&ctx.last_exit_code.to_string());
                    }
                    '$' => {
                        i += 1;
                        result.push_str(&std::process::id().to_string());
                    }
                    '0' => {
                        i += 1;
                        result.push_str("aster");
                    }
                    '#' => {
                        i += 1;
                        result.push_str(&ctx.positional_args.len().to_string());
                    }
                    '!' => {
                        i += 1;
                        result.push_str(&ctx.last_background_pid.to_string());
                    }
                    '-' => {
                        i += 1;
                        result.push_str(&ctx.last_shell_options);
                    }
                    c if c.is_ascii_digit() && c != '0' => {
                        let idx = c.to_digit(10).unwrap() as usize;
                        i += 1;
                        if idx <= ctx.positional_args.len() {
                            result.push_str(&ctx.positional_args[idx - 1]);
                        }
                    }
                    '(' => {
                        i += 1;
                        if i < chars.len() && chars[i] == '(' {
                            // $(( - arithmetic expansion
                            i += 1;
                            if let Some(end) = find_matching_paren(&chars, i, true) {
                                let inner: String = chars[i..end.saturating_sub(1)].iter().collect();
                                i = end + 1;
                                match eval_arithmetic(&inner, ctx) {
                                    Ok(val) => result.push_str(&val.to_string()),
                                    Err(_) => {}
                                }
                            } else {
                                result.push_str("$ ((");
                            }
                        } else {
                            // $( - command substitution
                            if let Some(end) = find_matching_paren(&chars, i, false) {
                                let inner: String = chars[i..end].iter().collect();
                                i = end + 1;
                                match Self::execute_captured(&inner, ctx) {
                                    Ok(output) => result.push_str(&output),
                                    Err(_) => {}
                                }
                            } else {
                                result.push_str("$(");
                            }
                        }
                    }
                    '{' => {
                        i += 1;
                        let start = i;
                        let mut brace_depth = 1;
                        while i < chars.len() && brace_depth > 0 {
                            if chars[i] == '{' {
                                brace_depth += 1;
                            } else if chars[i] == '}' {
                                brace_depth -= 1;
                            }
                            if brace_depth > 0 {
                                i += 1;
                            }
                        }
                        if i < chars.len() {
                            let content: String = chars[start..i].iter().collect();
                            i += 1;

                            if content.starts_with('#') {
                                let var_name = &content[1..];
                                let val = get_var_value(var_name, ctx);
                                result.push_str(&val.len().to_string());
                            } else {
                                let mut var_name_end = 0;
                                for c in content.chars() {
                                    if c.is_ascii_alphanumeric() || c == '_' {
                                        var_name_end += 1;
                                    } else {
                                        break;
                                    }
                                }
                                let var_name = &content[..var_name_end];
                                let rest = &content[var_name_end..];

                                if rest.is_empty() {
                                    let val = get_var_value(var_name, ctx);
                                    result.push_str(&val);
                                } else if rest.starts_with(":-") {
                                    let default_val = &rest[2..];
                                    let val = get_var_value(var_name, ctx);
                                    if val.is_empty() {
                                        result.push_str(default_val);
                                    } else {
                                        result.push_str(&val);
                                    }
                                } else if rest.starts_with(":=") {
                                    let default_val = &rest[2..].to_string();
                                    let val = get_var_value(var_name, ctx);
                                    if val.is_empty() {
                                        ctx.variables
                                            .insert(var_name.to_string(), default_val.clone());
                                        result.push_str(default_val);
                                    } else {
                                        result.push_str(&val);
                                    }
                                } else if rest.starts_with(":+") {
                                    let alt_val = &rest[2..];
                                    let val = get_var_value(var_name, ctx);
                                    if !val.is_empty() {
                                        result.push_str(alt_val);
                                    }
                                } else if rest.starts_with(":?") {
                                    let error_msg = &rest[2..];
                                    let val = get_var_value(var_name, ctx);
                                    if val.is_empty() {
                                        result.push_str(error_msg);
                                    } else {
                                        result.push_str(&val);
                                    }
                                } else if rest.starts_with("%%") {
                                    let pattern = &rest[2..];
                                    let val = get_var_value(var_name, ctx);
                                    let expanded =
                                        shell_pattern_remove_suffix(&val, pattern, true);
                                    result.push_str(&expanded);
                                } else if rest.starts_with('%') {
                                    let pattern = &rest[1..];
                                    let val = get_var_value(var_name, ctx);
                                    let expanded =
                                        shell_pattern_remove_suffix(&val, pattern, false);
                                    result.push_str(&expanded);
                                } else if rest.starts_with("##") {
                                    let pattern = &rest[2..];
                                    let val = get_var_value(var_name, ctx);
                                    let expanded =
                                        shell_pattern_remove_prefix(&val, pattern, true);
                                    result.push_str(&expanded);
                                } else if rest.starts_with('#') {
                                    let pattern = &rest[1..];
                                    let val = get_var_value(var_name, ctx);
                                    let expanded =
                                        shell_pattern_remove_prefix(&val, pattern, false);
                                    result.push_str(&expanded);
                                } else if rest.starts_with("//") {
                                    let slash_content = &rest[2..];
                                    if let Some(slash_pos) = slash_content.find('/') {
                                        let pattern = &slash_content[..slash_pos];
                                        let replacement = &slash_content[slash_pos + 1..];
                                        let val = get_var_value(var_name, ctx);
                                        let expanded =
                                            shell_str_replace_all(&val, pattern, replacement);
                                        result.push_str(&expanded);
                                    } else {
                                        let val = get_var_value(var_name, ctx);
                                        result.push_str(&val);
                                    }
                                } else if rest.starts_with('/') {
                                    let slash_content = &rest[1..];
                                    if let Some(slash_pos) = slash_content.find('/') {
                                        let pattern = &slash_content[..slash_pos];
                                        let replacement = &slash_content[slash_pos + 1..];
                                        let val = get_var_value(var_name, ctx);
                                        let expanded =
                                            shell_str_replace_first(&val, pattern, replacement);
                                        result.push_str(&expanded);
                                    } else {
                                        let val = get_var_value(var_name, ctx);
                                        result.push_str(&val);
                                    }
                                } else {
                                    let val = get_var_value(var_name, ctx);
                                    result.push_str(&val);
                                }
                            }
                        } else {
                            result.push_str("${");
                            result.push_str(&chars[start..].iter().collect::<String>());
                            break;
                        }
                    }
                    c if c.is_ascii_alphanumeric() || c == '_' => {
                        let mut var_name = String::new();
                        var_name.push(c);
                        i += 1;
                        while i < chars.len()
                            && (chars[i].is_ascii_alphanumeric() || chars[i] == '_')
                        {
                            var_name.push(chars[i]);
                            i += 1;
                        }
                        match var_name.as_str() {
                            "RANDOM" => {
                                use std::collections::hash_map::RandomState;
                                use std::hash::{BuildHasher, Hasher};
                                let val = RandomState::new()
                                    .build_hasher()
                                    .finish() as u32
                                    % 32768;
                                result.push_str(&val.to_string());
                            }
                            "LINENO" => {
                                result.push_str(&ctx.current_line.to_string());
                            }
                            "PIPESTATUS" => {
                                if ctx.pipeline_exit_codes.is_empty() {
                                    result.push_str("0");
                                } else {
                                    let statuses: Vec<String> = ctx
                                        .pipeline_exit_codes
                                        .iter()
                                        .map(|c| c.to_string())
                                        .collect();
                                    result.push_str(&statuses.join(" "));
                                }
                            }
                            "SECONDS" => {
                                result.push_str(&ctx.start_time.elapsed().as_secs().to_string());
                            }
                            "EPOCHSECONDS" => {
                                let secs = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0);
                                result.push_str(&secs.to_string());
                            }
                            _ => {
                                if let Some(val) = ctx.variables.get(&var_name) {
                                    result.push_str(val);
                                } else {
                                    match std::env::var(&var_name) {
                                        Ok(val) => result.push_str(&val),
                                        Err(_) => {
                                            result.push('$');
                                            result.push_str(&var_name);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        result.push('$');
                    }
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }

        result
    }

    /// Expands a SimpleCommand's name and arguments with variable and glob expansion.
    fn expand_cmd(cmd: &SimpleCommand, ctx: &mut ExecContext) -> SimpleCommand {
        let expanded_args: Vec<String> = cmd
            .args
            .iter()
            .map(|a| Self::expand_variables(a, ctx))
            .map(|a| expand_tilde(&a))
            .collect();
        let expanded_args: Vec<String> = expanded_args
            .iter()
            .map(|a| Self::expand_process_substitution(a, ctx))
            .collect();
        let expanded_args = aster_shell_core::glob::expand(&expanded_args);

        SimpleCommand {
            name: Self::expand_variables(&cmd.name, ctx),
            args: expanded_args,
            redirects: cmd.redirects.clone(),
            span: cmd.span,
        }
    }

    /// Expands process substitution `<(...)` and `>(...)` in a single argument.
    #[allow(unsafe_code)]
    fn expand_process_substitution(arg: &str, _ctx: &mut ExecContext) -> String {
        let mut result = String::with_capacity(arg.len());
        let mut chars = arg.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '<' && chars.peek() == Some(&'(') {
                chars.next(); // consume '('
                // Collect until matching ')'
                let mut depth = 1;
                let mut inner = String::new();
                while let Some(cc) = chars.next() {
                    if cc == '(' {
                        depth += 1;
                        inner.push(cc);
                    } else if cc == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        inner.push(cc);
                    } else {
                        inner.push(cc);
                    }
                }
                // Create a pipe, run the command, feed stdout to the pipe
                if let Ok((read_fd, write_fd)) = pipe() {
                    // Spawn command writing to pipe
                    let inner_cmd = inner.clone();
                    let _ = std::thread::spawn(move || {
                        unsafe {
                            libc::dup2(write_fd, libc::STDOUT_FILENO);
                            libc::close(write_fd);
                        }
                        let _ = std::process::Command::new("/bin/sh")
                            .arg("-c")
                            .arg(&inner_cmd)
                            .status();
                        unsafe { libc::close(libc::STDOUT_FILENO); }
                    });
                    unsafe { libc::close(write_fd); }
                    result.push_str(&format!("/proc/self/fd/{read_fd}"));
                } else {
                    result.push_str(&format!("<({inner})"));
                }
            } else if c == '>' && chars.peek() == Some(&'(') {
                chars.next(); // consume '('
                let mut depth = 1;
                let mut inner = String::new();
                while let Some(cc) = chars.next() {
                    if cc == '(' {
                        depth += 1;
                        inner.push(cc);
                    } else if cc == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        inner.push(cc);
                    } else {
                        inner.push(cc);
                    }
                }
                if let Ok((read_fd, write_fd)) = pipe() {
                    let inner_cmd = inner.clone();
                    let _ = std::thread::spawn(move || {
                        unsafe {
                            libc::dup2(read_fd, libc::STDIN_FILENO);
                            libc::close(read_fd);
                        }
                        let _ = std::process::Command::new("/bin/sh")
                            .arg("-c")
                            .arg(&inner_cmd)
                            .status();
                        unsafe { libc::close(libc::STDIN_FILENO); }
                    });
                    unsafe { libc::close(read_fd); }
                    result.push_str(&format!("/proc/self/fd/{write_fd}"));
                } else {
                    result.push_str(&format!(">({inner})"));
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Executes a command string and captures its stdout. Used for `$(cmd)`.
    fn execute_captured(
        cmd_str: &str,
        ctx: &mut ExecContext,
    ) -> Result<String, ShellError> {
        let tokens = Lexer::new(cmd_str).tokenize().map_err(|e| {
            ShellError::Exec(ExecError::SpawnFailed {
                command: "command substitution".into(),
                reason: e.to_string(),
            })
        })?;
        let program = Parser::new(&tokens).parse().map_err(|e| {
            ShellError::Exec(ExecError::SpawnFailed {
                command: "command substitution".into(),
                reason: e.to_string(),
            })
        })?;

        // For simple commands, capture output directly
        if program.statements.len() == 1 {
            if let Statement::Pipe(pipe) = &program.statements[0] {
                if pipe.atoms.len() == 1 {
                    if let Atom::Command(cmd) = &pipe.atoms[0] {
                        let expanded = Self::expand_cmd(cmd, ctx);
                        let (mut command, heredocs) = Self::build_command(&expanded)?;
                        command.stdout(Stdio::piped());
                        command.stderr(Stdio::piped());
                        if !heredocs.is_empty() {
                            command.stdin(Stdio::piped());
                        }
                        let mut child = command.spawn().map_err(|e| ExecError::SpawnFailed {
                            command: expanded.name.clone(),
                            reason: e.to_string(),
                        })?;
                        if !heredocs.is_empty() {
                            if let Some(mut stdin) = child.stdin.take() {
                                for content in &heredocs {
                                    stdin
                                        .write_all(content.as_bytes())
                                        .map_err(|e| ExecError::SpawnFailed {
                                            command: expanded.name.clone(),
                                            reason: e.to_string(),
                                        })?;
                                }
                            }
                        }
                        let output = child.wait_with_output().map_err(|e| {
                            ExecError::SpawnFailed {
                                command: expanded.name.clone(),
                                reason: e.to_string(),
                            }
                        })?;
                        return Ok(String::from_utf8_lossy(&output.stdout)
                            .trim_end_matches('\n')
                            .to_string());
                    }
                }
            }
        }

        Ok(String::new())
    }

    /// Creates a `Command` from a [`SimpleCommand`] by resolving the executable path.
    fn build_command(cmd: &SimpleCommand) -> Result<(Command, Vec<String>), ShellError> {
        let path = aster_utils::find_executable(&cmd.name)
            .ok_or_else(|| ExecError::CommandNotFound(cmd.name.clone()))?;
        let mut command = Command::new(&path);
        command.args(&cmd.args);
        let heredocs = Self::apply_redirects(&mut command, &cmd.redirects)?;
        Ok((command, heredocs))
    }

    fn execute_background(inner: &Statement, ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        // Execute the inner statement in the background
        // For simple commands, spawn as a background job
        match inner {
            Statement::Pipe(pipe) if pipe.atoms.len() == 1 => {
                if let Atom::Command(cmd) = &pipe.atoms[0] {
                    let expanded = Self::expand_cmd(cmd, ctx);
                    let (mut command, _heredocs) = Self::build_command(&expanded)?;

                    // Create a new process group for the child
                    #[allow(unsafe_code)]
                    unsafe {
                        command.pre_exec(|| {
                            libc::setpgid(0, 0);
                            Ok(())
                        });
                    }

                    let mut child = command.spawn().map_err(|e| ExecError::SpawnFailed {
                        command: expanded.name.clone(),
                        reason: e.to_string(),
                    })?;

                    let pid = child.id();

                    // Create job entry
                    let job_id = ctx.jobs.next_id();
                    let processes = vec![aster_shell_core::jobs::ProcessInfo::new(pid, &expanded.name)];
                    let job = aster_shell_core::jobs::Job::new(job_id, processes, format!("{} {}", expanded.name, expanded.args.join(" ")), true);
                    ctx.jobs.add(job);

                    ctx.last_background_pid = pid;
                    println!("[{job_id}] {pid}");

                    // Don't wait — let it run in background
                    // Detach stdout/stderr so they don't block
                    let _ = child.stdout.take();
                    let _ = child.stderr.take();

                    return Ok(ExecOutcome::Success(0));
                }
            }
            _ => {
                // For complex background commands, run in a subshell-like fashion
                // Just execute it and print status
                let outcome = Self::execute_statement(inner, ctx)?;
                return Ok(outcome);
            }
        }
        Ok(ExecOutcome::Success(0))
    }

    fn execute_pipe(pipe: &PipeExpr, ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        if pipe.atoms.len() == 1 {
            return Self::execute_atom(&pipe.atoms[0], ctx);
        }

        let mut children: Vec<Child> = Vec::new();
        let mut prev_stdout: Option<std::process::ChildStdout> = None;
        let mut last_exit = 0;
        let mut first_pid: Option<u32> = None;

        for (i, atom) in pipe.atoms.iter().enumerate() {
            match atom {
                Atom::Command(cmd) => {
                    let expanded = Self::expand_cmd(cmd, ctx);
                    let (mut command, heredocs) = Self::build_command(&expanded)?;

                    let has_prev_stdout = prev_stdout.is_some();
                    if let Some(stdout) = prev_stdout.take() {
                        command.stdin(stdout);
                    } else if !heredocs.is_empty() {
                        command.stdin(Stdio::piped());
                    }

                    if i < pipe.atoms.len() - 1 {
                        command.stdout(Stdio::piped());
                    }

                    // Put child in its own process group (first child is the group leader)
                    let pgid_val = first_pid;
                    #[allow(unsafe_code)]
                    unsafe {
                        command.pre_exec(move || {
                            match pgid_val {
                                Some(leader) => libc::setpgid(0, leader as i32),
                                None => libc::setpgid(0, 0),
                            };
                            Ok(())
                        });
                    }

                    let mut child = command.spawn().map_err(|e| ExecError::SpawnFailed {
                        command: expanded.name.clone(),
                        reason: e.to_string(),
                    })?;

                    if first_pid.is_none() {
                        first_pid = Some(child.id());
                    }

                    if !has_prev_stdout && !heredocs.is_empty() {
                        if let Some(mut stdin) = child.stdin.take() {
                            for content in &heredocs {
                                stdin
                                    .write_all(content.as_bytes())
                                    .map_err(|e| ExecError::SpawnFailed {
                                        command: expanded.name.clone(),
                                        reason: e.to_string(),
                                    })?;
                            }
                        }
                    }

                    prev_stdout = child.stdout.take();
                    children.push(child);
                }
                Atom::Group(group) => {
                    let outcome = Self::execute_group(group, ctx)?;
                    last_exit = match outcome {
                        ExecOutcome::Success(code) => code,
                        ExecOutcome::Exit(code) => return Ok(ExecOutcome::Exit(code)),
                        ExecOutcome::Break => return Ok(ExecOutcome::Break),
                        ExecOutcome::Continue => return Ok(ExecOutcome::Continue),
                    };
                }
            }
        }

        let mut pipe_exit_codes = Vec::new();
        for mut child in children {
            let status = child.wait().map_err(|e| ExecError::SpawnFailed {
                command: "pipeline".into(),
                reason: e.to_string(),
            })?;
            let code = status.code().unwrap_or(1);
            pipe_exit_codes.push(code);
            last_exit = code;
        }

        ctx.pipeline_exit_codes = pipe_exit_codes;
        ctx.last_exit_code = last_exit;
        Ok(ExecOutcome::Success(last_exit))
    }

    fn execute_atom(atom: &Atom, ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        match atom {
            Atom::Command(cmd) => Self::execute_simple(cmd, ctx),
            Atom::Group(group) => Self::execute_group(group, ctx),
        }
    }

    fn execute_simple(
        cmd: &SimpleCommand,
        ctx: &mut ExecContext,
    ) -> Result<ExecOutcome, ShellError> {
        // Expand abbreviations: if command name matches, replace with expansion + original args
        if let Some(expansion) = ctx.abbreviations.get(&cmd.name) {
            let parts: Vec<&str> = expansion.splitn(2, ' ').collect();
            let new_name = parts[0].to_string();
            let new_args_str = parts.get(1).unwrap_or(&"");
            let mut new_args: Vec<String> = if new_args_str.is_empty() {
                Vec::new()
            } else {
                new_args_str.split_whitespace().map(String::from).collect()
            };
            new_args.extend(cmd.args.iter().cloned());

            let expanded_name = Self::expand_variables(&new_name, ctx);
            let expanded_args: Vec<String> = new_args
                .iter()
                .map(|a| Self::expand_variables(a, ctx))
                .map(|a| expand_tilde(&a))
                .collect();
            let expanded_args = aster_shell_core::glob::expand(&expanded_args);

            let expanded_cmd = SimpleCommand {
                name: expanded_name,
                args: expanded_args,
                redirects: cmd.redirects.clone(),
                span: cmd.span,
            };
            return Self::dispatch_simple(&expanded_cmd, ctx);
        }

        // Expand aliases on the command name
        let (cmd_name, alias_args) = if let Some((name, extra_args)) = ctx.aliases.expand(&cmd.name)
        {
            (name, extra_args)
        } else {
            (cmd.name.clone(), Vec::new())
        };

        let expanded_name = Self::expand_variables(&cmd_name, ctx);
        let mut expanded_args: Vec<String> = cmd
            .args
            .iter()
            .map(|a| Self::expand_variables(a, ctx))
            .map(|a| expand_tilde(&a))
            .collect();
        expanded_args.splice(0..0, alias_args);
        let expanded_args = aster_shell_core::glob::expand(&expanded_args);

        let expanded_cmd = SimpleCommand {
            name: expanded_name,
            args: expanded_args,
            redirects: cmd.redirects.clone(),
            span: cmd.span,
        };

        Self::dispatch_simple(&expanded_cmd, ctx)
    }

    /// Dispatches an expanded simple command to builtins, functions, or external execution.
    fn dispatch_simple(
        expanded_cmd: &SimpleCommand,
        ctx: &mut ExecContext,
    ) -> Result<ExecOutcome, ShellError> {
        match expanded_cmd.name.as_str() {
            "cd" => return Self::builtin_cd(&expanded_cmd.args, ctx),
            "exit" => {
                let code = expanded_cmd
                    .args
                    .first()
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(ctx.last_exit_code);
                // Run EXIT trap handler if set
                if let Some(action) = ctx.trap_handlers.get(&0).cloned() {
                    if action != "-" && !action.is_empty() {
                        let _ = Self::run_trap_action(&action, ctx);
                    }
                }
                return Ok(ExecOutcome::Exit(code));
            }
            "history" => {
                return Ok(ExecOutcome::Success(0));
            }
            "eval" => {
                let joined = expanded_cmd.args.join(" ");
                let tokens = Lexer::new(&joined).tokenize().map_err(|e| {
                    ShellError::Exec(ExecError::SpawnFailed {
                        command: "eval".into(),
                        reason: e.to_string(),
                    })
                })?;
                let program = Parser::new(&tokens).parse().map_err(|e| {
                    ShellError::Exec(ExecError::SpawnFailed {
                        command: "eval".into(),
                        reason: e.to_string(),
                    })
                })?;
                return Self::execute(&program, ctx);
            }
            "source" | "." => {
                let path = expanded_cmd
                    .args
                    .first()
                    .ok_or_else(|| ExecError::CdError("source: missing file argument".into()))?;
                let contents = std::fs::read_to_string(path).map_err(|e| {
                    ShellError::Exec(ExecError::SpawnFailed {
                        command: "source".into(),
                        reason: e.to_string(),
                    })
                })?;
                let tokens = Lexer::new(&contents).tokenize().map_err(|e| {
                    ShellError::Exec(ExecError::SpawnFailed {
                        command: "source".into(),
                        reason: e.to_string(),
                    })
                })?;
                let program = Parser::new(&tokens).parse().map_err(|e| {
                    ShellError::Exec(ExecError::SpawnFailed {
                        command: "source".into(),
                        reason: e.to_string(),
                    })
                })?;
                return Self::execute(&program, ctx);
            }
            "clear" => {
                print!("\x1B[2J\x1B[H");
                let _ = std::io::stdout().flush();
                return Ok(ExecOutcome::Success(0));
            }
            "exec" => {
                if expanded_cmd.args.is_empty() {
                    // exec with no args: just return success (replaces shell with nothing)
                    return Ok(ExecOutcome::Success(0));
                }
                // exec replaces the shell with the given command
                let status = std::process::Command::new(&expanded_cmd.args[0])
                    .args(&expanded_cmd.args[1..])
                    .status()
                    .map_err(|e| ExecError::SpawnFailed {
                        command: expanded_cmd.args[0].clone(),
                        reason: e.to_string(),
                    })?;
                let code = status.code().unwrap_or(1);
                return Ok(ExecOutcome::Exit(code));
            }
            "trap" => {
                return Self::builtin_trap(&expanded_cmd.args, ctx);
            }
            "wait" => {
                return Self::builtin_wait(&expanded_cmd.args, ctx);
            }
            "disown" => {
                // disown: remove jobs from job table
                if expanded_cmd.args.is_empty() {
                    // disown all background jobs
                    ctx.jobs.cleanup();
                } else {
                    for arg in &expanded_cmd.args {
                        let id = arg.trim_start_matches('%').parse::<u32>().unwrap_or(0);
                        ctx.jobs.remove(id);
                    }
                }
                return Ok(ExecOutcome::Success(0));
            }
            "jobs" => {
                let jobs_list = ctx.jobs.list();
                if jobs_list.is_empty() {
                    // nothing to show
                } else {
                    for job in &jobs_list {
                        let state_str = match job.state() {
                            aster_shell_core::jobs::JobState::Running => "Running",
                            aster_shell_core::jobs::JobState::Stopped => "Stopped",
                            aster_shell_core::jobs::JobState::Completed => "Done",
                        };
                        let bg_marker = if job.background { " &" } else { "" };
                        println!("[{}] {}+ {}{}", job.id, job.pgid().unwrap_or(0), state_str, bg_marker);
                    }
                }
                return Ok(ExecOutcome::Success(0));
            }
            "fg" => {
                let id_str = expanded_cmd.args.first().map(String::as_str).unwrap_or("%1");
                let id = id_str.trim_start_matches('%').parse::<u32>().unwrap_or(1);
                if let Some(job) = ctx.jobs.get(id) {
                    let pgid = job.pgid().unwrap_or(0) as i32;
                    println!("{}", job.command_string);
                    // Bring process group to foreground
                    #[allow(unsafe_code)]
                    unsafe {
                        libc::tcsetpgrp(libc::STDIN_FILENO, pgid);
                    }
                    // Resume if stopped
                    if job.state() == aster_shell_core::jobs::JobState::Stopped {
                        #[allow(unsafe_code)]
                        unsafe {
                            libc::kill(-pgid, libc::SIGCONT);
                        }
                        job.set_state(aster_shell_core::jobs::JobState::Running);
                    }
                    // Wait for the process group
                    let mut status: i32 = 0;
                    #[allow(unsafe_code)]
                    unsafe {
                        libc::waitpid(pgid, &mut status, libc::WUNTRACED);
                    }
                    // Restore terminal to shell
                    #[allow(unsafe_code)]
                    unsafe {
                        libc::tcsetpgrp(libc::STDIN_FILENO, ctx.shell_pgid);
                    }
                    if libc::WIFEXITED(status) {
                        let code = libc::WEXITSTATUS(status);
                        ctx.last_exit_code = code;
                        job.set_state(aster_shell_core::jobs::JobState::Completed);
                        return Ok(ExecOutcome::Success(code));
                    } else if libc::WIFSTOPPED(status) {
                        job.set_state(aster_shell_core::jobs::JobState::Stopped);
                    }
                    return Ok(ExecOutcome::Success(0));
                } else {
                    eprintln!("fg: job {id} not found");
                    return Ok(ExecOutcome::Success(1));
                }
            }
            "command" => {
                if expanded_cmd.args.is_empty() {
                    eprintln!("command: missing operand");
                    return Ok(ExecOutcome::Success(1));
                }
                let mut cmd_name = String::new();
                let mut skip_next = false;
                for arg in &expanded_cmd.args {
                    if skip_next {
                        skip_next = false;
                        continue;
                    }
                    if arg == "-v" || arg == "-V" {
                        skip_next = true;
                        continue;
                    }
                    if arg == "-p" {
                        continue;
                    }
                    if cmd_name.is_empty() {
                        cmd_name.clone_from(arg);
                    }
                }
                if cmd_name.is_empty() {
                    eprintln!("command: missing operand");
                    return Ok(ExecOutcome::Success(1));
                }
                if aster_builtins::is_builtin(&cmd_name) {
                    println!("{cmd_name}");
                    return Ok(ExecOutcome::Success(0));
                }
                if ctx.functions.contains_key(&cmd_name) {
                    println!("{cmd_name}");
                    return Ok(ExecOutcome::Success(0));
                }
                if let Some(path) = which_path(&cmd_name) {
                    println!("{}", path.display());
                    return Ok(ExecOutcome::Success(0));
                }
                eprintln!("command: {cmd_name}: not found");
                return Ok(ExecOutcome::Success(1));
            }
            "bg" => {
                let id_str = expanded_cmd.args.first().map(String::as_str).unwrap_or("%1");
                let id = id_str.trim_start_matches('%').parse::<u32>().unwrap_or(1);
                if let Some(job) = ctx.jobs.get(id) {
                    let pgid = job.pgid().unwrap_or(0) as i32;
                    println!("[{}] {} &", id, job.command_string);
                    // Resume the stopped process group in background
                    #[allow(unsafe_code)]
                    unsafe {
                        libc::kill(-pgid, libc::SIGCONT);
                    }
                    job.set_state(aster_shell_core::jobs::JobState::Running);
                } else {
                    eprintln!("bg: job {id} not found");
                    return Ok(ExecOutcome::Success(1));
                }
                return Ok(ExecOutcome::Success(0));
            }
            #[allow(unsafe_code)]
            "kill" => {
                if expanded_cmd.args.is_empty() {
                    eprintln!("kill: usage: kill [-signal] pid");
                    return Ok(ExecOutcome::Success(1));
                }
                let mut sig = 15; // SIGTERM default
                let pid_str = if expanded_cmd.args[0].starts_with('-') {
                    sig = expanded_cmd.args[0]
                        .trim_start_matches('-')
                        .parse::<i32>()
                        .unwrap_or(15);
                    expanded_cmd.args.get(1).map(String::as_str).unwrap_or("0")
                } else {
                    expanded_cmd.args[0].as_str()
                };
                if let Ok(pid) = pid_str.parse::<i32>() {
                    unsafe {
                        libc::kill(pid, sig);
                    }
                    return Ok(ExecOutcome::Success(0));
                }
                eprintln!("kill: invalid pid '{pid_str}'");
                return Ok(ExecOutcome::Success(1));
            }
            "set" => {
                // set: display variables; set -e/-u/-x etc (mostly no-ops for compat)
                if expanded_cmd.args.is_empty() {
                    let mut vars: Vec<_> = ctx.variables.iter().collect();
                    vars.sort_by(|a, b| a.0.cmp(b.0));
                    for (name, value) in vars {
                        println!("{name}={value}");
                    }
                    return Ok(ExecOutcome::Success(0));
                }
                // set -- sets positional args
                if expanded_cmd.args.first().map(String::as_str) == Some("--") {
                    ctx.positional_args = expanded_cmd.args[1..].to_vec();
                    return Ok(ExecOutcome::Success(0));
                }
                // set -e/-u/-x etc — silently accept for compat
                if expanded_cmd.args[0].starts_with('-') {
                    return Ok(ExecOutcome::Success(0));
                }
                // set VAR=value — set variables
                for arg in &expanded_cmd.args {
                    if let Some((name, value)) = arg.split_once('=') {
                        let expanded_value = Self::expand_variables(value, ctx);
                        ctx.variables.insert(name.to_string(), expanded_value);
                    }
                }
                return Ok(ExecOutcome::Success(0));
            }
            "read" => {
                let mut prompt = String::new();
                let mut silent = false;
                let mut raw = false;
                let mut args_iter = expanded_cmd.args.iter();
                while let Some(arg) = args_iter.next() {
                    if arg == "-p" {
                        prompt = args_iter.next().cloned().unwrap_or_default();
                    } else if arg == "-s" {
                        silent = true;
                    } else if arg == "-r" {
                        raw = true;
                    } else if arg.starts_with('-') && arg.len() > 1 && !arg.starts_with("--") {
                        // Combine flags like -sr, -rp
                        for ch in arg[1..].chars() {
                            match ch {
                                'p' => { prompt = args_iter.next().cloned().unwrap_or_default(); }
                                's' => { silent = true; }
                                'r' => { raw = true; }
                                _ => {}
                            }
                        }
                    } else {
                        // First non-flag arg is the variable name
                        if !prompt.is_empty() || !arg.starts_with('-') {
                            let var_name = arg;
                            let mut input = String::new();
                            if !prompt.is_empty() {
                                use std::io::Write;
                                let _ = print!("{prompt}");
                                let _ = std::io::stdout().flush();
                            }
                            if silent {
                                // Read without echo (for passwords)
                                let _ = std::io::stdin().read_line(&mut input);
                            } else {
                                let _ = std::io::stdin().read_line(&mut input);
                            }
                            let value = if raw {
                                input.trim_end_matches('\n').trim_end_matches('\r').to_string()
                            } else {
                                input.trim_end_matches('\n').trim_end_matches('\r')
                                    .replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\")
                            };
                            ctx.variables.insert(var_name.to_string(), value);
                            return Ok(ExecOutcome::Success(0));
                        }
                    }
                }
                // Default: no args, read into REPLY
                let mut input = String::new();
                if !prompt.is_empty() {
                    use std::io::Write;
                    let _ = print!("{prompt}");
                    let _ = std::io::stdout().flush();
                }
                let _ = std::io::stdin().read_line(&mut input);
                let input = input.trim_end_matches('\n').trim_end_matches('\r').to_string();
                ctx.variables.insert("REPLY".to_string(), input);
                return Ok(ExecOutcome::Success(0));
            }
            "compgen" => {
                return Self::builtin_compgen(&expanded_cmd.args, ctx);
            }
            "shift" => {
                let n: usize = expanded_cmd.args.first()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1);
                if n > 0 && n <= ctx.positional_args.len() {
                    ctx.positional_args.drain(..n);
                } else if n > ctx.positional_args.len() {
                    ctx.positional_args.clear();
                }
                return Ok(ExecOutcome::Success(0));
            }
            "mapfile" | "readarray" => {
                let mut var_name = "MAPFILE".to_string();
                let mut skip = 0usize;
                let mut count = 0usize;
                let mut i = 0;
                while i < expanded_cmd.args.len() {
                    if expanded_cmd.args[i] == "-t" {
                        // trim newlines
                        i += 1;
                    } else if expanded_cmd.args[i] == "-O" {
                        i += 1;
                        skip = expanded_cmd.args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
                        i += 1;
                    } else if expanded_cmd.args[i] == "-n" {
                        i += 1;
                        count = expanded_cmd.args.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
                        i += 1;
                    } else if expanded_cmd.args[i].starts_with('-') {
                        i += 1;
                    } else {
                        var_name = expanded_cmd.args[i].clone();
                        i += 1;
                    }
                }
                let mut lines = Vec::new();
                let mut buf = String::new();
                let _ = std::io::stdin().read_to_string(&mut buf);
                for (idx, line) in buf.lines().enumerate() {
                    if idx < skip {
                        continue;
                    }
                    lines.push(line.to_string());
                    if count > 0 && lines.len() >= count {
                        break;
                    }
                }
                ctx.variables.insert(var_name, lines.join("\n"));
                return Ok(ExecOutcome::Success(0));
            }
            "dirname" => {
                for arg in &expanded_cmd.args {
                    let p = std::path::Path::new(arg.as_str());
                    if let Some(parent) = p.parent() {
                        println!("{}", parent.display());
                    } else {
                        println!(".");
                    }
                }
                return Ok(ExecOutcome::Success(0));
            }
            "basename" => {
                let args: Vec<&String> = expanded_cmd.args.iter().filter(|a| !a.starts_with('-')).collect();
                if args.is_empty() {
                    eprintln!("basename: missing operand");
                    return Ok(ExecOutcome::Success(1));
                }
                let p = std::path::Path::new(args[0].as_str());
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                if args.len() >= 2 {
                    if let Some(stripped) = name.strip_suffix(args[1].as_str()) {
                        println!("{stripped}");
                    } else {
                        println!("{name}");
                    }
                } else {
                    println!("{name}");
                }
                return Ok(ExecOutcome::Success(0));
            }
            _ => {}
        }

        // Inline math: =2+3 prints 5
        if expanded_cmd.name.starts_with('=') {
            let expr = &expanded_cmd.name[1..];
            let result = eval_arithmetic(expr, ctx)?;
            println!("{result}");
            return Ok(ExecOutcome::Success(0));
        }

        // Handle export VAR=value directly (syncs with ctx.variables)
        if expanded_cmd.name == "export" {
            if expanded_cmd.args.is_empty() {
                // export without args: export all ctx.variables
                let mut vars: Vec<_> = ctx.variables.iter().collect();
                vars.sort_by(|a, b| a.0.cmp(b.0));
                for (name, value) in vars {
                    #[allow(unsafe_code)]
                    unsafe { std::env::set_var(name.as_str(), value.as_str()); }
                    println!("declare -x {name}=\"{value}\"");
                }
                return Ok(ExecOutcome::Success(0));
            }
            for arg in &expanded_cmd.args {
                if let Some((name, value)) = arg.split_once('=') {
                    let expanded_value = Self::expand_variables(value, ctx);
                    ctx.variables.insert(name.to_string(), expanded_value.clone());
                    #[allow(unsafe_code)]
                    unsafe { std::env::set_var(name, expanded_value.as_str()); }
                } else if let Some(value) = ctx.variables.get(arg).cloned() {
                    #[allow(unsafe_code)]
                    unsafe { std::env::set_var(arg.as_str(), value.as_str()); }
                } else if let Ok(value) = std::env::var(arg) {
                    ctx.variables.insert(arg.clone(), value);
                }
            }
            return Ok(ExecOutcome::Success(0));
        }

        // Handle declare/local/readonly
        if expanded_cmd.name == "declare" || expanded_cmd.name == "typeset" {
            for arg in &expanded_cmd.args {
                if let Some((name, value)) = arg.split_once('=') {
                    let expanded_value = Self::expand_variables(value, ctx);
                    ctx.variables.insert(name.to_string(), expanded_value);
                } else {
                    // declare VAR (no value): just export
                    if let Ok(value) = std::env::var(arg) {
                        ctx.variables.insert(arg.clone(), value);
                    }
                }
            }
            return Ok(ExecOutcome::Success(0));
        }
        if expanded_cmd.name == "local" {
            // In a function, set in ctx.variables
            for arg in &expanded_cmd.args {
                if let Some((name, value)) = arg.split_once('=') {
                    let expanded_value = Self::expand_variables(value, ctx);
                    ctx.variables.insert(name.to_string(), expanded_value);
                }
            }
            return Ok(ExecOutcome::Success(0));
        }
        if expanded_cmd.name == "readonly" {
            for arg in &expanded_cmd.args {
                if let Some((name, value)) = arg.split_once('=') {
                    let expanded_value = Self::expand_variables(value, ctx);
                    ctx.variables.insert(name.to_string(), expanded_value.clone());
                    #[allow(unsafe_code)]
                    unsafe { std::env::set_var(name, expanded_value.as_str()); }
                }
            }
            return Ok(ExecOutcome::Success(0));
        }

        // Handle unset directly
        if expanded_cmd.name == "unset" {
            for arg in &expanded_cmd.args {
                ctx.variables.remove(arg);
                #[allow(unsafe_code)]
                unsafe { std::env::remove_var(arg.as_str()); }
            }
            return Ok(ExecOutcome::Success(0));
        }

        // Handle env directly
        if expanded_cmd.name == "env" {
            let mut vars: Vec<_> = ctx.variables.iter().collect();
            vars.sort_by(|a, b| a.0.cmp(b.0));
            for (name, value) in vars {
                println!("{name}={value}");
            }
            return Ok(ExecOutcome::Success(0));
        }

        // Check for function invocation
        if let Some(body) = ctx.functions.get(&expanded_cmd.name).cloned() {
            let was_in_function = ctx.in_function;
            ctx.in_function = true;
            let result = Self::execute_body(&body, ctx);
            ctx.in_function = was_in_function;
            return result;
        }

        // Check for standard builtins
        if aster_builtins::is_builtin(&expanded_cmd.name) {
            let mut env = aster_shell_core::ShellEnvironment::from_process();
            if let Some(result) = aster_builtins::execute(
                &expanded_cmd.name,
                &expanded_cmd.args,
                &mut env,
                &mut ctx.aliases,
            )? {
                return Ok(ExecOutcome::Success(result));
            }
        }

        // External command
        match Self::execute_external(&expanded_cmd) {
            Err(ShellError::Exec(ExecError::CommandNotFound(name))) => {
                let suggestion = suggest_command(&name);
                if let Some(sug) = suggestion {
                    eprintln!(
                        "aster: command not found: {name}\n       did you mean `{sug}`?"
                    );
                } else {
                    eprintln!("aster: command not found: {name}");
                }
                Ok(ExecOutcome::Success(127))
            }
            Err(e) => Err(e),
            Ok(outcome) => Ok(outcome),
        }
    }

    fn execute_external(cmd: &SimpleCommand) -> Result<ExecOutcome, ShellError> {
        let (mut command, heredocs) = Self::build_command(cmd)?;

        if !heredocs.is_empty() {
            command.stdin(Stdio::piped());
        }

        let mut child = command.spawn().map_err(|e| ExecError::SpawnFailed {
            command: cmd.name.clone(),
            reason: e.to_string(),
        })?;

        if !heredocs.is_empty() {
            if let Some(mut stdin) = child.stdin.take() {
                for content in &heredocs {
                    stdin
                        .write_all(content.as_bytes())
                        .map_err(|e| ExecError::SpawnFailed {
                            command: cmd.name.clone(),
                            reason: e.to_string(),
                        })?;
                }
            }
        }

        let status = child.wait().map_err(|e| ExecError::SpawnFailed {
            command: cmd.name.clone(),
            reason: e.to_string(),
        })?;

        let code = status.code().unwrap_or(1);
        Ok(ExecOutcome::Success(code))
    }

    #[allow(unsafe_code)]
    fn execute_group(group: &Group, ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        if group.body.statements.is_empty() {
            return Ok(ExecOutcome::Success(0));
        }

        // Fork a child process for true variable/cd isolation (POSIX subshell)
        let child_pid = unsafe { libc::fork() };
        if child_pid < 0 {
            return Self::execute(&group.body, ctx);
        }

        if child_pid == 0 {
            // Child — isolated context
            let mut child_ctx = ExecContext { ..ExecContext::default() };
            child_ctx.variables = ctx.variables.clone();
            child_ctx.positional_args = ctx.positional_args.clone();
            child_ctx.functions = ctx.functions.clone();
            child_ctx.aliases = ctx.aliases.clone();
            child_ctx.abbreviations = ctx.abbreviations.clone();
            child_ctx.trap_handlers = ctx.trap_handlers.clone();
            child_ctx.current_line = ctx.current_line;
            child_ctx.start_time = ctx.start_time;
            child_ctx.last_shell_options = ctx.last_shell_options.clone();

            let code = match Self::execute(&group.body, &mut child_ctx) {
                Ok(ExecOutcome::Success(c)) => c,
                Ok(ExecOutcome::Exit(c)) => c,
                Ok(ExecOutcome::Break) | Ok(ExecOutcome::Continue) => 0,
                Err(_) => 1,
            };
            unsafe { libc::_exit(code); }
        }

        // Parent — wait for child
        let mut status: libc::c_int = 0;
        unsafe { libc::waitpid(child_pid, &mut status, 0); }
        let code = if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else {
            1
        };
        ctx.last_exit_code = code;
        Ok(ExecOutcome::Success(code))
    }

    fn builtin_trap(args: &[String], ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        fn parse_signal(name: &str) -> Option<i32> {
            match name {
                "SIGHUP" | "HUP" | "1" => Some(1),
                "SIGINT" | "INT" | "2" => Some(2),
                "SIGQUIT" | "QUIT" | "3" => Some(3),
                "SIGTERM" | "TERM" | "15" => Some(15),
                "SIGUSR1" | "USR1" | "10" => Some(10),
                "SIGUSR2" | "USR2" | "12" => Some(12),
                "SIGCHLD" | "CHLD" | "17" => Some(17),
                "SIGSTOP" | "STOP" | "19" => Some(19),
                "SIGCONT" | "CONT" | "18" => Some(18),
                "SIGPIPE" | "PIPE" | "13" => Some(13),
                "SIGALRM" | "ALRM" | "14" => Some(14),
                "SIGTSTP" | "TSTP" | "20" => Some(20),
                "SIGTTIN" | "TTIN" | "21" => Some(21),
                "SIGTTOU" | "TTOU" | "22" => Some(22),
                "SIGWINCH" | "WINCH" | "28" => Some(28),
                "EXIT" | "0" => Some(0),
                _ => None,
            }
        }

        match args.len() {
            0 => {
                let mut handlers: Vec<_> = ctx.trap_handlers.iter().collect();
                handlers.sort_by_key(|(k, _)| *k);
                for (sig, cmd) in handlers {
                    let name = match sig {
                        0 => "EXIT", 1 => "SIGHUP", 2 => "SIGINT", 3 => "SIGQUIT",
                        10 => "SIGUSR1", 12 => "SIGUSR2", 13 => "SIGPIPE", 14 => "SIGALRM",
                        15 => "SIGTERM", 17 => "SIGCHLD", 18 => "SIGCONT", 19 => "SIGSTOP",
                        20 => "SIGTSTP", 21 => "SIGTTIN", 22 => "SIGTTOU", 28 => "SIGWINCH",
                        _ => "UNKNOWN",
                    };
                    println!("trap -- '{cmd}' {name}");
                }
                return Ok(ExecOutcome::Success(0));
            }
            1 => {
                if args[0] == "-" {
                    ctx.trap_handlers.clear();
                }
                return Ok(ExecOutcome::Success(0));
            }
            _ => {
                let action = &args[0];
                for sig_name in &args[1..] {
                    if let Some(sig) = parse_signal(sig_name) {
                        if action == "-" {
                            ctx.trap_handlers.remove(&sig);
                        } else {
                            ctx.trap_handlers.insert(sig, action.clone());
                        }
                    } else {
                        eprintln!("trap: unknown signal '{sig_name}'");
                    }
                }
                return Ok(ExecOutcome::Success(0));
            }
        }
    }

    /// Runs a trap action string (as a command).
    fn run_trap_action(action: &str, ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        let tokens = Lexer::new(action).tokenize().map_err(|e| {
            ShellError::Exec(ExecError::SpawnFailed {
                command: "trap".into(),
                reason: e.to_string(),
            })
        })?;
        let program = Parser::new(&tokens).parse().map_err(|e| {
            ShellError::Exec(ExecError::SpawnFailed {
                command: "trap".into(),
                reason: e.to_string(),
            })
        })?;
        Self::execute(&program, ctx)
    }

    fn builtin_wait(args: &[String], ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        if args.is_empty() {
            // wait: wait for all background jobs
            let jobs: Vec<u32> = ctx.jobs.list().iter().map(|j| j.id).collect();
            for id in jobs {
                if let Some(job) = ctx.jobs.get(id) {
                    if let Some(pgid) = job.pgid() {
                        let mut status: i32 = 0;
                        #[allow(unsafe_code)]
                        unsafe {
                            libc::waitpid(pgid as i32, &mut status, libc::WUNTRACED);
                        }
                        if libc::WIFEXITED(status) {
                            job.set_state(aster_shell_core::jobs::JobState::Completed);
                        } else if libc::WIFSTOPPED(status) {
                            job.set_state(aster_shell_core::jobs::JobState::Stopped);
                        }
                    }
                }
            }
            ctx.jobs.cleanup();
            return Ok(ExecOutcome::Success(ctx.last_exit_code));
        }

        // wait %N — wait for specific job
        let mut last_code = 0;
        for arg in args {
            let id = arg.trim_start_matches('%').parse::<u32>().unwrap_or(0);
            if let Some(job) = ctx.jobs.get(id) {
                if let Some(pgid) = job.pgid() {
                    let mut status: i32 = 0;
                    #[allow(unsafe_code)]
                    unsafe {
                        libc::waitpid(pgid as i32, &mut status, libc::WUNTRACED);
                    }
                    if libc::WIFEXITED(status) {
                        last_code = libc::WEXITSTATUS(status);
                        job.set_state(aster_shell_core::jobs::JobState::Completed);
                    } else if libc::WIFSTOPPED(status) {
                        job.set_state(aster_shell_core::jobs::JobState::Stopped);
                    }
                }
            }
        }
        ctx.last_exit_code = last_code;
        Ok(ExecOutcome::Success(last_code))
    }

    fn builtin_compgen(args: &[String], ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        let mut word = String::new();
        let mut options = Vec::new();

        let mut i = 0;
        while i < args.len() {
            if args[i] == "-W" {
                i += 1;
                if let Some(w) = args.get(i) {
                    options.push(("W", w.clone()));
                }
            } else if args[i] == "-A" {
                i += 1;
                if let Some(a) = args.get(i) {
                    options.push(("A", a.clone()));
                }
            } else if args[i].starts_with('-') {
                // -a, -b, -c, -d, -e, -f, -g, -v, -u, -k, -s, -t
                let flags: Vec<char> = args[i].trim_start_matches('-').chars().collect();
                for f in flags {
                    options.push(("F", f.to_string()));
                }
            } else {
                word = args[i].clone();
            }
            i += 1;
        }

        let mut completions = Vec::new();

        // Check for -W (word list)
        for (kind, val) in &options {
            if *kind == "W" {
                for w in val.split_whitespace() {
                    if w.starts_with(&word) || word.is_empty() {
                        completions.push(w.to_string());
                    }
                }
            }
        }

        // Check for -A (action) and -F (flags)
        let mut actions = Vec::new();
        let mut flags = Vec::new();
        for (kind, val) in &options {
            match *kind {
                "A" => actions.push(val.as_str()),
                "F" => flags.push(val.as_str()),
                _ => {}
            }
        }

        // If no -W, generate from flags/actions
        if !actions.is_empty() || !flags.is_empty() {
            for action in &actions {
                match *action {
                    "command" | "c" => {
                        let cmds = aster_completion::Completer::complete_commands();
                        for c in cmds {
                            if c.text.starts_with(&word) {
                                completions.push(c.text);
                            }
                        }
                    }
                    "file" | "f" => {
                        let files = aster_completion::Completer::complete_files(&word);
                        for f in files {
                            completions.push(f.text);
                        }
                    }
                    "directory" | "d" => {
                        let dirs = aster_completion::Completer::complete_directories(&word);
                        for d in dirs {
                            completions.push(d.text);
                        }
                    }
                    "variable" | "v" => {
                        for (name, _) in &ctx.variables {
                            if name.starts_with(&word) {
                                completions.push(name.clone());
                            }
                        }
                    }
                    "export" | "e" => {
                        for (name, _) in &ctx.variables {
                            if name.starts_with(&word) {
                                completions.push(name.clone());
                            }
                        }
                    }
                    "alias" | "a" => {
                        for (name, _) in ctx.aliases.entries() {
                            if name.starts_with(&word) {
                                completions.push(name.to_string());
                            }
                        }
                    }
                    "builtin" | "b" => {
                        for (name, _) in aster_builtins::builtin_list() {
                            if name.starts_with(&word) {
                                completions.push(name.to_string());
                            }
                        }
                    }
                    "job" | "j" => {
                        // Not implemented
                    }
                    _ => {}
                }
            }

            for flag in &flags {
                match *flag {
                    "a" => {
                        for (name, _) in ctx.aliases.entries() {
                            if name.starts_with(&word) {
                                completions.push(name.to_string());
                            }
                        }
                    }
                    "b" => {
                        for (name, _) in aster_builtins::builtin_list() {
                            if name.starts_with(&word) {
                                completions.push(name.to_string());
                            }
                        }
                    }
                    "c" | "e" | "g" | "k" | "s" | "t" | "v" | "u" => {
                        // Simplified — most map to variable name completion
                        for (name, _) in &ctx.variables {
                            if name.starts_with(&word) {
                                completions.push(name.clone());
                            }
                        }
                    }
                    "f" => {
                        let files = aster_completion::Completer::complete_files(&word);
                        for f in files {
                            completions.push(f.text);
                        }
                    }
                    "d" => {
                        let dirs = aster_completion::Completer::complete_directories(&word);
                        for d in dirs {
                            completions.push(d.text);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Default: if nothing specified, complete commands
        if completions.is_empty() && actions.is_empty() && flags.is_empty() && options.is_empty() {
            let cmds = aster_completion::Completer::complete_commands();
            for c in cmds {
                if c.text.starts_with(&word) {
                    completions.push(c.text);
                }
            }
        }

        completions.sort();
        completions.dedup();
        for c in &completions {
            println!("{c}");
        }
        Ok(ExecOutcome::Success(0))
    }

    fn builtin_cd(args: &[String], ctx: &mut ExecContext) -> Result<ExecOutcome, ShellError> {
        let target = if args.is_empty() {
            dirs::home_dir().ok_or_else(|| ExecError::CdError("HOME not set".into()))?
        } else if args[0] == "-" {
            ctx.prev_dir
                .as_ref()
                .ok_or_else(|| ExecError::CdError("OLDPWD not set".into()))?
                .clone()
        } else if args[0].starts_with('~') {
            let rest = &args[0][1..];
            let home = dirs::home_dir().ok_or_else(|| ExecError::CdError("HOME not set".into()))?;
            home.join(rest)
        } else if args[0] == ".." || args[0] == "." {
            // Standard: .. goes up 1, . stays
            let current = env::current_dir().map_err(|e| ExecError::DirError(e.to_string()))?;
            if args[0] == ".." {
                current.parent().unwrap_or(&current).to_path_buf()
            } else {
                current
            }
        } else if args[0].starts_with("..") && args[0].len() > 2 {
            // Smart cd: ..N goes up N directories (e.g., ..3 = cd ../../../)
            let n_str = &args[0][2..];
            if let Ok(n) = n_str.parse::<usize>() {
                if n > 0 && n <= 10 {
                    let mut current =
                        env::current_dir().map_err(|e| ExecError::DirError(e.to_string()))?;
                    for _ in 0..n {
                        current = current
                            .parent()
                            .unwrap_or(&current)
                            .to_path_buf();
                    }
                    current
                } else {
                    return Err(ExecError::CdError(format!(
                        "{}: invalid directory count (1-10)",
                        args[0]
                    ))
                    .into());
                }
            } else {
                PathBuf::from(&args[0])
            }
        } else {
            PathBuf::from(&args[0])
        };

        let current = env::current_dir().map_err(|e| ExecError::DirError(e.to_string()))?;

        env::set_current_dir(&target)
            .map_err(|e| ExecError::CdError(format!("{}: {}", target.display(), e)))?;

        if args.first().map_or(false, |a| a == "-") {
            println!("{}", target.display());
        }

        ctx.prev_dir = Some(current);
        Ok(ExecOutcome::Success(0))
    }

    fn apply_redirects(
        command: &mut Command,
        redirects: &[Redirect],
    ) -> Result<Vec<String>, ShellError> {
        let mut heredocs = Vec::new();
        for redirect in redirects {
            match redirect.kind {
                RedirectKind::Input => {
                    let file =
                        File::open(&redirect.target).map_err(|e| ExecError::RedirectFailed {
                            target: redirect.target.clone(),
                            reason: e.to_string(),
                        })?;
                    command.stdin(file);
                }
                RedirectKind::Output => {
                    let file =
                        File::create(&redirect.target).map_err(|e| ExecError::RedirectFailed {
                            target: redirect.target.clone(),
                            reason: e.to_string(),
                        })?;
                    match redirect.fd {
                        Some(2) => { command.stderr(file); }
                        _ => { command.stdout(file); }
                    }
                }
                RedirectKind::Append => {
                    let file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&redirect.target)
                        .map_err(|e| ExecError::RedirectFailed {
                            target: redirect.target.clone(),
                            reason: e.to_string(),
                        })?;
                    match redirect.fd {
                        Some(2) => { command.stderr(file); }
                        _ => { command.stdout(file); }
                    }
                }
                RedirectKind::FdOutput => {
                    let target_str = redirect.target.trim();
                    // Handle 2>&1 (dup stderr to stdout) and 1>&2 (dup stdout to stderr)
                    if target_str == "1" && redirect.fd == Some(2) {
                        command.stderr(std::process::Stdio::inherit());
                    } else if target_str == "2" && redirect.fd == Some(1) {
                        command.stdout(std::process::Stdio::piped()); // simplified
                    } else {
                        let file =
                            File::create(&redirect.target).map_err(|e| ExecError::RedirectFailed {
                                target: redirect.target.clone(),
                                reason: e.to_string(),
                            })?;
                        match redirect.fd {
                            Some(2) => { command.stderr(file); }
                            Some(1) | None => { command.stdout(file); }
                            _ => { command.stdout(file); }
                        }
                    }
                }
                RedirectKind::FdAppend => {
                    let file = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&redirect.target)
                        .map_err(|e| ExecError::RedirectFailed {
                            target: redirect.target.clone(),
                            reason: e.to_string(),
                        })?;
                    match redirect.fd {
                        Some(2) => { command.stderr(file); }
                        _ => { command.stdout(file); }
                    }
                }
                RedirectKind::HereDoc | RedirectKind::HereString => {
                    if let Some(body) = &redirect.body {
                        heredocs.push(body.clone());
                    }
                }
                _ => {
                    // FdInput, FdDup, FdClose — not yet fully implemented
                }
            }
        }
        Ok(heredocs)
    }
}

/// Creates a pipe and returns `(read_fd, write_fd)`.
#[allow(unsafe_code)]
fn pipe() -> Result<(i32, i32), std::io::Error> {
    let mut fds = [0i32; 2];
    let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if ret == 0 {
        Ok((fds[0], fds[1]))
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Simple glob matching for case patterns: `*` matches any, `?` matches one char, `[...]` matches character class.
fn simple_glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_match_inner(&p, &t)
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    let mut pi = 0;
    let mut ti = 0;

    while pi < pattern.len() {
        match pattern[pi] {
            '*' => {
                // Skip consecutive stars
                while pi + 1 < pattern.len() && pattern[pi + 1] == '*' {
                    pi += 1;
                }
                // Try matching zero or more characters
                let rest = &pattern[pi + 1..];
                for skip in 0..=text.len() - ti {
                    if glob_match_inner(rest, &text[ti + skip..]) {
                        return true;
                    }
                }
                return false;
            }
            '?' => {
                if ti >= text.len() {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
            '[' => {
                if ti >= text.len() {
                    return false;
                }
                // Find closing bracket
                if let Some(end) = pattern[pi + 1..].iter().position(|&c| c == ']') {
                    let charset = &pattern[pi + 1..pi + 1 + end];
                    let negate = charset.first() == Some(&'!');
                    let chars_to_check = if negate { &charset[1..] } else { charset };
                    let matched = chars_to_check.contains(&text[ti]);
                    if negate == matched {
                        return false;
                    }
                    pi += end + 2;
                    ti += 1;
                } else {
                    // No closing bracket, treat '[' as literal
                    if pattern[pi] != text[ti] {
                        return false;
                    }
                    pi += 1;
                    ti += 1;
                }
            }
            c => {
                if ti >= text.len() || text[ti] != c {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
        }
    }

    ti == text.len() && pi == pattern.len()
}

// ---------------------------------------------------------------------------
// Arithmetic expression evaluator
// ---------------------------------------------------------------------------

struct ArithParser<'a> {
    input: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
}

impl<'a> ArithParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.char_indices().peekable(),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&(_, ch)) = self.chars.peek() {
            if ch.is_ascii_whitespace() {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        self.skip_whitespace();
        self.chars.peek().map(|&(_, c)| c)
    }

    fn peek_two(&mut self) -> Option<(char, char)> {
        self.skip_whitespace();
        let saved: Vec<_> = self.chars.clone().take(2).collect();
        if saved.len() == 2 {
            Some((saved[0].1, saved[1].1))
        } else if saved.len() == 1 {
            Some((saved[0].1, '\0'))
        } else {
            None
        }
    }

    fn consume(&mut self, expected: &str) -> bool {
        self.skip_whitespace();
        let remaining = &self.input[self.chars.clone().next().map_or(self.input.len(), |(i, _)| i)..];
        if remaining.starts_with(expected) {
            for _ in 0..expected.len() {
                self.chars.next();
            }
            true
        } else {
            false
        }
    }

    fn parse_expr(&mut self) -> Result<i64, ShellError> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<i64, ShellError> {
        let val = self.parse_logical()?;
        if self.peek_char() == Some('?') {
            self.consume("?");
            let then_val = self.parse_expr()?;
            if !self.consume(":") {
                return Err(ShellError::Exec(ExecError::ArithmeticError(
                    "expected ':' in ternary".into(),
                )));
            }
            let else_val = self.parse_expr()?;
            Ok(if val != 0 { then_val } else { else_val })
        } else {
            Ok(val)
        }
    }

    fn parse_logical(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_bitwise()?;
        loop {
            if self.consume("&&") {
                let right = self.parse_bitwise()?;
                left = if left != 0 && right != 0 { 1 } else { 0 };
            } else if self.consume("||") {
                let right = self.parse_bitwise()?;
                left = if left != 0 || right != 0 { 1 } else { 0 };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_bitwise(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_comparison()?;
        loop {
            if self.peek_char() == Some('&') && !self.peek_two().map_or(false, |(a, b)| a == '&' && b == '&') {
                self.consume("&");
                let right = self.parse_comparison()?;
                left = left & right;
            } else if self.peek_char() == Some('|') && !self.peek_two().map_or(false, |(a, b)| a == '|' && b == '|') {
                self.consume("|");
                let right = self.parse_comparison()?;
                left = left | right;
            } else if self.consume("^") {
                let right = self.parse_comparison()?;
                left = left ^ right;
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_shift()?;
        loop {
            if self.consume("==") {
                let right = self.parse_shift()?;
                left = if left == right { 1 } else { 0 };
            } else if self.consume("!=") {
                let right = self.parse_shift()?;
                left = if left != right { 1 } else { 0 };
            } else if self.consume("<=") {
                let right = self.parse_shift()?;
                left = if left <= right { 1 } else { 0 };
            } else if self.consume(">=") {
                let right = self.parse_shift()?;
                left = if left >= right { 1 } else { 0 };
            } else if self.consume("<") {
                let right = self.parse_shift()?;
                left = if left < right { 1 } else { 0 };
            } else if self.consume(">") {
                let right = self.parse_shift()?;
                left = if left > right { 1 } else { 0 };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_additive()?;
        loop {
            if self.consume("<<") {
                let right = self.parse_additive()?;
                left = left.wrapping_shl(right as u32);
            } else if self.consume(">>") {
                let right = self.parse_additive()?;
                left = left.wrapping_shr(right as u32);
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            if self.peek_char() == Some('+') {
                self.consume("+");
                let right = self.parse_multiplicative()?;
                left = left.wrapping_add(right);
            } else if self.peek_char() == Some('-') {
                self.consume("-");
                let right = self.parse_multiplicative()?;
                left = left.wrapping_sub(right);
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<i64, ShellError> {
        let mut left = self.parse_power()?;
        loop {
            if self.peek_char() == Some('*') && !self.peek_two().map_or(false, |(a, b)| a == '*' && b == '*') {
                self.consume("*");
                let right = self.parse_power()?;
                left = left.wrapping_mul(right);
            } else if self.consume("/") {
                let right = self.parse_power()?;
                if right == 0 {
                    return Err(ShellError::Exec(ExecError::ArithmeticError(
                        "division by zero".into(),
                    )));
                }
                left = left.wrapping_div(right);
            } else if self.consume("%") {
                let right = self.parse_power()?;
                if right == 0 {
                    return Err(ShellError::Exec(ExecError::ArithmeticError(
                        "division by zero".into(),
                    )));
                }
                left = left.wrapping_rem(right);
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<i64, ShellError> {
        let base = self.parse_unary()?;
        if self.consume("**") {
            let exp = self.parse_power()?; // right-associative
            if exp < 0 {
                return Err(ShellError::Exec(ExecError::ArithmeticError(
                    "negative exponent".into(),
                )));
            }
            let exp = exp as u32;
            Ok(base.wrapping_pow(exp))
        } else {
            Ok(base)
        }
    }

    fn parse_unary(&mut self) -> Result<i64, ShellError> {
        self.skip_whitespace();
        if self.consume("-") {
            let val = self.parse_unary()?;
            Ok(val.wrapping_neg())
        } else if self.consume("+") {
            self.parse_unary()
        } else if self.consume("~") {
            let val = self.parse_unary()?;
            Ok(!val)
        } else if self.consume("!") {
            let val = self.parse_unary()?;
            Ok(if val == 0 { 1 } else { 0 })
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<i64, ShellError> {
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<i64, ShellError> {
        self.skip_whitespace();
        if self.peek_char() == Some('(') {
            self.consume("(");
            let val = self.parse_expr()?;
            if !self.consume(")") {
                return Err(ShellError::Exec(ExecError::ArithmeticError(
                    "unmatched parenthesis".into(),
                )));
            }
            return Ok(val);
        }

        // ${VAR}
        if self.peek_two() == Some(('{', '$')) {
            return Err(ShellError::Exec(ExecError::ArithmeticError(
                "unexpected '${' in expression".into(),
            )));
        }
        if self.peek_char() == Some('$') {
            // Check for ${VAR} syntax used in arithmetic context
            self.consume("$");
            if self.peek_char() == Some('{') {
                self.consume("{");
                let mut var_name = String::new();
                while let Some(ch) = self.peek_char() {
                    if ch == '}' {
                        self.consume("}");
                        break;
                    }
                    var_name.push(ch);
                    self.chars.next();
                }
                return self.resolve_var(&var_name);
            }
            let mut var_name = String::new();
            while let Some(ch) = self.peek_char() {
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    var_name.push(ch);
                    self.chars.next();
                } else {
                    break;
                }
            }
            return self.resolve_var(&var_name);
        }

        // Integer literal
        self.parse_integer()
    }

    fn resolve_var(&self, name: &str) -> Result<i64, ShellError> {
        let val = std::env::var(name)
            .or_else(|_| {
                // We don't have ctx here, so check env only
                Err(std::env::VarError::NotPresent)
            })
            .unwrap_or_default();
        Ok(val.parse::<i64>().unwrap_or(0))
    }

    fn parse_integer(&mut self) -> Result<i64, ShellError> {
        self.skip_whitespace();
        let start = self
            .chars
            .clone()
            .next()
            .map_or(self.input.len(), |(i, _)| i);

        // Collect all digit chars (and hex digits if hex)
        let mut s = String::new();
        let mut is_hex = false;

        if let Some(&(_, '0')) = self.chars.peek() {
            let mut ahead = self.chars.clone();
            ahead.next();
            if let Some(&(_, c)) = ahead.peek() {
                if c == 'x' || c == 'X' {
                    is_hex = true;
                    s.push('0');
                    self.chars.next(); // skip '0'
                    self.chars.next(); // skip 'x'
                    s.push_str("x");
                } else if c == 'o' || c == 'O' {
                    self.chars.next();
                    self.chars.next();
                    // octal
                    while let Some(&(_, ch)) = self.chars.peek() {
                        if ch >= '0' && ch <= '7' {
                            s.push(ch);
                            self.chars.next();
                        } else {
                            break;
                        }
                    }
                    if s.is_empty() {
                        return Err(ShellError::Exec(ExecError::ArithmeticError(
                            "invalid octal literal".into(),
                        )));
                    }
                    let val = i64::from_str_radix(&s, 8).map_err(|e| {
                        ShellError::Exec(ExecError::ArithmeticError(format!(
                            "octal parse error: {e}"
                        )))
                    })?;
                    return Ok(val);
                } else if c == 'b' || c == 'B' {
                    self.chars.next();
                    self.chars.next();
                    while let Some(&(_, ch)) = self.chars.peek() {
                        if ch == '0' || ch == '1' {
                            s.push(ch);
                            self.chars.next();
                        } else {
                            break;
                        }
                    }
                    if s.is_empty() {
                        return Err(ShellError::Exec(ExecError::ArithmeticError(
                            "invalid binary literal".into(),
                        )));
                    }
                    let val = i64::from_str_radix(&s, 2).map_err(|e| {
                        ShellError::Exec(ExecError::ArithmeticError(format!(
                            "binary parse error: {e}"
                        )))
                    })?;
                    return Ok(val);
                }
            }
        }

        if is_hex {
            while let Some(&(_, ch)) = self.chars.peek() {
                if ch.is_ascii_hexdigit() {
                    s.push(ch);
                    self.chars.next();
                } else {
                    break;
                }
            }
            let hex_str = &s[2..]; // skip "0x"
            if hex_str.is_empty() {
                return Err(ShellError::Exec(ExecError::ArithmeticError(
                    "invalid hex literal".into(),
                )));
            }
            let val = i64::from_str_radix(hex_str, 16).map_err(|e| {
                ShellError::Exec(ExecError::ArithmeticError(format!("hex parse error: {e}")))
            })?;
            return Ok(val);
        }

        while let Some(&(_, ch)) = self.chars.peek() {
            if ch.is_ascii_digit() {
                s.push(ch);
                self.chars.next();
            } else {
                break;
            }
        }

        if s.is_empty() {
            return Err(ShellError::Exec(ExecError::ArithmeticError(format!(
                "unexpected token at position {start}"
            ))));
        }

        let val = s.parse::<i64>().map_err(|e| {
            ShellError::Exec(ExecError::ArithmeticError(format!("integer parse error: {e}")))
        })?;
        Ok(val)
    }
}

/// Evaluates an arithmetic expression string.
///
/// Supports integer literals (decimal, 0x hex, 0 octal, 0b binary),
/// variable references (`$var`, `${var}`), operators: `+`, `-`, `*`, `/`,
/// `%` (mod), `**` (power), parentheses, unary minus/plus/`~`/`!`,
/// comparison operators, bitwise operators, logical operators, and ternary.
pub fn eval_arithmetic(expr: &str, ctx: &ExecContext) -> Result<i64, ShellError> {
    // Expand variables in the expression first
    let expanded = expand_arith_vars(expr, ctx);
    let mut parser = ArithParser::new(&expanded);
    let result = parser.parse_expr()?;
    // Ensure all input consumed
    parser.skip_whitespace();
    if parser.chars.peek().is_some() {
        return Err(ShellError::Exec(ExecError::ArithmeticError(format!(
            "unexpected trailing input: {}",
            &expanded[parser.chars.peek().map_or(expanded.len(), |&(i, _)| i)..]
        ))));
    }
    Ok(result)
}

fn expand_arith_vars(input: &str, ctx: &ExecContext) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            match chars.peek() {
                Some('{') => {
                    chars.next();
                    let mut var_name = String::new();
                    let mut closed = false;
                    for c in chars.by_ref() {
                        if c == '}' {
                            closed = true;
                            break;
                        }
                        var_name.push(c);
                    }
                    if closed {
                        if let Some(val) = ctx.variables.get(&var_name) {
                            result.push_str(val);
                        } else if let Ok(val) = std::env::var(&var_name) {
                            result.push_str(&val);
                        } else {
                            result.push('0');
                        }
                    } else {
                        result.push_str("${");
                        result.push_str(&var_name);
                    }
                }
                Some(c) if c.is_ascii_alphabetic() || *c == '_' => {
                    let mut var_name = String::new();
                    var_name.push(*c);
                    chars.next();
                    for c in chars.by_ref() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            var_name.push(c);
                        } else {
                            break;
                        }
                    }
                    if let Some(val) = ctx.variables.get(&var_name) {
                        result.push_str(val);
                    } else if let Ok(val) = std::env::var(&var_name) {
                        result.push_str(&val);
                    } else {
                        result.push('0');
                    }
                }
                Some('?') => {
                    chars.next();
                    result.push_str(&ctx.last_exit_code.to_string());
                }
                _ => {
                    result.push('0');
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Brace expansion
// ---------------------------------------------------------------------------

/// Expands brace patterns in a list of arguments.
///
/// Supports `{a,b,c}`, `{1..5}`, `{a..z}`, `{01..10}` (with padding),
/// `{a,b}{c,d}` (combinatorial), and nested braces `{a,{b,c}}`.
/// Unmatched braces are left as-is.
pub fn expand_braces(args: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for arg in args {
        if let Some(expanded) = expand_single_braces(arg) {
            result.extend(expanded);
        } else {
            result.push(arg.clone());
        }
    }
    result
}

fn expand_single_braces(input: &str) -> Option<Vec<String>> {
    // Find the outermost unescaped '{' that is part of a brace expansion
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut depth = 0;
    let mut brace_start = None;
    let mut i = 0;

    while i < len {
        match chars[i] {
            '\\' if i + 1 < len => {
                i += 2;
                continue;
            }
            '{' => {
                if depth == 0 {
                    brace_start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = brace_start {
                        // Found a complete brace pair at depth 0
                        let inner: String = chars[(start + 1)..i].iter().collect();
                        let before: String = chars[..start].iter().collect();
                        let after: String = chars[(i + 1)..].iter().collect();

                        // Try to expand the inner content
                        let expanded_items = expand_brace_inner(&inner)?;
                        // Recursively expand after (in case of multiple brace groups)
                        let after_expanded = expand_single_braces(&after).unwrap_or_else(|| {
                            vec![after]
                        });
                        let mut result = Vec::new();
                        for item in &expanded_items {
                            for after_item in &after_expanded {
                                result.push(format!("{before}{item}{after_item}"));
                            }
                        }
                        return Some(result);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn expand_brace_inner(inner: &str) -> Option<Vec<String>> {
    // Check if it's a range {start..end}
    if let Some(items) = try_range_expansion(inner) {
        return Some(items);
    }

    // Otherwise it's a comma-separated list, but we need to handle nested braces
    let items = split_brace_items(inner);
    if items.len() < 2 {
        return None; // Need at least 2 items for brace expansion
    }

    // Recursively expand each item
    let mut result = Vec::new();
    for item in items {
        match expand_single_braces(&item) {
            Some(expanded) => result.extend(expanded),
            None => result.push(item),
        }
    }
    Some(result)
}

fn split_brace_items(inner: &str) -> Vec<String> {
    let chars: Vec<char> = inner.chars().collect();
    let len = chars.len();
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for i in 0..len {
        match chars[i] {
            '\\' if i + 1 < len => {
                current.push(chars[i]);
                current.push(chars[i + 1]);
                // Skip next char in the loop
                // Actually we need to handle this differently
            }
            '{' => {
                depth += 1;
                current.push(chars[i]);
            }
            '}' => {
                depth -= 1;
                current.push(chars[i]);
            }
            ',' if depth == 0 => {
                items.push(current.clone());
                current.clear();
            }
            _ => {
                current.push(chars[i]);
            }
        }
    }
    items.push(current);
    items
}

fn try_range_expansion(inner: &str) -> Option<Vec<String>> {
    // Look for '..' that's not inside nested braces
    let mut depth = 0;
    let chars: Vec<char> = inner.chars().collect();
    let len = chars.len();

    for i in 0..len.saturating_sub(1) {
        match chars[i] {
            '{' => depth += 1,
            '}' => depth -= 1,
            '.' if depth == 0 && i + 1 < len && chars[i + 1] == '.' => {
                let start_str: String = chars[..i].iter().collect();
                let end_str: String = chars[(i + 2)..].iter().collect();
                return expand_range(&start_str, &end_str);
            }
            _ => {}
        }
    }
    None
}

fn expand_range(start: &str, end: &str) -> Option<Vec<String>> {
    // Try integer range
    if let (Ok(start_num), Ok(end_num)) = (start.parse::<i64>(), end.parse::<i64>()) {
        if start_num <= end_num {
            let width = start.len().max(end.len());
            let items: Vec<String> = (start_num..=end_num)
                .map(|n| {
                    let s = n.to_string();
                    if start.starts_with('0') && s.len() < width {
                        format!("{s:0>width$}", width = width)
                    } else {
                        s
                    }
                })
                .collect();
            return Some(items);
        }
        return Some(vec![]);
    }

    // Try lowercase letter range
    if start.len() == 1 && end.len() == 1 {
        let s = start.chars().next()?;
        let e = end.chars().next()?;
        if s.is_ascii_lowercase() && e.is_ascii_lowercase() && s <= e {
            let items: Vec<String> = (s..=e).map(|c| c.to_string()).collect();
            return Some(items);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Tilde expansion
// ---------------------------------------------------------------------------

/// Expands a leading `~` to the value of `$HOME`.
/// `~user` expansion is not supported (returns as-is).
pub fn expand_tilde(input: &str) -> String {
    if input == "~" {
        return env::var("HOME").unwrap_or_else(|_| input.to_string());
    }
    if let Some(rest) = input.strip_prefix("~/") {
        match env::var("HOME") {
            Ok(home) => format!("{home}/{rest}"),
            Err(_) => input.to_string(),
        }
    } else if input.starts_with('~') && !input.starts_with("~/") {
        // ~user is not supported, return as-is
        input.to_string()
    } else {
        input.to_string()
    }
}

// ---------------------------------------------------------------------------
// Parameter expansion & command substitution helpers
// ---------------------------------------------------------------------------

/// Finds the matching closing parenthesis in a char slice.
/// If `double` is true, looks for `))` (for arithmetic expansion).
/// Returns the index of the last `)` character.
fn find_matching_paren(chars: &[char], start: usize, double: bool) -> Option<usize> {
    let mut depth = 0;
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            '(' => {
                depth += 1;
                i += 1;
            }
            ')' if depth > 0 => {
                depth -= 1;
                i += 1;
            }
            ')' => {
                if double {
                    if i + 1 < chars.len() && chars[i + 1] == ')' {
                        return Some(i + 1);
                    } else {
                        return None;
                    }
                } else {
                    return Some(i);
                }
            }
            '\'' => {
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// Resolves a variable name to its value from the context or environment.
fn get_var_value(var_name: &str, ctx: &ExecContext) -> String {
    if let Some(val) = ctx.variables.get(var_name) {
        val.clone()
    } else if let Ok(val) = std::env::var(var_name) {
        val
    } else {
        String::new()
    }
}

/// Removes the shortest/longest suffix matching a glob pattern.
fn shell_pattern_remove_suffix(val: &str, pattern: &str, longest: bool) -> String {
    if pattern.is_empty() {
        return val.to_string();
    }
    let val_chars: Vec<char> = val.chars().collect();
    let pat_chars: Vec<char> = pattern.chars().collect();

    if longest {
        for start in (0..=val_chars.len()).rev() {
            let suffix = &val_chars[start..];
            if glob_match_inner(&pat_chars, suffix) {
                return val_chars[..start].iter().collect();
            }
        }
    } else {
        for start in 0..=val_chars.len() {
            let suffix = &val_chars[start..];
            if glob_match_inner(&pat_chars, suffix) {
                return val_chars[..start].iter().collect();
            }
        }
    }
    val.to_string()
}

/// Removes the shortest/longest prefix matching a glob pattern.
fn shell_pattern_remove_prefix(val: &str, pattern: &str, longest: bool) -> String {
    if pattern.is_empty() {
        return val.to_string();
    }
    let val_chars: Vec<char> = val.chars().collect();
    let pat_chars: Vec<char> = pattern.chars().collect();

    if longest {
        for end in (0..=val_chars.len()).rev() {
            let prefix = &val_chars[..end];
            if glob_match_inner(&pat_chars, prefix) {
                return val_chars[end..].iter().collect();
            }
        }
    } else {
        for end in 0..=val_chars.len() {
            let prefix = &val_chars[..end];
            if glob_match_inner(&pat_chars, prefix) {
                return val_chars[end..].iter().collect();
            }
        }
    }
    val.to_string()
}

/// Replaces the first literal occurrence of `pattern` with `replacement`.
fn shell_str_replace_first(val: &str, pattern: &str, replacement: &str) -> String {
    if let Some(pos) = val.find(pattern) {
        let mut result = String::with_capacity(val.len() - pattern.len() + replacement.len());
        result.push_str(&val[..pos]);
        result.push_str(replacement);
        result.push_str(&val[pos + pattern.len()..]);
        result
    } else {
        val.to_string()
    }
}

/// Replaces all literal occurrences of `pattern` with `replacement`.
fn shell_str_replace_all(val: &str, pattern: &str, replacement: &str) -> String {
    val.replace(pattern, replacement)
}

// ===========================================================================
// Levenshtein distance + command suggestion
// ===========================================================================

/// Computes the Levenshtein edit distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for (i, ca) in a.chars().enumerate() {
        for (j, cb) in b.chars().enumerate() {
            let cost = usize::from(ca != cb);
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[a_len][b_len]
}

/// Suggests a similar command from PATH + builtins using Levenshtein distance.
fn suggest_command(name: &str) -> Option<String> {
    let mut candidates: Vec<String> = Vec::new();

    // Add builtins
    for builtin in &[
        "echo", "printf", "pwd", "true", "false", "which", "type", "help", "version",
        "alias", "unalias", "export", "unset", "env", "pushd", "popd", "dirs", "eval",
        "source", "wait", "test", "jobs", "fg", "bg", "kill", "cd", "exit", "history",
        "clear",
    ] {
        candidates.push(builtin.to_string());
    }

    // Add commands from PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(meta) = path.metadata() {
                            if meta.permissions().mode() & 0o111 != 0 {
                                if let Some(file_name) = path.file_name() {
                                    if let Some(n) = file_name.to_str() {
                                        candidates.push(n.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Deduplicate
    candidates.sort();
    candidates.dedup();

    // Find best match with distance <= 3
    let mut best: Option<(String, usize)> = None;
    for candidate in &candidates {
        let dist = levenshtein(name, candidate);
        if dist > 0 && dist <= 3 {
            match &best {
                None => best = Some((candidate.clone(), dist)),
                Some((_, best_dist)) if dist < *best_dist => {
                    best = Some((candidate.clone(), dist));
                }
                _ => {}
            }
        }
    }

    best.map(|(name, _)| name)
}

/// Search PATH for a command, returning the full path if found.
fn which_path(name: &str) -> Option<std::path::PathBuf> {
    if name.contains('/') {
        let p = std::path::Path::new(name);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        return None;
    }
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        let candidate = std::path::Path::new(dir).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_lexer::Lexer;
    use aster_parser::Parser;

    fn run(input: &str) -> i32 {
        let tokens = Lexer::new(input).tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut ctx = ExecContext::default();
        match Executor::execute(&program, &mut ctx).unwrap() {
            ExecOutcome::Success(code) => code,
            ExecOutcome::Exit(code) => code,
            ExecOutcome::Break | ExecOutcome::Continue => 0,
        }
    }

    #[test]
    fn test_execute_echo() {
        let code = run("echo hello");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_execute_true() {
        assert_eq!(run("true"), 0);
    }

    #[test]
    fn test_execute_false() {
        assert_eq!(run("false"), 1);
    }

    #[test]
    fn test_execute_and_success() {
        assert_eq!(run("true && echo ok"), 0);
    }

    #[test]
    fn test_execute_and_failure() {
        assert_eq!(run("false && echo should_not_run"), 1);
    }

    #[test]
    fn test_execute_or_success() {
        assert_eq!(run("true || echo should_not_run"), 0);
    }

    #[test]
    fn test_execute_or_failure() {
        assert_eq!(run("false || echo recovered"), 0);
    }

    #[test]
    fn test_execute_sequence() {
        assert_eq!(run("true ; true ; true"), 0);
        assert_eq!(run("true ; false"), 1);
    }

    #[test]
    fn test_execute_exit() {
        let tokens = Lexer::new("exit 42").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut ctx = ExecContext::default();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Exit(42));
    }

    #[test]
    fn test_execute_pwd() {
        assert_eq!(run("pwd"), 0);
    }

    #[test]
    fn test_execute_version() {
        assert_eq!(run("version"), 0);
    }

    #[test]
    fn test_expand_variables_dollar_question() {
        let mut ctx = ExecContext {
            last_exit_code: 42,
            ..ExecContext::default()
        };
        let result = Executor::expand_variables("$?", &mut ctx);
        assert_eq!(result, "42");
    }

    #[test]
    fn test_expand_variables_unknown() {
        let mut ctx = ExecContext::default();
        let result = Executor::expand_variables("$UNKNOWN_VAR", &mut ctx);
        assert_eq!(result, "$UNKNOWN_VAR");
    }

    #[test]
    fn test_expand_variables_local() {
        let mut ctx = ExecContext::default();
        ctx.variables.insert("MY_VAR".into(), "hello".into());
        assert_eq!(Executor::expand_variables("$MY_VAR", &mut ctx), "hello");
        assert_eq!(Executor::expand_variables("${MY_VAR}", &mut ctx), "hello");
    }

    #[test]
    fn test_assignment() {
        let tokens = Lexer::new("FOO=bar").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut ctx = ExecContext::default();
        Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(ctx.variables.get("FOO").map(String::as_str), Some("bar"));
    }

    #[test]
    fn test_function_def_and_call() {
        let tokens = Lexer::new("function greet { echo hello ; } ; greet")
            .tokenize()
            .unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut ctx = ExecContext::default();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Success(0));
    }

    #[test]
    fn test_while_loop() {
        let code = run("COUNT=0 ; while true ; do COUNT=1 ; break ; done");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_for_loop() {
        assert_eq!(run("for i in 1 2 3 ; do echo $i ; done"), 0);
    }

    #[test]
    fn test_case_statement() {
        assert_eq!(run("case x in x) echo match ;; esac"), 0);
    }

    #[test]
    fn test_glob_match() {
        assert!(simple_glob_match("*", "anything"));
        assert!(simple_glob_match("f*", "foo"));
        assert!(simple_glob_match("*.txt", "hello.txt"));
        assert!(!simple_glob_match("*.txt", "hello.rs"));
        assert!(simple_glob_match("?", "a"));
        assert!(!simple_glob_match("?", "ab"));
        assert!(simple_glob_match("[abc]", "b"));
        assert!(!simple_glob_match("[abc]", "d"));
        assert!(simple_glob_match("[!abc]", "d"));
        assert!(!simple_glob_match("[!abc]", "a"));
    }

    // -----------------------------------------------------------------------
    // eval_arithmetic tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_arith_literals() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("42", &ctx).unwrap(), 42);
        assert_eq!(eval_arithmetic("0", &ctx).unwrap(), 0);
        assert_eq!(eval_arithmetic("0xff", &ctx).unwrap(), 255);
        assert_eq!(eval_arithmetic("0xFF", &ctx).unwrap(), 255);
        assert_eq!(eval_arithmetic("0o17", &ctx).unwrap(), 15);
        assert_eq!(eval_arithmetic("0b1010", &ctx).unwrap(), 10);
    }

    #[test]
    fn test_arith_basic_ops() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("2 + 3", &ctx).unwrap(), 5);
        assert_eq!(eval_arithmetic("10 - 4", &ctx).unwrap(), 6);
        assert_eq!(eval_arithmetic("3 * 7", &ctx).unwrap(), 21);
        assert_eq!(eval_arithmetic("20 / 4", &ctx).unwrap(), 5);
        assert_eq!(eval_arithmetic("17 % 5", &ctx).unwrap(), 2);
    }

    #[test]
    fn test_arith_div_zero() {
        let ctx = ExecContext::default();
        assert!(eval_arithmetic("1 / 0", &ctx).is_err());
        assert!(eval_arithmetic("5 % 0", &ctx).is_err());
    }

    #[test]
    fn test_arith_power() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("2 ** 10", &ctx).unwrap(), 1024);
        assert_eq!(eval_arithmetic("3 ** 0", &ctx).unwrap(), 1);
        assert_eq!(eval_arithmetic("2 ** 3 ** 2", &ctx).unwrap(), 512);
    }

    #[test]
    fn test_arith_parens() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("(2 + 3) * 4", &ctx).unwrap(), 20);
        assert_eq!(eval_arithmetic("((1 + 2) * (3 + 4))", &ctx).unwrap(), 21);
    }

    #[test]
    fn test_arith_unary() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("-5", &ctx).unwrap(), -5);
        assert_eq!(eval_arithmetic("+5", &ctx).unwrap(), 5);
        assert_eq!(eval_arithmetic("~0", &ctx).unwrap(), -1);
        assert_eq!(eval_arithmetic("!0", &ctx).unwrap(), 1);
        assert_eq!(eval_arithmetic("!1", &ctx).unwrap(), 0);
    }

    #[test]
    fn test_arith_comparison() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("1 == 1", &ctx).unwrap(), 1);
        assert_eq!(eval_arithmetic("1 != 2", &ctx).unwrap(), 1);
        assert_eq!(eval_arithmetic("3 < 5", &ctx).unwrap(), 1);
        assert_eq!(eval_arithmetic("5 > 3", &ctx).unwrap(), 1);
        assert_eq!(eval_arithmetic("5 <= 5", &ctx).unwrap(), 1);
        assert_eq!(eval_arithmetic("5 >= 6", &ctx).unwrap(), 0);
    }

    #[test]
    fn test_arith_bitwise() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("0xff & 0x0f", &ctx).unwrap(), 0x0f);
        assert_eq!(eval_arithmetic("0xf0 | 0x0f", &ctx).unwrap(), 0xff);
        assert_eq!(eval_arithmetic("0xff ^ 0x0f", &ctx).unwrap(), 0xf0);
        assert_eq!(eval_arithmetic("1 << 4", &ctx).unwrap(), 16);
        assert_eq!(eval_arithmetic("16 >> 2", &ctx).unwrap(), 4);
    }

    #[test]
    fn test_arith_logical() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("1 && 1", &ctx).unwrap(), 1);
        assert_eq!(eval_arithmetic("1 && 0", &ctx).unwrap(), 0);
        assert_eq!(eval_arithmetic("0 || 1", &ctx).unwrap(), 1);
        assert_eq!(eval_arithmetic("0 || 0", &ctx).unwrap(), 0);
    }

    #[test]
    fn test_arith_ternary() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("1 ? 10 : 20", &ctx).unwrap(), 10);
        assert_eq!(eval_arithmetic("0 ? 10 : 20", &ctx).unwrap(), 20);
    }

    #[test]
    fn test_arith_variables() {
        let mut ctx = ExecContext::default();
        ctx.variables.insert("X".into(), "10".into());
        ctx.variables.insert("Y".into(), "20".into());
        assert_eq!(eval_arithmetic("$X + $Y", &ctx).unwrap(), 30);
        assert_eq!(eval_arithmetic("${X} * 2", &ctx).unwrap(), 20);
    }

    #[test]
    fn test_arith_complex() {
        let ctx = ExecContext::default();
        assert_eq!(eval_arithmetic("(3 + 4) * (10 - 6) / 2", &ctx).unwrap(), 14);
        assert_eq!(eval_arithmetic("2 ** 3 + 1", &ctx).unwrap(), 9);
        assert_eq!(eval_arithmetic("-(2 + 3)", &ctx).unwrap(), -5);
    }

    // -----------------------------------------------------------------------
    // expand_braces tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_braces_basic() {
        let input = vec!["{a,b,c}".to_string()];
        let result = expand_braces(&input);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_braces_integers() {
        let input = vec!["{1..5}".to_string()];
        let result = expand_braces(&input);
        assert_eq!(result, vec!["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn test_braces_letters() {
        let input = vec!["{a..d}".to_string()];
        let result = expand_braces(&input);
        assert_eq!(result, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_braces_padded() {
        let input = vec!["{01..05}".to_string()];
        let result = expand_braces(&input);
        assert_eq!(result, vec!["01", "02", "03", "04", "05"]);
    }

    #[test]
    fn test_braces_combinatorial() {
        let input = vec!["{a,b}{c,d}".to_string()];
        let result = expand_braces(&input);
        assert_eq!(result, vec!["ac", "ad", "bc", "bd"]);
    }

    #[test]
    fn test_braces_nested() {
        let input = vec!["{a,{b,c}}".to_string()];
        let result = expand_braces(&input);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_braces_no_match() {
        let input = vec!["hello".to_string(), "no{braces".to_string()];
        let result = expand_braces(&input);
        assert_eq!(result, vec!["hello", "no{braces"]);
    }

    #[test]
    fn test_braces_empty() {
        let input = vec!["{}".to_string()];
        let result = expand_braces(&input);
        // Empty brace group: {} -> no items, nothing to expand
        assert_eq!(result, vec!["{}"]);
    }

    #[test]
    fn test_braces_with_prefix_suffix() {
        let input = vec!["file{1,2,3}.txt".to_string()];
        let result = expand_braces(&input);
        assert_eq!(result, vec!["file1.txt", "file2.txt", "file3.txt"]);
    }

    // -----------------------------------------------------------------------
    // expand_tilde tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_tilde_home() {
        let home = env::var("HOME").unwrap_or_default();
        assert_eq!(expand_tilde("~"), home);
    }

    #[test]
    fn test_tilde_slash() {
        let home = env::var("HOME").unwrap_or_default();
        assert_eq!(expand_tilde("~/Documents"), format!("{home}/Documents"));
    }

    #[test]
    fn test_tilde_no_tilde() {
        assert_eq!(expand_tilde("no_tilde"), "no_tilde");
        assert_eq!(expand_tilde("/absolute/path"), "/absolute/path");
    }

    #[test]
    fn test_tilde_user() {
        // ~user is not supported, should be returned as-is
        assert_eq!(expand_tilde("~root"), "~root");
    }

    // -----------------------------------------------------------------------
    // Job control builtin tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_jobs_empty() {
        let tokens = Lexer::new("jobs").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut ctx = ExecContext::default();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Success(0));
        assert!(ctx.jobs.is_empty());
    }

    #[test]
    fn test_jobs_list_after_add() {
        use aster_shell_core::jobs::{Job, ProcessInfo};

        let mut ctx = ExecContext::default();
        ctx.jobs.add(Job::new(
            1,
            vec![ProcessInfo::new(100, "sleep")],
            "sleep 10 &",
            true,
        ));
        assert_eq!(ctx.jobs.len(), 1);

        let tokens = Lexer::new("jobs").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Success(0));
    }

    #[test]
    fn test_fg_nonexistent_job() {
        let tokens = Lexer::new("fg %99").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut ctx = ExecContext::default();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Success(1));
    }

    #[test]
    fn test_fg_default_job() {
        use aster_shell_core::jobs::{Job, ProcessInfo};

        let mut ctx = ExecContext::default();
        ctx.jobs.add(Job::new(
            1,
            vec![ProcessInfo::new(200, "echo")],
            "echo hello",
            true,
        ));

        let tokens = Lexer::new("fg").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Success(0));
    }

    #[test]
    fn test_bg_nonexistent_job() {
        let tokens = Lexer::new("bg %99").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut ctx = ExecContext::default();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Success(1));
    }

    #[test]
    fn test_bg_existing_job() {
        use aster_shell_core::jobs::{Job, JobState, ProcessInfo};

        let mut ctx = ExecContext::default();
        let proc = ProcessInfo::new(300, "sleep");
        proc.set_state(JobState::Stopped);
        let job = Job::new(1, vec![proc], "sleep 5", false);
        job.set_state(JobState::Stopped);
        ctx.jobs.add(job);

        let tokens = Lexer::new("bg %1").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Success(0));
    }

    #[test]
    fn test_kill_no_args() {
        let tokens = Lexer::new("kill").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut ctx = ExecContext::default();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Success(1));
    }

    #[test]
    fn test_kill_invalid_pid() {
        let tokens = Lexer::new("kill not_a_number").tokenize().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        let mut ctx = ExecContext::default();
        let result = Executor::execute(&program, &mut ctx).unwrap();
        assert_eq!(result, ExecOutcome::Success(1));
    }

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn test_levenshtein_one_edit() {
        assert_eq!(levenshtein("abc", "abx"), 1);
        assert_eq!(levenshtein("abc", "axc"), 1);
        assert_eq!(levenshtein("abc", "xbc"), 1);
    }

    #[test]
    fn test_levenshtein_two_edits() {
        assert_eq!(levenshtein("abc", "axy"), 2);
    }

    #[test]
    fn test_suggest_command_similar() {
        // "ls" should suggest something close
        if let Some(suggestion) = suggest_command("sl") {
            assert!(!suggestion.is_empty());
        }
    }

    #[test]
    fn test_suggest_command_no_match() {
        // Very long gibberish should return None
        assert!(suggest_command("xyzzyplugh12345").is_none());
    }

    #[test]
    fn test_double_bracket_string_eq() {
        let mut ctx = ExecContext::default();
        assert!(Executor::eval_double_bracket(&["hello".into(), "==".into(), "hello".into()], &mut ctx).unwrap());
        assert!(!Executor::eval_double_bracket(&["hello".into(), "==".into(), "world".into()], &mut ctx).unwrap());
    }

    #[test]
    fn test_double_bracket_string_ne() {
        let mut ctx = ExecContext::default();
        assert!(Executor::eval_double_bracket(&["hello".into(), "!=".into(), "world".into()], &mut ctx).unwrap());
        assert!(!Executor::eval_double_bracket(&["hello".into(), "!=".into(), "hello".into()], &mut ctx).unwrap());
    }

    #[test]
    fn test_double_bracket_glob() {
        let mut ctx = ExecContext::default();
        assert!(Executor::eval_double_bracket(&["hello.txt".into(), "==".into(), "*.txt".into()], &mut ctx).unwrap());
        assert!(!Executor::eval_double_bracket(&["hello.rs".into(), "==".into(), "*.txt".into()], &mut ctx).unwrap());
    }

    #[test]
    fn test_double_bracket_negation() {
        let mut ctx = ExecContext::default();
        assert!(!Executor::eval_double_bracket(&["!".into(), "hello".into(), "==".into(), "hello".into()], &mut ctx).unwrap());
    }

    #[test]
    fn test_double_bracket_numeric() {
        let mut ctx = ExecContext::default();
        assert!(Executor::eval_double_bracket(&["5".into(), "-gt".into(), "3".into()], &mut ctx).unwrap());
        assert!(Executor::eval_double_bracket(&["3".into(), "-lt".into(), "5".into()], &mut ctx).unwrap());
        assert!(!Executor::eval_double_bracket(&["5".into(), "-lt".into(), "3".into()], &mut ctx).unwrap());
    }

    #[test]
    fn test_double_bracket_file_tests() {
        let mut ctx = ExecContext::default();
        let manifest = std::path::PathBuf::from(std::env!("CARGO_MANIFEST_DIR"));
        let cargo_toml = manifest.join("../Cargo.toml").canonicalize().unwrap();
        let exec_src = manifest.join("src").canonicalize().unwrap();
        assert!(Executor::eval_double_bracket(&["-f".into(), cargo_toml.to_string_lossy().into_owned()], &mut ctx).unwrap());
        assert!(Executor::eval_double_bracket(&["-d".into(), exec_src.to_string_lossy().into_owned()], &mut ctx).unwrap());
        assert!(Executor::eval_double_bracket(&["-e".into(), cargo_toml.to_string_lossy().into_owned()], &mut ctx).unwrap());
        assert!(!Executor::eval_double_bracket(&["-e".into(), "/nonexistent_path_xyz".into()], &mut ctx).unwrap());
    }

    #[test]
    fn test_double_bracket_and_or() {
        let mut ctx = ExecContext::default();
        assert!(Executor::eval_double_bracket(
            &["hello".into(), "==".into(), "hello".into(), "&&".into(), "world".into(), "==".into(), "world".into()],
            &mut ctx
        ).unwrap());
        assert!(!Executor::eval_double_bracket(
            &["hello".into(), "==".into(), "hello".into(), "&&".into(), "world".into(), "==".into(), "foo".into()],
            &mut ctx
        ).unwrap());
        assert!(Executor::eval_double_bracket(
            &["hello".into(), "==".into(), "foo".into(), "||".into(), "world".into(), "==".into(), "world".into()],
            &mut ctx
        ).unwrap());
    }
}
