<div align="center">

# ⭐ AsterShell

**A bash-compatible shell that runs at the speed of Rust**

[![CI](https://github.com/salzcill-cmd/AsterShell/actions/workflows/ci.yml/badge.svg)](https://github.com/salzcill-cmd/AsterShell/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/aster-shell.svg)](https://crates.io/crates/aster-shell)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Website](https://img.shields.io/badge/website-salzcill--cmd.github.io%2FAsterShell-blue)](https://salzcill-cmd.github.io/AsterShell/)

```
$ ls -la 8192 files           # brace expansion {a,b,c}, {1..8192}
$ for f in *.rs; do echo $f; done  # bash-style for loops
$ echo ${name:-default}       # full parameter expansion
$ echo $(whoami)              # command substitution
$ cat <<EOF ... EOF           # heredocs
```

</div>

---

AsterShell is a command-line shell written in Rust that runs your existing bash scripts unchanged, while giving you syntax highlighting, autosuggestion, tab completion, and 8 built-in themes — all in a single 2.0 MB static binary.

🌐 **Website**: [salzcill-cmd.github.io/AsterShell](https://salzcill-cmd.github.io/AsterShell/)
📋 **Changelog**: [CHANGELOG.md](CHANGELOG.md)

## Why AsterShell?

| Feature | AsterShell | Bash | Zsh | Fish |
|---------|:----------:|:----:|:---:|:----:|
| Bash script compatible | ✅ | ✅ | ❌ | ❌ |
| Startup time (low-end) | **14 ms** | 18 ms | 16 ms | 178 ms |
| Binary size | **2.0 MB** | 1.2 MB | 600 KB | 5.2 MB |
| Syntax highlighting | ✅ built-in | ❌ | via plugin | ✅ built-in |
| Autosuggestion | ✅ built-in | ❌ | via plugin | ✅ built-in |
| Tab completion | ✅ built-in | ✅ | ✅ | ✅ |
| Themes | **8** | 0 | limited | limited |
| Plugin system | TOML | bash scripts | zsh scripts | none |
| Rust safety | ✅ `unsafe=deny` | C | C | C++ |
| Written by a student | ❓ | ❌ | ❌ | ❌ |

## Install

```bash
# From crates.io
cargo install aster-shell

# From source
git clone https://github.com/salzcill-cmd/AsterShell.git
cd AsterShell
cargo install --path .
```

## Features

### Shell Expansions
- **Command Substitution** — `$(cmd)` and backticks: `echo $(whoami)`, `files=$(ls *.txt)`
- **Arithmetic** — `$((expr))`: `echo $((2 + 3 * 4))`, `i=$((i + 1))`
- **Parameter Expansion** — `${var:-default}`, `${var:=value}`, `${var:+alt}`, `${var:?error}`, `${#var}`, `${var%pat}`, `${var/old/new}`
- **Brace Expansion** — `{a,b,c}`, `{1..5}`, `{a..z}`, `{01..10}`, nested: `{a,b}{c,d}`
- **Tilde Expansion** — `~` → `$HOME`
- **Glob Expansion** — `*`, `?`, `[...]`, `**` recursive

### Interactive
- **Multi-line Input** — Automatic `> ` continuation for unclosed quotes/braces
- **Syntax Highlighting** — Real-time colorization as you type
- **Autosuggestion** — History-based inline suggestions (accept with →)
- **Tab Completion** — Commands, paths, dirs, env vars (`$TAB`), tilde (`~/`)
- **Ctrl+R Search** — Reverse incremental history search

### Control Flow & Functions
- **If/Elif/Else** — `if cmd; then ...; elif cmd; then ...; else ...; fi`
- **While/For Loops** — `while cond; do ...; done`, `for var in words; do ...; done`
- **Case Statements** — `case $var in pattern) cmds ;; esac`
- **Functions** — `function name { ...; }` or `name() { ...; }`
- **Break/Continue** — Loop control flow
- **Logical Operators** — `cmd1 && cmd2`, `cmd1 || cmd2`
- **Pipelines** — `cmd1 | cmd2 | cmd3`
- **Here-Documents** — `cat <<EOF ... EOF`

### Job Control
- **Background Jobs** — `cmd &`
- **Jobs** — `jobs` lists active jobs
- **Foreground/Resume** — `fg %N`, `bg %N`
- **Kill** — `kill [-signal] pid`

### Themes & Configuration
- **8 Built-in Themes** — default, nord, catppuccin, tokyonight, gruvbox, solarized, dracula, onedark
- **TOML Configuration** — `~/.config/aster/config.toml`
- **Plugin System** — TOML-based `.aster` files with dependency resolution

### Safety & Performance
- **Unsafe code denied** — workspace-wide `unsafe_code = "deny"` with strict Clippy lints
- **Single binary** — no runtime dependencies
- **2.0 MB** stripped, LTO, `panic=abort`
- **312 tests** — integration + unit, 0 failures

## Configuration

```toml
[shell]
welcome_message = true

[prompt]
symbol = "❯"
segments = ["dir"]

[history]
max_size = 10000
persistent = true

[theme]
name = "catppuccin"
```

| File | Path |
|------|------|
| Config | `~/.config/aster/config.toml` |
| Plugins | `~/.config/aster/plugins/*.aster` |
| History | `~/.local/share/aster/history` |

## Plugin Example

```toml
name        = "git-utils"
version     = "0.1.0"
description = "Handy git aliases"
enabled     = true

[aliases]
gs = "git status"
gd = "git diff"
gp = "git pull"
gc = "git commit -m"
```

See [`examples/plugins/`](examples/plugins/) for ready-to-use plugins.

## Built-in Commands (23)

| Command | Description |
|---------|-------------|
| `echo` | Display text (`-n`, `-e` flags) |
| `printf` | Formatted output |
| `pwd` | Current directory |
| `pushd` / `popd` | Directory stack |
| `export` / `unset` / `env` | Environment |
| `alias` / `unalias` | Shell aliases |
| `eval` / `source` | Execution |
| `which` / `type` | Command info |
| `test` / `[` | Conditionals |
| `jobs` / `fg` / `bg` / `kill` | Job control |
| `help` / `version` | Info |
| `true` / `false` | Exit codes |

## Built-in Themes

| Theme | Description |
|-------|-------------|
| `default` | Monokai-inspired |
| `nord` | Arctic north-bluish |
| `catppuccin` | Soothing pastel (Mocha) |
| `tokyonight` | Dark Tokyo night |
| `gruvbox` | Retro groove |
| `solarized` | Precision engineered |
| `dracula` | 21st century dark |
| `onedark` | Atom One Dark |

## Project Structure

```
AsterShell/
├── aster/          # Binary entry point
├── shell-core/     # AST, types, errors, environment
├── lexer/          # Tokenizer
├── parser/         # Parser → AST
├── executor/       # Command execution
├── builtins/       # 23 built-in commands
├── prompt/         # Multi-segment prompt
├── history/        # Persistent history
├── completion/     # Tab completion
├── highlight/      # Syntax highlighting
├── editor/         # Line editor (rustyline)
├── theme/          # 8 built-in themes
├── plugin/         # Plugin lifecycle
├── config/         # TOML config loader
└── utils/          # Shared utilities
```

## Demo

```bash
# Install and run
cargo install aster-shell
aster

# Inside AsterShell, try the demo:
source examples/demo.sh

# Or load the dev workflow:
source examples/dev-workflow.sh
```

See [`examples/`](examples/) for demo scripts and plugin files.

## Requirements

- **Rust** 1.85+ (2024 edition)
- **OS** Linux

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b my-feature`)
3. Run checks:
   ```bash
   cargo fmt --check
   cargo clippy --workspace --all-targets
   cargo test --workspace
   ```
4. Commit and open a pull request

## License

MIT License — see [LICENSE](LICENSE) for details.
