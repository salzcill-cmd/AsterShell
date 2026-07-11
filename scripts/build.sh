#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$ROOT_DIR"

case "${1:-build}" in
    build)
        echo "Building AsterShell (debug)..."
        cargo build
        echo "Binary: target/debug/aster"
        ;;
    release)
        echo "Building AsterShell (release)..."
        cargo build --release
        echo "Binary: target/release/aster"
        ;;
    install)
        echo "Building and installing AsterShell..."
        cargo build --release
        install -Dm755 target/release/aster "${HOME}/.local/bin/aster"
        echo "Installed to ~/.local/bin/aster"
        ;;
    test)
        echo "Running tests..."
        cargo test --workspace
        ;;
    lint)
        echo "Running clippy..."
        cargo clippy --workspace -- -D warnings
        echo "Running fmt check..."
        cargo fmt --all -- --check
        ;;
    clean)
        cargo clean
        ;;
    *)
        echo "Usage: $0 {build|release|install|test|lint|clean}"
        exit 1
        ;;
esac
