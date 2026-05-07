#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"

test -x "$REPO/bin/agent-codex-desktop"
grep -q 'desktop)' "$REPO/bin/agent"
grep -q 'agent-codex-desktop' "$REPO/scripts/install.sh"
test -f "$REPO/share/applications/agent-codex-desktop.desktop"
grep -q '^Exec=agent-codex-desktop %U$' \
    "$REPO/share/applications/agent-codex-desktop.desktop"

printf 'desktop launcher tests passed\n'
