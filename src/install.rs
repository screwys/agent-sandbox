use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{AppConfig, command_exists, ensure_base_dirs};
use crate::desktop;

pub fn install(args: &[String], config: &AppConfig) -> Result<String, String> {
    let mut build_image = true;
    let mut setup_android = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--build-image" => build_image = true,
            "--no-build-image" => build_image = false,
            "--setup-android-sdk" => setup_android = true,
            "-h" | "--help" => return Ok(install_usage()),
            other => return Err(format!("unknown argument: {other}\n{}", install_usage())),
        }
        i += 1;
    }
    if let Ok(value) = std::env::var("AGENT_INSTALL_BUILD_IMAGE") {
        build_image = truthy(&value);
    }

    ensure_base_dirs(config)?;
    link_commands(config)?;
    link_systemd(config)?;
    systemctl_user(&["daemon-reload"]);
    if truthy_env("INSTALL_AGENT_HOST_BRIDGE", true) {
        systemctl_user(&["enable", "--now", "agent-sandbox-broker.service"]);
    } else {
        println!("SKIP: host broker disabled");
    }
    if truthy_env("INSTALL_AGENT_AUTO_UPDATE", true) {
        systemctl_user(&["enable", "--now", "agent-sandbox-update.timer"]);
    } else {
        systemctl_user(&["disable", "--now", "agent-sandbox-update.timer"]);
        println!("SKIP: auto-update disabled");
    }
    if build_image {
        build_image_cmd(config)?;
    }
    match std::env::var("INSTALL_CODEX_DESKTOP_LAUNCHER")
        .unwrap_or_else(|_| "auto".into())
        .to_lowercase()
        .as_str()
    {
        "0" | "no" | "false" | "off" | "never" => println!("SKIP: Codex desktop launcher disabled"),
        "auto" => {
            if let Ok(out) = desktop::desktop_cmd(&["install".into(), "codex".into()], config) {
                print!("{out}");
            } else {
                println!(
                    "SKIP: Codex desktop launcher not installed; unsupported local Codex Desktop layout"
                );
            }
        }
        other if truthy(other) => {
            print!(
                "{}",
                desktop::desktop_cmd(&["install".into(), "codex".into()], config)?
            );
        }
        other => {
            return Err(format!(
                "unknown INSTALL_CODEX_DESKTOP_LAUNCHER value: {other}"
            ));
        }
    }
    if setup_android {
        setup_android_sdk(&[], config)?;
    }
    Ok("Agent sandbox installed.\n".to_string())
}

pub fn build_image_cmd(config: &AppConfig) -> Result<(), String> {
    if !command_exists("podman") {
        return Err("missing required command: podman".to_string());
    }
    Command::new("cargo")
        .args(["build", "--release", "--manifest-path"])
        .arg(config.repo.join("Cargo.toml"))
        .env("RUSTUP_HOME", rustup_home())
        .env("CARGO_HOME", cargo_home())
        .status()
        .map_err(|err| format!("could not build Rust binary: {err}"))
        .and_then(|status| {
            status
                .success()
                .then_some(())
                .ok_or_else(|| "cargo build --release failed".to_string())
        })?;
    let status = Command::new("podman")
        .arg("build")
        .arg("-t")
        .arg(&config.image)
        .arg("-f")
        .arg(&config.containerfile)
        .arg(&config.context)
        .status()
        .map_err(|err| format!("could not run podman build: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("podman build failed".to_string())
    }
}

pub fn setup_android_sdk(args: &[String], config: &AppConfig) -> Result<i32, String> {
    ensure_base_dirs(config)?;
    let status = Command::new(config.repo.join("scripts/setup-android-sdk"))
        .arg(config.agent_home.join("Android/Sdk"))
        .args(args)
        .status()
        .map_err(|err| format!("could not run setup-android-sdk: {err}"))?;
    Ok(status.code().unwrap_or(1))
}

fn link_commands(config: &AppConfig) -> Result<(), String> {
    let bin = config.home.join(".local/bin");
    fs::create_dir_all(&bin).map_err(|err| err.to_string())?;
    link_file(
        &config.repo.join("target/release/agent-sandbox"),
        &bin.join("agent-sandbox"),
    )?;
    for name in ["agent", "agent-broker", "agent-codex", "agent-host"] {
        link_file(&config.repo.join(format!("bin/{name}")), &bin.join(name))?;
    }
    if truthy_env("INSTALL_CODEX_AGENT_SHIM", false) {
        link_file(&config.repo.join("bin/codex"), &bin.join("codex"))?;
        println!("CONFIG: codex shim -> agent codex");
    } else {
        remove_owned_symlink(&bin.join("codex"), &config.repo.join("bin/codex"))?;
        println!("SKIP: codex PATH shim disabled by INSTALL_CODEX_AGENT_SHIM=0");
    }
    remove_owned_symlink(
        &bin.join("agent-codex-desktop"),
        &config.repo.join("bin/agent-codex-desktop"),
    )?;
    remove_owned_desktop_entry(
        &config
            .home
            .join(".local/share/applications/agent-codex-desktop.desktop"),
    )?;
    Ok(())
}

fn link_systemd(config: &AppConfig) -> Result<(), String> {
    let dir = config.home.join(".config/systemd/user");
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    for unit in [
        "agent-sandbox-broker.service",
        "agent-sandbox-update.service",
        "agent-sandbox-update.timer",
    ] {
        link_file(
            &config.repo.join(format!("systemd/{unit}")),
            &dir.join(unit),
        )?;
    }
    Ok(())
}

fn link_file(src: &Path, dest: &Path) -> Result<(), String> {
    if dest.exists() && !dest.is_symlink() {
        let backup = PathBuf::from(format!("{}.bak", dest.display()));
        fs::rename(dest, &backup)
            .map_err(|err| format!("could not back up {}: {err}", dest.display()))?;
        println!("BACKUP: {} -> {}", dest.display(), backup.display());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let _ = fs::remove_file(dest);
    symlink(src, dest).map_err(|err| {
        format!(
            "could not link {} -> {}: {err}",
            dest.display(),
            src.display()
        )
    })?;
    println!("LINK: {} -> {}", dest.display(), src.display());
    Ok(())
}

fn remove_owned_symlink(dest: &Path, src: &Path) -> Result<(), String> {
    if dest.is_symlink() {
        if fs::read_link(dest).ok().as_deref() == Some(src) {
            fs::remove_file(dest).map_err(|err| err.to_string())?;
            println!("REMOVE: {}", dest.display());
        }
    }
    Ok(())
}

fn remove_owned_desktop_entry(dest: &Path) -> Result<(), String> {
    if fs::read_to_string(dest)
        .map(|data| data.contains("Exec=agent-codex-desktop %U"))
        .unwrap_or(false)
    {
        fs::remove_file(dest).map_err(|err| err.to_string())?;
        println!("REMOVE: {}", dest.display());
    }
    Ok(())
}

fn systemctl_user(args: &[&str]) {
    if command_exists("systemctl") {
        let _ = Command::new("systemctl").arg("--user").args(args).status();
    }
}

fn truthy_env(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| truthy(&value))
        .unwrap_or(default)
}

fn truthy(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "yes" | "true" | "on" | "always"
    )
}

fn install_usage() -> String {
    "usage: install.sh [--no-build-image] [--setup-android-sdk]\n".to_string()
}

fn real_home() -> PathBuf {
    if let Some(home) = std::env::var_os("REAL_HOME").map(PathBuf::from) {
        return home;
    }
    std::process::Command::new("getent")
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
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "1000".to_string())
}
