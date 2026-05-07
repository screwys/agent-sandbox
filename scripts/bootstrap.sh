#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${AGENT_SANDBOX_REPO:-https://github.com/screwys/agent-sandbox.git}"
DEST="${AGENT_SANDBOX_DIR:-$HOME/Projects/agent-sandbox}"

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        printf 'missing required command: %s\n' "$1" >&2
        exit 127
    }
}

need_cmd git

if [ -d "$DEST/.git" ]; then
    git -C "$DEST" pull --ff-only
elif [ -e "$DEST" ]; then
    printf 'install destination exists but is not a git repo: %s\n' "$DEST" >&2
    exit 1
else
    mkdir -p "$(dirname "$DEST")"
    git clone "$REPO_URL" "$DEST"
fi

exec "$DEST/scripts/install.sh" "$@"
