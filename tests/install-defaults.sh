#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p \
    "$tmp/home/.local/bin" \
    "$tmp/home/.local/share/applications" \
    "$tmp/bin" \
    "$tmp/root/usr/bin" \
    "$tmp/root/usr/lib/electron39" \
    "$tmp/root/usr/lib/openai-codex-desktop/content/webview" \
    "$tmp/root/usr/lib/openai-codex-desktop/resources"
cat >"$tmp/bin/podman" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$PODMAN_LOG"
EOF
chmod +x "$tmp/bin/podman"
printf '#!/usr/bin/env bash\nexit 0\n' >"$tmp/root/usr/lib/electron39/electron"
chmod +x "$tmp/root/usr/lib/electron39/electron"
printf 'asar\n' >"$tmp/root/usr/lib/openai-codex-desktop/resources/app.asar"
printf '<!doctype html>\n' >"$tmp/root/usr/lib/openai-codex-desktop/content/webview/index.html"
cat >"$tmp/root/usr/bin/codex-desktop" <<'EOF'
#!/usr/bin/env bash
export CODEX_CLI_PATH="${CODEX_CLI_PATH:-$(command -v codex || true)}"
export ELECTRON_RENDERER_URL="${ELECTRON_RENDERER_URL:-http://localhost:5175/}"
python - 5175 "${webview_dir}" "${ready_file}" "${fail_file}" >/dev/null 2>&1
exec /usr/lib/electron39/electron /usr/lib/openai-codex-desktop/resources/app.asar "$@"
EOF
chmod +x "$tmp/root/usr/bin/codex-desktop"

ln -s "$REPO/bin/agent-codex-desktop" "$tmp/home/.local/bin/agent-codex-desktop"
cat >"$tmp/home/.local/share/applications/agent-codex-desktop.desktop" <<'EOF'
[Desktop Entry]
Exec=agent-codex-desktop %U
EOF

HOME="$tmp/home" AGENT_STATE_DIR="$tmp/state" AGENT_DESKTOP_ROOT="$tmp/root" PODMAN_LOG="$tmp/podman.log" PATH="$tmp/bin:$PATH" \
    "$REPO/scripts/install.sh" >/dev/null

test -L "$tmp/home/.local/bin/agent"
test -L "$tmp/home/.local/bin/agent-codex"
test ! -e "$tmp/home/.local/bin/codex"
test ! -L "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/agent-codex-desktop"
test -x "$tmp/home/.local/bin/codex-desktop-sandboxed"
test ! -e "$tmp/home/.local/share/applications/agent-codex-desktop.desktop"
test -f "$tmp/home/.local/share/applications/codex-sandboxed.desktop"
grep -q '^build ' "$tmp/podman.log"
grep -Fq -- "-f $REPO/Containerfile $REPO" "$tmp/podman.log"
if grep -Fq "$tmp/home/.local/Containerfile" "$tmp/podman.log"; then
    cat "$tmp/podman.log" >&2
    exit 1
fi

HOME="$tmp/home" \
AGENT_STATE_DIR="$tmp/state" \
AGENT_INSTALL_BUILD_IMAGE=0 \
INSTALL_CODEX_AGENT_SHIM=1 \
INSTALL_CODEX_DESKTOP_LAUNCHER=0 \
    "$REPO/scripts/install.sh" >/dev/null

test -L "$tmp/home/.local/bin/codex"
test ! -L "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/agent-codex-desktop"
test -f "$tmp/home/.local/share/applications/codex-sandboxed.desktop"

HOME="$tmp/home" \
AGENT_STATE_DIR="$tmp/state" \
AGENT_INSTALL_BUILD_IMAGE=0 \
AGENT_DESKTOP_ROOT="$tmp/root" \
INSTALL_CODEX_DESKTOP_LAUNCHER=1 \
    "$REPO/scripts/install.sh" >"$tmp/install-desktop.out"

grep -q 'installed Codex Desktop adapter' "$tmp/install-desktop.out"
test ! -L "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/agent-codex-desktop"
test -f "$tmp/home/.local/share/applications/codex-sandboxed.desktop"

HOME="$tmp/home" AGENT_STATE_DIR="$tmp/state" INSTALL_CODEX_DESKTOP_LAUNCHER=0 "$REPO/scripts/install.sh" --no-build-image >/dev/null

test ! -e "$tmp/home/.local/bin/codex"
test ! -L "$tmp/home/.local/bin/agent-codex-desktop"
test ! -e "$tmp/home/.local/bin/agent-codex-desktop"
test -f "$tmp/home/.local/share/applications/codex-sandboxed.desktop"

printf 'install default tests passed\n'
