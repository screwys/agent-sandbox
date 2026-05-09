use std::env;
use std::fs;
use std::path::PathBuf;

use agent_sandbox::profile::{Profile, SandboxMode};
use agent_sandbox::run_plan::{CommandKind, RunPlan, RunPlanInput};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [scope, action, rest @ ..] if scope == "agent" && action == "run-plan" => {
            print_agent_run_plan(rest)
        }
        _ => Err(
            "usage: agent-sandbox agent run-plan codex|shell|exec [--profile PATH] [-- ARG...]"
                .to_string(),
        ),
    }
}

fn print_agent_run_plan(args: &[String]) -> Result<(), String> {
    let (kind, rest) = args
        .split_first()
        .ok_or_else(|| "missing run-plan target: codex, shell, or exec".to_string())?;
    let parsed = ParsedRunPlanArgs::parse(rest)?;
    let profiles = parsed.load_profiles()?;

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

    let home = env_path("HOME").ok_or_else(|| "HOME is not set".to_string())?;
    let state_dir = env_path("AGENT_STATE_DIR").unwrap_or_else(|| home.join(".agent-sandbox"));
    let projects_dir = env_path("AGENT_PROJECTS_DIR").unwrap_or_else(|| home.join("Projects"));
    let broker_dir = env_path("AGENT_BROKER_DIR").unwrap_or_else(|| {
        env_path("XDG_RUNTIME_DIR")
            .unwrap_or_else(|| PathBuf::from("/run/agent-sandbox"))
            .join("agent-sandbox")
    });
    let sandbox_mode = env::var("AGENT_SANDBOX")
        .ok()
        .and_then(|value| SandboxMode::parse(&value))
        .unwrap_or(SandboxMode::Strict);

    let plan = RunPlan::build(RunPlanInput {
        home,
        repo: env_path("AGENT_REPO").unwrap_or(env::current_dir().map_err(|err| err.to_string())?),
        projects_dir,
        agent_home: env_path("AGENT_HOME").unwrap_or_else(|| state_dir.join("home")),
        broker_dir,
        image: env::var("AGENT_IMAGE")
            .unwrap_or_else(|_| "localhost/agent-sandbox:latest".to_string()),
        sandbox_mode,
        command,
        profiles,
    })
    .map_err(|err| err.to_string())?;

    print!("{}", plan.to_human());
    Ok(())
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

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).map(PathBuf::from)
}
