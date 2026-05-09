use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use agent_sandbox::config::{AppConfig, canonical_path, command_exists, ensure_base_dirs};
use agent_sandbox::desktop;
use agent_sandbox::host_bridge;
use agent_sandbox::install;
use agent_sandbox::profile::{Profile, SandboxMode};
use agent_sandbox::run_plan::{CommandKind, RunPlan, RunPlanInput};
use agent_sandbox::sandbox_control;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<i32, String> {
    let mut raw_args = std::env::args().collect::<Vec<_>>();
    let argv0 = PathBuf::from(raw_args.first().cloned().unwrap_or_default());
    let program = argv0
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let mut args = raw_args.drain(1..).collect::<Vec<_>>();
    match program {
        "agent" => args.insert(0, "agent".to_string()),
        "agent-host" => args.insert(0, "host".to_string()),
        "agent-broker" => args.insert(0, "broker".to_string()),
        "agent-codex" | "codex" => {
            args.insert(0, "codex".to_string());
            args.insert(0, "agent".to_string());
        }
        _ => {}
    }
    let config = AppConfig::load()?;
    match args.as_slice() {
        [scope, rest @ ..] if scope == "agent" => agent_cmd(rest, &config),
        [scope, rest @ ..] if scope == "host" => host_cmd(rest, &config),
        [scope, rest @ ..] if scope == "broker" => broker_cmd(rest, config),
        [scope, rest @ ..] if scope == "install" => {
            print!("{}", install::install(rest, &config)?);
            Ok(0)
        }
        _ => {
            eprint!("{}", top_usage());
            Ok(2)
        }
    }
}

fn agent_cmd(args: &[String], config: &AppConfig) -> Result<i32, String> {
    match args
        .split_first()
        .map(|(cmd, rest)| (cmd.as_str(), rest))
        .unwrap_or(("help", &[]))
    {
        ("build-image", _) => {
            install::build_image_cmd(config)?;
            Ok(0)
        }
        ("self-update", rest) => self_update(rest, config),
        ("setup-android-sdk", rest) => install::setup_android_sdk(rest, config),
        ("broker-start", _) => {
            start_broker(config);
            Ok(0)
        }
        ("shell", rest) => run_container(CommandKind::Shell(rest.to_vec()), config),
        ("codex", rest) => run_container(CommandKind::Codex(rest.to_vec()), config),
        ("run-plan", rest) => {
            print_run_plan(rest, config)?;
            Ok(0)
        }
        ("desktop", rest) => {
            print!("{}", desktop::desktop_cmd(rest, config)?);
            Ok(0)
        }
        ("allow", rest) => {
            print!("{}", allow_cmd(rest, config)?);
            Ok(0)
        }
        ("config", rest) => {
            print!("{}", config_cmd(rest, config)?);
            Ok(0)
        }
        ("exec", rest) => {
            if rest.is_empty() {
                eprint!("{}", agent_usage());
                return Ok(2);
            }
            run_container(CommandKind::Exec(rest.to_vec()), config)
        }
        ("sandbox", rest) => sandbox_cmd(rest, config),
        ("doctor", _) => {
            print!("{}", doctor(config));
            Ok(0)
        }
        ("help" | "" | "-h" | "--help", _) => {
            print!("{}", agent_usage());
            Ok(0)
        }
        (other, _) => {
            eprintln!("unknown command: {other}");
            eprint!("{}", agent_usage());
            Ok(2)
        }
    }
}

fn host_cmd(args: &[String], config: &AppConfig) -> Result<i32, String> {
    if args.first().map(String::as_str) == Some("run") {
        host_bridge::host_client(args, config)
    } else {
        host_bridge::host_client(args, config)
    }
}

fn broker_cmd(args: &[String], config: AppConfig) -> Result<i32, String> {
    match args {
        [cmd] if cmd == "serve" => {
            host_bridge::serve(config)?;
            Ok(0)
        }
        _ => {
            eprintln!("usage: agent-broker serve");
            Ok(2)
        }
    }
}

fn sandbox_cmd(args: &[String], config: &AppConfig) -> Result<i32, String> {
    match args.first().map(String::as_str).unwrap_or("status") {
        "off" | "disable" => {
            print!(
                "{}",
                sandbox_control::disable(config, args.get(1).map(String::as_str))?
            );
            Ok(0)
        }
        "on" | "enable" => {
            print!("{}", sandbox_control::enable(config)?);
            Ok(0)
        }
        "status" | "" => {
            print!("{}", sandbox_control::status(config));
            Ok(0)
        }
        other => {
            eprintln!("unknown sandbox command: {other}");
            eprint!("{}", agent_usage());
            Ok(2)
        }
    }
}

fn run_container(command: CommandKind, config: &AppConfig) -> Result<i32, String> {
    ensure_base_dirs(config)?;
    if config.sandbox_mode == SandboxMode::Disabled {
        sandbox_control::require_active_disable(config)?;
    }
    ensure_image(config)?;
    start_broker(config);
    start_adb_server();
    let plan = build_plan(command, config)?;
    let status = Command::new("podman")
        .args(plan.podman_args())
        .status()
        .map_err(|err| format!("could not run podman: {err}"))?;
    Ok(status.code().unwrap_or(1))
}

fn build_plan(command: CommandKind, config: &AppConfig) -> Result<RunPlan, String> {
    RunPlan::build(RunPlanInput {
        home: config.home.clone(),
        repo: canonical_path(&config.repo),
        projects_dir: canonical_path(&config.projects_dir),
        agent_home: config.agent_home.clone(),
        broker_dir: config.broker_dir.clone(),
        image: config.image.clone(),
        network: config.network.clone(),
        userns: config.userns.clone(),
        sandbox_mode: config.sandbox_mode,
        command,
        profiles: config.profiles_with_extra_mounts(),
    })
    .map_err(|err| err.to_string())
}

fn print_run_plan(args: &[String], config: &AppConfig) -> Result<(), String> {
    let (kind, rest) = args
        .split_first()
        .ok_or_else(|| "missing run-plan target: codex, shell, or exec".to_string())?;
    let parsed = ParsedRunPlanArgs::parse(rest)?;
    let mut config = config.clone();
    config.profiles.extend(parsed.load_profiles()?);
    let command = match kind.as_str() {
        "codex" => CommandKind::Codex(parsed.command_args),
        "shell" => CommandKind::Shell(parsed.command_args),
        "exec" => CommandKind::Exec(parsed.command_args),
        other => {
            return Err(format!(
                "unknown run-plan target: {other}; expected codex, shell, or exec"
            ));
        }
    };
    print!("{}", build_plan(command, &config)?.to_human());
    Ok(())
}

fn ensure_image(config: &AppConfig) -> Result<(), String> {
    if !command_exists("podman") {
        return Err("missing required command: podman".to_string());
    }
    let exists = Command::new("podman")
        .args(["image", "exists", &config.image])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !exists {
        eprintln!(
            "agent sandbox image {} is missing; building it now",
            config.image
        );
        install::build_image_cmd(config)?;
    }
    Ok(())
}

fn start_broker(_config: &AppConfig) {
    if command_exists("systemctl") {
        let _ = Command::new("systemctl")
            .args(["--user", "start", "agent-sandbox-broker.service"])
            .status();
    }
}

fn start_adb_server() {
    if command_exists("adb") {
        let _ = Command::new("adb").arg("start-server").status();
    }
}

fn allow_cmd(args: &[String], config: &AppConfig) -> Result<String, String> {
    sandbox_control::require_host_runtime()?;
    if args.is_empty() {
        return Err("usage: agent allow PATH [PATH...]".to_string());
    }
    let mut mounts = load_configured_mounts(config)?;
    let mut out = String::new();
    let mut changed = false;
    for raw in args {
        let path = PathBuf::from(raw);
        if !path.is_dir() {
            return Err(format!("not a directory: {}", path.display()));
        }
        let real = canonical_path(&path);
        if mounts.iter().any(|mount| mount == &real) {
            out.push_str(&format!("already allowed: {}\n", real.display()));
            continue;
        }
        mounts.push(real.clone());
        changed = true;
        out.push_str(&format!("allowed: {}\n", real.display()));
    }
    if changed {
        write_configured_mounts(config, &mounts)?;
        out.push_str(&format!(
            "config: {}\n",
            config.config_file_for_mounts().display()
        ));
    }
    Ok(out)
}

fn load_configured_mounts(config: &AppConfig) -> Result<Vec<PathBuf>, String> {
    Ok(config.extra_mounts.clone())
}

fn write_configured_mounts(config: &AppConfig, mounts: &[PathBuf]) -> Result<(), String> {
    fs::create_dir_all(&config.permissions_dir).map_err(|err| err.to_string())?;
    let value = mounts
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    let file = config.config_file_for_mounts();
    fs::write(
        &file,
        format!(
            "# edited by agent allow\nAGENT_EXTRA_MOUNTS='{}'\n",
            value.replace('\'', "'\\''")
        ),
    )
    .map_err(|err| format!("could not write {}: {err}", file.display()))?;
    chmod(&file, 0o600)?;
    Ok(())
}

fn config_cmd(args: &[String], config: &AppConfig) -> Result<String, String> {
    sandbox_control::require_host_runtime()?;
    let subcmd = args.first().map(String::as_str).unwrap_or("edit");
    let file = config.config_file_for_mounts();
    match subcmd {
        "path" => Ok(format!("{}\n", file.display())),
        "open" => {
            fs::create_dir_all(&config.permissions_dir).map_err(|err| err.to_string())?;
            if command_exists("xdg-open") {
                let _ = Command::new("xdg-open")
                    .arg(&config.permissions_dir)
                    .spawn();
                Ok(String::new())
            } else {
                Ok(format!("{}\n", config.permissions_dir.display()))
            }
        }
        "edit" | "" => {
            fs::create_dir_all(&config.permissions_dir).map_err(|err| err.to_string())?;
            if !file.exists() {
                write_configured_mounts(config, &load_configured_mounts(config)?)?;
            }
            if let Ok(editor) = std::env::var("EDITOR") {
                let _ = Command::new(editor).arg(&file).status();
                Ok(String::new())
            } else {
                Ok(format!("{}\n", file.display()))
            }
        }
        other => Err(format!(
            "unknown config command: {other}\nusage: agent config [edit|open|path]"
        )),
    }
}

fn doctor(config: &AppConfig) -> String {
    let image_present = Command::new("podman")
        .args(["image", "exists", &config.image])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    format!(
        "image: {} ({})\nagent home: {}\npermissions dir: {}\nprojects mount: {} {}\nnetwork: {}\nuserns: {}\nsandbox mode: {}\n{}broker socket: {} {}\nadb server: {}\n",
        if image_present { "present" } else { "missing" },
        config.image,
        config.agent_home.display(),
        config.permissions_dir.display(),
        config.projects_dir.display(),
        if config.projects_dir.is_dir() {
            "(present)"
        } else {
            "(missing)"
        },
        config.network,
        config.userns,
        config.sandbox_mode.as_str(),
        sandbox_control::status(config),
        config.broker_socket.display(),
        if config.broker_socket.exists() {
            "(present)"
        } else {
            "(missing)"
        },
        if command_exists("adb")
            && Command::new("adb")
                .arg("devices")
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        {
            "reachable"
        } else {
            "not reachable"
        }
    )
}

fn self_update(args: &[String], config: &AppConfig) -> Result<i32, String> {
    let quiet = args.first().map(String::as_str) == Some("--quiet");
    sandbox_control::require_host_runtime()?;
    if !config.repo.join(".git").is_dir() {
        if !quiet {
            println!(
                "self-update skipped: not a git checkout: {}",
                config.repo.display()
            );
        }
        return Ok(0);
    }
    let dirty = Command::new("git")
        .arg("-C")
        .arg(&config.repo)
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .output()
        .map_err(|err| err.to_string())?;
    if !dirty.stdout.is_empty() {
        if !quiet {
            println!(
                "self-update skipped: checkout has local changes: {}",
                config.repo.display()
            );
        }
        return Ok(0);
    }
    let before = git_output(&config.repo, &["rev-parse", "HEAD"])?;
    let mut pull_args = vec!["pull", "--ff-only"];
    if quiet {
        pull_args.push("--quiet");
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(&config.repo)
        .args(pull_args)
        .status()
        .map_err(|err| err.to_string())?;
    if !status.success() {
        return Err("self-update failed: could not fast-forward".to_string());
    }
    let after = git_output(&config.repo, &["rev-parse", "HEAD"])?;
    if before == after {
        if !quiet {
            println!("self-update: already up to date");
        }
        return Ok(0);
    }
    if !quiet {
        println!("self-update: updated {}..{}", &before[..12], &after[..12]);
    }
    if command_exists("systemctl") {
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
    }
    let changed = git_output(
        &config.repo,
        &["diff", "--name-only", &before, &after, "--"],
    )?;
    let changed_runtime = changed.lines().any(|line| {
        line == "Containerfile"
            || line.starts_with("src/")
            || line == "Cargo.toml"
            || line == "Cargo.lock"
            || line.starts_with("bin/wrappers/")
    });
    if changed_runtime && config.repo.join("Cargo.toml").exists() {
        Command::new("cargo")
            .args(["build", "--release", "--manifest-path"])
            .arg(config.repo.join("Cargo.toml"))
            .env("RUSTUP_HOME", rustup_home())
            .env("CARGO_HOME", cargo_home())
            .status()
            .map_err(|err| err.to_string())?;
        let _ = install::build_image_cmd(config);
    }
    Ok(0)
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

struct ParsedRunPlanArgs {
    profile_paths: Vec<PathBuf>,
    command_args: Vec<String>,
}

impl ParsedRunPlanArgs {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut profile_paths = Vec::new();
        let mut command_args = Vec::new();
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--" => {
                    command_args.extend(args[i + 1..].iter().cloned());
                    break;
                }
                "--profile" => {
                    let path = args
                        .get(i + 1)
                        .ok_or_else(|| "--profile needs a path".to_string())?;
                    profile_paths.push(PathBuf::from(path));
                    i += 2;
                }
                other => {
                    return Err(format!(
                        "unknown run-plan option: {other}; use -- before command args"
                    ));
                }
            }
        }
        Ok(Self {
            profile_paths,
            command_args,
        })
    }

    fn load_profiles(&self) -> Result<Vec<Profile>, String> {
        self.profile_paths
            .iter()
            .map(|path| {
                let data = fs::read_to_string(path)
                    .map_err(|err| format!("could not read profile {}: {err}", path.display()))?;
                Profile::from_toml_str(&data)
                    .map_err(|err| format!("could not parse profile {}: {err}", path.display()))
            })
            .collect()
    }
}

fn chmod(path: &Path, mode: u32) -> Result<(), String> {
    let mut perms = fs::metadata(path)
        .map_err(|err| err.to_string())?
        .permissions();
    perms.set_mode(mode);
    fs::set_permissions(path, perms).map_err(|err| err.to_string())
}

fn agent_usage() -> String {
    "usage: agent COMMAND [ARG...]\n\ncommands:\n  build-image\n  self-update\n  setup-android-sdk\n  broker-start\n  shell\n  codex [ARG...]\n  run-plan codex|shell|exec\n  desktop COMMAND ADAPTER\n  allow PATH [PATH...]\n  config [edit|open|path]\n  exec COMMAND [ARG...]\n  sandbox off|on|status\n  doctor\n".to_string()
}

fn top_usage() -> String {
    "usage: agent-sandbox agent|host|broker|install ...\n".to_string()
}

fn real_home() -> PathBuf {
    if let Some(home) = std::env::var_os("REAL_HOME").map(PathBuf::from) {
        return home;
    }
    Command::new("getent")
        .arg("passwd")
        .arg(current_uid())
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|line| line.split(':').nth(5).map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()))
}

fn rustup_home() -> PathBuf {
    std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| real_home().join(".rustup"))
}

fn cargo_home() -> PathBuf {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| real_home().join(".cargo"))
}

fn current_uid() -> String {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "1000".to_string())
}
