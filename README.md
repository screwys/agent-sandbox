# Agent Sandbox

A basic Podman sandbox for autonomous CLI agents.

Most agent tools include their own sandbox controls, but those controls are
often broad: deny a few commands, or switch to a whitelist mode and approve
every little command. Agent Sandbox puts the agent in a small Linux container
with its own home directory and only the folders you choose to mount.

This is not a VM and it does not claim to sandbox every Desktop app. It is a
simple default boundary for CLI agents.

## Install

Requires `git` and `podman`.

```sh
curl -fsSL https://raw.githubusercontent.com/screwys/agent-sandbox/main/scripts/bootstrap.sh | bash
```

That clones the repo to `~/Projects/agent-sandbox`, installs `agent` and
`agent-codex`, builds the Podman image, and creates `Codex (Sandboxed)` if a
supported Codex Desktop install is detected.

Local install from a clone:

```sh
./scripts/install.sh
```

## Use

```sh
agent shell
agent-codex
agent exec rg TODO ~/Projects
```

By default the agent gets:

- its own home at `~/.agent-sandbox/home`
- read/write access to `~/Projects`
- no access to your normal home, browser profile, keyring, session D-Bus, or
  window-manager IPC
- a read-only container OS during normal runs

## Give Access To A Folder

```sh
agent allow ~/Server/Bunker
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

## Commands

```sh
agent shell                 # interactive sandbox shell
agent-codex                 # Codex CLI in the sandbox
agent codex                 # same, through the main command
agent exec <command>        # run any command in the sandbox
agent allow <folder>        # mount another folder read/write
agent config edit           # edit folder access config
agent doctor                # show basic status
```

`agent-codex` is explicit on purpose. The installer does not replace your
native `/usr/bin/codex` unless you opt in:

```sh
INSTALL_CODEX_AGENT_SHIM=1 ./scripts/install.sh
```

## Desktop

Desktop launchers are host-side adapters, not the core sandbox guarantee.

The installer creates `Codex (Sandboxed)` by default when it detects the tested
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

Native Codex Desktop stays native. Local chats are not shared by default.

Claude Desktop adapter: TODO.

## Disable The Sandbox Temporarily

Use this only from a host terminal:

```sh
agent sandbox off 240m
AGENT_SANDBOX=disabled agent shell
agent sandbox on
```

Disabled mode mounts your real host home read/write. It is not a security
boundary.

## Limits

The boundary is the set of mounted folders, the Podman network mode, and any
host brokers you add. If a folder is mounted read/write, the agent can read,
edit, or delete files there.
