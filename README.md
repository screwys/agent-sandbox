# Agent Sandbox

A small Podman sandbox for coding agents.

Default behavior:

- persistent isolated home at `~/.agent-sandbox/home`
- host-side config and permission overrides under `~/.agent-sandbox`
- read/write access to projects under `~/Projects`
- read-only access to this launcher repo when it lives under `~/Projects`
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

This installs `agent`, `agent-codex`, `agent-codex-desktop`, and the
`Codex (Agent Sandbox)` desktop entry.

## Use

```sh
agent shell
agent codex
agent-codex
agent-codex-desktop
agent exec rg TODO ~/Projects
```

## Codex App And Desktop

This wrapper only sandboxes processes launched through it.

`agent sandbox on` only clears a disabled-mode grant for future `agent ...`
runs. It does not move an already-running Codex Desktop/App session into the
container.

If Codex App was started normally, that session is host-runtime. Use one of
these for sandboxed CLI work:

```sh
agent codex
agent-codex
```

For sandboxed desktop work, use:

```sh
agent-codex-desktop
```

The installer adds a `Codex (Agent Sandbox)` desktop entry that runs that
wrapper. It starts the desktop app inside the container and mounts only narrow
GUI sockets for display/audio. It does not mount the session D-Bus or
window-manager IPC.

The host `/usr/bin/codex-desktop` app is not reused. The desktop app must exist
inside the agent home or image.

To build the community Linux desktop app into the agent home:

```sh
agent setup-codex-desktop
agent-codex-desktop
```

That clones [ilysenko/codex-desktop-linux](https://github.com/ilysenko/codex-desktop-linux)
under `~/.agent-sandbox/home/.local/share/codex-desktop-linux` and builds its
`codex-app/start.sh` launcher inside the sandbox.

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

Strict mode is the default:

```sh
agent shell
AGENT_SANDBOX=strict agent shell
```

Strict mode mounts projects read/write, but overlays this launcher repo itself
as read-only when the repo is under `AGENT_PROJECTS_DIR`.

That matters because `scripts/install.sh` installs `~/.local/bin/agent` as a
symlink to this repo. If an agent can edit this repo, it can change the host
command that future terminals will run.

Comfortable mode keeps that self-protection:

```sh
AGENT_SANDBOX=comfortable agent shell
```

It still mounts this launcher repo read-only. Comfortable mode is not the way to
let an agent edit the sandbox rules.

To edit this repo, use a host terminal:

```sh
cd ~/Projects/agent-sandbox
$EDITOR README.md bin/agent
```

Or intentionally enter disabled mode first.

Disabled mode is a temporary host-granted escape hatch:

```sh
agent sandbox off 240m
AGENT_SANDBOX=disabled agent shell
agent sandbox status
agent sandbox on
```

The `agent sandbox off` command must be run from an interactive host
terminal and is capped at 240 minutes. It refuses to run from inside a
container.

Turning the sandbox off permanently is explicit:

```sh
agent sandbox off --forever
AGENT_SANDBOX=disabled agent shell
agent sandbox on
```

Disabled mode mounts your real host home read/write into the container. It still
keeps `~/.agent-sandbox` read-only, then remounts `~/.agent-sandbox/home`
writable, so the agent can keep its own state but cannot extend the lease or
rewrite installed host policy directly.

Disabled mode is not a security boundary. In that mode the agent can edit host
files, including this repo and the symlinked launcher target. `--forever`
therefore means you are choosing to keep that broad access available until you
run `agent sandbox on`.

## Configuration

```sh
AGENT_PROJECTS_DIR=~/Projects
AGENT_STATE_DIR=~/.agent-sandbox
AGENT_IMAGE=localhost/agent-sandbox:latest
AGENT_NETWORK=host
AGENT_EXTRA_MOUNTS=/path/a:/path/b
AGENT_DESKTOP_COMMAND=/path/or/command
```

- `AGENT_PROJECTS_DIR` is the default writable project mount.
- `AGENT_STATE_DIR` stores the persistent agent home.
- `AGENT_IMAGE` selects the Podman image.
- `AGENT_NETWORK=host` is convenient for local dev servers and adb. Use another
  Podman network mode if you do not want host-local network access.
- `AGENT_EXTRA_MOUNTS` adds extra writable paths. Every extra path is fully
  readable, searchable, and editable by the agent.
- `AGENT_DESKTOP_COMMAND` overrides the command used by `agent desktop`.

In normal modes, extra mounts that contain this launcher repo are skipped. Use a
host terminal or disabled mode when you want to edit the sandbox itself.

Host-control grants live under:

```text
~/.agent-sandbox/host-control/
```

Do not mount that path read/write.

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

Pin a mode explicitly:

```sh
cat > ~/.agent-sandbox/config.env <<'EOF'
AGENT_SANDBOX=strict
EOF
```

Strict is already the built-in default. Use `AGENT_SANDBOX=comfortable` only if
you want the agent to be able to edit the sandbox repo during normal runs.

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
- run Codex Desktop if the desktop app is installed inside the sandbox
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
