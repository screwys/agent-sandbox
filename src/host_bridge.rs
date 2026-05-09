use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use crate::config::AppConfig;
use crate::profile::HostHelper;

const EXIT_MARKER: &[u8] = b"\n__AGENT_EXIT__=";

pub fn host_client(args: &[String], config: &AppConfig) -> Result<i32, String> {
    let socket = std::env::var_os("AGENT_BROKER_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| config.broker_socket.clone());
    let mut stream = UnixStream::connect(&socket)
        .map_err(|err| format!("agent-host: could not connect {}: {err}", socket.display()))?;
    let request = encode_request(args);
    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|err| format!("agent-host: could not send request: {err}"))?;

    let mut payload = Vec::new();
    let mut buf = [0_u8; 8192];
    loop {
        let read = stream
            .read(&mut buf)
            .map_err(|err| format!("agent-host: could not read response: {err}"))?;
        if read == 0 {
            eprintln!("agent-host: broker closed without exit marker");
            return Ok(1);
        }
        payload.extend_from_slice(&buf[..read]);
        if let Some(marker_at) = find_marker(&payload) {
            std::io::stdout()
                .write_all(&payload[..marker_at])
                .map_err(|err| err.to_string())?;
            let tail = &payload[marker_at + EXIT_MARKER.len()..];
            let line_end = tail.iter().position(|b| *b == b'\n').unwrap_or(tail.len());
            let code = String::from_utf8_lossy(&tail[..line_end])
                .trim()
                .parse()
                .unwrap_or(1);
            return Ok(code);
        }

        let keep = payload.len().saturating_sub(EXIT_MARKER.len());
        if keep > 0 {
            std::io::stdout()
                .write_all(&payload[..keep])
                .map_err(|err| err.to_string())?;
            payload = payload[keep..].to_vec();
        }
    }
}

pub fn serve(config: AppConfig) -> Result<(), String> {
    fs::create_dir_all(&config.broker_dir)
        .map_err(|err| format!("could not create {}: {err}", config.broker_dir.display()))?;
    if config.broker_socket.exists() {
        fs::remove_file(&config.broker_socket).map_err(|err| {
            format!(
                "could not remove stale socket {}: {err}",
                config.broker_socket.display()
            )
        })?;
    }
    let listener = UnixListener::bind(&config.broker_socket)
        .map_err(|err| format!("could not bind {}: {err}", config.broker_socket.display()))?;
    println!(
        "agent-broker listening on {}",
        config.broker_socket.display()
    );
    for conn in listener.incoming() {
        let config = config.clone();
        match conn {
            Ok(stream) => {
                thread::spawn(move || {
                    handle(stream, &config);
                });
            }
            Err(err) => eprintln!("agent-broker: {err}"),
        }
    }
    Ok(())
}

fn handle(mut conn: UnixStream, config: &AppConfig) {
    let mut code = 1;
    let result = (|| -> Result<(), String> {
        let mut raw = Vec::new();
        loop {
            let mut byte = [0_u8; 1];
            let read = conn.read(&mut byte).map_err(|err| err.to_string())?;
            if read == 0 || byte[0] == b'\n' {
                break;
            }
            raw.push(byte[0]);
            if raw.len() > 65_536 {
                return Err("request is too large".to_string());
            }
        }
        let argv = decode_request(&String::from_utf8_lossy(&raw))?;
        let plans = command_plan(&argv, config)?;
        for plan in plans {
            code = run_process(&plan, &mut conn)?;
            if code != 0 {
                break;
            }
        }
        Ok(())
    })();
    if let Err(err) = result {
        let _ = writeln!(conn, "agent-broker: {err}");
        code = 2;
    }
    let _ = conn.write_all(EXIT_MARKER);
    let _ = writeln!(conn, "{code}");
}

#[derive(Clone, Debug)]
pub struct CommandPlan {
    pub argv: Vec<String>,
}

fn command_plan(argv: &[String], config: &AppConfig) -> Result<Vec<CommandPlan>, String> {
    if argv.is_empty() {
        return Err("missing command".to_string());
    }
    if argv[0] == "run" {
        let helper_name = argv
            .get(1)
            .ok_or_else(|| "usage: run HELPER [ARG...]".to_string())?;
        let trailing = argv[2..].to_vec();
        let helper = all_helpers(config)
            .into_iter()
            .find(|helper| helper.name == *helper_name)
            .ok_or_else(|| format!("helper is not allowed: {helper_name}"))?;
        return Ok(vec![CommandPlan {
            argv: helper
                .command_plan(&trailing)
                .map_err(|err| err.to_string())?
                .argv,
        }]);
    }
    if argv[0] == "open-url" {
        return open_url_plan(argv);
    }
    legacy_service_plan(argv, config)
}

fn open_url_plan(argv: &[String]) -> Result<Vec<CommandPlan>, String> {
    if argv.len() != 2 {
        return Err("usage: open-url URL".to_string());
    }
    ensure_allowed_url(&argv[1])?;

    let opener = std::env::var("AGENT_HOST_OPEN_COMMAND").unwrap_or_else(|_| "xdg-open".into());
    let mut command = opener
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(String::from)
        .collect::<Vec<_>>();
    if command.is_empty() {
        return Err("AGENT_HOST_OPEN_COMMAND is empty".to_string());
    }
    command.push(argv[1].clone());
    Ok(vec![CommandPlan { argv: command }])
}

fn ensure_allowed_url(url: &str) -> Result<(), String> {
    if url.len() > 4096 || url.is_empty() {
        return Err("URL is empty or too long".to_string());
    }
    if url.chars().any(|ch| ch.is_control()) {
        return Err("URL contains control characters".to_string());
    }

    let lower = url.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("https://") {
        ensure_plain_authority(rest)?;
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("http://") {
        let host = authority_host(rest)?;
        if is_loopback_host(host) {
            return Ok(());
        }
        return Err("only loopback HTTP URLs may be opened on the host".to_string());
    }
    Err("only HTTPS and loopback HTTP URLs may be opened on the host".to_string())
}

fn ensure_plain_authority(rest: &str) -> Result<(), String> {
    authority_host(rest)?;
    Ok(())
}

fn authority_host(rest: &str) -> Result<&str, String> {
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or_else(|| "URL is missing a host".to_string())?;
    if authority.contains('@') {
        return Err("URL userinfo is not allowed".to_string());
    }
    if let Some(after_bracket) = authority.strip_prefix('[') {
        let end = after_bracket
            .find(']')
            .ok_or_else(|| "URL IPv6 host is missing a closing bracket".to_string())?;
        let suffix = &after_bracket[end + 1..];
        if !suffix.is_empty() && !suffix.starts_with(':') {
            return Err("URL IPv6 host has an invalid suffix".to_string());
        }
        let host = &after_bracket[..end];
        if host.is_empty() {
            return Err("URL is missing a host".to_string());
        }
        return Ok(host);
    }
    let host = authority.split(':').next().unwrap_or(authority);
    if host.is_empty() {
        Err("URL is missing a host".to_string())
    } else {
        Ok(host)
    }
}

fn is_loopback_host(host: &str) -> bool {
    host == "localhost" || host == "::1" || is_ipv4_loopback(host)
}

fn is_ipv4_loopback(host: &str) -> bool {
    let octets = host.split('.').collect::<Vec<_>>();
    octets.len() == 4
        && octets[0] == "127"
        && octets
            .iter()
            .all(|octet| !octet.is_empty() && octet.parse::<u8>().is_ok())
}

fn legacy_service_plan(argv: &[String], config: &AppConfig) -> Result<Vec<CommandPlan>, String> {
    if argv[0] != "service" {
        return Err(format!("command group is not allowed: {}", argv[0]));
    }
    if argv.len() < 2 {
        return Err("usage: service ACTION [UNIT]".to_string());
    }
    let action = argv[1].as_str();
    if action == "daemon-reload" {
        if argv.len() != 2 {
            return Err("usage: service daemon-reload".to_string());
        }
        return Ok(vec![CommandPlan {
            argv: vec!["systemctl".into(), "--user".into(), "daemon-reload".into()],
        }]);
    }
    if action == "logs" {
        if argv.len() < 3 {
            return Err("usage: service logs UNIT [--follow] [-n LINES]".to_string());
        }
        ensure_allowed_unit(&argv[2], config)?;
        let mut follow = false;
        let mut lines = "120".to_string();
        let mut i = 3;
        while i < argv.len() {
            match argv[i].as_str() {
                "--follow" => {
                    follow = true;
                    i += 1;
                }
                "-n" if i + 1 < argv.len() => {
                    if !argv[i + 1].chars().all(|c| c.is_ascii_digit()) || argv[i + 1].len() > 4 {
                        return Err(format!("invalid line count: {}", argv[i + 1]));
                    }
                    lines = argv[i + 1].clone();
                    i += 2;
                }
                other => return Err(format!("unsupported logs argument: {other}")),
            }
        }
        let mut cmd = vec![
            "journalctl".into(),
            "--user".into(),
            "-u".into(),
            argv[2].clone(),
            "-n".into(),
            lines,
            "--no-pager".into(),
        ];
        if follow {
            cmd.push("-f".into());
        }
        return Ok(vec![CommandPlan { argv: cmd }]);
    }
    if ![
        "start",
        "stop",
        "restart",
        "status",
        "is-active",
        "is-failed",
    ]
    .contains(&action)
    {
        return Err(format!("service action is not allowed: {action}"));
    }
    if argv.len() != 3 {
        return Err(format!("usage: service {action} UNIT"));
    }
    ensure_allowed_unit(&argv[2], config)?;
    Ok(vec![CommandPlan {
        argv: vec![
            "systemctl".into(),
            "--user".into(),
            action.into(),
            argv[2].clone(),
        ],
    }])
}

fn run_process(plan: &CommandPlan, conn: &mut UnixStream) -> Result<i32, String> {
    writeln!(conn, "[broker] {}", plan.argv.join(" ")).map_err(|err| err.to_string())?;
    let mut child = Command::new(&plan.argv[0])
        .args(&plan.argv[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("could not run {}: {err}", plan.argv[0]))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut conn_out = conn.try_clone().map_err(|err| err.to_string())?;
    let out_thread = stdout.map(|mut stdout| {
        thread::spawn(move || {
            let _ = std::io::copy(&mut stdout, &mut conn_out);
        })
    });
    let mut conn_err = conn.try_clone().map_err(|err| err.to_string())?;
    let err_thread = stderr.map(|mut stderr| {
        thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut conn_err);
        })
    });
    let status = child.wait().map_err(|err| err.to_string())?;
    if let Some(thread) = out_thread {
        let _ = thread.join();
    }
    if let Some(thread) = err_thread {
        let _ = thread.join();
    }
    Ok(status.code().unwrap_or(1))
}

fn default_allowed_units() -> Vec<String> {
    vec![
        "igloo.service",
        "igloo-nginx.service",
        "rsshub.service",
        "radicale.service",
        "todo-sync.service",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn ensure_allowed_unit(unit: &str, config: &AppConfig) -> Result<(), String> {
    if allowed_units(config).iter().any(|allowed| allowed == unit) {
        Ok(())
    } else {
        Err(format!("unit is not allowed: {unit}"))
    }
}

fn allowed_units(_config: &AppConfig) -> Vec<String> {
    let mut units = default_allowed_units();
    if let Ok(raw) = std::env::var("AGENT_HOST_ALLOWED_UNITS") {
        units.extend(
            raw.split([' ', ',', ':'])
                .filter(|unit| !unit.is_empty())
                .map(String::from),
        );
    }
    units.sort();
    units.dedup();
    units
}

fn all_helpers(config: &AppConfig) -> Vec<HostHelper> {
    let mut helpers = default_helpers(config);
    for profile in &config.profiles {
        helpers.extend(profile.host_helpers.clone());
    }
    helpers
}

fn default_helpers(config: &AppConfig) -> Vec<HostHelper> {
    vec![
        HostHelper {
            name: "systemd.user.daemon-reload".into(),
            command: "systemctl".into(),
            args: vec!["--user".into(), "daemon-reload".into()],
            allowed_trailing_args: vec![],
            timeout_secs: 20,
        },
        HostHelper {
            name: "systemd.user.restart".into(),
            command: "systemctl".into(),
            args: vec!["--user".into(), "restart".into()],
            allowed_trailing_args: allowed_units(config),
            timeout_secs: 30,
        },
    ]
}

fn find_marker(payload: &[u8]) -> Option<usize> {
    payload
        .windows(EXIT_MARKER.len())
        .position(|window| window == EXIT_MARKER)
}

fn encode_request(args: &[String]) -> String {
    let mut out = String::from("{\"argv\":[");
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        for ch in arg.chars() {
            match ch {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                other => out.push(other),
            }
        }
        out.push('"');
    }
    out.push_str("]}");
    out
}

fn decode_request(raw: &str) -> Result<Vec<String>, String> {
    let raw = raw.trim();
    let prefix = "{\"argv\":[";
    let suffix = "]}";
    if !raw.starts_with(prefix) || !raw.ends_with(suffix) {
        return Err("request must contain string argv list".to_string());
    }
    let mut args = Vec::new();
    let mut chars = raw[prefix.len()..raw.len() - suffix.len()]
        .chars()
        .peekable();
    loop {
        while matches!(chars.peek(), Some(' ' | '\t' | '\n' | ',')) {
            chars.next();
        }
        let Some(ch) = chars.next() else {
            break;
        };
        if ch != '"' {
            return Err("request argv entries must be strings".to_string());
        }
        let mut arg = String::new();
        let mut closed = false;
        while let Some(ch) = chars.next() {
            match ch {
                '"' => {
                    closed = true;
                    break;
                }
                '\\' => match chars.next() {
                    Some('"') => arg.push('"'),
                    Some('\\') => arg.push('\\'),
                    Some('n') => arg.push('\n'),
                    Some('t') => arg.push('\t'),
                    Some(other) => arg.push(other),
                    None => return Err("unterminated escape in request".to_string()),
                },
                other => arg.push(other),
            }
        }
        if !closed {
            return Err("unterminated string in request".to_string());
        }
        args.push(arg);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_url_accepts_https_and_loopback_http() {
        for url in [
            "https://auth.openai.com/oauth/authorize?client_id=codex",
            "https://example.com/path",
            "http://127.0.0.1:1455/callback",
            "http://127.42.0.1/callback",
            "http://localhost:1455/callback",
            "http://[::1]:1455/callback",
        ] {
            ensure_allowed_url(url).expect(url);
        }
    }

    #[test]
    fn open_url_rejects_unsafe_urls() {
        for url in [
            "",
            "file:///home/laptop/.ssh/id_ed25519",
            "mailto:test@example.com",
            "http://example.com",
            "http://127.evil/callback",
            "http://[::1]evil/callback",
            "https://user@example.com",
            "https://",
            "https://example.com/\nnext",
        ] {
            assert!(ensure_allowed_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn open_url_plan_uses_default_host_opener() {
        let plans = open_url_plan(&["open-url".into(), "https://auth.openai.com/".into()]).unwrap();
        assert_eq!(
            plans[0].argv,
            vec![
                "xdg-open".to_string(),
                "https://auth.openai.com/".to_string()
            ]
        );
    }
}

#[allow(dead_code)]
fn _socket_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}
