#!/bin/sh
# sscli installer script
# Usage: curl -sSL https://raw.githubusercontent.com/jwcraig/sql-server-cli/main/install.sh | sh
set -eu

REPO="jwcraig/sql-server-cli"
BINARY="sscli"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    printf "${GREEN}info${NC}: %s\n" "$1"
}

warn() {
    printf "${YELLOW}warn${NC}: %s\n" "$1"
}

error() {
    printf "${RED}error${NC}: %s\n" "$1" >&2
    exit 1
}

sha256_file() {
    file_path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file_path" | awk '{print $1}'
        return 0
    fi

    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file_path" | awk '{print $1}'
        return 0
    fi

    error "No SHA256 tool found (need sha256sum or shasum)"
}

# Detect OS and architecture
detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        linux)
            case "$ARCH" in
                x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
                aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
                *) error "Unsupported architecture: $ARCH" ;;
            esac
            EXT="tar.gz"
            ;;
        darwin)
            case "$ARCH" in
                x86_64) TARGET="x86_64-apple-darwin" ;;
                arm64) TARGET="aarch64-apple-darwin" ;;
                *) error "Unsupported architecture: $ARCH" ;;
            esac
            EXT="tar.gz"
            ;;
        msys*|mingw*|cygwin*)
            error "For Windows, please download from: https://github.com/$REPO/releases"
            ;;
        *)
            error "Unsupported OS: $OS"
            ;;
    esac
}

# Get latest version from GitHub API
get_latest_version() {
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

    if [ -z "$VERSION" ] || [ "$VERSION" = "null" ]; then
        error "Failed to fetch latest version. Check your internet connection."
    fi
}

# Download and install
install() {
    ARTIFACT="$BINARY-$TARGET.$EXT"
    URL="https://github.com/$REPO/releases/download/$VERSION/$ARTIFACT"
    CHECKSUMS_URL="https://github.com/$REPO/releases/download/$VERSION/checksums-sha256.txt"
    INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
    TMP_DIR=$(mktemp -d)
    CHECKSUMS_FILE="$TMP_DIR/checksums-sha256.txt"
    ARCHIVE_PATH="$TMP_DIR/archive.$EXT"

    info "Downloading $BINARY $VERSION for $TARGET..."

    if ! curl -fsSL "$URL" -o "$ARCHIVE_PATH"; then
        error "Failed to download $URL"
    fi

    info "Verifying download checksum..."
    if ! curl -fsSL "$CHECKSUMS_URL" -o "$CHECKSUMS_FILE"; then
        error "Failed to download release checksums from $CHECKSUMS_URL"
    fi

    EXPECTED_SHA=$(awk -v wanted="$ARTIFACT" '$2 == wanted { print $1 }' "$CHECKSUMS_FILE" | head -n 1)
    if [ -z "$EXPECTED_SHA" ]; then
        error "No checksum found for $ARTIFACT in checksums-sha256.txt"
    fi

    ACTUAL_SHA=$(sha256_file "$ARCHIVE_PATH")
    if [ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]; then
        error "Checksum mismatch for $ARTIFACT (expected $EXPECTED_SHA, got $ACTUAL_SHA)"
    fi

    info "Extracting..."
    tar xzf "$ARCHIVE_PATH" -C "$TMP_DIR"

    if [ ! -f "$TMP_DIR/$BINARY" ]; then
        error "Archive did not contain expected binary: $BINARY"
    fi

    info "Installing to $INSTALL_DIR..."
    if [ -w "$INSTALL_DIR" ]; then
        mv "$TMP_DIR/$BINARY" "$INSTALL_DIR/"
    else
        warn "Need sudo to install to $INSTALL_DIR"
        sudo mv "$TMP_DIR/$BINARY" "$INSTALL_DIR/"
    fi

    chmod +x "$INSTALL_DIR/$BINARY"

    # Cleanup
    rm -rf "$TMP_DIR"

    info "Successfully installed $BINARY $VERSION to $INSTALL_DIR/$BINARY"
    echo ""
    "$INSTALL_DIR/$BINARY" --version
    echo ""
    info "Run '$BINARY --help' to get started"
}

# Verify installation
verify() {
    if command -v "$BINARY" >/dev/null 2>&1; then
        return 0
    fi

    warn "$BINARY is installed but not in PATH"
    warn "Add $INSTALL_DIR to your PATH, or run:"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
}

main() {
    detect_platform
    get_latest_version
    install
    verify
}

main
