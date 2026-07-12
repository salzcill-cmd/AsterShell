#!/usr/bin/env aster
# dev-workflow.sh — Common Rust development workflow helpers
# Run: source examples/dev-workflow.sh

echo "=== AsterShell Dev Workflow ==="

# --- Git shortcuts ---
alias gs="git status"
alias gd="git diff"
alias gl="git log --oneline --graph --decorate -20"
alias gp="git pull --rebase"
alias gpu="git push"
alias gco="git checkout"
alias gcb="git checkout -b"
alias gc="git commit -m"
alias gclean="git clean -fd"
alias gundo="git reset --soft HEAD~1"
alias gwip="git add -A && git commit -m 'WIP'"

# --- Cargo shortcuts ---
alias cb="cargo build"
alias cr="cargo run"
alias ct="cargo test"
alias cbr="cargo build --release"
alias cw="cargo watch -x check -x test"
alias cc="cargo clippy --workspace --all-targets"
alias cf="cargo fmt --all --check"

# --- Quick functions ---
build-release() {
    echo "Building release..."
    cargo build --release
    if [ $? -eq 0 ]; then
        ls -lh target/release/aster
        echo "Build successful!"
    else
        echo "Build failed!"
    fi
}

quick-test() {
    echo "Running tests..."
    cargo test --workspace 2>&1 | tail -5
}

check-all() {
    echo "=== fmt ==="
    cargo fmt --check
    echo ""
    echo "=== clippy ==="
    cargo clippy --workspace --all-targets 2>&1 | tail -5
    echo ""
    echo "=== test ==="
    cargo test --workspace 2>&1 | tail -5
    echo ""
    echo "=== Done ==="
}

new-plugin() {
    local name=$1
    if [ -z "$name" ]; then
        echo "Usage: new-plugin <name>"
        return 1
    fi
    cat <<EOF
name        = "$name"
version     = "0.1.0"
description = "My new plugin"
enabled     = true

[aliases]
# Add your aliases here
EOF
}

echo "Dev workflow loaded!"
echo "Available: gs gd gl gp gpu gco gcb gc gclean gundo gwip"
echo "Available: cb cr ct cbr cw cc cf"
echo "Available: build-release quick-test check-all new-plugin"
