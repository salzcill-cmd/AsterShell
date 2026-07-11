# Architecture

## Overview

`AsterShell` is a Cargo workspace of 13 crates, organized as a layered architecture:

```
┌──────────────────────────────────────────┐
│                 aster (REPL)              │  Binary crate — entry point
├──────────────────────────────────────────┤
│  executor  │  prompt  │  history  │ config│  Subsystem crates
├──────────────────────────────────────────┤
│  parser    │  builtins│  completion│ ... │  Processing crates
├──────────────────────────────────────────┤
│            lexer                          │  Tokenization
├──────────────────────────────────────────┤
│            shell-core                     │  Core types (AST, Span, Error)
├──────────────────────────────────────────┤
│            utils                          │  Shared utilities
└──────────────────────────────────────────┘
```

## Data Flow

```
User Input ──▶ Lexer ──▶ Tokens ──▶ Parser ──▶ AST ──▶ Executor ──▶ Output
                                                       │
                                                  ┌────┴────┐
                                                  │ builtins │
                                                  │ PATH     │
                                                  │ lookup   │
                                                  └─────────┘
```

## Crate Responsibilities

### `shell-core` (leaf)
Foundation types shared by all crates:
- `Span` — source location tracking (line, column, offset, length)
- `ast` — AST node types (`Program`, `Statement`, `PipeExpr`, `Atom`, `SimpleCommand`, `Group`, `Redirect`)
- `error` — unified error hierarchy (`ShellError`, `LexerError`, `ParseError`, `ExecError`, `ConfigError`, `HistoryError`)

### `utils` (leaf)
Platform-independent utilities:
- `find_executable()` — PATH lookup
- `is_executable()` — permission check
- `abbreviate_path()` — `~` home directory abbreviation
- `split_words()` — quote-aware word splitting

### `lexer`
Tokenizes shell input into a stream of `Token` values with source spans. Handles:
- Words, single-quoted strings, double-quoted strings
- Operators: `|`, `&&`, `||`, `;`, `<`, `>`, `>>`, `(`, `)`
- Escape sequences in double-quoted strings
- Comments (`# ...`)

### `parser`
Recursive-descent parser producing an AST. Grammar (lowest to highest precedence):
```
program     = statement (';' statement)*
statement   = pipe ('&&' pipe)* | pipe ('||' pipe)*
pipe        = atom ('|' atom)*
atom        = command | '(' program ')'
command     = WORD+ redirect*
redirect    = ('<' | '>' | '>>') WORD
```

### `executor`
Walks the AST and executes commands:
- External commands via `std::process::Command`
- Pipelines with inter-process pipes
- Logical AND/OR short-circuit evaluation
- Redirect setup (stdin, stdout, append)
- Built-in command dispatch

### `builtins`
Built-in commands that run inside the shell process:
- `echo`, `pwd`, `true`, `false`
- `which`, `type`, `help`, `version`
- `alias`, `unalias`

### `prompt`
Two-line prompt renderer showing:
- Current directory (abbreviated with `~`)
- Exit status indicator (✗ on failure)

### `history`
Command history with:
- In-memory `Vec<String>` storage
- Persistent file at `~/.local/share/aster/history`
- Duplicate skip, configurable max size

### `config`
TOML configuration loading from `~/.config/aster/config.toml` with:
- Default config creation
- Validation
- Pretty-print serialization

### `completion`, `highlight`, `plugin`
Stub crates reserved for future phases.
