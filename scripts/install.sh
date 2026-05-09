#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo="${AGENT_REPO:-$(cd "$script_dir/.." && pwd -P)}"
real_home="$(getent passwd "$(id -u)" | cut -d: -f6)"
export RUSTUP_HOME="${RUSTUP_HOME:-$real_home/.rustup}"
export CARGO_HOME="${CARGO_HOME:-$real_home/.cargo}"

command -v cargo >/dev/null 2>&1 || {
    echo "missing required command: cargo" >&2
    exit 127
}

cargo build --release --manifest-path "$repo/Cargo.toml"
exec "$repo/target/release/agent-sandbox" install "$@"
