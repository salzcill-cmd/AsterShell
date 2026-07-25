#!/usr/bin/env bash
set -euo pipefail

ASTER_BIN="aster"
ASTER_URL=""
INSTALL_DIR="/usr/local/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${CYAN}[*]${NC} $1"; }
ok()    { echo -e "${GREEN}[✓]${NC} $1"; }
warn()  { echo -e "${YELLOW}[!]${NC} $1"; }
err()   { echo -e "${RED}[✗]${NC} $1" >&2; }

detect_platform() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)  os="unknown-linux-gnu" ;;
        Darwin) os="apple-darwin" ;;
        *)      err "Unsupported OS: $os"; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)             err "Unsupported arch: $arch"; exit 1 ;;
    esac

    echo "${arch}-${os}"
}

download_binary() {
    local platform="$1"
    local tag="v0.4.0"
    local ext="tar.gz"
    [[ "$platform" == *"windows"* ]] && ext="zip"

    local url="https://github.com/salzcill-cmd/AsterShell/releases/download/${tag}/aster-${platform}.${ext}"

    info "Downloading aster for ${platform}..."
    if command -v curl &>/dev/null; then
        curl -sL "$url" | tar xz -C /tmp/
    elif command -v wget &>/dev/null; then
        wget -qO- "$url" | tar xz -C /tmp/
    else
        err "Neither curl nor wget found. Install one first."
        exit 1
    fi

    if [[ ! -f /tmp/aster ]]; then
        err "Download failed — binary not found."
        exit 1
    fi
}

build_from_source() {
    info "Building from source..."

    if ! command -v cargo &>/dev/null; then
        err "Rust/Cargo not installed."
        echo "  Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi

    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local project_dir
    project_dir="$(dirname "$script_dir")"

    cargo build --release --manifest-path "$project_dir/Cargo.toml" --bin aster
    cp "$project_dir/target/release/aster" /tmp/aster
}

install_binary() {
    local binary="$1"

    if [[ ! -f "$binary" ]]; then
        err "Binary not found at $binary"
        exit 1
    fi

    chmod +x "$binary"

    if [[ -w "$INSTALL_DIR" ]]; then
        mv "$binary" "$INSTALL_DIR/$ASTER_BIN"
    else
        info "Installing to $INSTALL_DIR (sudo required)..."
        sudo mv "$binary" "$INSTALL_DIR/$ASTER_BIN"
    fi

    ok "Installed: $INSTALL_DIR/$ASTER_BIN"
}

setup_path() {
    local cargo_bin="$HOME/.cargo/bin"
    local rc_file=""

    # Find the user's shell rc file
    case "$(basename "${SHELL:-/bin/bash}")" in
        bash)  rc_file="$HOME/.bashrc" ;;
        zsh)   rc_file="$HOME/.zshrc" ;;
        fish)  rc_file="$HOME/.config/fish/config.fish" ;;
        *)     rc_file="$HOME/.profile" ;;
    esac

    # Check if cargo/bin is already in PATH
    if echo "$PATH" | tr ':' '\n' | grep -qx "$cargo_bin"; then
        return 0
    fi

    # Check if already in rc file
    if [[ -f "$rc_file" ]] && grep -q 'cargo/bin' "$rc_file"; then
        return 0
    fi

    warn "Adding ~/.cargo/bin to PATH in $rc_file"
    if [[ "$rc_file" == *"fish"* ]]; then
        echo "set -gx PATH \$HOME/.cargo/bin \$PATH" >> "$rc_file"
    else
        echo '' >> "$rc_file"
        echo '# Rust/Cargo' >> "$rc_file"
        echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> "$rc_file"
    fi
}

set_as_default() {
    local aster_path="$INSTALL_DIR/$ASTER_BIN"

    # Verify the binary exists and is executable
    if [[ ! -x "$aster_path" ]]; then
        # Check cargo bin
        if [[ -x "$HOME/.cargo/bin/$ASTER_BIN" ]]; then
            aster_path="$HOME/.cargo/bin/$ASTER_BIN"
        else
            err "aster binary not found"
            return 1
        fi
    fi

    # Check if aster is already the default shell
    if [[ "$(basename "${SHELL:-}")" == "$ASTER_BIN" ]]; then
        ok "AsterShell is already your default shell."
        return 0
    fi

    # Try to copy to /usr/local/bin first (preferred)
    if [[ "$aster_path" != "$INSTALL_DIR/$ASTER_BIN" ]]; then
        if sudo cp "$aster_path" "$INSTALL_DIR/$ASTER_BIN" 2>/dev/null; then
            sudo chmod +x "$INSTALL_DIR/$ASTER_BIN"
            aster_path="$INSTALL_DIR/$ASTER_BIN"
            ok "Copied to $INSTALL_DIR/$ASTER_BIN"
        fi
    fi

    # Add to /etc/shells if not present
    if ! grep -qx "$aster_path" /etc/shells 2>/dev/null; then
        info "Adding $aster_path to /etc/shells..."
        if echo "$aster_path" | sudo tee -a /etc/shells >/dev/null 2>&1; then
            ok "Added to /etc/shells"
        else
            warn "Could not add to /etc/shells (sudo required)."
            warn "Try manually: echo '$aster_path' | sudo tee -a /etc/shells"
            return 1
        fi
    fi

    # Change default shell
    info "Changing default shell to AsterShell..."
    if chsh -s "$aster_path"; then
        ok "Default shell changed to AsterShell"
    else
        err "Failed to change shell. Try manually:"
        echo "  chsh -s $aster_path"
        return 1
    fi

    # Handle kitty terminal — it reads its own config, not /etc/passwd
    if command -v kitty &>/dev/null || [[ -d "$HOME/.config/kitty" ]]; then
        local kitty_conf="$HOME/.config/kitty/kitty.conf"
        mkdir -p "$HOME/.config/kitty"
        if [[ -f "$kitty_conf" ]] && grep -q '^shell ' "$kitty_conf"; then
            # Replace existing shell line
            sed -i "s|^shell .*|shell $aster_path|" "$kitty_conf"
        else
            echo "shell $aster_path" >> "$kitty_conf"
        fi
        ok "Updated kitty config: $kitty_conf"
    fi

    warn "Quit ALL terminal windows (not just new tab), then reopen."
}

print_usage() {
    cat <<'EOF'
AsterShell Installer

Usage:
  ./install.sh              Install AsterShell (download or build)
  ./install.sh --set-default  Install + set as default shell
  ./install.sh --build       Build from source only
  --help                    Show this help

Examples:
  ./install.sh                    # install only
  ./install.sh --set-default      # install + change default shell

EOF
}

# ===========================================================================
# Main
# ===========================================================================

ACTION="install"

for arg in "$@"; do
    case "$arg" in
        --set-default) ACTION="set-default" ;;
        --build)       ACTION="build" ;;
        --help|-h)     print_usage; exit 0 ;;
        *)             err "Unknown option: $arg"; print_usage; exit 1 ;;
    esac
done

echo ""
echo "  _          ____  ___ __  __"
echo " / \   _ __ / ___||__ _|  \\/  |"
echo "/ _ \ | '__\\___ \\ / _ \\ |\\/| |"
echo "/ ___ \\| |   ___) |  __/ |  | |"
echo "/_/   \\_\\_|  |____/ \\___|_|  |_|"
echo ""
echo "  Installer v0.4.0"
echo ""

# Step 1: Get the binary
if [[ "$ACTION" == "build" ]]; then
    build_from_source
else
    platform="$(detect_platform)"
    if download_binary "$platform" 2>/dev/null; then
        ok "Downloaded pre-built binary"
    else
        warn "Pre-built binary not available for $platform, building from source..."
        build_from_source
    fi
fi

# Step 2: Install
install_binary /tmp/aster
rm -f /tmp/aster

# Step 3: Setup PATH
setup_path

# Step 4: Set as default (if requested)
if [[ "$ACTION" == "set-default" ]]; then
    set_as_default
fi

echo ""
ok "Done! Run 'aster' to start."
echo ""
