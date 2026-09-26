#!/usr/bin/env sh
set -e

# DELTU Installer Script — https://github.com/ecocee/deltu
# Usage: curl -fsSL https://deltu.dev/install.sh | sh

echo "🚀 Installing DELTU Engine (Make Data Behave)..."

# Detect OS & Architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux*)
        case "${ARCH}" in
            x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
            aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
            *) echo "❌ Unsupported Linux architecture: ${ARCH}"; exit 1 ;;
        esac
        ;;
    Darwin*)
        case "${ARCH}" in
            arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
            x86_64) echo "❌ macOS x86_64 is not currently pre-built. Please install Rust and run: cargo build --release"; exit 1 ;;
            *) echo "❌ Unsupported macOS architecture: ${ARCH}"; exit 1 ;;
        esac
        ;;
    *)
        echo "❌ Unsupported OS: ${OS}"
        exit 1
        ;;
esac

echo "✓ Target platform: ${TARGET}"

# Installation directory
INSTALL_DIR="${HOME}/.deltu/bin"
mkdir -p "${INSTALL_DIR}"

BINARY_PATH="${INSTALL_DIR}/deltu"

# If local cargo binary exists, copy it as a fallback
if [ -f "./target/release/deltu" ]; then
    echo "✓ Using local release binary..."
    cp "./target/release/deltu" "${BINARY_PATH}"
else
    # Download latest release binary
    DOWNLOAD_URL="https://github.com/ecocee/Deltu/releases/latest/download/deltu-${TARGET}"
    echo "📥 Downloading from ${DOWNLOAD_URL}..."
    if command -v curl >/dev/null 2>&1; then
        HTTP_CODE=$(curl -fsSL -w "%{http_code}" -o "${BINARY_PATH}.tmp" "${DOWNLOAD_URL}")
        if [ "$HTTP_CODE" = "200" ] || [ "$HTTP_CODE" = "301" ] || [ "$HTTP_CODE" = "302" ]; then
            mv "${BINARY_PATH}.tmp" "${BINARY_PATH}"
        else
            rm -f "${BINARY_PATH}.tmp"
            echo "⚠️ Could not download prebuilt release binary (HTTP ${HTTP_CODE})."
            echo "Falling back to 'cargo build --release'..."
            if command -v cargo >/dev/null 2>&1; then
                cargo build --release
                cp "./target/release/deltu" "${BINARY_PATH}"
            else
                echo "❌ Cargo not found. Please install Rust & Cargo or build from source."
                exit 1
            fi
        fi
    else
        echo "❌ curl is required to download the binary."
        exit 1
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
