use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::profile::{MountMode, Profile, SandboxMode};

#[derive(Debug, Error)]
pub enum RunPlanError {
    #[error("exec command cannot be empty")]
    EmptyExecCommand,
}

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
    pub sandbox_mode: SandboxMode,
    pub command: CommandKind,
    pub profiles: Vec<Profile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunPlan {
    pub image: String,
    pub sandbox_mode: SandboxMode,
    pub read_only: bool,
    pub workdir: PathBuf,
    pub command: Vec<String>,
    pub env: Vec<EnvVar>,
    pub mounts: Vec<MountSpec>,
    pub blocked_mounts: Vec<BlockedMount>,
}

impl RunPlan {
    pub fn build(input: RunPlanInput) -> Result<Self, RunPlanError> {
        let command = command_argv(input.command)?;
        let read_only = input.sandbox_mode != SandboxMode::Disabled;
        let mut mounts = vec![
            MountSpec {
                source: input.agent_home.clone(),
                target: input.home.clone(),
                mode: MountMode::ReadWrite,
            },
            MountSpec {
                source: input.broker_dir.clone(),
                target: PathBuf::from("/run/agent-sandbox"),
                mode: MountMode::ReadWrite,
            },
        ];
        let mut blocked_mounts = Vec::new();

        if path_covers(&input.projects_dir, &input.repo) {
            mounts.push(MountSpec {
                source: input.repo.clone(),
                target: input.repo.clone(),
                mode: MountMode::ReadOnly,
            });
        } else {
            mounts.push(MountSpec {
                source: input.projects_dir.clone(),
                target: input.projects_dir.clone(),
                mode: MountMode::ReadWrite,
            });
        }

        for profile in &input.profiles {
            for mount in &profile.mounts {
                let source = expand_home(&mount.path, &input.home);
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
            sandbox_mode: input.sandbox_mode,
            read_only,
            workdir: input.projects_dir,
            command,
            env: default_env(&input.home),
            mounts,
            blocked_mounts,
        })
    }

    pub fn to_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("image: {}\n", self.image));
        out.push_str(&format!("sandbox: {}\n", self.sandbox_mode.as_str()));
        out.push_str(&format!(
            "read-only: {}\n",
            if self.read_only { "yes" } else { "no" }
        ));
        out.push_str(&format!("workdir: {}\n", self.workdir.display()));
        out.push_str(&format!("command: {}\n", self.command.join(" ")));

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
