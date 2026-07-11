<div align="center">

# AsterShell

**A modern, fast, lightweight, extensible Linux shell written in Rust**

[![CI](https://github.com/salzcill-cmd/AsterShell/actions/workflows/ci.yml/badge.svg)](https://github.com/salzcill-cmd/AsterShell/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/aster-shell.svg)](https://crates.io/crates/aster-shell)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

</div>

---

AsterShell is a feature-rich shell built in Rust, designed to be a fast and extensible replacement for traditional shells. It ships with syntax highlighting, autosuggestion, tab completion, a theming system, and a TOML-based plugin architecture -- all with a single static binary.

## Features

### Shell Expansions (bash/fish/zsh-compatible)
- **Command Substitution** -- `$(cmd)` and backtick syntax: `echo $(whoami)`, `files=$(ls *.txt)`
- **Arithmetic Expansion** -- `$((expr))`: `echo $((2 + 3 * 4))`, `i=$((i + 1))`
- **Parameter Expansion** -- Full bash-compatible: `${var:-default}`, `${var:=value}`, `${var:+alt}`, `${var:?error}`, `${#var}` (length), `${var%pat}` / `${var%%pat}` (suffix removal), `${var#pat}` / `${var##pat}` (prefix removal), `${var/old/new}` / `${var//old/new}` (substitution)
- **Brace Expansion** -- `{a,b,c}`, `{1..5}`, `{a..z}`, `{01..10}`, nested/combinatorial: `{a,b}{c,d}`
- **Tilde Expansion** -- `~` → `$HOME`, `~/path` → `$HOME/path`
- **Glob Expansion** -- `*`, `?`, `[...]`, `**` recursive matching

### Interactive Features
- **Multi-line Input** -- Automatic continuation prompt (`> `) for unclosed quotes, braces, parentheses, and trailing operators
- **Syntax Highlighting** -- Real-time colorization of commands, strings, variables, operators, redirects, and comments as you type
- **Autosuggestion** -- History-based inline suggestions that you can accept with the right arrow key
- **Tab Completion** -- Context-aware completion: commands (builtins + PATH), file paths, directories, environment variables (`$TAB`), tilde (`~/`); smart completion for `cd` (dirs only), `source` (script files), `export`/`declare` (var names)
- **Ctrl+R History Search** -- Reverse incremental search through history (built-in via rustyline)

### Control Flow & Functions
- **If/Elif/Else** -- `if cmd; then ...; elif cmd; then ...; else ...; fi`
- **While/For Loops** -- `while cond; do ...; done`, `for var in words; do ...; done`
- **Case Statements** -- `case $var in pattern) cmds ;; esac`
- **Functions** -- `function name { ...; }` or `name() { ...; }`
- **Break/Continue** -- Loop control flow
- **Logical Operators** -- `cmd1 && cmd2`, `cmd1 || cmd2`
- **Pipelines** -- `cmd1 | cmd2 | cmd3`
- **Here-Documents** -- `cat <<EOF ... EOF`

### Job Control
- **Background Jobs** -- `cmd &` spawns in background
- **Jobs** -- `jobs` lists active jobs
- **Foreground** -- `fg %N` brings job N to foreground
- **Background Resume** -- `bg %N` resumes job N
- **Kill** -- `kill [-signal] pid` sends signals to processes

### Themes & Configuration
- **8 Built-in Themes** -- default, nord, catppuccin, tokyonight, gruvbox, solarized, dracula, onedark
- **TOML Configuration** -- `~/.config/aster/config.toml` with prompt, history, theme, and alias settings
- **Plugin System** -- TOML-based plugin format (`.aster` files) with dependency resolution, alias injection, and script sourcing

### Built-in Commands (23)
- **Text**: `echo` (`-n`, `-e`), `printf`
- **Filesystem**: `pwd`, `pushd`, `popd`, `dirs`
- **Environment**: `export`, `unset`, `env`
- **Aliases**: `alias`, `unalias`
- **Execution**: `eval`, `source`, `wait`
- **Utilities**: `which`, `type`, `test`/`[`, `help`, `version`, `true`, `false`

### Safety & Performance
- **Safety First** -- `unsafe_code = "deny"` enforced workspace-wide with strict Clippy lints

## Quick Start

### Install from crates.io

```bash
cargo install aster-shell
```

### Build from source

```bash
git clone https://github.com/salzcill-cmd/AsterShell.git
cd AsterShell
cargo install --path .
```

### Run directly

```bash
git clone https://github.com/salzcill-cmd/AsterShell.git
cd AsterShell
cargo run --release
```

## Benchmarks

### Binary Size

```
$ ls -lh target/release/aster
2.0M   target/release/aster
```

Compiled with `opt-level = 3`, `lto = "fat"`, `strip = "symbols"`, `panic = "abort"`.

### Startup Time

Tested on Intel Celeron N3060 @ 1.60GHz (low-end CPU):

```
$ for i in $(seq 1 10); do
    time echo exit | ./aster >/dev/null 2>&1
  done

Run 1:  0.020s
Run 2:  0.014s
Run 3:  0.020s
Run 4:  0.010s
Run 5:  0.020s
Run 6:  0.021s
Run 7:  0.026s
Run 8:  0.015s
Run 9:  0.010s
Run 10: 0.009s

Mean:   16.5 ms
Min:     9 ms
Max:    26 ms
```

On faster hardware (Ryzen/Apple Silicon), startup is well under 10ms.

## Configuration

AsterShell reads its configuration from `~/.config/aster/config.toml`. On first launch, a default configuration file is created automatically.

### Default configuration

```toml
[shell]
welcome_message = true

[prompt]
show_status = true
symbol = "❯"
segments = ["dir"]

[history]
max_size = 10000
persistent = true
timestamps = true

[theme]
name = "default"
syntax_highlighting = true

[aliases]
# ll = "ls -la"

[keybindings]
# custom key bindings
```

### File locations

| File | Path |
|------|------|
| Configuration | `~/.config/aster/config.toml` |
| Plugins | `~/.config/aster/plugins/*.aster` |
| History | `~/.local/share/aster/history` |

## Built-in Commands

| Command | Description |
|---------|-------------|
| `echo` | Display text (supports `-n`, `-e` flags) |
| `printf` | Formatted output with placeholder syntax |
| `pwd` | Print current working directory |
| `true` | Return success exit code (0) |
| `false` | Return failure exit code (1) |
| `which` | Locate a command in PATH |
| `type` | Describe how a command name is interpreted |
| `help` | Display available built-in commands |
| `version` | Print the shell version |
| `alias` | Define or display shell aliases |
| `unalias` | Remove a shell alias |
| `env` | Display environment variables |
| `export` | Export variables to the environment |
| `unset` | Remove environment variables |
| `pushd` | Push a directory onto the stack |
| `popd` | Pop a directory from the stack |
| `dirs` | Display the directory stack |
| `wait` | Wait for background processes |
| `eval` | Evaluate arguments as shell commands |
| `source` | Execute commands from a file |
| `test` / `[` | Evaluate conditional expressions (`-f`, `-d`, `-e`, `-z`, `-n`, `=`, `!=`) |
| `jobs` | List active background jobs |
| `fg` | Bring a background job to the foreground (`fg %N`) |
| `bg` | Resume a suspended job in the background (`bg %N`) |
| `kill` | Send a signal to a process (`kill [-signal] pid`) |

## Built-in Themes

| Theme | Description |
|-------|-------------|
| `default` | Monokai-inspired color scheme |
| `nord` | Arctic, north-bluish clean and elegant theme |
| `catppuccin` | Soothing pastel theme (Mocha variant) |
| `tokyonight` | Dark and vibrant theme inspired by Tokyo's night lights |
| `gruvbox` | Retro groove color scheme |
| `solarized` | Precision engineered color scheme (Dark variant) |
| `dracula` | A dark palette for the 21st century |
| `onedark` | Atom One Dark theme |

Set your theme in `config.toml`:

```toml
[theme]
name = "catppuccin"
```

## Plugin System

Plugins are TOML files with the `.aster` extension placed in `~/.config/aster/plugins/`.

### Example plugin (`git-utils.aster`)

```toml
name        = "git-utils"
version     = "0.1.0"
description = "Handy git aliases and functions"
enabled     = true
commands    = ["git-utils/init.sh", "git-utils/aliases.sh"]
dependencies = ["base-utils"]

[aliases]
gs = "git status"
gd = "git diff"
gp = "git pull"
gc = "git commit -m"
```

## Project Structure

```
AsterShell/
├── aster/          # Main binary entry point
├── shell-core/     # Core types, AST, errors, environment
├── lexer/          # Tokenizer for shell input
├── parser/         # Parser producing an AST
├── executor/       # Command execution engine
├── builtins/       # Built-in command implementations
├── prompt/         # Multi-segment prompt renderer
├── history/        # Persistent command history
├── completion/     # Tab completion engine
├── highlight/      # Syntax highlighting
├── editor/         # Line editor (rustyline wrapper)
├── theme/          # Color theme system
├── plugin/         # Plugin lifecycle manager
├── config/         # TOML configuration loader
└── utils/          # Shared utility functions
```

## Requirements

- **Rust** 1.85 or later (2024 edition)
- **OS** Linux (other platforms may work but are not officially supported)

## Contributing

Contributions are welcome! Please follow these steps:

1. Fork the repository
2. Create a feature branch (`git checkout -b my-feature`)
3. Make your changes
4. Run the checks:
   ```bash
   cargo fmt --check
   cargo clippy --workspace --all-targets
   cargo test --workspace
   ```
5. Commit your changes and open a pull request

Please read the [issue tracker](https://github.com/salzcill-cmd/AsterShell/issues) for open tasks and the [pull request template](.github/PULL_REQUEST_TEMPLATE.md) for guidelines.

## License

This project is licensed under the MIT License -- see the [LICENSE](LICENSE) file for details.
