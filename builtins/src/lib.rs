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
        text = text
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\\", "\\");
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
    let mut result = format.clone();
    for (i, arg) in args.iter().skip(1).enumerate() {
        let placeholder = format!("{{{i}}}");
        result = result.replace(&placeholder, arg);
    }
    println!("{result}");
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
    // Simplified wait - just return 0
    Ok(0)
}

fn test_cmd(args: &[String]) -> Result<i32, ExecError> {
    if args.is_empty() {
        return Ok(1);
    }

    // Simple test: -f, -d, -e file tests
    match args.first().map(String::as_str) {
        Some("-f") => {
            if args.len() < 2 {
                return Ok(1);
            }
            let path = std::path::Path::new(&args[1]);
            Ok(i32::from(!path.is_file()))
        }
        Some("-d") => {
            if args.len() < 2 {
                return Ok(1);
            }
            let path = std::path::Path::new(&args[1]);
            Ok(i32::from(!path.is_dir()))
        }
        Some("-e") => {
            if args.len() < 2 {
                return Ok(1);
            }
            let path = std::path::Path::new(&args[1]);
            Ok(i32::from(!path.exists()))
        }
        Some("-z") => {
            if args.len() < 2 {
                return Ok(1);
            }
            Ok(i32::from(!args[1].is_empty()))
        }
        Some("-n") => {
            if args.len() < 2 {
                return Ok(1);
            }
            Ok(i32::from(args[1].is_empty()))
        }
        Some("=") | Some("==") => {
            if args.len() < 3 {
                return Ok(1);
            }
            Ok(i32::from(args[1] != args[2]))
        }
        Some("!=") => {
            if args.len() < 3 {
                return Ok(1);
            }
            Ok(i32::from(args[1] == args[2]))
        }
        Some(s) if !s.starts_with('-') => {
            // test FILE: returns 0 if file exists
            let path = std::path::Path::new(s);
            Ok(i32::from(!path.exists()))
        }
        _ => Ok(1),
    }
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
        let result = execute("test", &["-z".into(), "".into()], &mut env, &mut aliases).unwrap();
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
}
