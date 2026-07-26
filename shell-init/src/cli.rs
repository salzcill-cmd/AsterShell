//! Command-line argument parsing for AsterShell.
//!
//! Implements POSIX-compliant shell argument parsing compatible with bash/zsh.
//!
//! # Supported flags
//!
//! | Flag | Meaning |
//! |------|---------|
//! | `-c 'cmd'` | Execute `cmd` then exit (non-interactive) |
//! | `-s` | Read commands from stdin (forced interactive) |
//! | `-l` / `--login` | Force login shell mode |
//! | `-h` / `--help` | Print usage and exit |
//! | `-v` / `--version` | Print version and exit |
//! | `--norc` | Skip user RC file for interactive non-login |
//! | `--noprofile` | Skip profile files for login shell |
//! | `--posix` | POSIX mode (accepted, no-op for now) |
//! | `--` | End of flags; next arg is script file |
//!
//! # Argument patterns
//!
//! - `aster` — interactive login or non-login shell
//! - `aster -c 'cmd'` — run command, exit
//! - `aster script.sh` — run script, exit
//! - `aster -l` — force login mode
//! - `aster -s` — read from stdin
//! - `aster -c 'cmd' arg1 arg2` — set positional parameters

use std::env;

/// The effective mode the shell should operate in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellMode {
    /// Interactive shell with prompt (REPL loop).
    Interactive,
    /// Non-interactive: execute a command string from `-c`.
    Command,
    /// Non-interactive: execute a script file (first positional arg).
    Script,
    /// Read from stdin (forced by `-s`).
    Stdin,
}

/// Parsed command-line arguments for AsterShell.
#[derive(Debug, Clone)]
pub struct ShellArgs {
    /// The raw `argv[0]` — used for login shell detection.
    pub argv0: String,
    /// Whether `-l`/`--login` was passed.
    pub login_flag: bool,
    /// Whether `--norc` was passed (skip RC for interactive non-login).
    pub norc: bool,
    /// Whether `--noprofile` was passed (skip profile for login).
    pub noprofile: bool,
    /// Whether `-s` was passed (read from stdin).
    pub stdin_mode: bool,
    /// The command string from `-c`, if any.
    pub command: Option<String>,
    /// The script file path (positional arg after flags), if any.
    pub script_file: Option<String>,
    /// Positional arguments after `-c 'cmd'` or script file.
    pub positional_args: Vec<String>,
}

impl ShellArgs {
    /// Parses command-line arguments from the process environment.
    ///
    /// This mirrors how bash/zsh parse their arguments. Unknown flags
    /// cause an error message and exit(2).
    #[must_use]
    pub fn parse() -> Self {
        let args: Vec<String> = env::args().collect();
        Self::parse_from(args)
    }

    /// Parses command-line arguments from a given argument vector.
    ///
    /// Used for testing. The real entry point uses `env::args()`.
    #[must_use]
    pub fn parse_from(args: Vec<String>) -> Self {
        let argv0 = args.first().cloned().unwrap_or_else(|| "aster".into());

        let mut result = Self {
            argv0,
            login_flag: false,
            norc: false,
            noprofile: false,
            stdin_mode: false,
            command: None,
            script_file: None,
            positional_args: Vec::new(),
        };

        let mut i = 1; // skip argv[0]
        let args_len = args.len();

        while i < args_len {
            let arg = &args[i];

            // End of flags
            if arg == "--" {
                i += 1;
                // Everything after -- is positional
                if i < args_len {
                    result.script_file = Some(args[i].clone());
                    i += 1;
                    result.positional_args = args[i..].to_vec();
                }
                break;
            }

            if !arg.starts_with('-') || arg == "-" {
                // Not a flag, or bare "-" (means read from stdin)
                if arg == "-" {
                    result.stdin_mode = true;
                } else {
                    // First non-flag arg is the script file
                    result.script_file = Some(arg.clone());
                    i += 1;
                    result.positional_args = args[i..].to_vec();
                }
                break;
            }

            // Handle combined flags: -lv, -lc, etc.
            let flag_content = if arg.starts_with("--") {
                arg[2..].to_string()
            } else {
                arg[1..].to_string()
            };

            // Long flags
            if arg.starts_with("--") {
                match flag_content.as_str() {
                    "login" | "l" => result.login_flag = true,
                    "norc" => result.norc = true,
                    "noprofile" => result.noprofile = true,
                    "posix" => { /* accepted, no-op */ }
                    "help" => {
                        Self::print_usage();
                        std::process::exit(0);
                    }
                    "version" | "V" => {
                        println!("aster {}", env!("CARGO_PKG_VERSION"));
                        std::process::exit(0);
                    }
                    _ => {
                        eprintln!("aster: unknown option --{flag_content}");
                        Self::print_usage();
                        std::process::exit(2);
                    }
                }
                i += 1;
                continue;
            }

            // Short flags (may be combined: -lc, -lv, etc.)
            let chars: Vec<char> = flag_content.chars().collect();
            let mut j = 0;
            while j < chars.len() {
                match chars[j] {
                    'l' => result.login_flag = true,
                    'h' => {
                        Self::print_usage();
                        std::process::exit(0);
                    }
                    'v' | 'V' => {
                        println!("aster {}", env!("CARGO_PKG_VERSION"));
                        std::process::exit(0);
                    }
                    'c' => {
                        // -c requires the next argument as the command
                        j += 1;
                        if j < chars.len() {
                            // -ccmd (no space)
                            result.command =
                                Some(chars[j..].iter().collect());
                        } else {
                            // -c cmd (with space)
                            i += 1;
                            result.command = args.get(i).cloned();
                        }
                        // After -c, remaining args become positional parameters
                        i += 1;
                        result.positional_args = args[i..].to_vec();
                        return result;
                    }
                    's' => result.stdin_mode = true,
                    '-' => {
                        // -- already handled above
                        break;
                    }
                    'n' => result.noprofile = true,
                    'r' => result.norc = true,
                    'e' => { /* -e: exit on error, accepted */ }
                    'i' => { /* -i: force interactive, accepted */ }
                    'a' | 'b' | 'f' | 'm' | 'p' | 'u' | 'x' | 'B' | 'C' | 'E' | 'H' | 'K' | 'P' | 'T' | 'W' => {
                        // Common shell flags — accepted silently
                    }
                    other => {
                        eprintln!("aster: -{other}: invalid option");
                        Self::print_usage();
                        std::process::exit(2);
                    }
                }
                j += 1;
            }
            i += 1;
        }

        result
    }

    /// Returns the effective shell mode based on parsed arguments.
    #[must_use]
    pub fn mode(&self) -> ShellMode {
        if self.stdin_mode {
            ShellMode::Stdin
        } else if self.command.is_some() {
            ShellMode::Command
        } else if self.script_file.is_some() {
            ShellMode::Script
        } else {
            ShellMode::Interactive
        }
    }

    fn print_usage() {
        eprintln!("Usage: aster [options] [file [arg ...]]");
        eprintln!("       aster [options] -c command [argument ...]");
        eprintln!("       aster [options] -s [argument ...]");
        eprintln!();
        eprintln!("Options:");
        eprintln!("  -c command   Read and execute commands from command");
        eprintln!("  -h, --help   Display this help and exit");
        eprintln!("  -l, --login  Make this shell a login shell");
        eprintln!("  -s           Read commands from standard input");
        eprintln!("  -v, --version Display version and exit");
        eprintln!("  --norc       Don't read ~/.config/astershell/shellrc");
        eprintln!("  --noprofile  Don't read /etc/profile or ~/.profile");
        eprintln!("  --posix      Accept POSIX mode (no-op)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_no_args() {
        let args = ShellArgs::parse_from(vec!["aster".into()]);
        assert_eq!(args.mode(), ShellMode::Interactive);
        assert!(!args.login_flag);
        assert!(args.command.is_none());
        assert!(args.script_file.is_none());
    }

    #[test]
    fn test_parse_dash_c() {
        let args = ShellArgs::parse_from(vec!["aster".into(), "-c".into(), "echo hello".into()]);
        assert_eq!(args.mode(), ShellMode::Command);
        assert_eq!(args.command.as_deref(), Some("echo hello"));
    }

    #[test]
    fn test_parse_script_file() {
        let args = ShellArgs::parse_from(vec!["aster".into(), "script.sh".into()]);
        assert_eq!(args.mode(), ShellMode::Script);
        assert_eq!(args.script_file.as_deref(), Some("script.sh"));
    }

    #[test]
    fn test_parse_login_flag() {
        let args = ShellArgs::parse_from(vec!["aster".into(), "-l".into()]);
        assert!(args.login_flag);
        assert_eq!(args.mode(), ShellMode::Interactive);
    }

    #[test]
    fn test_parse_combined_flags() {
        let args = ShellArgs::parse_from(vec![
            "aster".into(),
            "-lc".into(),
            "echo test".into(),
        ]);
        assert!(args.login_flag);
        assert_eq!(args.mode(), ShellMode::Command);
        assert_eq!(args.command.as_deref(), Some("echo test"));
    }

    #[test]
    fn test_parse_stdin_mode() {
        let args = ShellArgs::parse_from(vec!["aster".into(), "-s".into()]);
        assert!(args.stdin_mode);
        assert_eq!(args.mode(), ShellMode::Stdin);
    }

    #[test]
    fn test_parse_norc() {
        let args = ShellArgs::parse_from(vec!["aster".into(), "--norc".into()]);
        assert!(args.norc);
    }

    #[test]
    fn test_parse_noprofile() {
        let args = ShellArgs::parse_from(vec!["aster".into(), "--noprofile".into()]);
        assert!(args.noprofile);
    }

    #[test]
    fn test_parse_double_dash() {
        let args = ShellArgs::parse_from(vec![
            "aster".into(),
            "--".into(),
            "script.sh".into(),
            "arg1".into(),
        ]);
        assert_eq!(args.script_file.as_deref(), Some("script.sh"));
        assert_eq!(args.positional_args, vec!["arg1"]);
    }

    #[test]
    fn test_parse_c_with_positional() {
        let args = ShellArgs::parse_from(vec![
            "aster".into(),
            "-c".into(),
            "echo $1".into(),
            "hello".into(),
        ]);
        assert_eq!(args.command.as_deref(), Some("echo $1"));
        assert_eq!(args.positional_args, vec!["hello"]);
    }

    #[test]
    fn test_parse_bare_dash() {
        let args = ShellArgs::parse_from(vec!["aster".into(), "-".into()]);
        assert!(args.stdin_mode);
    }
}
