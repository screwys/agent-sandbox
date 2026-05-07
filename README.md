# Agent Sandbox

A small Podman sandbox for coding agents.

Default behavior:

- persistent isolated home at `~/.agent-sandbox/home`
- host-side config and permission overrides under `~/.agent-sandbox`
- read/write access to `~/Projects`
- no access to your normal host home, browser profile, keyring, Wayland socket,
  session D-Bus, or window-manager IPC
- read-only container OS during normal runs
- Codex CLI, `gh`, adb, Java, Go, Python, ripgrep, fd, SQLite, and Playwright
  browsers in the image

This is a practical development boundary, not a VM.

## Install

```sh
git clone https://github.com/yourname/agent-sandbox ~/Projects/agent-sandbox
cd ~/Projects/agent-sandbox
./scripts/install.sh --build-image
agent doctor
```

## Use

```sh
agent shell
agent codex
agent exec rg TODO ~/Projects
```

## GitHub Auth

Authenticate inside the sandbox. Host GitHub auth is not copied.

```sh
agent shell
gh auth login -h github.com -p https -w
gh auth setup-git
gh auth status
```

If browser opening fails, copy the printed GitHub URL/code into your host
browser. The token is stored in the agent home.

## Modes

Comfortable mode is the default:

```sh
AGENT_SANDBOX=comfortable agent shell
```

It mounts `~/Projects` read/write. If this repo is inside `~/Projects`, the
agent can edit the sandbox files and affect future runs. That is convenient for
iteration, but a malicious or confused agent could weaken the rules.

Strict mode overlays the sandbox repo itself as read-only:

```sh
AGENT_SANDBOX=strict agent shell
```

Use strict mode when you want the agent to work on projects without changing the
sandbox launcher, image definition, or install scripts.

## Configuration

```sh
AGENT_PROJECTS_DIR=~/Projects
AGENT_STATE_DIR=~/.agent-sandbox
AGENT_IMAGE=localhost/agent-sandbox:latest
AGENT_NETWORK=host
AGENT_EXTRA_MOUNTS=/path/a:/path/b
```

- `AGENT_PROJECTS_DIR` is the default writable project mount.
- `AGENT_STATE_DIR` stores the persistent agent home.
- `AGENT_IMAGE` selects the Podman image.
- `AGENT_NETWORK=host` is convenient for local dev servers and adb. Use another
  Podman network mode if you do not want host-local network access.
- `AGENT_EXTRA_MOUNTS` adds extra writable paths. Every extra path is fully
  readable, searchable, and editable by the agent.

## Permission Overrides

Change sandbox permissions by editing env files outside the mounted agent home.
Local host repos can own these files without changing this generic repo.

The launcher reads:

```text
~/.agent-sandbox/config.env
~/.agent-sandbox/permissions.d/*.env
```

Those files are outside the mounted agent home, so the agent cannot edit them by
default.

Add a writable mount:

```sh
mkdir -p ~/.agent-sandbox/permissions.d
cat > ~/.agent-sandbox/permissions.d/local.env <<'EOF'
AGENT_EXTRA_MOUNTS="${AGENT_EXTRA_MOUNTS:+$AGENT_EXTRA_MOUNTS:}$HOME/.config/niri:$HOME/Server/Igloo"
EOF
```

Or let a dotfiles repo own the local policy source and install it:

```sh
install -d -m 700 ~/.agent-sandbox ~/.agent-sandbox/permissions.d
install -m 600 apps/agent-sandbox/permissions.d/desktop.env \
  ~/.agent-sandbox/permissions.d/desktop.env
```

Force strict mode by default:

```sh
cat > ~/.agent-sandbox/config.env <<'EOF'
AGENT_SANDBOX=strict
EOF
```

Use a less broad network mode:

```sh
cat >> ~/.agent-sandbox/config.env <<'EOF'
AGENT_NETWORK=slirp4netns
EOF
```

These files are trusted host policy. They are the intended place for
machine-specific permissions such as desktop configs or local service checkouts.
Keep those defaults out of the open-source repo.

If you mount the dotfiles repo read/write, the agent can edit the policy source
stored there. It still cannot edit the installed host policy under
`~/.agent-sandbox` unless you mount that directory too.

## Browser State

Playwright browsers are baked into the image. Put persistent browser auth in the
agent home, for example:

```text
~/.agent-sandbox/home/.config/agent-browser
```

Do not mount or reuse your host browser profile.

## Android

If host `adb` is installed, the launcher starts the host adb server and the
container adb client talks to it through:

```sh
ADB_SERVER_SOCKET=tcp:127.0.0.1:5037
```

This supports live `adb logcat`, installs, and connected Gradle tests. It also
gives the agent broad access to attached Android devices.

## What It Can Do

- edit files in mounted project paths
- run arbitrary commands inside the container
- use network access according to `AGENT_NETWORK`
- use `gh` after sandbox-local auth
- run headless browser work through Playwright
- use adb if host adb is available

## What It Cannot Do By Default

- read your normal host home
- read host browser profiles or cookies
- read host keyrings
- inspect host windows through Wayland or window-manager IPC
- use session D-Bus
- install OS packages during normal runs
- restart host services

## Limits

Do not rely on command deny lists for privacy. The real boundary is the set of
mounted filesystems, network mode, and any explicit host brokers you add.

If a path is mounted read/write, the agent can list, search, read, edit, or
delete files there.

To add native tools, edit `Containerfile` and rebuild:

```sh
agent build-image
```
