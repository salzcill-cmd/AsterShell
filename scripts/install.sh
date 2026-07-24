#!/usr/bin/env bash
set -euo pipefail

echo "Installing AsterShell..."

if ! command -v cargo &> /dev/null; then
    echo "Error: cargo is not installed." >&2
    echo "Install Rust and Cargo via https://rustup.rs" >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cargo install --path "$PROJECT_DIR/aster"

echo "AsterShell installed successfully."
echo "Run 'aster' to start."
