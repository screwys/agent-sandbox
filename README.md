# Agent Sandbox

A basic terminal agent sandboxing I created for personal use, also usable with [Codex Desktop Linux community package](https://github.com/ilysenko/codex-desktop-linux).

Normally workspace read/write + auto-review (or equivalents) are safer, this is for people who want agent to work fully autonomously with full read/write access, while keeping it on a allow list for permissions instead of a deny list. Agent Sandbox puts the agent in a small Linux container (ubuntu for first class playwright integration) with its own home directory and only the folders you choose to mount. Only git & podman are enough, so this works out of the box on a normal Linux system.


To install, you can use the one-liner below:

```sh
curl -fsSL https://raw.githubusercontent.com/screwys/agent-sandbox/main/scripts/bootstrap.sh | bash
```

That clones the repo to `~/.local/share/agent-sandbox/repo`, installs `agent`
and `agent-codex`, builds the Podman image, starts a daily background
fast-forward update check, and creates a .desktop file `Codex (Sandboxed)` in
case the mentioned app is installed.

Local install from a clone:

```sh
./scripts/install.sh
```

The daily update check is intentionally quiet: it only fast-forwards a clean
install checkout. Local changes or a non-fast-forward update are skipped. Set
`INSTALL_AGENT_AUTO_UPDATE=0` during install if you do not want the timer.

## Use

```sh
agent shell
agent-codex
agent exec rg TODO ~/Projects
```

Test: tell the agent to write `test.txt` in your home. It will land under
`~/.agent-sandbox/home/test.txt`, while the sandboxed agent thinks it has
read/write access to `~/test.txt`.

By default the agent gets:

- its own home at `~/.agent-sandbox/home`
- read/write access to `~/Projects`
- installed launcher code outside `~/Projects`
- no access to your normal home, browser profile, keyring, session D-Bus, or
  window-manager IPC
- a read-only container OS during normal runs
- a narrow host broker for allowlisted `systemctl --user` and `journalctl --user`
  service commands, so project scripts can restart local dev services without
  mounting the full user session bus

## Give Access To A Folder

```sh
agent allow ~/Server/Igloo
agent allow ~/Development/my-app
```

Or edit the config directly:

```sh
agent config edit
agent config open
```

The config lives under:

```text
~/.agent-sandbox/permissions.d/local.env
```

## Reusable Local Scripts

For repeatable setup, put your machine-specific permissions in your own private
repo and install them into `~/.agent-sandbox/permissions.d/`.

Example:

```sh
mkdir -p ~/.agent-sandbox/permissions.d
cat > ~/.agent-sandbox/permissions.d/work.env <<'EOF'
AGENT_EXTRA_MOUNTS="${AGENT_EXTRA_MOUNTS:+$AGENT_EXTRA_MOUNTS:}$HOME/Work/my-app:$HOME/.config/my-tool"
EOF
chmod 600 ~/.agent-sandbox/permissions.d/work.env
```

Or make a private script that writes or copies that file:

```sh
#!/usr/bin/env sh
set -eu

install -d -m 700 "$HOME/.agent-sandbox/permissions.d"
install -m 600 ./agent-sandbox/work.env "$HOME/.agent-sandbox/permissions.d/work.env"
```

The public repo just reads `*.env` files from that directory. Your local paths,
service helpers, and private scripts can stay in your dotfiles or work repo.

## Commands

```sh
agent shell                     # interactive sandbox shell
agent-codex                     # Codex CLI in the sandbox
agent codex                     # same, through the main command
agent sandbox off [duration]    # max 240m, or can write --forever
agent sandbox on
agent exec <command>            # run any command in the sandbox
agent self-update               # fast-forward the installed checkout
agent broker-start              # start the allowlisted host-service broker
agent allow <folder>            # mount another folder read/write
agent config edit               # edit folder access config
agent config open               # open the folder access config directory
agent doctor                    # show basic status

```

The host broker is installed as `agent-sandbox-broker.service`. It only accepts
small allowlisted user-service commands such as restart/status/logs for known
local dev units.

`agent-codex` is explicit on purpose, the installer does not replace your
native `/usr/bin/codex` unless you explicitly tell it to do:

```sh
INSTALL_CODEX_AGENT_SHIM=1 ./scripts/install.sh
```

## Desktop

Codex Desktop itself is not sandboxed, only the codex cli it uses is.

The installer creates `Codex (Sandboxed)` by default when it detects a known
Codex Desktop layout from
[ilysenko/codex-desktop-linux](https://github.com/ilysenko/codex-desktop-linux).
That launcher runs host-native Electron, but points Codex's CLI/app-server
subprocess at:

```text
~/.local/bin/agent-codex
```

It also uses a separate config/profile directory:

```text
~/.config/codex-sandboxed
```

Native Codex Desktop stays native, local chats are **not shared** (annoying, but sync is also risky).

Known layouts:

- system package: `/usr/lib/openai-codex-desktop` plus `/usr/bin/codex-desktop`
- user-local install: `~/.local/opt/codex-desktop-linux/codex-app`

If your distro packages it somewhere else, set `AGENT_CODEX_DESKTOP_APPDIR` and
`AGENT_CODEX_DESKTOP_ELECTRON` before running `agent desktop install codex`.
`AGENT_CODEX_DESKTOP_SYSTEM_LAUNCHER` is only reported for diagnostics; the
sandboxed wrapper launches Electron directly.
