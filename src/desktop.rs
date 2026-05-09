use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::config::AppConfig;
use crate::sandbox_control;

#[derive(Clone, Debug)]
struct CodexPaths {
    layout: String,
    appdir: PathBuf,
    electron: PathBuf,
    system_launcher: PathBuf,
    start_sh: PathBuf,
    asar: PathBuf,
    webview: PathBuf,
    cli_path: PathBuf,
    config_home: PathBuf,
    state_home: PathBuf,
    cache_home: PathBuf,
    port: String,
}

pub fn desktop_cmd(args: &[String], config: &AppConfig) -> Result<String, String> {
    sandbox_control::require_host_runtime()?;
    let subcmd = args.first().map(String::as_str).unwrap_or("help");
    let adapter = args.get(1).map(String::as_str).unwrap_or("");
    match (subcmd, adapter) {
        ("detect", "codex") => Ok(codex_detect(config)),
        ("validate", "codex") => {
            codex_validate(config)?;
            Ok("Codex Desktop adapter validation passed (experimental)\n".to_string())
        }
        ("install", "codex") => codex_install(config),
        ("detect", "claude") => Ok(claude_detect(config)),
        ("validate", "claude") | ("install", "claude") => Err(claude_validate_message()),
        ("help" | "" | "-h" | "--help", _) => Ok(desktop_usage()),
        (_, "") => Err(format!("missing desktop adapter\n{}", desktop_usage())),
        (_, other) if other != "codex" && other != "claude" => Err(format!(
            "unknown desktop adapter: {other}\nsupported adapters: codex, claude\n"
        )),
        (other, _) => Err(format!(
            "unknown desktop command: {other}\n{}",
            desktop_usage()
        )),
    }
}

fn desktop_usage() -> String {
    "usage: agent desktop COMMAND ADAPTER\n\ncommands:\n  detect codex|claude     print detected host package layout\n  validate codex|claude   verify a supported host package layout\n  install codex|claude    install a host wrapper and .desktop launcher\n".to_string()
}

fn codex_paths(config: &AppConfig) -> CodexPaths {
    let root = std::env::var("AGENT_DESKTOP_ROOT").unwrap_or_default();
    let user_appdir = std::env::var_os("AGENT_CODEX_DESKTOP_USER_APPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| config.home.join(".local/opt/codex-desktop-linux/codex-app"));
    let system_appdir = PathBuf::from(format!("{root}/usr/lib/openai-codex-desktop"));
    let system_launcher = PathBuf::from(format!("{root}/usr/bin/codex-desktop"));

    let mut layout =
        std::env::var("AGENT_CODEX_DESKTOP_LAYOUT").unwrap_or_else(|_| "system".into());
    let mut appdir = std::env::var_os("AGENT_CODEX_DESKTOP_APPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| system_appdir.clone());
    let mut electron = std::env::var_os("AGENT_CODEX_DESKTOP_ELECTRON")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{root}/usr/lib/electron39/electron")));
    let mut system_launcher_value = std::env::var_os("AGENT_CODEX_DESKTOP_SYSTEM_LAUNCHER")
        .map(PathBuf::from)
        .unwrap_or(system_launcher);
    let mut start_sh = PathBuf::new();

    if std::env::var_os("AGENT_CODEX_DESKTOP_APPDIR").is_none()
        && user_appdir.join("resources/app.asar").exists()
    {
        layout = "user-local".into();
        appdir = user_appdir.clone();
        electron = std::env::var_os("AGENT_CODEX_DESKTOP_ELECTRON")
            .map(PathBuf::from)
            .unwrap_or_else(|| user_appdir.join("electron"));
        system_launcher_value = std::env::var_os("AGENT_CODEX_DESKTOP_SYSTEM_LAUNCHER")
            .map(PathBuf::from)
            .unwrap_or_else(|| user_appdir.join("start.sh"));
        start_sh = user_appdir.join("start.sh");
    }

    CodexPaths {
        asar: appdir.join("resources/app.asar"),
        webview: appdir.join("content/webview"),
        cli_path: std::env::var_os("AGENT_CODEX_CLI_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| config.home.join(".local/bin/agent-codex")),
        config_home: std::env::var_os("AGENT_CODEX_DESKTOP_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| config.home.join(".config/codex-sandboxed")),
        state_home: std::env::var_os("AGENT_CODEX_DESKTOP_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| config.home.join(".local/state/codex-sandboxed")),
        cache_home: std::env::var_os("AGENT_CODEX_DESKTOP_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| config.home.join(".cache/codex-sandboxed")),
        port: std::env::var("AGENT_CODEX_DESKTOP_WEBVIEW_PORT").unwrap_or_else(|_| "5176".into()),
        layout,
        appdir,
        electron,
        system_launcher: system_launcher_value,
        start_sh,
    }
}

fn codex_detect(config: &AppConfig) -> String {
    let paths = codex_paths(config);
    let errors = codex_validation_errors(&paths);
    let status = if errors.is_empty() {
        "experimental"
    } else {
        "unsupported"
    };
    let mut out = format!(
        "adapter: codex\nstatus: {status}\nelectron: host-native\napp-server: CODEX_CLI_PATH points at agent-codex\nshared local chats: no\nexperimental: yes, package-specific host wrapper\nlayout: {}\nsystem launcher: {}\nelectron: {}\napp asar: {}\nwebview dir: {}\nsandboxed CLI: {}\nprofile/config dir: {}\nstate dir: {}\ncache dir: {}\nwebview port: {}\nwrapper: {}\ndesktop entry: {}\n",
        paths.layout,
        paths.system_launcher.display(),
        paths.electron.display(),
        paths.asar.display(),
        paths.webview.display(),
        paths.cli_path.display(),
        paths.config_home.display(),
        paths.state_home.display(),
        paths.cache_home.display(),
        paths.port,
        desktop_dest_bin(config, "codex").display(),
        desktop_dest_entry(config, "codex").display(),
    );
    if !errors.is_empty() {
        out.push_str("detected problems:\n");
        out.push_str(&errors);
    }
    out
}

fn codex_validate(config: &AppConfig) -> Result<(), String> {
    let paths = codex_paths(config);
    let errors = codex_validation_errors(&paths);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "unsupported Codex Desktop package layout\n\n{}\ndetected problems:\n{}\nCodex Desktop support is experimental and limited to known package layouts.\nElectron stays host-native; only the Codex CLI/app-server path is redirected into Podman.\n",
            codex_supported_layout(&paths),
            errors
        ))
    }
}

fn codex_install(config: &AppConfig) -> Result<String, String> {
    codex_validate(config)?;
    let paths = codex_paths(config);
    write_codex_wrapper(config, &paths)?;
    write_desktop_entry(config, "codex")?;
    Ok(format!(
        "installed Codex Desktop adapter (experimental)\nwrapper: {}\ndesktop entry: {}\nElectron stays host-native; Codex CLI/app-server uses {}\n",
        desktop_dest_bin(config, "codex").display(),
        desktop_dest_entry(config, "codex").display(),
        paths.cli_path.display()
    ))
}

fn codex_validation_errors(paths: &CodexPaths) -> String {
    let mut out = String::new();
    if !is_executable(&paths.electron) {
        out.push_str(&format!(
            "  missing executable: {}\n",
            paths.electron.display()
        ));
    }
    if !paths.asar.is_file() {
        out.push_str(&format!("  missing app asar: {}\n", paths.asar.display()));
    }
    if !paths.webview.is_dir() {
        out.push_str(&format!(
            "  missing webview dir: {}\n",
            paths.webview.display()
        ));
    } else if fs::read_dir(&paths.webview)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(true)
    {
        out.push_str(&format!(
            "  empty webview dir: {}\n",
            paths.webview.display()
        ));
    }
    if paths.layout == "user-local"
        && !paths.start_sh.as_os_str().is_empty()
        && !is_executable(&paths.start_sh)
    {
        out.push_str(&format!(
            "  missing executable start script: {}\n",
            paths.start_sh.display()
        ));
    } else if paths.layout != "system" && paths.layout != "user-local" {
        out.push_str(&format!(
            "  unknown Codex Desktop layout: {}\n",
            paths.layout
        ));
    }
    if !is_executable(&paths.cli_path) {
        out.push_str(&format!(
            "  missing sandboxed CLI executable: {}\n",
            paths.cli_path.display()
        ));
    }
    out
}

fn codex_supported_layout(paths: &CodexPaths) -> String {
    format!(
        "supported layouts:\n  layout: {}\n  system launcher: {} (optional)\n  electron: {}\n  app asar: {}\n  webview dir: {}\n  sandboxed CLI: {}\n  profile/config dir: {}\n  state dir: {}\n  cache dir: {}\n  webview port: {}\n",
        paths.layout,
        paths.system_launcher.display(),
        paths.electron.display(),
        paths.asar.display(),
        paths.webview.display(),
        paths.cli_path.display(),
        paths.config_home.display(),
        paths.state_home.display(),
        paths.cache_home.display(),
        paths.port,
    )
}

fn write_codex_wrapper(config: &AppConfig, paths: &CodexPaths) -> Result<(), String> {
    let wrapper = desktop_dest_bin(config, "codex");
    if let Some(parent) = wrapper.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let body = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

electron="{electron}"
app_asar="{asar}"
appdir="{appdir}"
webview_dir="{webview}"
webview_port="${{AGENT_CODEX_DESKTOP_WEBVIEW_PORT:-{port}}}"
layout="{layout}"
start_sh="{start_sh}"

export CODEX_CLI_PATH="${{CODEX_CLI_PATH:-{cli}}}"
export CODEX_LINUX_CLI_PATH="${{CODEX_LINUX_CLI_PATH:-${{CODEX_CLI_PATH}}}}"
export CUSTOM_CLI_PATH="${{CUSTOM_CLI_PATH:-${{CODEX_CLI_PATH}}}}"
export XDG_CONFIG_HOME="${{XDG_CONFIG_HOME:-{config_home}}}"
export XDG_STATE_HOME="${{XDG_STATE_HOME:-{state_home}}}"
export XDG_CACHE_HOME="${{XDG_CACHE_HOME:-{cache_home}}}"
export BUILD_FLAVOR="${{BUILD_FLAVOR:-prod}}"
export NODE_ENV="${{NODE_ENV:-production}}"
export LD_PRELOAD="${{AGENT_CODEX_DESKTOP_LD_PRELOAD-}}"
export AGENT_CODEX_APP_SERVER_OPEN_AUTH_URL="${{AGENT_CODEX_APP_SERVER_OPEN_AUTH_URL:-1}}"
export ELECTRON_RENDERER_URL="${{ELECTRON_RENDERER_URL:-http://127.0.0.1:${{webview_port}}/}}"
export CODEX_WEBVIEW_PORT="${{CODEX_WEBVIEW_PORT:-${{webview_port}}}}"

[[ -x "${{electron}}" ]] || {{ echo "missing Electron runtime: ${{electron}}" >&2; exit 1; }}
[[ -f "${{app_asar}}" ]] || {{ echo "missing Codex Desktop app: ${{app_asar}}" >&2; exit 1; }}
[[ -x "${{CODEX_CLI_PATH}}" ]] || {{ echo "missing sandboxed Codex CLI: ${{CODEX_CLI_PATH}}" >&2; exit 127; }}

mkdir -p "${{XDG_CONFIG_HOME}}" "${{XDG_STATE_HOME}}" "${{XDG_CACHE_HOME}}"

if [[ "${{layout}}" == "user-local" && -x "${{start_sh}}" ]]; then
  tmp_start="$(mktemp "${{appdir%/}}/.agent-sandbox-start.XXXXXX")"
  {{
    printf '#!/usr/bin/env bash\n'
    printf 'rm -f "$0"\n'
    sed \
      -e 's/^CODEX_LINUX_APP_ID=.*/CODEX_LINUX_APP_ID=Codex/' \
      -e 's/^CODEX_LINUX_APP_DISPLAY_NAME=.*/CODEX_LINUX_APP_DISPLAY_NAME=CodexSandboxed/' \
      "${{start_sh}}"
  }} >"${{tmp_start}}"
  chmod +x "${{tmp_start}}"

  launch_args=("$@")
  has_platform_arg=0
  for arg in "${{launch_args[@]}}"; do
    case "${{arg}}" in
      --wayland|--x11|--safe-mode|--ozone-platform=*|--ozone-platform-hint=*)
        has_platform_arg=1
        ;;
    esac
  done
  if [[ "${{has_platform_arg}}" == 0 ]]; then
    case "${{AGENT_CODEX_DESKTOP_PLATFORM:-auto}}" in
      wayland)
        launch_args=(--wayland "${{launch_args[@]}}")
        ;;
      x11)
        launch_args=(--x11 "${{launch_args[@]}}")
        ;;
      auto|"")
        if [[ -n "${{WAYLAND_DISPLAY:-}}" && -z "${{SOMMELIER_VERSION:-}}" ]]; then
          launch_args=(--wayland "${{launch_args[@]}}")
        fi
        ;;
      *)
        echo "unsupported AGENT_CODEX_DESKTOP_PLATFORM: ${{AGENT_CODEX_DESKTOP_PLATFORM}}" >&2
        exit 2
        ;;
    esac
  fi

  exec "${{tmp_start}}" "${{launch_args[@]}}"
fi

http_pid=""
electron_pid=""
tmpdir=""
cleanup() {{
  [[ -n "${{electron_pid}}" ]] && wait "${{electron_pid}}" 2>/dev/null || true
  [[ -n "${{http_pid}}" ]] && kill "${{http_pid}}" 2>/dev/null || true
  [[ -n "${{http_pid}}" ]] && wait "${{http_pid}}" 2>/dev/null || true
  [[ -n "${{tmpdir}}" ]] && rm -rf "${{tmpdir}}"
}}
trap cleanup EXIT

if [[ -d "${{webview_dir}}" ]] && find "${{webview_dir}}" -mindepth 1 -maxdepth 1 -print -quit | grep -q .; then
  python_cmd="${{PYTHON:-}}"
  [[ -n "${{python_cmd}}" ]] || python_cmd="$(command -v python3 || command -v python || true)"
  [[ -n "${{python_cmd}}" ]] || {{ echo "missing python or python3 for local Codex webview server" >&2; exit 127; }}
  tmpdir="$(mktemp -d)"
  (cd "${{webview_dir}}" && "${{python_cmd}}" -m http.server "${{webview_port}}" --bind 127.0.0.1 >/dev/null 2>&1) &
  http_pid=$!
  sleep 0.2
fi

if [[ "${{layout}}" == "user-local" ]]; then
  "${{electron}}" --no-sandbox --disable-dev-shm-usage --disable-gpu-sandbox --disable-gpu-compositing --ozone-platform-hint=auto --app-id=Codex --class=Codex "${{app_asar}}" "$@" &
else
  "${{electron}}" --enable-sandbox --ozone-platform-hint=auto --app-id=Codex --class=Codex "${{app_asar}}" "$@" &
fi
electron_pid=$!
wait "${{electron_pid}}"
"#,
        electron = paths.electron.display(),
        asar = paths.asar.display(),
        appdir = paths.appdir.display(),
        webview = paths.webview.display(),
        port = paths.port,
        layout = paths.layout,
        start_sh = paths.start_sh.display(),
        cli = paths.cli_path.display(),
        config_home = paths.config_home.display(),
        state_home = paths.state_home.display(),
        cache_home = paths.cache_home.display(),
    );
    fs::write(&wrapper, body).map_err(|err| err.to_string())?;
    chmod(&wrapper, 0o755)
}

pub fn write_desktop_entry(config: &AppConfig, adapter: &str) -> Result<(), String> {
    let entry = desktop_dest_entry(config, adapter);
    if let Some(parent) = entry.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let wrapper = desktop_dest_bin(config, adapter);
    let (name, comment, icon, wmclass) = match adapter {
        "codex" => (
            "Codex (Sandboxed)",
            "Codex Desktop with sandboxed Codex CLI app-server",
            "openai-codex-desktop",
            "Codex",
        ),
        "claude" => (
            "Claude Desktop (Sandboxed)",
            "Claude Desktop with sandboxed agent subprocesses",
            "claude",
            "Claude",
        ),
        _ => return Err(format!("unknown desktop adapter: {adapter}")),
    };
    fs::write(
        &entry,
        format!(
            "[Desktop Entry]\nType=Application\nName={name}\nComment={comment}\nExec={} %U\nIcon={icon}\nCategories=Development;IDE;\nTerminal=false\nStartupNotify=true\nStartupWMClass={wmclass}\n",
            wrapper.display()
        ),
    )
    .map_err(|err| err.to_string())?;
    chmod(&entry, 0o644)
}

fn claude_detect(config: &AppConfig) -> String {
    format!(
        "adapter: claude\nstatus: unsupported\nelectron: host-native if a future supported Desktop layout is added\nshared local chats: no\nwrapper: {}\ndesktop entry: {}\n\ndetected problems:\n  no supported Claude Desktop package layout is registered yet\n  no stable launcher contract for CLI path, profile/config dir, or app-server transport is known\n",
        desktop_dest_bin(config, "claude").display(),
        desktop_dest_entry(config, "claude").display()
    )
}

fn claude_validate_message() -> String {
    "unsupported Claude Desktop package layout\n\nsupported exact layout:\n  none registered\n\ndetected problems:\n  no supported Claude Desktop package layout is registered yet\n  no stable launcher contract for CLI path, profile/config dir, or app-server transport is known\n".to_string()
}

fn desktop_dest_bin(config: &AppConfig, adapter: &str) -> PathBuf {
    match adapter {
        "codex" => config.home.join(".local/bin/codex-desktop-sandboxed"),
        "claude" => config.home.join(".local/bin/claude-desktop-sandboxed"),
        _ => config.home.join(".local/bin/unknown-desktop-sandboxed"),
    }
}

fn desktop_dest_entry(config: &AppConfig, adapter: &str) -> PathBuf {
    match adapter {
        "codex" => config
            .home
            .join(".local/share/applications/codex-sandboxed.desktop"),
        "claude" => config
            .home
            .join(".local/share/applications/claude-sandboxed.desktop"),
        _ => config
            .home
            .join(".local/share/applications/unknown-sandboxed.desktop"),
    }
}

fn is_executable(path: &PathBuf) -> bool {
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn chmod(path: &PathBuf, mode: u32) -> Result<(), String> {
    let mut perms = fs::metadata(path)
        .map_err(|err| err.to_string())?
        .permissions();
    perms.set_mode(mode);
    fs::set_permissions(path, perms).map_err(|err| err.to_string())
}
