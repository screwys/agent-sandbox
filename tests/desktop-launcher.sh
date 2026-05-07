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
    "$root/usr/lib/electron39" \
    "$root/usr/lib/openai-codex-desktop/content/webview" \
    "$root/usr/lib/openai-codex-desktop/resources"

printf '#!/usr/bin/env bash\nexit 0\n' >"$home/.local/bin/agent-codex"
chmod +x "$home/.local/bin/agent-codex"

printf '#!/usr/bin/env bash\nexit 0\n' >"$root/usr/lib/electron39/electron"
chmod +x "$root/usr/lib/electron39/electron"
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
grep -q '^Name=Codex (Sandboxed)$' "$desktop"
grep -q "^Exec=$wrapper %U$" "$desktop"

bad_root="$tmp/bad-root"
mkdir -p "$bad_root"
if HOME="$home" AGENT_REPO="$REPO" AGENT_DESKTOP_ROOT="$bad_root" "$REPO/bin/agent" desktop validate codex >"$tmp/bad.out" 2>&1; then
    cat "$tmp/bad.out" >&2
    printf 'expected unsupported Codex layout to fail\n' >&2
    exit 1
fi
assert_contains "$(cat "$tmp/bad.out")" "unsupported Codex Desktop package layout"
assert_contains "$(cat "$tmp/bad.out")" "supported exact layout"

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
