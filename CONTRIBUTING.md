# Contributing to AsterShell

Thank you for considering contributing!

## Getting Started

1. Fork the repository
2. Clone your fork
3. Create a feature branch: `git checkout -b my-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Run lints: `cargo clippy`
7. Format: `cargo fmt`
8. Commit and push
9. Open a Pull Request

## Development Setup

### Prerequisites

- Rust stable (latest recommended)
- Linux (x86_64)

### Build

```bash
cargo build
cargo build --release
```

### Test

```bash
cargo test
```

### Lint & Format

```bash
cargo clippy -- -D warnings
cargo fmt --check
```

## Code Style

- **Rust Edition 2024**
- **No unsafe** unless absolutely required
- **Idiomatic Rust** — use `?`, iterators, pattern matching
- **Small modules** — keep files focused
- **Document public APIs** — `///` doc comments on all public items
- **No magic numbers** — use named constants
- **Traits over inheritance** — composition over complexity
- **Consistent naming** — follow Rust conventions

## Commit Messages

Use clear, concise commit messages:
- `feat: add tab completion`
- `fix: handle empty pipeline gracefully`
- `docs: update README`
- `test: add lexer edge cases`
- `refactor: simplify executor dispatch`

## Architecture Guidelines

- **shell-core** is the foundation — all shared types live here
- **No circular dependencies** between crates
- **Each crate** has a single, clear responsibility
- **Public APIs** should be minimal and well-documented
- **Tests** go in `#[cfg(test)] mod tests` at the bottom of each file

## Reporting Issues

- Use the GitHub issue tracker
- Include your Rust version (`rustc --version`)
- Include your OS and kernel version
- Provide a minimal reproduction

## License

By contributing, you agree that your contributions will be licensed under MIT.
