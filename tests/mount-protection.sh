#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
PROJECTS_DIR="${AGENT_PROJECTS_DIR:-$HOME/Projects}"

case "$REPO/" in
    "$PROJECTS_DIR"/*)
        ;;
    *)
        printf 'skipping: repo is not under AGENT_PROJECTS_DIR (%s)\n' "$PROJECTS_DIR"
        exit 0
        ;;
esac

SIBLING_DIR="$(mktemp -d "$PROJECTS_DIR/agent-sandbox-mount-test.XXXXXX")"
trap 'rm -rf "$SIBLING_DIR" "$REPO/.mount-protection-test"' EXIT

assert_repo_write_blocked() {
    local mode="$1"

    AGENT_SANDBOX="$mode" "$REPO/bin/agent" exec env TEST_REPO="$REPO" bash -lc '
set -euo pipefail
target="$TEST_REPO/.mount-protection-test"
if touch "$target" 2>/tmp/repo-touch.err; then
    rm -f "$target"
    printf "repo_write=unexpected_success\n" >&2
    exit 10
fi
printf "repo_write=blocked\n"
'
}

assert_repo_write_blocked_with_extra_mount() {
    local mode="$1"
    local extra_mount="$2"

    AGENT_SANDBOX="$mode" AGENT_EXTRA_MOUNTS="$extra_mount" \
        "$REPO/bin/agent" exec env TEST_REPO="$REPO" bash -lc '
set -euo pipefail
target="$TEST_REPO/.mount-protection-test"
if touch "$target" 2>/tmp/repo-touch.err; then
    rm -f "$target"
    printf "repo_write=unexpected_success\n" >&2
    exit 10
fi
printf "repo_write=blocked\n"
'
}

assert_sibling_writable() {
    local mode="$1"

    AGENT_SANDBOX="$mode" "$REPO/bin/agent" exec env TEST_SIBLING="$SIBLING_DIR" bash -lc '
set -euo pipefail
target="$TEST_SIBLING/.mount-protection-sibling"
touch "$target"
rm -f "$target"
printf "sibling_write=ok\n"
'
}

assert_repo_write_blocked strict
assert_repo_write_blocked comfortable
assert_repo_write_blocked_with_extra_mount comfortable "$PROJECTS_DIR"
assert_sibling_writable strict
assert_sibling_writable comfortable

printf 'mount protection tests passed\n'
