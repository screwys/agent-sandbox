use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum ProfileError {
    Parse(String),
    TrailingArgsNotAllowed(String),
    TrailingArgNotAllowed(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(err) => write!(f, "invalid profile: {err}"),
            Self::TrailingArgsNotAllowed(helper) => {
                write!(f, "trailing arguments are not allowed for helper: {helper}")
            }
            Self::TrailingArgNotAllowed(arg) => {
                write!(f, "trailing argument is not allowed: {arg}")
            }
        }
    }
}

impl Error for ProfileError {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    pub settings: Settings,
    pub mounts: Vec<Mount>,
    pub host_helpers: Vec<HostHelper>,
}

impl Profile {
    pub fn from_toml_str(input: &str) -> Result<Self, ProfileError> {
        parse_profile(input)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub projects_dir: Option<String>,
    pub sandbox: Option<SandboxMode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    pub path: String,
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
    pub fn parse(value: &str) -> Result<Self, ProfileError> {
        match value {
            "rw" => Ok(Self::ReadWrite),
            "ro" => Ok(Self::ReadOnly),
            other => Err(ProfileError::Parse(format!(
                "invalid mount mode {other:?}; expected \"rw\" or \"ro\""
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadWrite => "rw",
            Self::ReadOnly => "ro",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostHelper {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub allowed_trailing_args: Vec<String>,
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

enum Section {
    None,
    Settings,
    Mount(usize),
    HostHelper(usize),
}

fn parse_profile(input: &str) -> Result<Profile, ProfileError> {
    let mut profile = Profile::default();
    let mut section = Section::None;
    for raw in input.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        match line {
            "[settings]" => {
                section = Section::Settings;
                continue;
            }
            "[[mount]]" => {
                profile.mounts.push(Mount {
                    path: String::new(),
                    mode: MountMode::ReadWrite,
                });
                section = Section::Mount(profile.mounts.len() - 1);
                continue;
            }
            "[[host_helper]]" => {
                profile.host_helpers.push(HostHelper {
                    name: String::new(),
                    command: String::new(),
                    args: Vec::new(),
                    allowed_trailing_args: Vec::new(),
                    timeout_secs: 30,
                });
                section = Section::HostHelper(profile.host_helpers.len() - 1);
                continue;
            }
            _ => {}
        }

        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| ProfileError::Parse(format!("expected key = value: {line}")))?;
        let key = key.trim();
        let value = value.trim();
        match section {
            Section::Settings => match key {
                "projects_dir" => profile.settings.projects_dir = Some(parse_string(value)?),
                "sandbox" => {
                    let value = parse_string(value)?;
                    profile.settings.sandbox =
                        Some(SandboxMode::parse(&value).ok_or_else(|| {
                            ProfileError::Parse(format!("invalid sandbox mode: {value}"))
                        })?);
                }
                other => {
                    return Err(ProfileError::Parse(format!(
                        "unknown settings key: {other}"
                    )));
                }
            },
            Section::Mount(index) => match key {
                "path" => profile.mounts[index].path = parse_string(value)?,
                "mode" => profile.mounts[index].mode = MountMode::parse(&parse_string(value)?)?,
                other => return Err(ProfileError::Parse(format!("unknown mount key: {other}"))),
            },
            Section::HostHelper(index) => match key {
                "name" => profile.host_helpers[index].name = parse_string(value)?,
                "command" => profile.host_helpers[index].command = parse_string(value)?,
                "args" => profile.host_helpers[index].args = parse_string_array(value)?,
                "allowed_trailing_args" => {
                    profile.host_helpers[index].allowed_trailing_args = parse_string_array(value)?;
                }
                "timeout_secs" => {
                    profile.host_helpers[index].timeout_secs = value.parse().map_err(|_| {
                        ProfileError::Parse(format!("invalid timeout_secs: {value}"))
                    })?;
                }
                other => {
                    return Err(ProfileError::Parse(format!(
                        "unknown host_helper key: {other}"
                    )));
                }
            },
            Section::None => {
                return Err(ProfileError::Parse(format!("key outside a section: {key}")));
            }
        }
    }
    Ok(profile)
}

fn parse_string(value: &str) -> Result<String, ProfileError> {
    let value = value.trim();
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        Ok(unescape_basic(&value[1..value.len() - 1]))
    } else {
        Err(ProfileError::Parse(format!(
            "expected quoted string: {value}"
        )))
    }
}

fn parse_string_array(value: &str) -> Result<Vec<String>, ProfileError> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return Err(ProfileError::Parse(format!(
            "expected string array: {value}"
        )));
    }
    let inner = value[1..value.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|part| parse_string(part.trim()))
        .collect()
}

fn unescape_basic(value: &str) -> String {
    value
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
        .replace("\\n", "\n")
}
