#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/bin"
cat >"$tmp/bin/agent-host" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$AGENT_HOST_LOG"
EOF
chmod +x "$tmp/bin/agent-host"

export AGENT_HOST_LOG="$tmp/agent-host.log"
export PATH="$REPO/bin/wrappers:$tmp/bin:$PATH"

systemctl --user daemon-reload
systemctl --user restart igloo.service
systemctl --user status igloo.service --no-pager
systemctl --user is-active --quiet igloo.service || true
systemctl --user is-failed --quiet igloo.service || true
journalctl --user -u igloo.service -n 80 --no-pager

grep -Fxq 'service daemon-reload' "$AGENT_HOST_LOG"
grep -Fxq 'service restart igloo.service' "$AGENT_HOST_LOG"
grep -Fxq 'service status igloo.service' "$AGENT_HOST_LOG"
grep -Fxq 'service is-active igloo.service' "$AGENT_HOST_LOG"
grep -Fxq 'service is-failed igloo.service' "$AGENT_HOST_LOG"
grep -Fxq 'service logs igloo.service -n 80' "$AGENT_HOST_LOG"

printf 'host bridge tests passed\n'
