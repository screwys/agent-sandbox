use std::fs;
use std::io::IsTerminal;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::AppConfig;
use crate::profile::SandboxMode;

pub fn status(config: &AppConfig) -> String {
    match remaining(config) {
        Remaining::Forever => "sandbox: off (forever)\n".to_string(),
        Remaining::Seconds(seconds) if seconds > 0 => {
            let until = disabled_until_raw(config).unwrap_or_else(|| "0".to_string());
            format!("sandbox: off ({seconds}s remaining, until @{until})\n")
        }
        _ => format!("sandbox: on ({})\n", on_mode_summary(config.sandbox_mode)),
    }
}

pub fn disable(config: &AppConfig, duration: Option<&str>) -> Result<String, String> {
    require_host_terminal()?;
    fs::create_dir_all(&config.host_control_dir).map_err(|err| {
        format!(
            "could not create {}: {err}",
            config.host_control_dir.display()
        )
    })?;
    let file = disabled_until_file(config);
    if matches!(duration, Some("--forever" | "forever")) {
        fs::write(&file, "forever\n")
            .map_err(|err| format!("could not write {}: {err}", file.display()))?;
        return Ok(status(config));
    }
    let seconds = parse_duration_seconds(
        duration.unwrap_or("240m"),
        config.sandbox_disable_max_seconds,
    )?;
    let until = now_secs() + seconds;
    fs::write(&file, format!("{until}\n"))
        .map_err(|err| format!("could not write {}: {err}", file.display()))?;
    Ok(status(config))
}

pub fn enable(config: &AppConfig) -> Result<String, String> {
    require_host_terminal()?;
    let file = disabled_until_file(config);
    let _ = fs::remove_file(file);
    Ok(status(config))
}

pub fn require_active_disable(config: &AppConfig) -> Result<(), String> {
    match remaining(config) {
        Remaining::Forever => require_host_terminal(),
        Remaining::Seconds(seconds) if seconds > 0 => require_host_terminal(),
        _ => Err(
            "sandbox is on; run `agent sandbox off 240m` from a host terminal first".to_string(),
        ),
    }
}

pub fn parse_duration_seconds(raw: &str, max_seconds: u64) -> Result<u64, String> {
    let (number, unit) = if let Some(number) = raw.strip_suffix('s') {
        (number, 1)
    } else if let Some(number) = raw.strip_suffix('m') {
        (number, 60)
    } else if let Some(number) = raw.strip_suffix('h') {
        (number, 3600)
    } else {
        (raw, 1)
    };
    let value = number.parse::<u64>().map_err(|_| {
        format!("invalid duration: {raw}\nuse a value like 240m, 900s, 1m, or --forever")
    })?;
    let seconds = value
        .checked_mul(unit)
        .ok_or_else(|| format!("invalid duration: {raw}"))?;
    if seconds == 0 {
        return Err("duration must be greater than zero".to_string());
    }
    if seconds > max_seconds {
        return Err(format!(
            "sandbox off duration maximum is {}m",
            max_seconds / 60
        ));
    }
    Ok(seconds)
}

#[derive(Debug, PartialEq, Eq)]
enum Remaining {
    Forever,
    Seconds(u64),
}

fn remaining(config: &AppConfig) -> Remaining {
    match disabled_until_raw(config).as_deref() {
        Some("forever") => Remaining::Forever,
        Some(raw) => {
            let until = raw.parse::<u64>().unwrap_or(0);
            Remaining::Seconds(until.saturating_sub(now_secs()))
        }
        None => Remaining::Seconds(0),
    }
}

fn disabled_until_raw(config: &AppConfig) -> Option<String> {
    fs::read_to_string(disabled_until_file(config))
        .ok()
        .and_then(|value| {
            let line = value.lines().next().unwrap_or("").trim().to_string();
            (!line.is_empty()).then_some(line)
        })
}

fn disabled_until_file(config: &AppConfig) -> std::path::PathBuf {
    config.host_control_dir.join("sandbox-disabled-until")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn on_mode_summary(mode: SandboxMode) -> &'static str {
    match mode {
        SandboxMode::Strict => "strict",
        SandboxMode::Comfortable => "comfortable",
        SandboxMode::Disabled => "disabled requested, no off grant",
    }
}

fn require_host_terminal() -> Result<(), String> {
    if running_inside_container() {
        return Err("sandbox controls must be run from an interactive host terminal".to_string());
    }
    if std::env::var("AGENT_HOST_CONTROL_TEST").ok().as_deref() == Some("1") {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err("sandbox controls must be run from an interactive host terminal".to_string());
    }
    Ok(())
}

pub fn require_host_runtime() -> Result<(), String> {
    if running_inside_container() {
        Err("this command must be run from a host terminal".to_string())
    } else {
        Ok(())
    }
}

pub fn running_inside_container() -> bool {
    std::env::var("AGENT_FORCE_CONTAINER").ok().as_deref() == Some("1")
        || std::path::Path::new("/.dockerenv").exists()
        || std::path::Path::new("/run/.containerenv").exists()
        || std::env::var_os("container").is_some()
}
