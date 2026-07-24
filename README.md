<div align="center">

```
     _          ____  ___ __  __
    / \   _ __ / ___||__ _|  \/  |
   / _ \ | '__\___ \ / _ \ |\/| |
  / ___ \| |   ___) |  __/ |  | |
 /_/   \_\_|  |____/ \___|_|  |_|
```

# AsterShell

**A shell for people who want bash compatibility without the pain.**

[![CI](https://github.com/salzcill-cmd/AsterShell/actions/workflows/ci.yml/badge.svg)](https://github.com/salzcill-cmd/AsterShell/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)

```
$ for f in *.rs; do echo "$f"; done       # bash-style loops, works as-is
$ echo ${name:-default}                    # full parameter expansion
$ [[ $file =~ ^.*\.rs$ ]] && echo match   # regex matching in [[ ]]
$ ls <(sort -u file1) <(sort -u file2)    # process substitution
$ echo $((2**10 + 512))                    # arithmetic, naturally
```

</div>

---

AsterShell is a command-line shell written in Rust. It runs your existing bash scripts without modification while adding syntax highlighting, autosuggestion, tab completion, and themes out of the box. No plugins needed to get started — everything just works.

🌐 **Website** — [salzcill-cmd.github.io/AsterShell](https://salzcill-cmd.github.io/AsterShell/)
📋 **Changelog** — [CHANGELOG.md](CHANGELOG.md)

## How does it compare?

| | AsterShell | Bash | Zsh | Fish |
|---|:---:|:---:|:---:|:---:|
| Runs bash scripts | ✅ | ✅ | ❌ | ❌ |
| Startup time (low-end) | **14ms** | 18ms | 16ms | 178ms |
| Syntax highlighting | built-in | — | plugin | built-in |
| Autosuggestion | built-in | — | plugin | built-in |
| Tab completion | built-in | built-in | built-in | built-in |
| Themes | **8** | 0 | limited | limited |
| `[[ ]]` support | ✅ | ✅ | ✅ | — |
| `select` menu | ✅ | ✅ | ✅ | — |
| `trap` / signals | ✅ | ✅ | ✅ | — |
| Job control | ✅ | ✅ | ✅ | ✅ |
| Vi mode | ✅ | — | plugin | — |
| Written in safe Rust | ✅ | C | C | C++ |

## Install

**Quick install** (download pre-built binary, no Rust needed):

```bash
curl -sSL https://raw.githubusercontent.com/salzcill-cmd/AsterShell/main/scripts/install.sh | bash
```

Or **set as default shell**:

```bash
curl -sSL https://raw.githubusercontent.com/salzcill-cmd/AsterShell/main/scripts/install.sh | bash -s -- --set-default
```

Or **build from source** (requires [Rust 1.85+](https://rustup.rs)):

```bash
cargo install --git https://github.com/salzcill-cmd/AsterShell.git aster
```

## Features

### Expansions & Expressions

AsterShell supports all the expansions you'd expect from a modern shell:

| Expansion | Example |
|---|---|
| Command substitution | `echo $(whoami)` or `` echo `whoami` `` |
| Arithmetic | `echo $((2 + 3 * 4))`, `i=$((i + 1))` |
| Parameter | `${var:-default}`, `${var:=value}`, `${var/old/new}` |
| Brace | `{a,b,c}`, `{1..5}`, `{a..z}`, `{01..10}` |
| Tilde | `~` → `$HOME` |
| Glob | `*`, `?`, `[...]`, `**` (recursive) |
| Process substitution | `diff <(sort a) <(sort b)` |
| `[[ ]]` patterns | `[[ $x == *.log ]]`, `[[ $x =~ ^error ]]` |

### Control Flow

```bash
if [[ -f "$file" ]]; then
    echo "exists"
elif [[ -d "$file" ]]; then
    echo "it's a directory"
else
    echo "not found"
fi

for i in {1..5}; do
    echo "$i"
done

case "$lang" in
    rust)   echo "nice" ;;
    python) echo "ok" ;;
    *)      echo "hm" ;;
esac

select opt in "yes" "no" "maybe"; do
    [[ -n "$opt" ]] && break
done
```

Functions work the POSIX way and the bash way:

```bash
greet() {
    echo "hello, $1"
}

function deploy {
    cargo build --release
    scp target/release/myapp server:/opt/
}
```

### Job Control

Real job control, not a toy implementation. Background processes, `fg`/`bg` resume, `disown`, `wait`, proper process groups — the works.

```bash
long_task &
echo "running as job $!"
wait
fg %1
```

### Vi Mode

Set `edit_mode = "vi"` in your config and you get proper modal editing with rustyline's Vi implementation. Normal mode, insert mode, visual mode — all there.

### Shell Builtins (28)

`echo` · `printf` · `pwd` · `true` · `false` · `which` · `type` · `alias` · `unalias` · `export` · `unset` · `env` · `pushd` · `popd` · `dirs` · `eval` · `source` · `test` · `trap` · `exec` · `set` · `read` · `shift` · `wait` · `jobs` · `fg` · `bg` · `kill` · `disown` · `compgen` · `string` · `mapfile` · `dirname` · `basename` · `command` · `help` · `version`

### Themes

Eight built-in themes. Switch with `theme.name = "..."` in your config.

| Theme | Vibe |
|---|---|
| `default` | Monokai-inspired dark |
| `nord` | Arctic blue |
| `catppuccin` | Pastel Mocha |
| `tokyonight` | Tokyo evening |
| `gruvbox` | Retro warm |
| `solarized` | Precision earth tones |
| `dracula` | Purple-tinted dark |
| `onedark` | Atom One Dark |

## Configuration

Everything lives in `~/.config/aster/config.toml`.

```toml
[shell]
welcome_message = true
edit_mode = "emacs"          # or "vi"

[prompt]
symbol = ">"

[history]
max_size = 10000
persistent = true

[theme]
name = "catppuccin"

[aliases]
gs = "git status"
gc = "git commit -m"
ll = "ls -la"
```

Abbreviations expand inline before execution (like fish):

```toml
[abbreviations]
gc = "git commit -m"
gco = "git checkout"
```

Plugins are TOML files in `~/.config/aster/plugins/`:

```toml
name        = "git-utils"
version     = "0.1.0"
description = "git shortcuts"
enabled     = true

[aliases]
gs = "git status"
gd = "git diff"
gp = "git pull"
```

| File | Location |
|---|---|
| Config | `~/.config/aster/config.toml` |
| Plugins | `~/.config/aster/plugins/*.aster` |
| History | `~/.local/share/aster/history` |

## Project Structure

```
AsterShell/
├── aster/          binary entry point
├── shell-core/     AST, types, errors
├── lexer/          tokenizer
├── parser/         parser → AST
├── executor/       command execution
├── builtins/       27 built-in commands
├── prompt/         multi-segment prompt
├── history/        persistent history
├── completion/     tab completion engine
├── highlight/      syntax highlighting
├── editor/         line editor (rustyline)
├── theme/          8 built-in themes
├── plugin/         plugin lifecycle
├── config/         TOML config loader
├── utils/          shared utilities
└── tests/          integration tests
```

15 crates. One binary. Zero unsafe.

## Performance

- **3.2 MB** stripped, LTO, `panic=abort`
- **14ms** startup on an Intel Celeron N3060
- No runtime dependencies — it's just a binary

## Developing

```bash
git clone https://github.com/salzcill-cmd/AsterShell.git
cd AsterShell
cargo test            # 358 tests
cargo fmt --check
cargo clippy --workspace --all-targets
```

Pull requests welcome. Fork, branch, test, PR. Keep it clean.

## License

[MIT](LICENSE)
