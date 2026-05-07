#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO="${AGENT_REPO:-$(cd "$SCRIPT_DIR/.." && pwd -P)}"
BUILD_IMAGE=0
SETUP_ANDROID_SDK=0
INSTALL_CODEX_AGENT_SHIM="${INSTALL_CODEX_AGENT_SHIM:-1}"
AGENT_STATE_DIR="${AGENT_STATE_DIR:-$HOME/.agent-sandbox}"

usage() {
    cat <<'EOF'
usage: install.sh [--build-image] [--setup-android-sdk]

options:
  --build-image          build localhost/agent-sandbox:latest after linking commands
  --setup-android-sdk    install Android SDK into the agent home
  -h, --help             show this help

environment:
  AGENT_REPO                  repo path (default: parent of this script)
  AGENT_STATE_DIR             state/config dir (default: ~/.agent-sandbox)
  INSTALL_CODEX_AGENT_SHIM    install ~/.local/bin/codex shim (default: 1)
EOF
}

truthy() {
    case "${1,,}" in
        1|yes|true|on|always)
            return 0
            ;;
    esac
    return 1
}

link_file() {
    local rel="$1"
    local dest="$2"
    local src="$REPO/$rel"

    if [ -e "$dest" ] && [ ! -L "$dest" ]; then
        echo "BACKUP: $dest -> ${dest}.bak"
        mv "$dest" "${dest}.bak"
    fi
    mkdir -p "$(dirname "$dest")"
    ln -sf "$src" "$dest"
    echo "LINK: $dest -> $src"
}

while [ $# -gt 0 ]; do
    case "$1" in
        --build-image)
            BUILD_IMAGE=1
            shift
            ;;
        --setup-android-sdk)
            SETUP_ANDROID_SDK=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

[ -f "$REPO/Containerfile" ] || {
    echo "missing Containerfile at $REPO/Containerfile" >&2
    exit 1
}

chmod +x \
    "$REPO/bin/agent" \
    "$REPO/bin/agent-codex" \
    "$REPO/bin/codex" \
    "$REPO/scripts/setup-android-sdk"

install -d -m 700 "$AGENT_STATE_DIR" "$AGENT_STATE_DIR/permissions.d"

link_file bin/agent "$HOME/.local/bin/agent"
link_file bin/agent-codex "$HOME/.local/bin/agent-codex"

if truthy "$INSTALL_CODEX_AGENT_SHIM"; then
    link_file bin/codex "$HOME/.local/bin/codex"
    echo "CONFIG: codex shim -> agent codex"
else
    echo "SKIP: codex PATH shim disabled by INSTALL_CODEX_AGENT_SHIM=$INSTALL_CODEX_AGENT_SHIM"
fi

if [ "$BUILD_IMAGE" -eq 1 ]; then
    "$HOME/.local/bin/agent" build-image
fi

if [ "$SETUP_ANDROID_SDK" -eq 1 ]; then
    "$HOME/.local/bin/agent" setup-android-sdk
fi

echo "Agent sandbox installed."
