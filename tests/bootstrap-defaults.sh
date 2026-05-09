#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/home" "$tmp/bin"

cat >"$tmp/bin/systemctl" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$SYSTEMCTL_LOG"
EOF
chmod +x "$tmp/bin/systemctl"

export SYSTEMCTL_LOG="$tmp/systemctl.log"
export PATH="$tmp/bin:$PATH"

HOME="$tmp/home" \
AGENT_SANDBOX_REPO="$REPO" \
AGENT_INSTALL_BUILD_IMAGE=0 \
INSTALL_CODEX_DESKTOP_LAUNCHER=0 \
INSTALL_AGENT_HOST_BRIDGE=0 \
INSTALL_AGENT_AUTO_UPDATE=0 \
    "$REPO/scripts/bootstrap.sh" >/dev/null

install_repo="$tmp/home/.local/share/agent-sandbox/repo"
test -d "$install_repo/.git"
test -L "$tmp/home/.local/bin/agent"
test "$(readlink "$tmp/home/.local/bin/agent")" = "$install_repo/bin/agent"
test -d "$tmp/home/.agent-sandbox/permissions.d"
test ! -e "$tmp/home/Projects/agent-sandbox"

printf 'bootstrap default tests passed\n'
