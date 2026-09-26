#!/usr/bin/env sh
set -e

# DELTU Installer Script — https://github.com/ecocee/deltu
# Usage: curl -fsSL https://deltu.dev/install.sh | sh

echo "🚀 Installing DELTU Engine (Make Data Behave)..."

# Detect OS
OS="$(uname -s)"
case "${OS}" in
    Linux*)     PLATFORM="linux";;
    Darwin*)    PLATFORM="macos";;
    *)          echo "❌ Unsupported OS: ${OS}"; exit 1;;
esac

# Detect Architecture
ARCH="$(uname -m)"
case "${ARCH}" in
    x86_64)     ARCH="x86_64";;
    arm64|aarch64) ARCH="arm64";;
    *)          echo "❌ Unsupported Architecture: ${ARCH}"; exit 1;;
esac

echo "✓ Target platform: ${PLATFORM}/${ARCH}"

# Installation directory
INSTALL_DIR="${HOME}/.deltu/bin"
mkdir -p "${INSTALL_DIR}"

BINARY_PATH="${INSTALL_DIR}/deltu"

# If local cargo binary exists, copy it as a fallback
if [ -f "./target/release/deltu" ]; then
    echo "✓ Using local release binary..."
    cp "./target/release/deltu" "${BINARY_PATH}"
else
    # Download latest release binary (Placeholder URL for GitHub releases)
    DOWNLOAD_URL="https://github.com/ecocee/deltu/releases/latest/download/deltu-${PLATFORM}-${ARCH}.tar.gz"
    echo "📥 Downloading from ${DOWNLOAD_URL}..."
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "${DOWNLOAD_URL}" | tar -xz -C "${INSTALL_DIR}" 2>/dev/null || {
            echo "⚠️ Could not download prebuilt release binary. Falling back to 'cargo build --release'..."
            if command -v cargo >/dev/null 2>&1; then
                cargo build --release
                cp "./target/release/deltu" "${BINARY_PATH}"
            else
                echo "❌ Cargo not found. Please install Rust & Cargo or build from source."
                exit 1
            fi
        }
    fi
fi

chmod +x "${BINARY_PATH}"

echo "──────────────────────────────────────────────────"
echo "✅ DELTU successfully installed to ${BINARY_PATH}"
echo ""
echo "To add DELTU to your PATH, add this line to your shell profile (~/.bashrc or ~/.zshrc):"
echo "  export PATH=\"\${HOME}/.deltu/bin:\${PATH}\""
echo ""
echo "Quickstart:"
echo "  deltu demo"
echo "  deltu init"
echo "  deltu run"
echo "──────────────────────────────────────────────────"
