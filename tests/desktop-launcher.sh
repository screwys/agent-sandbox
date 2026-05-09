#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

assert_contains() {
    local haystack="$1"
    local needle="$2"
    case "$haystack" in
        *"$needle"*)
            ;;
        *)
            printf 'expected output to contain: %s\n' "$needle" >&2
            printf 'actual output:\n%s\n' "$haystack" >&2
            exit 1
            ;;
    esac
}

home="$tmp/home"
root="$tmp/root"
mkdir -p \
    "$home/.local/bin" \
    "$root/usr/bin" \
    "$root/usr/share/icons/hicolor/512x512/apps" \
    "$root/usr/lib/electron39" \
    "$root/usr/lib/openai-codex-desktop/content/webview" \
    "$root/usr/lib/openai-codex-desktop/resources"

printf '#!/usr/bin/env bash\nexit 0\n' >"$home/.local/bin/agent-codex"
chmod +x "$home/.local/bin/agent-codex"

printf '#!/usr/bin/env bash\nexit 0\n' >"$root/usr/lib/electron39/electron"
chmod +x "$root/usr/lib/electron39/electron"
printf 'icon\n' >"$root/usr/share/icons/hicolor/512x512/apps/codex-desktop.png"
printf 'asar\n' >"$root/usr/lib/openai-codex-desktop/resources/app.asar"
printf '<!doctype html>\n' >"$root/usr/lib/openai-codex-desktop/content/webview/index.html"

cat >"$root/usr/bin/codex-desktop" <<'EOF'
#!/usr/bin/env bash
export CODEX_CLI_PATH="${CODEX_CLI_PATH:-$(command -v codex || true)}"
export ELECTRON_RENDERER_URL="${ELECTRON_RENDERER_URL:-http://localhost:5175/}"
python - 5175 "${webview_dir}" "${ready_file}" "${fail_file}" >/dev/null 2>&1
exec /usr/lib/electron39/electron /usr/lib/openai-codex-desktop/resources/app.asar "$@"
EOF
chmod +x "$root/usr/bin/codex-desktop"

detect="$(HOME="$home" AGENT_REPO="$REPO" AGENT_DESKTOP_ROOT="$root" "$REPO/bin/agent" desktop detect codex)"
assert_contains "$detect" "adapter: codex"
assert_contains "$detect" "status: experimental"
assert_contains "$detect" "layout: system"
assert_contains "$detect" "electron: $root/usr/lib/electron39/electron"
assert_contains "$detect" "webview port: 5176"
assert_contains "$detect" "electron: host-native"

HOME="$home" AGENT_REPO="$REPO" AGENT_DESKTOP_ROOT="$root" "$REPO/bin/agent" desktop validate codex >/dev/null
HOME="$home" AGENT_REPO="$REPO" AGENT_DESKTOP_ROOT="$root" "$REPO/bin/agent" desktop install codex >/dev/null

wrapper="$home/.local/bin/codex-desktop-sandboxed"
desktop="$home/.local/share/applications/codex-sandboxed.desktop"

test -x "$wrapper"
test -f "$desktop"
grep -Fq "$home/.local/bin/agent-codex" "$wrapper"
grep -Fq "$home/.config/codex-sandboxed" "$wrapper"
grep -Fq "127.0.0.1" "$wrapper"
grep -Fq "5176" "$wrapper"
grep -Fq "$root/usr/lib/electron39/electron" "$wrapper"
grep -Fq "AGENT_CODEX_APP_SERVER_OPEN_AUTH_URL" "$wrapper"
grep -Fq "AGENT_CODEX_DESKTOP_APP_NAME" "$wrapper"
grep -Fq "codex-desktop-name-shim.js" "$wrapper"
grep -Fq 'exports.app.setName = function(_name)' "$wrapper"
grep -Fq "NODE_OPTIONS" "$wrapper"
grep -Fq -- "--app-id=codex-sandboxed --class=codex-sandboxed" "$wrapper"
test -f "$home/.local/share/icons/hicolor/512x512/apps/codex-sandboxed.png"
grep -q '^Name=Codex (Sandboxed)$' "$desktop"
grep -q "^Exec=$wrapper %U$" "$desktop"
grep -q '^Icon=codex-sandboxed$' "$desktop"
grep -q '^StartupWMClass=codex-desktop$' "$desktop"

user_home="$tmp/user-home"
user_appdir="$user_home/.local/opt/codex-desktop-linux/codex-app"
mkdir -p \
    "$user_home/.local/bin" \
    "$user_appdir/.codex-linux" \
    "$user_appdir/content/webview" \
    "$user_appdir/resources"

printf '#!/usr/bin/env bash\nexit 0\n' >"$user_home/.local/bin/agent-codex"
chmod +x "$user_home/.local/bin/agent-codex"
printf '#!/usr/bin/env bash\nexit 0\n' >"$user_appdir/electron"
chmod +x "$user_appdir/electron"
printf 'icon\n' >"$user_appdir/.codex-linux/codex-desktop.png"
printf 'asar\n' >"$user_appdir/resources/app.asar"
printf '<!doctype html>\n' >"$user_appdir/content/webview/index.html"
cat >"$user_appdir/start.sh" <<'EOF'
#!/usr/bin/env bash
CODEX_LINUX_APP_ID=Codex
CODEX_LINUX_APP_DISPLAY_NAME=Codex
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
[ "$script_dir" = "$EXPECTED_APPDIR" ] || {
  printf 'unexpected script dir: %s\n' "$script_dir" >&2
  exit 42
}
if [ -n "${EXPECTED_ARGS:-}" ]; then
  printf '%s\n' "$@" >"$EXPECTED_ARGS"
fi
exec "$CODEX_CLI_PATH" --version
EOF
chmod +x "$user_appdir/start.sh"

detect_user="$(HOME="$user_home" AGENT_REPO="$REPO" "$REPO/bin/agent" desktop detect codex)"
assert_contains "$detect_user" "status: experimental"
assert_contains "$detect_user" "layout: user-local"
assert_contains "$detect_user" "electron: $user_appdir/electron"

HOME="$user_home" AGENT_REPO="$REPO" "$REPO/bin/agent" desktop validate codex >/dev/null
HOME="$user_home" AGENT_REPO="$REPO" "$REPO/bin/agent" desktop install codex >/dev/null
user_wrapper="$user_home/.local/bin/codex-desktop-sandboxed"
test -x "$user_wrapper"
grep -Fq 'layout="user-local"' "$user_wrapper"
grep -Fq "start_sh=\"$user_appdir/start.sh\"" "$user_wrapper"
grep -Fq "CODEX_WEBVIEW_PORT=" "$user_wrapper"
grep -Fq "AGENT_CODEX_DESKTOP_LD_PRELOAD-" "$user_wrapper"
grep -Fq "CODEX_LINUX_APP_ID=codex-sandboxed" "$user_wrapper"
grep -Fq "CODEX_LINUX_APP_DISPLAY_NAME=CodexSandboxed" "$user_wrapper"
grep -Fq "AGENT_CODEX_DESKTOP_APP_NAME" "$user_wrapper"
grep -Fq "codex-desktop-name-shim.js" "$user_wrapper"
test -f "$user_home/.local/share/icons/hicolor/512x512/apps/codex-sandboxed.png"
EXPECTED_APPDIR="$user_appdir" "$user_wrapper" >/dev/null
EXPECTED_ARGS="$tmp/wayland.args" EXPECTED_APPDIR="$user_appdir" WAYLAND_DISPLAY=wayland-1 "$user_wrapper" >/dev/null
grep -Fxq -- '--wayland' "$tmp/wayland.args"
EXPECTED_ARGS="$tmp/x11.args" EXPECTED_APPDIR="$user_appdir" WAYLAND_DISPLAY=wayland-1 "$user_wrapper" --x11 >/dev/null
test "$(sed -n '1p' "$tmp/x11.args")" = "--x11"

custom_home="$tmp/custom-home"
custom_appdir="$tmp/custom-codex"
mkdir -p \
    "$custom_home/.local/bin" \
    "$custom_appdir/content/webview" \
    "$custom_appdir/resources"
printf '#!/usr/bin/env bash\nexit 0\n' >"$custom_home/.local/bin/agent-codex"
chmod +x "$custom_home/.local/bin/agent-codex"
printf '#!/usr/bin/env bash\nexit 0\n' >"$custom_appdir/electron"
chmod +x "$custom_appdir/electron"
printf 'asar\n' >"$custom_appdir/resources/app.asar"
printf '<!doctype html>\n' >"$custom_appdir/content/webview/index.html"

HOME="$custom_home" \
AGENT_REPO="$REPO" \
AGENT_CODEX_DESKTOP_APPDIR="$custom_appdir" \
AGENT_CODEX_DESKTOP_ELECTRON="$custom_appdir/electron" \
    "$REPO/bin/agent" desktop validate codex >/dev/null

HOME="$custom_home" \
AGENT_REPO="$REPO" \
AGENT_CODEX_DESKTOP_APPDIR="$custom_appdir" \
AGENT_CODEX_DESKTOP_ELECTRON="$custom_appdir/electron" \
    "$REPO/bin/agent" desktop install codex >/dev/null
custom_wrapper="$custom_home/.local/bin/codex-desktop-sandboxed"
grep -Fq "appdir=\"$custom_appdir\"" "$custom_wrapper"
grep -Fq "electron=\"$custom_appdir/electron\"" "$custom_wrapper"

bad_root="$tmp/bad-root"
mkdir -p "$bad_root"
if HOME="$home" AGENT_REPO="$REPO" AGENT_DESKTOP_ROOT="$bad_root" "$REPO/bin/agent" desktop validate codex >"$tmp/bad.out" 2>&1; then
    cat "$tmp/bad.out" >&2
    printf 'expected unsupported Codex layout to fail\n' >&2
    exit 1
fi
assert_contains "$(cat "$tmp/bad.out")" "unsupported Codex Desktop package layout"
assert_contains "$(cat "$tmp/bad.out")" "supported layouts"

if HOME="$home" AGENT_REPO="$REPO" "$REPO/bin/agent" desktop install claude >"$tmp/claude.out" 2>&1; then
    cat "$tmp/claude.out" >&2
    printf 'expected unsupported Claude layout to fail\n' >&2
    exit 1
fi
assert_contains "$(cat "$tmp/claude.out")" "unsupported Claude Desktop package layout"

if HOME="$home" AGENT_REPO="$REPO" AGENT_FORCE_CONTAINER=1 "$REPO/bin/agent" desktop detect codex >"$tmp/container.out" 2>&1; then
    cat "$tmp/container.out" >&2
    printf 'expected desktop adapter commands inside container to fail\n' >&2
    exit 1
fi
assert_contains "$(cat "$tmp/container.out")" "host terminal"

printf 'desktop launcher tests passed\n'
