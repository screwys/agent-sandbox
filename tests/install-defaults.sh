#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

HOME="$tmp/home" AGENT_STATE_DIR="$tmp/state" "$REPO/scripts/install.sh" >/dev/null

test -L "$tmp/home/.local/bin/agent"
test -L "$tmp/home/.local/bin/agent-codex"
test -L "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/codex"
test ! -e "$tmp/home/.local/share/applications/agent-codex-desktop.desktop"

HOME="$tmp/home" \
AGENT_STATE_DIR="$tmp/state" \
INSTALL_CODEX_AGENT_SHIM=1 \
INSTALL_CODEX_DESKTOP_LAUNCHER=1 \
    "$REPO/scripts/install.sh" >/dev/null

test -L "$tmp/home/.local/bin/codex"
test -f "$tmp/home/.local/share/applications/agent-codex-desktop.desktop"

HOME="$tmp/home" AGENT_STATE_DIR="$tmp/state" "$REPO/scripts/install.sh" >/dev/null

test ! -e "$tmp/home/.local/bin/codex"
test ! -e "$tmp/home/.local/share/applications/agent-codex-desktop.desktop"

printf 'install default tests passed\n'
