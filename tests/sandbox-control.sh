#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

export HOME="$TMPDIR/home"
export AGENT_REPO="$REPO"
export AGENT_HOST_CONTROL_TEST=1
mkdir -p "$HOME"

run_agent() {
    "$REPO/bin/agent" "$@"
}

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

status="$(run_agent sandbox status)"
assert_contains "$status" "sandbox: on (strict)"

status="$(AGENT_SANDBOX=comfortable run_agent sandbox status)"
assert_contains "$status" "sandbox: on (comfortable)"

run_agent sandbox off 2m >/dev/null
lease_file="$HOME/.agent-sandbox/host-control/sandbox-disabled-until"
test -f "$lease_file"

status="$(run_agent sandbox status)"
assert_contains "$status" "sandbox: off"

run_agent sandbox off 240m >/dev/null
status="$(run_agent sandbox status)"
assert_contains "$status" "sandbox: off"
run_agent sandbox on >/dev/null

if run_agent sandbox off 241m >/tmp/agent-sandbox-too-long.out 2>&1; then
    cat /tmp/agent-sandbox-too-long.out >&2
    printf 'expected sandbox off 241m to fail\n' >&2
    exit 1
fi
assert_contains "$(cat /tmp/agent-sandbox-too-long.out)" "maximum is 240m"

run_agent sandbox off --forever >/dev/null
lease_contents="$(cat "$lease_file")"
test "$lease_contents" = "forever"
status="$(run_agent sandbox status)"
assert_contains "$status" "sandbox: off (forever)"

run_agent sandbox on >/dev/null
status="$(run_agent sandbox status)"
assert_contains "$status" "sandbox: on (strict)"

run_agent sandbox disable 1m >/dev/null
status="$(run_agent sandbox status)"
assert_contains "$status" "sandbox: off"
run_agent sandbox enable >/dev/null
status="$(run_agent sandbox status)"
assert_contains "$status" "sandbox: on (strict)"

if AGENT_SANDBOX=disabled run_agent exec true >/tmp/agent-sandbox-disabled.out 2>&1; then
    cat /tmp/agent-sandbox-disabled.out >&2
    printf 'expected disabled mode while sandbox is on to fail\n' >&2
    exit 1
fi
assert_contains "$(cat /tmp/agent-sandbox-disabled.out)" "sandbox is on"

if AGENT_FORCE_CONTAINER=1 run_agent sandbox off 1m >/tmp/agent-sandbox-container.out 2>&1; then
    cat /tmp/agent-sandbox-container.out >&2
    printf 'expected sandbox off inside container to fail\n' >&2
    exit 1
fi
assert_contains "$(cat /tmp/agent-sandbox-container.out)" "host terminal"

printf 'sandbox control tests passed\n'
