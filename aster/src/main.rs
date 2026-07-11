//! `AsterShell` — a modern, fast, lightweight, extensible Linux shell.
//!
//! This is the main binary crate that ties together all subsystems into
//! an interactive read-eval-print loop.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use aster_config::Config;
use aster_executor::{ExecContext, ExecOutcome, Executor};
use aster_history::History;
use aster_lexer::Lexer;
use aster_parser::Parser;
use aster_shell_core::{AliasMap, ShellError};

struct Shell {
    config: Config,
    history: History,
    ctx: ExecContext,
    running: Arc<AtomicBool>,
}

impl Shell {
    /// Initializes the shell: loads config, history, sets up signal handling.
    fn init() -> Result<Self, ShellError> {
        let config = aster_config::ensure_config()?;
        let history = History::new(config.history.max_size)?;

        let mut aliases = AliasMap::new();
        for (name, value) in &config.aliases {
            aliases.insert(name, value);
        }

        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        if ctrlc::set_handler(move || {
            r.store(false, Ordering::Relaxed);
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
                ..ExecContext::default()
            },
            running,
        })
    }

    /// Loads and executes a startup script from a file path.
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

    /// Loads startup scripts in order: /etc/aster/shellrc, ~/.aster/shellrc, ~/.asterrc
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

    /// Runs the main interactive REPL loop.
    fn run(&mut self) {
        // Load startup scripts
        self.load_startup_scripts();

        if self.config.shell.welcome_message {
            eprintln!(
                "{} {} — type `help` for builtins, `exit` to quit.",
                aster_shell_core::SHELL_NAME,
                aster_shell_core::VERSION,
            );
        }

        // Build the prompt
        let prompt = aster_prompt::Prompt::new(
            self.config.prompt.show_status,
            self.config.prompt.symbol.clone(),
            self.config.prompt.segments.clone(),
        );

        // Determine theme
        let theme = aster_theme::find_theme(&self.config.theme.name)
            .unwrap_or_else(|| Box::new(aster_theme::DefaultTheme));

        // Create the line editor
        let mut editor = match aster_editor::EditorWrapper::new(theme) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("aster: failed to initialize editor: {e}");
                return;
            }
        };

        loop {
            // Reset Ctrl-C flag
            if !self.running.load(Ordering::Relaxed) {
                self.running.store(true, Ordering::Relaxed);
                eprintln!();
                continue;
            }

            // Print prompt
            let prompt_str = prompt.render(self.ctx.last_exit_code);

            // Read line
            let input = match editor.readline(&prompt_str) {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(e) => {
                    eprintln!("aster: read error: {e}");
                    break;
                }
            };

            let trimmed = input.trim();

            // Skip empty input
            if trimmed.is_empty() {
                continue;
            }

            // Handle Ctrl-C reset
            self.running.store(true, Ordering::Relaxed);

            // Handle exit
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

            // Record history
            self.history.add(trimmed.to_string());
            let _ = editor.add_history_entry(trimmed);

            // Handle history command
            if trimmed == "history" {
                for (i, entry) in self.history.entries().iter().enumerate() {
                    println!(" {:>5}  {}", i + 1, entry.command);
                }
                continue;
            }

            // Lex + Parse + Execute
            if let Err(e) = self.execute_line(trimmed) {
                eprintln!("aster: {e}");
            }
        }

        // Save history on exit
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

    let mut shell = match Shell::init() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aster: initialization error: {e}");
            std::process::exit(1);
        }
    };

    shell.run();
}
