#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

export HOME="$tmp/home"
export AGENT_REPO="$REPO"
mkdir -p "$HOME" "$tmp/project one" "$tmp/project-two"

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

out="$("$REPO/bin/agent" allow "$tmp/project one" "$tmp/project-two")"
assert_contains "$out" "allowed: $(readlink -f "$tmp/project one")"
assert_contains "$out" "allowed: $(readlink -f "$tmp/project-two")"

config="$("$REPO/bin/agent" config path)"
test "$config" = "$HOME/.agent-sandbox/permissions.d/local.env"
test -f "$config"
configured="$(
    AGENT_EXTRA_MOUNTS=""
    # shellcheck disable=SC1090
    . "$config"
    printf '%s\n' "$AGENT_EXTRA_MOUNTS"
)"
assert_contains "$configured" "$(readlink -f "$tmp/project one")"
assert_contains "$configured" "$(readlink -f "$tmp/project-two")"

"$REPO/bin/agent" allow "$tmp/project one" >"$tmp/again.out"
grep -q '^already allowed: ' "$tmp/again.out"

if "$REPO/bin/agent" allow "$tmp/missing" >"$tmp/missing.out" 2>&1; then
    cat "$tmp/missing.out" >&2
    printf 'expected missing folder to fail\n' >&2
    exit 1
fi
grep -q 'not a directory' "$tmp/missing.out"

printf 'config helper tests passed\n'
