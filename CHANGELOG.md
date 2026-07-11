# Changelog

All notable changes to AsterShell will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.1.0] — 2024-01-01

### Added
- Cargo workspace with 13 crates
- Core types: AST, source spans, error hierarchy
- Lexer supporting words, quoted strings, operators, escapes, comments
- Recursive-descent parser with proper operator precedence
- Execution engine with pipelines, redirects, logical operators
- Built-in commands: `echo`, `pwd`, `cd`, `exit`, `history`, `clear`, `which`, `type`, `help`, `version`, `alias`, `unalias`, `true`, `false`
- PATH executable lookup
- Command history with file persistence
- TOML configuration system
- Two-line prompt with exit status indicator
- Interactive REPL with Ctrl+C handling
- 81 unit tests across all crates
