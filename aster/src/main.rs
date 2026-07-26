//! `AsterShell` — a modern, fast, lightweight, extensible Linux shell.
//!
//! This is the main binary crate that ties together all subsystems into
//! the correct shell mode: interactive REPL, command execution, script
//! execution, or login shell initialization.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use aster_config::Config;
use aster_executor::{ExecContext, ExecOutcome, Executor};
use aster_history::History;
use aster_lexer::Lexer;
use aster_parser::Parser;
use aster_shell_core::{AliasMap, ShellError};
use aster_shell_init::{ShellInit, ShellMode};

struct Shell {
    config: Config,
    history: History,
    ctx: ExecContext,
    running: Arc<AtomicBool>,
    last_cmd_duration: std::time::Duration,
}

impl Shell {
    fn init() -> Result<Self, ShellError> {
        let config = match aster_config::ensure_config() {
            Ok(c) => c,
            Err(e) => {
                // Configuration errors must NEVER crash the shell.
                // Print diagnostic and use defaults.
                eprintln!("aster: config error (using defaults): {e}");
                Config::default()
            }
        };

        let history = History::new(config.history.max_size)?;

        let mut aliases = AliasMap::new();
        for (name, value) in &config.aliases {
            aliases.insert(name, value);
        }

        let mut abbreviations = std::collections::HashMap::new();
        for (name, value) in &config.abbreviations {
            abbreviations.insert(name.clone(), value.clone());
        }

        let running = Arc::new(AtomicBool::new(true));
        let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let r = running.clone();
        let ic = interrupted.clone();
        if ctrlc::set_handler(move || {
            r.store(false, Ordering::Relaxed);
            ic.store(true, Ordering::Relaxed);
            // Send SIGINT to the foreground process group if one exists
            #[allow(unsafe_code)]
            unsafe {
                let pgid = libc::getpgid(0);
                if pgid > 0 {
                    libc::kill(-pgid, libc::SIGINT);
                }
            }
        })
        .is_err()
        {
            eprintln!("aster: warning: could not set Ctrl-C handler");
        }

        Ok(Self {
            config,
            history,
            ctx: ExecContext {
                last_exit_code: 0,
                prev_dir: None,
                aliases,
                abbreviations,
                interrupted,
                ..ExecContext::default()
            },
            running,
            last_cmd_duration: std::time::Duration::ZERO,
        })
    }

    fn load_startup_script(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                if let Err(e) = self.execute_line(&content) {
                    eprintln!("aster: {}: {e}", path.display());
                }
            }
            Err(_) => {
                // Silently ignore missing startup scripts
            }
        }
    }

    /// Loads RC scripts for interactive non-login shells.
    fn load_startup_scripts(&mut self) {
        // System-wide config
        if let Some(config_dir) = dirs::config_dir() {
            let system_rc = config_dir.join("aster").join("shellrc");
            self.load_startup_script(&system_rc);
        }

        // User config
        if let Some(home) = dirs::home_dir() {
            let user_dir = home.join(".aster");
            let user_rc = user_dir.join("shellrc");
            self.load_startup_script(&user_rc);

            let home_rc = home.join(".asterrc");
            self.load_startup_script(&home_rc);
        }
    }

    fn run(&mut self) {
        self.load_startup_scripts();

        if self.config.shell.welcome_message {
            let version = aster_shell_core::VERSION;
            eprintln!(
                "\x1b[38;2;167;139;250m  \u{2726} \x1b[1;38;2;196;167;231mAsterShell\x1b[0m \
                 \x1b[38;2;110;106;134mv{version}\x1b[0m"
            );
            eprintln!(
                "\x1b[38;2;110;106;134m  \u{2500}\u{2500}\u{2500} \
                 type \x1b[1;38;2;156;207;216mhelp\x1b[0m \
                 \x1b[38;2;110;106;134mfor builtins, \
                 \x1b[1;38;2;156;207;216mexit\x1b[0m \
                 \x1b[38;2;110;106;134mto quit\x1b[0m"
            );
            eprintln!();
        }

        let prompt = aster_prompt::Prompt::new(
            self.config.prompt.show_status,
            self.config.prompt.symbol.clone(),
            self.config.prompt.segments.clone(),
        );

        let theme = aster_theme::find_theme(&self.config.theme.name)
            .unwrap_or_else(|| Box::new(aster_theme::DefaultTheme));

        let mut editor = match aster_editor::EditorWrapper::new(theme, &self.config.shell.edit_mode)
        {
            Ok(e) => e,
            Err(e) => {
                eprintln!("aster: failed to initialize editor: {e}");
                return;
            }
        };

        loop {
            if !self.running.load(Ordering::Relaxed) {
                self.running.store(true, Ordering::Relaxed);
                eprintln!();
                continue;
            }

            let prompt_str = prompt.render(self.ctx.last_exit_code, self.last_cmd_duration);

            let input = match editor.readline(&prompt_str) {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(e) => {
                    eprintln!("aster: read error: {e}");
                    break;
                }
            };

            let trimmed = input.trim();

            if trimmed.is_empty() {
                continue;
            }

            self.running.store(true, Ordering::Relaxed);

            if trimmed == "exit" || trimmed.starts_with("exit ") {
                let code = trimmed
                    .strip_prefix("exit")
                    .unwrap_or("")
                    .trim()
                    .parse::<i32>()
                    .unwrap_or(0);
                let _ = editor.save_history();
                std::process::exit(code);
            }

            self.history.add(trimmed.to_string());
            let _ = editor.add_history_entry(trimmed);
            editor.update_history_cache(trimmed);

            let cmd_start = std::time::Instant::now();

            if trimmed == "history" {
                for (i, entry) in self.history.entries().iter().enumerate() {
                    println!(" {:>5}  {}", i + 1, entry.command);
                }
                continue;
            }

            if let Err(e) = self.execute_line(trimmed) {
                eprintln!("aster: {e}");
            }

            if self.ctx.interrupted.swap(false, Ordering::Relaxed) {
                eprintln!();
            }

            self.last_cmd_duration = cmd_start.elapsed();
        }

        self.history.save().unwrap_or_else(|e| {
            eprintln!("aster: failed to save history: {e}");
        });
    }

    fn execute_line(&mut self, input: &str) -> Result<(), ShellError> {
        let tokens = Lexer::new(input).tokenize()?;
        let program = Parser::new(&tokens).parse()?;

        match Executor::execute(&program, &mut self.ctx)? {
            ExecOutcome::Success(_) | ExecOutcome::Break | ExecOutcome::Continue => {}
            ExecOutcome::Exit(code) => {
                self.history.save().unwrap_or_else(|e| {
                    eprintln!("aster: failed to save history: {e}");
                });
                std::process::exit(code);
            }
        }

        Ok(())
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    // ── Phase 1: Initialize shell detection and environment preservation ──
    let init = ShellInit::initialize();

    // ── Phase 2: Source profile files (login/interactive) ──
    init.source_profiles();

    // ── Phase 3: Dispatch based on shell mode ──
    match init.mode() {
        // ── Interactive / Login shell → enter REPL ──
        ShellMode::Interactive => {
            let mut shell = match Shell::init() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("aster: initialization error: {e}");
                    std::process::exit(1);
                }
            };
            shell.run();
            // Shell exited normally — this is fine for interactive shells.
            // For login shells started by display managers, the REPL loop
            // means the shell is the session leader and keeps the session alive.
        }

        // ── Non-interactive: execute a command string (`-c`) ──
        ShellMode::Command => {
            let command = match init.command() {
                Some(c) => c,
                None => {
                    eprintln!("aster: -c: missing command argument");
                    std::process::exit(2);
                }
            };

            let mut shell = match Shell::init() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("aster: initialization error: {e}");
                    std::process::exit(1);
                }
            };

            // Set positional parameters from -c args
            shell.ctx.positional_args = init
                .invocation
                .positional_args
                .iter()
                .cloned()
                .collect();

            match shell.execute_line(command) {
                Ok(()) => {
                    std::process::exit(shell.ctx.last_exit_code);
                }
                Err(e) => {
                    eprintln!("aster: {e}");
                    std::process::exit(1);
                }
            }
        }

        // ── Non-interactive: execute a script file ──
        ShellMode::Script => {
            let script_path = match init.script_file() {
                Some(p) => p,
                None => {
                    eprintln!("aster: no script file specified");
                    std::process::exit(2);
                }
            };

            let contents = match std::fs::read_to_string(script_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("aster: {script_path}: {e}");
                    std::process::exit(127);
                }
            };

            let mut shell = match Shell::init() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("aster: initialization error: {e}");
                    std::process::exit(1);
                }
            };

            // Set $0 to the script name, positional args from remaining args
            shell.ctx.positional_args = init
                .invocation
                .positional_args
                .iter()
                .cloned()
                .collect();

            match shell.execute_line(&contents) {
                Ok(()) => {
                    std::process::exit(shell.ctx.last_exit_code);
                }
                Err(e) => {
                    eprintln!("aster: {script_path}: {e}");
                    std::process::exit(1);
                }
            }
        }

        // ── Non-interactive: read from stdin (`-s`) ──
        ShellMode::Stdin => {
            let mut contents = String::new();
            match std::io::Read::read_to_string(&mut std::io::stdin(), &mut contents) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("aster: failed to read stdin: {e}");
                    std::process::exit(1);
                }
            }

            let mut shell = match Shell::init() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("aster: initialization error: {e}");
                    std::process::exit(1);
                }
            };

            shell.ctx.positional_args = init
                .invocation
                .positional_args
                .iter()
                .cloned()
                .collect();

            match shell.execute_line(&contents) {
                Ok(()) => {
                    std::process::exit(shell.ctx.last_exit_code);
                }
                Err(e) => {
                    eprintln!("aster: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
}
