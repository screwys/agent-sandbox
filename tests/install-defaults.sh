#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/home/.local/bin" "$tmp/home/.local/share/applications"
ln -s "$REPO/bin/agent-codex-desktop" "$tmp/home/.local/bin/agent-codex-desktop"
cat >"$tmp/home/.local/share/applications/agent-codex-desktop.desktop" <<'EOF'
[Desktop Entry]
Exec=agent-codex-desktop %U
EOF

HOME="$tmp/home" AGENT_STATE_DIR="$tmp/state" "$REPO/scripts/install.sh" >/dev/null

test -L "$tmp/home/.local/bin/agent"
test -L "$tmp/home/.local/bin/agent-codex"
test ! -e "$tmp/home/.local/bin/codex"
test ! -L "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/codex-desktop-sandboxed"
test ! -e "$tmp/home/.local/share/applications/agent-codex-desktop.desktop"
test ! -e "$tmp/home/.local/share/applications/codex-sandboxed.desktop"

HOME="$tmp/home" \
AGENT_STATE_DIR="$tmp/state" \
INSTALL_CODEX_AGENT_SHIM=1 \
    "$REPO/scripts/install.sh" >/dev/null

test -L "$tmp/home/.local/bin/codex"
test ! -L "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/share/applications/codex-sandboxed.desktop"

HOME="$tmp/home" \
AGENT_STATE_DIR="$tmp/state" \
INSTALL_CODEX_DESKTOP_LAUNCHER=1 \
    "$REPO/scripts/install.sh" >"$tmp/install-deprecated.out"

grep -q 'agent desktop install codex' "$tmp/install-deprecated.out"
test ! -L "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/share/applications/codex-sandboxed.desktop"

HOME="$tmp/home" AGENT_STATE_DIR="$tmp/state" "$REPO/scripts/install.sh" >/dev/null

test ! -e "$tmp/home/.local/bin/codex"
test ! -L "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/share/applications/codex-sandboxed.desktop"

printf 'install default tests passed\n'
