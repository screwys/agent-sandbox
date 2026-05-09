#!/usr/bin/env bash
set -euo pipefail

REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/bin" "$tmp/home"

cat >"$tmp/bin/systemctl" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$SYSTEMCTL_LOG"
EOF
chmod +x "$tmp/bin/systemctl"

export HOME="$tmp/home"
export PATH="$tmp/bin:$PATH"
export SYSTEMCTL_LOG="$tmp/systemctl.log"

git -c init.defaultBranch=main init "$tmp/source" >/dev/null
git -C "$tmp/source" config user.email "test@example.invalid"
git -C "$tmp/source" config user.name "test"
printf 'one\n' >"$tmp/source/version.txt"
git -C "$tmp/source" add version.txt
git -C "$tmp/source" commit -m "initial" >/dev/null
git clone --bare "$tmp/source" "$tmp/origin.git" >/dev/null 2>&1
git clone "$tmp/origin.git" "$tmp/install" >/dev/null 2>&1

git clone "$tmp/origin.git" "$tmp/work" >/dev/null 2>&1
git -C "$tmp/work" config user.email "test@example.invalid"
git -C "$tmp/work" config user.name "test"
printf 'two\n' >"$tmp/work/version.txt"
git -C "$tmp/work" commit -am "update" >/dev/null
git -C "$tmp/work" push origin HEAD >/dev/null 2>&1

AGENT_REPO="$tmp/install" "$REPO/bin/agent" self-update --quiet
grep -qx 'two' "$tmp/install/version.txt"
grep -Fxq -- "--user daemon-reload" "$SYSTEMCTL_LOG"

printf 'local change\n' >"$tmp/install/version.txt"
printf 'three\n' >"$tmp/work/version.txt"
git -C "$tmp/work" commit -am "second update" >/dev/null
git -C "$tmp/work" push origin HEAD >/dev/null 2>&1

AGENT_REPO="$tmp/install" "$REPO/bin/agent" self-update --quiet
grep -qx 'local change' "$tmp/install/version.txt"

printf 'self-update tests passed\n'
