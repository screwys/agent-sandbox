use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::profile::{Mount, MountMode, Profile, SandboxMode};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub repo: PathBuf,
    pub home: PathBuf,
    pub state_dir: PathBuf,
    pub permissions_dir: PathBuf,
    pub profiles_dir: PathBuf,
    pub agent_home: PathBuf,
    pub projects_dir: PathBuf,
    pub image: String,
    pub containerfile: PathBuf,
    pub context: PathBuf,
    pub network: String,
    pub userns: String,
    pub sandbox_mode: SandboxMode,
    pub host_control_dir: PathBuf,
    pub sandbox_disable_max_seconds: u64,
    pub broker_dir: PathBuf,
    pub broker_socket: PathBuf,
    pub extra_mounts: Vec<PathBuf>,
    pub profiles: Vec<Profile>,
}

impl AppConfig {
    pub fn load() -> Result<Self, String> {
        let home = env_path("HOME").ok_or_else(|| "HOME is not set".to_string())?;
        let repo = env_path("AGENT_REPO").unwrap_or_else(|| {
            infer_repo().unwrap_or(home.join(".local/share/agent-sandbox/repo"))
        });
        let default_state = home.join(".agent-sandbox");
        let legacy = load_legacy_env(&home, &default_state)?;

        let state_dir = env_path("AGENT_STATE_DIR")
            .or_else(|| legacy.path("AGENT_STATE_DIR"))
            .unwrap_or(default_state);
        let permissions_dir = env_path("AGENT_PERMISSIONS_DIR")
            .or_else(|| legacy.path("AGENT_PERMISSIONS_DIR"))
            .unwrap_or_else(|| state_dir.join("permissions.d"));
        let profiles_dir = env_path("AGENT_PROFILES_DIR")
            .or_else(|| legacy.path("AGENT_PROFILES_DIR"))
            .unwrap_or_else(|| state_dir.join("profiles.d"));
        let agent_home = env_path("AGENT_HOME")
            .or_else(|| legacy.path("AGENT_HOME"))
            .unwrap_or_else(|| state_dir.join("home"));
        let projects_dir = env_path("AGENT_PROJECTS_DIR")
            .or_else(|| legacy.path("AGENT_PROJECTS_DIR"))
            .unwrap_or_else(|| home.join("Projects"));
        let image = env::var("AGENT_IMAGE")
            .ok()
            .or_else(|| legacy.nonempty("AGENT_IMAGE"))
            .unwrap_or_else(|| "localhost/agent-sandbox:latest".to_string());
        let containerfile = env_path("AGENT_CONTAINERFILE")
            .or_else(|| legacy.path("AGENT_CONTAINERFILE"))
            .unwrap_or_else(|| repo.join("Containerfile"));
        let context = env_path("AGENT_CONTEXT")
            .or_else(|| legacy.path("AGENT_CONTEXT"))
            .unwrap_or_else(|| repo.clone());
        let network = env::var("AGENT_NETWORK")
            .ok()
            .or_else(|| legacy.nonempty("AGENT_NETWORK"))
            .unwrap_or_else(|| "host".to_string());
        let userns = env::var("AGENT_USERNS")
            .ok()
            .or_else(|| legacy.nonempty("AGENT_USERNS"))
            .unwrap_or_else(|| "host".to_string());
        let sandbox_mode = env::var("AGENT_SANDBOX")
            .ok()
            .or_else(|| legacy.nonempty("AGENT_SANDBOX"))
            .as_deref()
            .and_then(SandboxMode::parse)
            .unwrap_or(SandboxMode::Strict);
        let host_control_dir = env_path("AGENT_HOST_CONTROL_DIR")
            .or_else(|| legacy.path("AGENT_HOST_CONTROL_DIR"))
            .unwrap_or_else(|| state_dir.join("host-control"));
        let sandbox_disable_max_seconds = env::var("AGENT_SANDBOX_DISABLE_MAX_SECONDS")
            .ok()
            .or_else(|| legacy.nonempty("AGENT_SANDBOX_DISABLE_MAX_SECONDS"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(14_400);
        let broker_dir = env_path("AGENT_BROKER_DIR")
            .or_else(|| legacy.path("AGENT_BROKER_DIR"))
            .unwrap_or_else(|| {
                env_path("XDG_RUNTIME_DIR")
                    .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", current_uid())))
                    .join("agent-sandbox")
            });
        let broker_socket =
            env_path("AGENT_BROKER_SOCKET").unwrap_or_else(|| broker_dir.join("broker.sock"));
        let extra_mounts = env::var("AGENT_EXTRA_MOUNTS")
            .ok()
            .or_else(|| legacy.nonempty("AGENT_EXTRA_MOUNTS"))
            .map(|value| split_mounts(&value))
            .unwrap_or_default();
        let profiles = load_profiles(&profiles_dir)?;

        Ok(Self {
            repo,
            home,
            state_dir,
            permissions_dir,
            profiles_dir,
            agent_home,
            projects_dir,
            image,
            containerfile,
            context,
            network,
            userns,
            sandbox_mode,
            host_control_dir,
            sandbox_disable_max_seconds,
            broker_dir,
            broker_socket,
            extra_mounts,
            profiles,
        })
    }

    pub fn config_file_for_mounts(&self) -> PathBuf {
        self.permissions_dir.join("local.env")
    }

    pub fn profiles_with_extra_mounts(&self) -> Vec<Profile> {
        let mut profiles = self.profiles.clone();
        if !self.extra_mounts.is_empty() {
            profiles.push(Profile {
                mounts: self
                    .extra_mounts
                    .iter()
                    .map(|path| Mount {
                        path: path.display().to_string(),
                        mode: MountMode::ReadWrite,
                    })
                    .collect(),
                ..Profile::default()
            });
        }
        profiles
    }
}

pub fn ensure_base_dirs(config: &AppConfig) -> Result<(), String> {
    for dir in [
        &config.agent_home,
        &config.agent_home.join(".cache"),
        &config.agent_home.join(".codex"),
        &config.agent_home.join(".config"),
        &config.agent_home.join(".gradle"),
        &config.agent_home.join("Android"),
        &config.projects_dir,
        &config.permissions_dir,
        &config.profiles_dir,
        &config.state_dir,
    ] {
        fs::create_dir_all(dir)
            .map_err(|err| format!("could not create {}: {err}", dir.display()))?;
    }

    let codex_config = config.agent_home.join(".codex/config.toml");
    if !codex_config.exists() {
        fs::write(
            &codex_config,
            "sandbox_mode = \"danger-full-access\"\napproval_policy = \"never\"\n\n[sandbox_workspace_write]\nnetwork_access = true\n",
        )
        .map_err(|err| format!("could not write {}: {err}", codex_config.display()))?;
    }
    Ok(())
}

pub fn canonical_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!(
            "command -v '{}' >/dev/null 2>&1",
            name.replace('\'', "'\\''")
        ))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn infer_repo() -> Option<PathBuf> {
    let exe = env::current_exe().ok()?;
    let mut path = exe.as_path();
    while let Some(parent) = path.parent() {
        if parent.join("Cargo.toml").exists() && parent.join("Containerfile").exists() {
            return Some(parent.to_path_buf());
        }
        path = parent;
    }
    None
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).map(PathBuf::from)
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

fn split_mounts(value: &str) -> Vec<PathBuf> {
    value
        .split(':')
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn load_profiles(dir: &Path) -> Result<Vec<Profile>, String> {
    let mut profiles = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(profiles);
    };
    let mut paths = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let data = fs::read_to_string(&path)
            .map_err(|err| format!("could not read profile {}: {err}", path.display()))?;
        profiles.push(
            Profile::from_toml_str(&data)
                .map_err(|err| format!("could not parse profile {}: {err}", path.display()))?,
        );
    }
    Ok(profiles)
}

#[derive(Default)]
struct LegacyEnv {
    lines: Vec<(String, String)>,
}

impl LegacyEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.lines
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    }

    fn nonempty(&self, key: &str) -> Option<String> {
        self.get(key).filter(|value| !value.is_empty())
    }

    fn path(&self, key: &str) -> Option<PathBuf> {
        self.get(key)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }
}

fn load_legacy_env(home: &Path, default_state: &Path) -> Result<LegacyEnv, String> {
    let permissions_dir =
        env_path("AGENT_PERMISSIONS_DIR").unwrap_or_else(|| default_state.join("permissions.d"));
    let config_file = env_path("AGENT_CONFIG").unwrap_or_else(|| default_state.join("config.env"));
    let mut script = String::from("set -a\n");
    if config_file.exists() {
        script.push_str(&format!(". '{}'\n", shell_escape(&config_file)));
    }
    if let Ok(entries) = fs::read_dir(&permissions_dir) {
        let mut paths = entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|ext| ext == "env"))
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            script.push_str(&format!(". '{}'\n", shell_escape(&path)));
        }
    }
    script.push_str("for key in AGENT_STATE_DIR AGENT_PERMISSIONS_DIR AGENT_PROFILES_DIR AGENT_HOME AGENT_PROJECTS_DIR AGENT_IMAGE AGENT_CONTAINERFILE AGENT_CONTEXT AGENT_NETWORK AGENT_USERNS AGENT_SANDBOX AGENT_HOST_CONTROL_DIR AGENT_SANDBOX_DISABLE_MAX_SECONDS AGENT_BROKER_DIR AGENT_EXTRA_MOUNTS; do eval value=\\\"\\${$key-}\\\"; printf '%s=%s\\n' \"$key\" \"$value\"; done\n");

    let output = Command::new("bash")
        .arg("-lc")
        .arg(script)
        .env("HOME", home)
        .output()
        .map_err(|err| format!("could not evaluate legacy env files: {err}"))?;
    if !output.status.success() {
        return Ok(LegacyEnv::default());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(LegacyEnv {
        lines: stdout
            .lines()
            .filter_map(|line| {
                line.split_once('=')
                    .map(|(k, v)| (k.to_string(), v.to_string()))
            })
            .collect(),
    })
}

fn shell_escape(path: &Path) -> String {
    path.display().to_string().replace('\'', "'\\''")
}
