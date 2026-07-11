# Roadmap

## Phase 1 — Foundation ✅
- Cargo workspace setup
- Core types (AST, Span, Error)
- Toolchain configuration
- Documentation structure

## Phase 2 — Interactive Shell ✅
- Lexer with full token support
- Recursive-descent parser
- Command execution engine
- Pipeline support
- Logical operators (`&&`, `||`)
- Redirection (`<`, `>`, `>>`)
- Built-in commands
- Command history with persistence
- TOML configuration
- Interactive REPL
- Signal handling (Ctrl+C)

## Phase 3 — Completion & Autosuggestion
- Tab completion
- Path completion
- Command completion
- History-based autosuggestion
- Fish-style inline suggestions

## Phase 4 — Syntax Highlighting
- Real-time syntax highlighting
- ANSI color support
- Prompt theming
- Color configuration

## Phase 5 — Job Control
- Background jobs (`&`)
- Foreground/background switching (`fg`, `bg`)
- Job listing (`jobs`)
- Signal management (`SIGTSTP`, `SIGCONT`)

## Phase 6 — Scripting
- Shell variables (`$VAR`)
- Environment export (`export`)
- Conditionals (`if`/`else`/`fi`)
- Loops (`for`, `while`)
- Functions (`fn`)
- Command substitution `$(cmd)`
- Arithmetic expansion `$((expr))`

## Phase 7 — Advanced Features
- Plugin system (dynamic loading)
- Custom prompt themes
- Multi-line editing
- Vi/Emacs input modes
- Extended glob patterns

## Phase 8 — Ecosystem
- Package management integration
- Man page integration
- Configuration hot reload
- Performance benchmarking suite
