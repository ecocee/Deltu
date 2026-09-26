#!/usr/bin/env sh
set -e

# DELTU Uninstaller Script — https://github.com/ecocee/deltu

echo "🗑️ Uninstalling DELTU Engine..."

# Stop any running background deltu instances if deltu binary exists
if command -v deltu >/dev/null 2>&1; then
    echo " Stopping active DELTU instances..."
    deltu down 2>/dev/null || true
fi

# Remove installation directory
INSTALL_DIR="${HOME}/.deltu"
if [ -d "${INSTALL_DIR}" ]; then
    echo " Removing ${INSTALL_DIR}..."
    rm -rf "${INSTALL_DIR}"
fi

# Remove system paths if binary was manually placed there
for bin in /usr/local/bin/deltu /usr/bin/deltu; do
    if [ -f "$bin" ]; then
        echo " Removing $bin..."
        rm -f "$bin" 2>/dev/null || sudo rm -f "$bin"
    fi
done

# Clean up temporary PID & log files
rm -f /tmp/deltu.pid /tmp/deltu.log

echo "──────────────────────────────────────────────────"
echo "✅ DELTU has been successfully uninstalled."
echo "Note: Any PATH entries in ~/.bashrc or ~/.zshrc can now be removed."
echo "──────────────────────────────────────────────────"
