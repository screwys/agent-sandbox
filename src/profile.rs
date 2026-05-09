use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("invalid profile: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("trailing arguments are not allowed for helper: {0}")]
    TrailingArgsNotAllowed(String),
    #[error("trailing argument is not allowed: {0}")]
    TrailingArgNotAllowed(String),
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Profile {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default, rename = "mount")]
    pub mounts: Vec<Mount>,
    #[serde(default, rename = "host_helper")]
    pub host_helpers: Vec<HostHelper>,
}

impl Profile {
    pub fn from_toml_str(input: &str) -> Result<Self, ProfileError> {
        Ok(toml::from_str(input)?)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Settings {
    pub projects_dir: Option<String>,
    pub sandbox: Option<SandboxMode>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    Strict,
    Comfortable,
    Disabled,
}

impl SandboxMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "strict" => Some(Self::Strict),
            "comfortable" => Some(Self::Comfortable),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Comfortable => "comfortable",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Mount {
    pub path: String,
    #[serde(default)]
    pub mode: MountMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountMode {
    ReadWrite,
    ReadOnly,
}

impl Default for MountMode {
    fn default() -> Self {
        Self::ReadWrite
    }
}

impl MountMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadWrite => "rw",
            Self::ReadOnly => "ro",
        }
    }
}

impl<'de> Deserialize<'de> for MountMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "rw" => Ok(Self::ReadWrite),
            "ro" => Ok(Self::ReadOnly),
            other => Err(serde::de::Error::custom(format!(
                "invalid mount mode {other:?}; expected \"rw\" or \"ro\""
            ))),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct HostHelper {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub allowed_trailing_args: Vec<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

impl HostHelper {
    pub fn command_plan(&self, trailing_args: &[String]) -> Result<HostCommandPlan, ProfileError> {
        if self.allowed_trailing_args.is_empty() && !trailing_args.is_empty() {
            return Err(ProfileError::TrailingArgsNotAllowed(self.name.clone()));
        }

        for arg in trailing_args {
            if !self
                .allowed_trailing_args
                .iter()
                .any(|allowed| allowed == arg)
            {
                return Err(ProfileError::TrailingArgNotAllowed(arg.clone()));
            }
        }

        let mut argv = Vec::with_capacity(1 + self.args.len() + trailing_args.len());
        argv.push(self.command.clone());
        argv.extend(self.args.iter().cloned());
        argv.extend(trailing_args.iter().cloned());

        Ok(HostCommandPlan {
            helper: self.name.clone(),
            argv,
            timeout_secs: self.timeout_secs,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCommandPlan {
    pub helper: String,
    pub argv: Vec<String>,
    pub timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    30
}
