#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO="${AGENT_REPO:-$(cd "$SCRIPT_DIR/.." && pwd -P)}"
BUILD_IMAGE=1
SETUP_ANDROID_SDK=0
INSTALL_CODEX_AGENT_SHIM="${INSTALL_CODEX_AGENT_SHIM:-0}"
INSTALL_CODEX_DESKTOP_LAUNCHER="${INSTALL_CODEX_DESKTOP_LAUNCHER:-auto}"
INSTALL_AGENT_HOST_BRIDGE="${INSTALL_AGENT_HOST_BRIDGE:-1}"
INSTALL_AGENT_AUTO_UPDATE="${INSTALL_AGENT_AUTO_UPDATE:-1}"
AGENT_STATE_DIR="${AGENT_STATE_DIR:-$HOME/.agent-sandbox}"

usage() {
    cat <<'EOF'
usage: install.sh [--no-build-image] [--setup-android-sdk]

options:
  --no-build-image       only link commands, do not build the Podman image
  --setup-android-sdk    install Android SDK into the agent home
  -h, --help             show this help

environment:
  AGENT_REPO                  repo path (default: parent of this script)
  AGENT_STATE_DIR             state/config dir (default: ~/.agent-sandbox)
  INSTALL_CODEX_AGENT_SHIM    install ~/.local/bin/codex shim (default: 0)
  INSTALL_CODEX_DESKTOP_LAUNCHER
                              auto, 1, or 0 (default: auto)
  INSTALL_AGENT_HOST_BRIDGE   install and start host broker (default: 1)
  INSTALL_AGENT_AUTO_UPDATE   install and start daily update timer (default: 1)
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

if [ -n "${AGENT_INSTALL_BUILD_IMAGE:-}" ]; then
    if truthy "$AGENT_INSTALL_BUILD_IMAGE"; then
        BUILD_IMAGE=1
    else
        BUILD_IMAGE=0
    fi
fi

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

remove_owned_symlink() {
    local dest="$1"
    local src="$2"
    local target

    target="$(readlink "$dest" 2>/dev/null || true)"
    if [ -L "$dest" ] && { [ "$target" = "$src" ] || [ "$(readlink -f "$dest" 2>/dev/null || true)" = "$src" ]; }; then
        rm -f "$dest"
        echo "REMOVE: $dest"
    fi
}

remove_owned_desktop_entry() {
    local dest="$1"

    if [ -f "$dest" ] && grep -q '^Exec=agent-codex-desktop %U$' "$dest"; then
        rm -f "$dest"
        echo "REMOVE: $dest"
    fi
}

while [ $# -gt 0 ]; do
    case "$1" in
        --build-image)
            BUILD_IMAGE=1
            shift
            ;;
        --no-build-image)
            BUILD_IMAGE=0
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
    "$REPO/bin/agent-broker" \
    "$REPO/bin/agent-codex" \
    "$REPO/bin/agent-host" \
    "$REPO/bin/codex" \
    "$REPO/bin/wrappers/journalctl" \
    "$REPO/bin/wrappers/systemctl" \
    "$REPO/scripts/bootstrap.sh" \
    "$REPO/scripts/setup-android-sdk"

install -d -m 700 "$AGENT_STATE_DIR" "$AGENT_STATE_DIR/permissions.d"

link_file bin/agent "$HOME/.local/bin/agent"
link_file bin/agent-broker "$HOME/.local/bin/agent-broker"
link_file bin/agent-codex "$HOME/.local/bin/agent-codex"
link_file bin/agent-host "$HOME/.local/bin/agent-host"

if truthy "$INSTALL_CODEX_AGENT_SHIM"; then
    link_file bin/codex "$HOME/.local/bin/codex"
    echo "CONFIG: codex shim -> agent codex"
else
    remove_owned_symlink "$HOME/.local/bin/codex" "$REPO/bin/codex"
    echo "SKIP: codex PATH shim disabled by INSTALL_CODEX_AGENT_SHIM=$INSTALL_CODEX_AGENT_SHIM"
fi

remove_owned_symlink "$HOME/.local/bin/agent-codex-desktop" "$REPO/bin/agent-codex-desktop"
remove_owned_desktop_entry "$HOME/.local/share/applications/agent-codex-desktop.desktop"

link_file systemd/agent-sandbox-broker.service "$HOME/.config/systemd/user/agent-sandbox-broker.service"
link_file systemd/agent-sandbox-update.service "$HOME/.config/systemd/user/agent-sandbox-update.service"
link_file systemd/agent-sandbox-update.timer "$HOME/.config/systemd/user/agent-sandbox-update.timer"
if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload >/dev/null 2>&1 || true
    if truthy "$INSTALL_AGENT_HOST_BRIDGE"; then
        systemctl --user enable --now agent-sandbox-broker.service >/dev/null 2>&1 ||
            echo "WARN: could not enable agent-sandbox-broker.service"
    else
        echo "SKIP: host broker disabled by INSTALL_AGENT_HOST_BRIDGE=$INSTALL_AGENT_HOST_BRIDGE"
    fi
    if truthy "$INSTALL_AGENT_AUTO_UPDATE"; then
        systemctl --user enable --now agent-sandbox-update.timer >/dev/null 2>&1 ||
            echo "WARN: could not enable agent-sandbox-update.timer"
    else
        systemctl --user disable --now agent-sandbox-update.timer >/dev/null 2>&1 || true
        echo "SKIP: auto-update disabled by INSTALL_AGENT_AUTO_UPDATE=$INSTALL_AGENT_AUTO_UPDATE"
    fi
else
    echo "WARN: systemctl not found; host broker service and update timer were linked but not enabled"
fi

if [ "$BUILD_IMAGE" -eq 1 ]; then
    "$HOME/.local/bin/agent" build-image
fi

case "${INSTALL_CODEX_DESKTOP_LAUNCHER,,}" in
    0|no|false|off|never)
        echo "SKIP: Codex desktop launcher disabled by INSTALL_CODEX_DESKTOP_LAUNCHER=$INSTALL_CODEX_DESKTOP_LAUNCHER"
        ;;
    auto)
        desktop_install_log="$(mktemp)"
        if "$HOME/.local/bin/agent" desktop install codex >"$desktop_install_log" 2>&1; then
            cat "$desktop_install_log"
        else
            echo "SKIP: Codex desktop launcher not installed; unsupported local Codex Desktop layout"
        fi
        rm -f "$desktop_install_log"
        ;;
    *)
        if truthy "$INSTALL_CODEX_DESKTOP_LAUNCHER"; then
            "$HOME/.local/bin/agent" desktop install codex
        else
            echo "unknown INSTALL_CODEX_DESKTOP_LAUNCHER value: $INSTALL_CODEX_DESKTOP_LAUNCHER" >&2
            exit 2
        fi
        ;;
esac

if [ "$SETUP_ANDROID_SDK" -eq 1 ]; then
    "$HOME/.local/bin/agent" setup-android-sdk
fi

echo "Agent sandbox installed."
