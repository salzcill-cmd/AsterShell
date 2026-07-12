# Changelog

All notable changes to AsterShell will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.2.0] - 2026-07-12

### Fixed
- **Autosuggestion critical bug**: history_cache was never updated during session — commands typed in current session were invisible to hints
- **highlight_char()** now respects `forced` parameter (was unconditionally returning `true`)
- **Highlight hint styling**: fish-style dim gray ghost text for autosuggestions

### Added
- **Fish-style empty-line suggestion**: most recent history command shown when input is empty
- **Smart cd**: `..N` goes up N directories (e.g., `cd ..3` = `cd ../../../`), `..` works as expected
- **Inline math**: `=2+3` evaluates and prints `5` (built-in calculator)
- **Command duration in prompt**: `[1.5s]` shown for commands taking >100ms (new `duration` segment)
- **History cache live update**: `update_history_cache()` method on EditorWrapper, called after each command

### Changed
- Version bumped to 0.2.0

## [0.1.0] - 2026-07-12

### Added
- **Shell Core**
  - Command substitution `$(cmd)` and backtick syntax
  - Arithmetic expansion `$((expr))`
  - Parameter expansion: `${var:-default}`, `${var:=value}`, `${var:+alt}`, `${var:?error}`, `${#var}`, `${var%pat}`, `${var%%pat}`, `${var#pat}`, `${var##pat}`, `${var/old/new}`, `${var//old/new}`
  - Brace expansion: `{a,b,c}`, `{1..5}`, `{a..z}`, `{01..10}`, nested/combinatorial
  - Tilde expansion `~` → `$HOME`
  - Glob expansion: `*`, `?`, `[...]`, `**` recursive matching
  - Pipelines `cmd1 | cmd2 | cmd3`
  - Logical operators `cmd1 && cmd2`, `cmd1 || cmd2`
  - Here-documents `cat <<EOF ... EOF`
  - Here-strings `cmd <<< "text"`

- **Control Flow**
  - `if/elif/else/fi` conditionals
  - `while/do/done` loops
  - `for var in words; do/done` loops
  - `case/pattern/esac` statements
  - `function name {}` and `name() {}` functions
  - `break` and `continue` loop control

- **Job Control**
  - Background jobs with `cmd &`
  - `jobs`, `fg %N`, `bg %N`
  - `kill [-signal] pid`

- **Interactive Features**
  - Multi-line input with automatic continuation prompt
  - Syntax highlighting (commands, strings, variables, operators, redirects, comments)
  - History-based autosuggestion (accept with →)
  - Tab completion (commands, paths, dirs, env vars, tilde)
  - Ctrl+R reverse incremental history search

- **Built-in Commands (23)**
  - echo, printf, pwd, true, false, which, type, help, version
  - alias, unalias, export, unset, env
  - pushd, popd, dirs
  - eval, source, wait
  - test / `[`
  - jobs, fg, bg, kill

- **Themes (8)**
  - default (Monokai-inspired)
  - nord (Arctic north-bluish)
  - catppuccin (Mocha)
  - tokyonight
  - gruvbox
  - solarized
  - dracula
  - onedark

- **Plugin System**
  - TOML-based `.aster` plugin format
  - Dependency resolution between plugins
  - Alias injection from plugins
  - Script sourcing from plugins

- **Configuration**
  - TOML config at `~/.config/aster/config.toml`
  - Prompt customization (segments, symbol, colors)
  - History settings (size, persistence, timestamps)
  - Theme selection

- **Developer Experience**
  - Workspace: 15 crates + integration test crate
  - 312 tests, 0 failures
  - CI: fmt, clippy, test, tarpaulin coverage
  - 4 example plugins (git, docker, cargo-dev, python)
  - 2 example workflow scripts
  - Project website (GitHub Pages)

- **Performance**
  - 2.0 MB stripped binary (LTO, panic=abort)
  - 14ms startup on Intel Celeron N3060
  - Single static binary, no runtime dependencies
  - Unsafe code denied workspace-wide
