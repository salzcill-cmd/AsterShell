//! Built-in shell commands.
//!
//! Provides implementations for commands that must run inside the shell process
//! rather than as external binaries.

use std::path::PathBuf;
use std::sync::OnceLock;

use aster_shell_core::{AliasMap, ExecError, ShellEnvironment};

/// Checks whether `name` is a built-in command.
#[must_use]
pub fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "echo"
            | "printf"
            | "pwd"
            | "true"
            | "false"
            | "which"
            | "type"
            | "help"
            | "version"
            | "alias"
            | "unalias"
            | "env"
            | "export"
            | "unset"
            | "pushd"
            | "popd"
            | "dirs"
            | "wait"
            | "eval"
            | "source"
            | "test"
            | "["
            | "string"
    )
}

/// Executes a built-in command and returns its exit code.
///
/// Returns `Ok(Some(code))` when the command was handled, or `Ok(None)` if
/// the name is not a known builtin.
///
/// # Errors
///
/// Returns [`ExecError`] if the builtin fails.
pub fn execute(
    name: &str,
    args: &[String],
    env: &mut ShellEnvironment,
    aliases: &mut AliasMap,
) -> Result<Option<i32>, ExecError> {
    match name {
        "echo" => echo(args).map(Some),
        "printf" => printf(args).map(Some),
        "pwd" => pwd().map(Some),
        "true" => Ok(Some(0)),
        "false" => Ok(Some(1)),
        "which" => which(args).map(Some),
        "type" => type_cmd(args, aliases).map(Some),
        "help" => help().map(Some),
        "version" => version().map(Some),
        "alias" => alias(args, aliases).map(Some),
        "unalias" => unalias(args, aliases).map(Some),
        "env" => env_cmd(env).map(Some),
        "export" => export(args, env).map(Some),
        "unset" => unset(args, env).map(Some),
        "pushd" => pushd(args).map(Some),
        "popd" => popd().map(Some),
        "dirs" => dirs_cmd().map(Some),
        "wait" => wait_cmd().map(Some),
        "eval" => Ok(None),   // eval is handled by executor
        "source" => Ok(None), // source is handled by executor
        "test" | "[" => test_cmd(args).map(Some),
        "string" => string_cmd(args).map(Some),
        _ => Ok(None),
    }
}

/// Returns a list of all builtins with a short description.
#[must_use]
pub const fn builtin_list() -> &'static [(&'static str, &'static str)] {
    &[
        ("echo", "Display text"),
        ("printf", "Formatted output"),
        ("pwd", "Print working directory"),
        ("true", "Return success (0)"),
        ("false", "Return failure (1)"),
        ("which", "Locate a command in PATH"),
        ("type", "Describe how a command name is interpreted"),
        ("help", "Display available builtins"),
        ("version", "Print the shell version"),
        ("alias", "Define or display an alias"),
        ("unalias", "Remove an alias"),
        ("env", "Display environment variables"),
        ("export", "Export variables to environment"),
        ("unset", "Remove environment variables"),
        ("pushd", "Push directory onto stack"),
        ("popd", "Pop directory from stack"),
        ("dirs", "Display directory stack"),
        ("wait", "Wait for background processes"),
        ("eval", "Evaluate arguments as a command"),
        ("source", "Execute commands from a file"),
        ("test", "Evaluate conditional expression"),
        ("string", "String manipulation (length, sub, match, etc.)"),
        ("compgen", "Generate completion candidates"),
        ("shift", "Shift positional parameters"),
        ("mapfile", "Read lines into array variable"),
        ("dirname", "Strip last component from file name"),
        ("basename", "Strip directory from file name"),
        ("command", "Run or describe a command"),
    ]
}

fn echo(args: &[String]) -> Result<i32, ExecError> {
    let mut print_newline = true;
    let mut print_escape = false;
    let mut args_iter = args.iter().peekable();

    while let Some(first) = args_iter.peek() {
        if *first == "-n" {
            print_newline = false;
            args_iter.next();
        } else if *first == "-e" {
            print_escape = true;
            args_iter.next();
        } else {
            break;
        }
    }

    let output: Vec<&str> = args_iter.map(std::string::String::as_str).collect();
    let mut text = output.join(" ");

    if print_escape {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.peek() {
                    Some('n') => { chars.next(); out.push('\n'); }
                    Some('t') => { chars.next(); out.push('\t'); }
                    Some('\\') => { chars.next(); out.push('\\'); }
                    Some('r') => { chars.next(); out.push('\r'); }
                    Some('0') => { chars.next(); break; }
                    _ => out.push(c),
                }
            } else {
                out.push(c);
            }
        }
        text = out;
    }

    print!("{text}");
    if print_newline {
        println!();
    }
    Ok(0)
}

fn printf(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        return Ok(0);
    }
    let format = &args[0];
    let mut result = String::new();
    let mut arg_idx = 1;
    let mut chars = format.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Interpret backslash escapes in the format string
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some('0') => result.push('\0'),
                Some('a') => result.push('\x07'),
                Some('b') => result.push('\x08'),
                Some('f') => result.push('\x0C'),
                Some('v') => result.push('\x0B'),
                Some('x') => {
                    // \xHH hex escape
                    let mut hex = String::new();
                    for _ in 0..2 {
                        if let Some(h) = chars.next() {
                            if h.is_ascii_hexdigit() {
                                hex.push(h);
                            } else {
                                break;
                            }
                        }
                    }
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                    }
                }
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else if c == '%' {
            match chars.peek() {
                Some('s') => {
                    chars.next();
                    let arg = args.get(arg_idx).map(String::as_str).unwrap_or("");
                    result.push_str(arg);
                    arg_idx += 1;
                }
                Some('d') | Some('i') => {
                    chars.next();
                    let arg = args.get(arg_idx).map(String::as_str).unwrap_or("0");
                    result.push_str(arg);
                    arg_idx += 1;
                }
                Some('f') => {
                    chars.next();
                    let arg = args.get(arg_idx).map(String::as_str).unwrap_or("0");
                    result.push_str(arg);
                    arg_idx += 1;
                }
                Some('c') => {
                    chars.next();
                    if let Some(arg) = args.get(arg_idx) {
                        if let Some(ch) = arg.chars().next() {
                            result.push(ch);
                        }
                    }
                    arg_idx += 1;
                }
                Some('b') => {
                    chars.next();
                    let arg = args.get(arg_idx).map(String::as_str).unwrap_or("");
                    let expanded = arg
                        .replace("\\n", "\n")
                        .replace("\\t", "\t")
                        .replace("\\\\", "\\");
                    result.push_str(&expanded);
                    arg_idx += 1;
                }
                Some('%') => {
                    chars.next();
                    result.push('%');
                }
                Some('n') => {
                    chars.next();
                    result.push('\n');
                }
                _ => {
                    result.push(c);
                }
            }
        } else {
            result.push(c);
        }
    }

    print!("{result}");
    Ok(0)
}

fn pwd() -> Result<i32, ExecError> {
    match std::env::current_dir() {
        Ok(path) => {
            println!("{}", path.display());
            Ok(0)
        }
        Err(e) => Err(ExecError::DirError(e.to_string())),
    }
}

fn which(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        return Err(ExecError::CommandNotFound("which: missing argument".into()));
    }
    let mut exit_code = 0;
    for arg in args {
        if is_builtin(arg) {
            println!("{arg}: shell built-in command");
        } else if let Some(path) = aster_utils::find_executable(arg) {
            println!("{}", path.display());
        } else {
            eprintln!("{arg} not found");
            exit_code = 1;
        }
    }
    Ok(exit_code)
}

fn type_cmd(args: &[String], aliases: &AliasMap) -> Result<i32, ExecError> {
    if args.is_empty() {
        return Err(ExecError::CommandNotFound("type: missing argument".into()));
    }
    let mut exit_code = 0;
    for arg in args {
        if is_builtin(arg) {
            println!("{arg} is a shell builtin");
        } else if let Some(expansion) = aliases.get(arg) {
            println!("{arg} is aliased to `{expansion}'");
        } else if let Some(path) = aster_utils::find_executable(arg) {
            println!("{arg} is {}", path.display());
        } else {
            eprintln!("type: {arg}: not found");
            exit_code = 1;
        }
    }
    Ok(exit_code)
}

fn help() -> Result<i32, ExecError> {
    println!("AsterShell built-in commands:");
    for (name, desc) in builtin_list() {
        println!("  {name:<12} {desc}");
    }
    Ok(0)
}

fn version() -> Result<i32, ExecError> {
    println!(
        "{} {}",
        aster_shell_core::SHELL_NAME,
        aster_shell_core::VERSION
    );
    Ok(0)
}

fn alias(args: &[String], aliases: &mut AliasMap) -> Result<i32, ExecError> {
    if args.is_empty() {
        let mut entries: Vec<_> = aliases.entries();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (name, value) in entries {
            println!("alias {name}='{value}'");
        }
        return Ok(0);
    }
    for arg in args {
        if let Some((name, value)) = arg.split_once('=') {
            aliases.insert(name, value);
        } else if let Some(value) = aliases.get(arg) {
            println!("alias {arg}='{value}'");
        } else {
            eprintln!("alias: {arg}: not found");
            return Ok(1);
        }
    }
    Ok(0)
}

fn unalias(args: &[String], aliases: &mut AliasMap) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("unalias: missing operand");
        return Ok(1);
    }
    let mut exit_code = 0;
    for arg in args {
        if !aliases.remove(arg) {
            eprintln!("unalias: {arg}: not found");
            exit_code = 1;
        }
    }
    Ok(exit_code)
}

fn env_cmd(env: &ShellEnvironment) -> Result<i32, ExecError> {
    let mut vars: Vec<_> = env.exported_vars();
    vars.sort_by(|a, b| a.0.cmp(b.0));
    for (name, value) in vars {
        println!("{name}={value}");
    }
    Ok(0)
}

fn export(args: &[String], env: &mut ShellEnvironment) -> Result<i32, ExecError> {
    if args.is_empty() {
        return env_cmd(env);
    }
    for arg in args {
        if let Some((name, value)) = arg.split_once('=') {
            env.export(name, value);
        } else if let Some(value) = env.get(arg).map(str::to_string) {
            env.export(arg, &value);
        }
    }
    Ok(0)
}

fn unset(args: &[String], env: &mut ShellEnvironment) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("unset: missing operand");
        return Ok(1);
    }
    for arg in args {
        env.unset(arg);
    }
    Ok(0)
}

fn dir_stack() -> &'static std::sync::Mutex<Vec<PathBuf>> {
    static STACK: OnceLock<std::sync::Mutex<Vec<PathBuf>>> = OnceLock::new();
    STACK.get_or_init(|| {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        std::sync::Mutex::new(vec![cwd])
    })
}

fn pushd(args: &[String]) -> Result<i32, ExecError> {
    let target = if args.is_empty() {
        dirs::home_dir().ok_or_else(|| ExecError::CdError("HOME not set".into()))?
    } else {
        PathBuf::from(&args[0])
    };

    let current = std::env::current_dir().map_err(|e| ExecError::DirError(e.to_string()))?;

    std::env::set_current_dir(&target)
        .map_err(|e| ExecError::CdError(format!("{}: {}", target.display(), e)))?;

    let mut stack = dir_stack()
        .lock()
        .map_err(|e| ExecError::DirError(format!("lock error: {e}")))?;
    stack.push(current);

    println!("{}", target.display());
    Ok(0)
}

fn popd() -> Result<i32, ExecError> {
    let mut stack = dir_stack()
        .lock()
        .map_err(|e| ExecError::DirError(format!("lock error: {e}")))?;

    if stack.len() <= 1 {
        return Err(ExecError::DirError("popd: directory stack empty".into()));
    }

    stack.pop();
    let target = stack.last().cloned().unwrap_or_else(|| PathBuf::from("."));

    drop(stack);

    std::env::set_current_dir(&target)
        .map_err(|e| ExecError::CdError(format!("{}: {}", target.display(), e)))?;

    println!("{}", target.display());
    Ok(0)
}

fn dirs_cmd() -> Result<i32, ExecError> {
    let stack = dir_stack()
        .lock()
        .map_err(|e| ExecError::DirError(format!("lock error: {e}")))?;

    let cwd = std::env::current_dir().map_err(|e| ExecError::DirError(e.to_string()))?;

    print!("{}", cwd.display());
    for entry in stack.iter().rev() {
        if entry != &cwd {
            print!(" {entry}", entry = entry.display());
        }
    }
    println!();

    Ok(0)
}

fn wait_cmd() -> Result<i32, ExecError> {
    Ok(0)
}

fn test_cmd(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        return Ok(1);
    }

    // Handle unary file tests: -f, -d, -e, -r, -w, -x, -z, -n, -s, -L
    match args.first().map(String::as_str) {
        Some("-f") => {
            if args.len() < 2 { return Ok(1); }
            let path = std::path::Path::new(&args[1]);
            Ok(i32::from(!path.is_file()))
        }
        Some("-d") => {
            if args.len() < 2 { return Ok(1); }
            let path = std::path::Path::new(&args[1]);
            Ok(i32::from(!path.is_dir()))
        }
        Some("-e") => {
            if args.len() < 2 { return Ok(1); }
            let path = std::path::Path::new(&args[1]);
            Ok(i32::from(!path.exists()))
        }
        Some("-r") => {
            if args.len() < 2 { return Ok(1); }
            let path = std::path::Path::new(&args[1]);
            use std::os::unix::fs::PermissionsExt;
            let meta = path.metadata().map(|m| m.permissions());
            Ok(i32::from(!matches!(meta, Ok(p) if p.mode() & 0o444 != 0)))
        }
        Some("-w") => {
            if args.len() < 2 { return Ok(1); }
            let path = std::path::Path::new(&args[1]);
            use std::os::unix::fs::PermissionsExt;
            let meta = path.metadata().map(|m| m.permissions());
            Ok(i32::from(!matches!(meta, Ok(p) if p.mode() & 0o222 != 0)))
        }
        Some("-x") => {
            if args.len() < 2 { return Ok(1); }
            let path = std::path::Path::new(&args[1]);
            use std::os::unix::fs::PermissionsExt;
            let meta = path.metadata().map(|m| m.permissions());
            Ok(i32::from(!matches!(meta, Ok(p) if p.mode() & 0o111 != 0)))
        }
        Some("-s") => {
            if args.len() < 2 { return Ok(1); }
            let path = std::path::Path::new(&args[1]);
            let len = path.metadata().map(|m| m.len()).unwrap_or(0);
            Ok(i32::from(len == 0))
        }
        Some("-L") => {
            if args.len() < 2 { return Ok(1); }
            let path = std::path::Path::new(&args[1]);
            Ok(i32::from(!path.is_symlink()))
        }
        Some("-z") => {
            if args.len() < 2 { return Ok(1); }
            Ok(i32::from(!args[1].is_empty()))
        }
        Some("-n") => {
            if args.len() < 2 { return Ok(1); }
            Ok(i32::from(args[1].is_empty()))
        }
        Some("=") | Some("==") => {
            if args.len() < 3 { return Ok(1); }
            Ok(i32::from(args[1] != args[2]))
        }
        Some("!=") => {
            if args.len() < 3 { return Ok(1); }
            Ok(i32::from(args[1] == args[2]))
        }
        Some("-eq") => {
            if args.len() < 3 { return Ok(1); }
            let a = args[1].parse::<i64>().unwrap_or(0);
            let b = args[2].parse::<i64>().unwrap_or(0);
            Ok(i32::from(a != b))
        }
        Some("-ne") => {
            if args.len() < 3 { return Ok(1); }
            let a = args[1].parse::<i64>().unwrap_or(0);
            let b = args[2].parse::<i64>().unwrap_or(0);
            Ok(i32::from(a == b))
        }
        Some("-lt") => {
            if args.len() < 3 { return Ok(1); }
            let a = args[1].parse::<i64>().unwrap_or(0);
            let b = args[2].parse::<i64>().unwrap_or(0);
            Ok(i32::from(a >= b))
        }
        Some("-gt") => {
            if args.len() < 3 { return Ok(1); }
            let a = args[1].parse::<i64>().unwrap_or(0);
            let b = args[2].parse::<i64>().unwrap_or(0);
            Ok(i32::from(a <= b))
        }
        Some("-le") => {
            if args.len() < 3 { return Ok(1); }
            let a = args[1].parse::<i64>().unwrap_or(0);
            let b = args[2].parse::<i64>().unwrap_or(0);
            Ok(i32::from(a > b))
        }
        Some("-ge") => {
            if args.len() < 3 { return Ok(1); }
            let a = args[1].parse::<i64>().unwrap_or(0);
            let b = args[2].parse::<i64>().unwrap_or(0);
            Ok(i32::from(a < b))
        }
        Some(s) if !s.starts_with('-') => {
            let path = std::path::Path::new(s);
            Ok(i32::from(!path.exists()))
        }
        _ => Ok(1),
    }
}

/// Fish-style `string` builtin for string manipulation.
///
/// Usage: `string <SUBCOMMAND> [OPTIONS] [ARG ...]`
///
/// Subcommands: `length`, `sub`, `match`, `replace`, `trim`, `split`,
/// `join`, `repeat`, `escape`, `lower`, `upper`, `capital`.
fn string_cmd(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("string: missing subcommand");
        return Ok(1);
    }

    match args[0].as_str() {
        "length" => string_length(&args[1..]),
        "sub" => string_sub(&args[1..]),
        "match" => string_match(&args[1..]),
        "replace" => string_replace(&args[1..]),
        "trim" => string_trim(&args[1..]),
        "split" => string_split(&args[1..]),
        "join" => string_join(&args[1..]),
        "repeat" => string_repeat(&args[1..]),
        "escape" => string_escape(&args[1..]),
        "lower" => string_lower(&args[1..]),
        "upper" => string_upper(&args[1..]),
        "capital" => string_capital(&args[1..]),
        _ => {
            eprintln!("string: unknown subcommand '{}'", args[0]);
            Ok(1)
        }
    }
}

fn string_length(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("string length: missing argument");
        return Ok(1);
    }
    for arg in args {
        println!("{}", arg.chars().count());
    }
    Ok(0)
}

fn string_sub(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("string sub: missing argument");
        return Ok(1);
    }
    let range_str = &args[0];
    let text_args = &args[1..];
    if text_args.is_empty() {
        eprintln!("string sub: missing string argument");
        return Ok(1);
    }
    for arg in text_args {
        let char_count = arg.chars().count();
        let (start, end) = if let Some(colon_pos) = range_str.find(':') {
            let start: usize = range_str[..colon_pos].parse().unwrap_or(0);
            let end: usize = range_str[colon_pos + 1..].parse().unwrap_or(char_count);
            (start, end)
        } else {
            let start: usize = range_str.parse().unwrap_or(0);
            (start, char_count)
        };
        let result: String = arg.chars().skip(start).take(end.saturating_sub(start)).collect();
        println!("{result}");
    }
    Ok(0)
}

fn string_match(args: &[String]) -> Result<i32, ExecError> {
    if args.len() < 2 {
        eprintln!("string match: needs PATTERN and STRING");
        return Ok(1);
    }
    let pattern = &args[0];
    let mut exit_code = 0;

    // Check if pattern starts with -r for regex
    if pattern == "-r" {
        if args.len() < 3 {
            eprintln!("string match: -r needs PATTERN and STRING");
            return Ok(1);
        }
        let re_pattern = &args[1];
        for arg in &args[2..] {
            if let Ok(re) = regex::Regex::new(re_pattern) {
                if let Some(caps) = re.captures(arg) {
                    for i in 1..caps.len() {
                        if let Some(m) = caps.get(i) {
                            println!("{}", m.as_str());
                        }
                    }
                } else {
                    exit_code = 1;
                }
            } else {
                eprintln!("string match: invalid regex '{re_pattern}'");
                return Ok(1);
            }
        }
    } else {
        for arg in &args[1..] {
            if glob_match(pattern, arg) {
                println!("{arg}");
            } else {
                exit_code = 1;
            }
        }
    }
    Ok(exit_code)
}

fn string_replace(args: &[String]) -> Result<i32, ExecError> {
    if args.len() < 3 {
        eprintln!("string replace: needs PATTERN REPLACEMENT STRING");
        return Ok(1);
    }
    let pattern = &args[0];
    let replacement = &args[1];
    for arg in &args[2..] {
        if let Some(re) = regex::Regex::new(pattern).ok() {
            let result = re.replace(arg, replacement.as_str());
            println!("{result}");
        } else {
            let result = arg.replacen(pattern.as_str(), replacement.as_str(), 1);
            println!("{result}");
        }
    }
    Ok(0)
}

fn string_trim(args: &[String]) -> Result<i32, ExecError> {
    let mut chars_to_trim: &str = " \t\n\r";
    let mut start = 0;

    if !args.is_empty() && args[0] == "-l" {
        chars_to_trim = if args.len() > 1 { &args[1] } else { " \t\n\r" };
        start = 1;
    } else if !args.is_empty() && args[0] == "-r" {
        chars_to_trim = if args.len() > 1 { &args[1] } else { " \t\n\r" };
        start = 1;
    } else if !args.is_empty() && args[0] == "-c" {
        chars_to_trim = if args.len() > 1 { &args[1] } else { " \t\n\r" };
        start = 1;
    }

    let text_args = &args[start..];
    if text_args.is_empty() {
        eprintln!("string trim: missing argument");
        return Ok(1);
    }

    for arg in text_args {
        println!("{}", arg.trim_matches(|c| chars_to_trim.contains(c)));
    }
    Ok(0)
}

fn string_split(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("string split: missing separator");
        return Ok(1);
    }
    let separator = &args[0];
    let text_args = &args[1..];
    if text_args.is_empty() {
        eprintln!("string split: missing string argument");
        return Ok(1);
    }
    for arg in text_args {
        for part in arg.split(separator.as_str()) {
            println!("{part}");
        }
    }
    Ok(0)
}

fn string_join(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("string join: missing separator");
        return Ok(1);
    }
    let separator = &args[0];
    let text_args = &args[1..];
    if text_args.is_empty() {
        eprintln!("string join: missing string argument");
        return Ok(1);
    }
    let result = text_args.join(separator);
    println!("{result}");
    Ok(0)
}

fn string_repeat(args: &[String]) -> Result<i32, ExecError> {
    if args.len() < 2 {
        eprintln!("string repeat: needs COUNT and STRING");
        return Ok(1);
    }
    let count: usize = args[0].parse().unwrap_or(0);
    for arg in &args[1..] {
        let result: String = arg.repeat(count);
        print!("{result}");
    }
    if args.len() > 2 {
        println!();
    }
    Ok(0)
}

fn string_escape(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("string escape: missing argument");
        return Ok(1);
    }
    for arg in args {
        let escaped = arg
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\'', "\\'")
            .replace('\n', "\\n")
            .replace('\t', "\\t");
        println!("{escaped}");
    }
    Ok(0)
}

fn string_lower(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("string lower: missing argument");
        return Ok(1);
    }
    for arg in args {
        println!("{}", arg.to_lowercase());
    }
    Ok(0)
}

fn string_upper(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("string upper: missing argument");
        return Ok(1);
    }
    for arg in args {
        println!("{}", arg.to_uppercase());
    }
    Ok(0)
}

fn string_capital(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        eprintln!("string capital: missing argument");
        return Ok(1);
    }
    for arg in args {
        let mut chars = arg.chars();
        if let Some(first) = chars.next() {
            let rest: String = chars.collect();
            println!("{}{}", first.to_uppercase(), rest.to_lowercase());
        } else {
            println!();
        }
    }
    Ok(0)
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_match_inner(&p, &t)
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = usize::MAX;
    let mut star_ti = 0;

    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == text[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < pattern.len() && pattern[pi] == '*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }

    pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin() {
        assert!(is_builtin("echo"));
        assert!(is_builtin("pwd"));
        assert!(is_builtin("true"));
        assert!(is_builtin("false"));
        assert!(is_builtin("which"));
        assert!(is_builtin("type"));
        assert!(is_builtin("help"));
        assert!(is_builtin("version"));
        assert!(is_builtin("alias"));
        assert!(is_builtin("unalias"));
        assert!(is_builtin("env"));
        assert!(is_builtin("export"));
        assert!(is_builtin("unset"));
        assert!(is_builtin("wait"));
        assert!(is_builtin("eval"));
        assert!(is_builtin("source"));
        assert!(is_builtin("test"));
        assert!(is_builtin("["));
        assert!(!is_builtin("ls"));
        assert!(!is_builtin("exit"));
        assert!(!is_builtin("cd"));
    }

    #[test]
    fn test_execute_true() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute("true", &[], &mut env, &mut aliases).unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_execute_false() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute("false", &[], &mut env, &mut aliases).unwrap();
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_execute_version() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute("version", &[], &mut env, &mut aliases).unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_execute_help() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute("help", &[], &mut env, &mut aliases).unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_execute_unknown() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute("notabuiltin", &[], &mut env, &mut aliases).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_builtin_list() {
        let list = builtin_list();
        assert!(!list.is_empty());
        assert!(list.iter().any(|(n, _)| *n == "echo"));
        assert!(list.iter().any(|(n, _)| *n == "wait"));
        assert!(list.iter().any(|(n, _)| *n == "eval"));
        assert!(list.iter().any(|(n, _)| *n == "test"));
    }
    #[test]
    fn test_test_cmd() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        // test with no args should return 1
        let result = execute("test", &[], &mut env, &mut aliases).unwrap();
        assert_eq!(result, Some(1));
        // test -z "" should return 0
        let result =
            execute("test", &["-z".into(), "".into()], &mut env, &mut aliases).unwrap();
        assert_eq!(result, Some(0));
        // test -n "hello" should return 0
        let result = execute(
            "test",
            &["-n".into(), "hello".into()],
            &mut env,
            &mut aliases,
        )
        .unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_is_builtin_string() {
        assert!(is_builtin("string"));
    }

    #[test]
    fn test_string_length() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute(
            "string",
            &["length".into(), "hello".into()],
            &mut env,
            &mut aliases,
        )
        .unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_string_lower() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute(
            "string",
            &["lower".into(), "HELLO".into()],
            &mut env,
            &mut aliases,
        )
        .unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_string_upper() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute(
            "string",
            &["upper".into(), "hello".into()],
            &mut env,
            &mut aliases,
        )
        .unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_string_join() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute(
            "string",
            &[
                "join".into(),
                ",".into(),
                "a".into(),
                "b".into(),
                "c".into(),
            ],
            &mut env,
            &mut aliases,
        )
        .unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_string_split() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute(
            "string",
            &["split".into(), ",".into(), "a,b,c".into()],
            &mut env,
            &mut aliases,
        )
        .unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_string_unknown_subcommand() {
        let mut env = ShellEnvironment::from_process();
        let mut aliases = AliasMap::new();
        let result = execute(
            "string",
            &["bogus".into()],
            &mut env,
            &mut aliases,
        )
        .unwrap();
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*", "hello"));
        assert!(glob_match("h*llo", "hello"));
        assert!(glob_match("h?llo", "hello"));
        assert!(!glob_match("h?llo", "hllo"));
        assert!(glob_match("*.rs", "main.rs"));
        assert!(!glob_match("*.rs", "main.c"));
    }
}
