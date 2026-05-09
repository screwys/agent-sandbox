use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::profile::{MountMode, Profile, SandboxMode};

#[derive(Debug)]
pub enum RunPlanError {
    EmptyExecCommand,
}

impl fmt::Display for RunPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExecCommand => write!(f, "exec command cannot be empty"),
        }
    }
}

impl Error for RunPlanError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Codex(Vec<String>),
    Shell(Vec<String>),
    Exec(Vec<String>),
}

#[derive(Clone, Debug)]
pub struct RunPlanInput {
    pub home: PathBuf,
    pub repo: PathBuf,
    pub projects_dir: PathBuf,
    pub agent_home: PathBuf,
    pub broker_dir: PathBuf,
    pub image: String,
    pub network: String,
    pub userns: String,
    pub sandbox_mode: SandboxMode,
    pub command: CommandKind,
    pub profiles: Vec<Profile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunPlan {
    pub image: String,
    pub network: String,
    pub userns: String,
    pub sandbox_mode: SandboxMode,
    pub read_only: bool,
    pub workdir: PathBuf,
    pub command: Vec<String>,
    pub env: Vec<EnvVar>,
    pub mounts: Vec<MountSpec>,
    pub blocked_mounts: Vec<BlockedMount>,
    pub tmpfs: Vec<String>,
    pub interactive: bool,
    pub tty: bool,
}

impl RunPlan {
    pub fn build(input: RunPlanInput) -> Result<Self, RunPlanError> {
        let needs_stdin = matches!(
            &input.command,
            CommandKind::Codex(_) | CommandKind::Shell(_)
        );
        let command = command_argv(input.command)?;
        let read_only = input.sandbox_mode != SandboxMode::Disabled;
        let mut mounts = Vec::new();
        let mut blocked_mounts = Vec::new();

        if input.sandbox_mode == SandboxMode::Disabled {
            add_mount_if_exists(&mut mounts, &input.home, &input.home, MountMode::ReadWrite);
            if !path_covers(&input.home, &input.projects_dir) {
                add_mount_if_exists(
                    &mut mounts,
                    &input.projects_dir,
                    &input.projects_dir,
                    MountMode::ReadWrite,
                );
            }
            add_mount_if_exists(
                &mut mounts,
                &input.agent_home,
                &input.agent_home,
                MountMode::ReadWrite,
            );
            add_mount_if_exists(
                &mut mounts,
                &input.broker_dir,
                &PathBuf::from("/run/agent-sandbox"),
                MountMode::ReadWrite,
            );
        } else {
            add_mount_if_exists(
                &mut mounts,
                &input.agent_home,
                &input.home,
                MountMode::ReadWrite,
            );
            add_mount_if_exists(
                &mut mounts,
                &input.broker_dir,
                &PathBuf::from("/run/agent-sandbox"),
                MountMode::ReadWrite,
            );
            mount_projects(&mut mounts, &input.projects_dir, &input.repo);
        }

        for profile in &input.profiles {
            for mount in &profile.mounts {
                let source = expand_home(&mount.path, &input.home);
                if !source.exists() {
                    continue;
                }
                if read_only && path_covers(&source, &input.repo) {
                    blocked_mounts.push(BlockedMount {
                        source,
                        reason: "mount covers protected launcher repo".to_string(),
                    });
                    continue;
                }

                mounts.push(MountSpec {
                    source: source.clone(),
                    target: source,
                    mode: mount.mode,
                });
            }
        }

        Ok(Self {
            image: input.image,
            network: input.network,
            userns: input.userns,
            sandbox_mode: input.sandbox_mode,
            read_only,
            workdir: workdir_for_container(&input.home, &input.projects_dir),
            command,
            env: default_env(&input.home),
            mounts,
            blocked_mounts,
            tmpfs: vec![
                "/tmp:rw,nosuid,nodev,mode=1777".to_string(),
                "/var/tmp:rw,nosuid,nodev,mode=1777".to_string(),
                "/root/.android:rw,nosuid,nodev,mode=700".to_string(),
            ],
            interactive: needs_stdin
                || std::io::IsTerminal::is_terminal(&std::io::stdin())
                || std::env::var("AGENT_PRESERVE_STDIN").ok().as_deref() == Some("1"),
            tty: std::io::IsTerminal::is_terminal(&std::io::stdin())
                && std::io::IsTerminal::is_terminal(&std::io::stdout()),
        })
    }

    pub fn to_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("image: {}\n", self.image));
        out.push_str(&format!("network: {}\n", self.network));
        out.push_str(&format!("userns: {}\n", self.userns));
        out.push_str(&format!("sandbox: {}\n", self.sandbox_mode.as_str()));
        out.push_str(&format!(
            "read-only: {}\n",
            if self.read_only { "yes" } else { "no" }
        ));
        out.push_str(&format!("workdir: {}\n", self.workdir.display()));
        out.push_str(&format!("command: {}\n", self.command.join(" ")));
        out.push_str(&format!(
            "interactive: {}\n",
            if self.interactive { "yes" } else { "no" }
        ));
        out.push_str(&format!("tty: {}\n", if self.tty { "yes" } else { "no" }));

        for env in &self.env {
            out.push_str(&format!("env: {}={}\n", env.key, env.value));
        }
        for mount in &self.mounts {
            out.push_str(&format!(
                "mount: {} -> {} {}\n",
                mount.source.display(),
                mount.target.display(),
                mount.mode.as_str()
            ));
        }
        for mount in &self.blocked_mounts {
            out.push_str(&format!(
                "blocked: {} ({})\n",
                mount.source.display(),
                mount.reason
            ));
        }

        out
    }

    pub fn podman_args(&self) -> Vec<String> {
        let mut args = vec![
            "run".to_string(),
            "--rm".to_string(),
            format!("--userns={}", self.userns),
            format!("--network={}", self.network),
            "--security-opt=no-new-privileges".to_string(),
            "--security-opt=label=disable".to_string(),
            "--cap-drop=all".to_string(),
            "--pids-limit=4096".to_string(),
        ];
        for tmpfs in &self.tmpfs {
            args.push("--tmpfs".to_string());
            args.push(tmpfs.clone());
        }
        for env in &self.env {
            args.push("--env".to_string());
            args.push(format!("{}={}", env.key, env.value));
        }
        args.push("--workdir".to_string());
        args.push(self.workdir.display().to_string());
        if self.read_only {
            args.push("--read-only".to_string());
        }
        if self.interactive {
            args.push("--interactive".to_string());
        }
        if self.tty {
            args.push("--tty".to_string());
        }
        for mount in &self.mounts {
            args.push("--volume".to_string());
            args.push(format!(
                "{}:{}:{}",
                mount.source.display(),
                mount.target.display(),
                mount.mode.as_str()
            ));
        }
        args.push(self.image.clone());
        args.extend(self.command.iter().cloned());
        args
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountSpec {
    pub source: PathBuf,
    pub target: PathBuf,
    pub mode: MountMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockedMount {
    pub source: PathBuf,
    pub reason: String,
}

fn command_argv(command: CommandKind) -> Result<Vec<String>, RunPlanError> {
    match command {
        CommandKind::Codex(args) => {
            let mut argv = vec!["codex".to_string()];
            argv.extend(args);
            Ok(argv)
        }
        CommandKind::Shell(args) => {
            let mut argv = vec!["bash".to_string()];
            argv.extend(args);
            Ok(argv)
        }
        CommandKind::Exec(args) => {
            if args.is_empty() {
                Err(RunPlanError::EmptyExecCommand)
            } else {
                Ok(args)
            }
        }
    }
}

fn default_env(home: &Path) -> Vec<EnvVar> {
    let home = home.display().to_string();
    vec![
        EnvVar {
            key: "HOME".to_string(),
            value: home.clone(),
        },
        EnvVar {
            key: "USER".to_string(),
            value: std::env::var("USER").unwrap_or_else(|_| "agent".to_string()),
        },
        EnvVar {
            key: "LOGNAME".to_string(),
            value: std::env::var("USER").unwrap_or_else(|_| "agent".to_string()),
        },
        EnvVar {
            key: "CODEX_HOME".to_string(),
            value: format!("{home}/.codex"),
        },
        EnvVar {
            key: "ANDROID_HOME".to_string(),
            value: format!("{home}/Android/Sdk"),
        },
        EnvVar {
            key: "ANDROID_SDK_ROOT".to_string(),
            value: format!("{home}/Android/Sdk"),
        },
        EnvVar {
            key: "ANDROID_USER_HOME".to_string(),
            value: format!("{home}/.android"),
        },
        EnvVar {
            key: "GRADLE_USER_HOME".to_string(),
            value: format!("{home}/.gradle"),
        },
        EnvVar {
            key: "ADB_SERVER_SOCKET".to_string(),
            value: "tcp:127.0.0.1:5037".to_string(),
        },
        EnvVar {
            key: "AGENT_BROKER_SOCKET".to_string(),
            value: "/run/agent-sandbox/broker.sock".to_string(),
        },
    ]
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    if path == "~" {
        return home.to_path_buf();
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest);
    }

    PathBuf::from(path)
}

fn path_covers(source: &Path, target: &Path) -> bool {
    source == target || target.starts_with(source)
}

fn add_mount_if_exists(mounts: &mut Vec<MountSpec>, source: &Path, target: &Path, mode: MountMode) {
    if source.exists() {
        mounts.push(MountSpec {
            source: source.to_path_buf(),
            target: target.to_path_buf(),
            mode,
        });
    }
}

fn mount_projects(mounts: &mut Vec<MountSpec>, projects_dir: &Path, repo: &Path) {
    if path_covers(projects_dir, repo) {
        let protected = repo
            .strip_prefix(projects_dir)
            .ok()
            .and_then(|rel| rel.components().next())
            .map(|first| projects_dir.join(first.as_os_str()))
            .unwrap_or_else(|| repo.to_path_buf());
        if let Ok(entries) = fs::read_dir(projects_dir) {
            for child in entries.filter_map(|entry| entry.ok().map(|entry| entry.path())) {
                if child == protected {
                    continue;
                }
                add_mount_if_exists(mounts, &child, &child, MountMode::ReadWrite);
            }
        }
        add_mount_if_exists(mounts, repo, repo, MountMode::ReadOnly);
    } else {
        add_mount_if_exists(mounts, projects_dir, projects_dir, MountMode::ReadWrite);
    }
}

fn workdir_for_container(home: &Path, projects_dir: &Path) -> PathBuf {
    let pwd = std::env::current_dir().unwrap_or_else(|_| projects_dir.to_path_buf());
    if path_covers(projects_dir, &pwd) {
        pwd
    } else if let Ok(relative) = projects_dir.strip_prefix(home) {
        home.join(relative)
    } else {
        projects_dir.to_path_buf()
    }
}
