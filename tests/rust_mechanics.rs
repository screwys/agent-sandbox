use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

use agent_sandbox::{
    profile::{MountMode, Profile, SandboxMode},
    run_plan::{CommandKind, RunPlan, RunPlanInput},
};

#[test]
fn parses_structured_profile_mounts_and_host_helpers() {
    let profile = Profile::from_toml_str(
        r#"
        [settings]
        projects_dir = "~/Projects"
        sandbox = "strict"

        [[mount]]
        path = "~/.config/niri"
        mode = "rw"

        [[host_helper]]
        name = "systemd.user.restart"
        command = "/usr/bin/systemctl"
        args = ["--user", "restart"]
        allowed_trailing_args = ["igloo.service", "rsshub.service"]
        timeout_secs = 30
        "#,
    )
    .expect("profile parses");

    assert_eq!(profile.settings.projects_dir.as_deref(), Some("~/Projects"));
    assert_eq!(profile.settings.sandbox, Some(SandboxMode::Strict));
    assert_eq!(profile.mounts.len(), 1);
    assert_eq!(profile.mounts[0].path, "~/.config/niri");
    assert_eq!(profile.mounts[0].mode, MountMode::ReadWrite);
    assert_eq!(profile.host_helpers.len(), 1);
    assert_eq!(profile.host_helpers[0].name, "systemd.user.restart");
    assert_eq!(profile.host_helpers[0].timeout_secs, 30);
}

#[test]
fn host_helper_accepts_only_allowed_trailing_args() {
    let profile = Profile::from_toml_str(
        r#"
        [[host_helper]]
        name = "systemd.user.restart"
        command = "/usr/bin/systemctl"
        args = ["--user", "restart"]
        allowed_trailing_args = ["igloo.service", "rsshub.service"]
        timeout_secs = 30
        "#,
    )
    .expect("profile parses");
    let helper = &profile.host_helpers[0];

    let accepted = helper
        .command_plan(&["igloo.service".to_string()])
        .expect("allowed unit is accepted");
    assert_eq!(
        accepted.argv,
        vec!["/usr/bin/systemctl", "--user", "restart", "igloo.service"]
    );

    let rejected = helper
        .command_plan(&["bunker.service".to_string()])
        .expect_err("unlisted unit is rejected");
    assert!(
        rejected
            .to_string()
            .contains("trailing argument is not allowed: bunker.service")
    );
}

#[test]
fn run_plan_keeps_podman_mechanics_typed_and_blocks_repo_covering_mounts() {
    let profile = Profile::from_toml_str(
        r#"
        [[mount]]
        path = "/home/screwy/.config/niri"
        mode = "rw"

        [[mount]]
        path = "/home/screwy/Projects"
        mode = "rw"
        "#,
    )
    .expect("profile parses");

    let plan = RunPlan::build(RunPlanInput {
        home: PathBuf::from("/home/screwy"),
        repo: PathBuf::from("/home/screwy/Projects/agent-sandbox"),
        projects_dir: PathBuf::from("/home/screwy/Projects"),
        agent_home: PathBuf::from("/home/screwy/.agent-sandbox/home"),
        broker_dir: PathBuf::from("/run/user/1000/agent-sandbox"),
        image: "localhost/agent-sandbox:latest".to_string(),
        sandbox_mode: SandboxMode::Strict,
        command: CommandKind::Codex(vec!["--help".to_string()]),
        profiles: vec![profile],
    })
    .expect("run plan builds");

    assert_eq!(plan.image, "localhost/agent-sandbox:latest");
    assert_eq!(plan.command, vec!["codex", "--help"]);
    assert!(plan.read_only);
    assert!(
        plan.env
            .iter()
            .any(|entry| entry.key == "AGENT_BROKER_SOCKET"
                && entry.value == "/run/agent-sandbox/broker.sock")
    );
    assert!(plan.mounts.iter().any(|mount| {
        mount.source == PathBuf::from("/home/screwy/.config/niri")
            && mount.target == PathBuf::from("/home/screwy/.config/niri")
            && mount.mode == MountMode::ReadWrite
    }));
    assert!(plan.mounts.iter().any(|mount| {
        mount.source == PathBuf::from("/home/screwy/Projects/agent-sandbox")
            && mount.mode == MountMode::ReadOnly
    }));

    let blocked = plan
        .blocked_mounts
        .iter()
        .find(|mount| mount.source == PathBuf::from("/home/screwy/Projects"))
        .expect("project-root profile mount is blocked");
    assert_eq!(blocked.reason, "mount covers protected launcher repo");
}

#[test]
fn cli_prints_human_readable_run_plan_for_codex() {
    let root = env::temp_dir().join(format!(
        "agent-sandbox-run-plan-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let home = root.join("home");
    let projects = home.join("Projects");
    let repo = projects.join("agent-sandbox");
    let runtime = root.join("runtime");
    fs::create_dir_all(&repo).expect("repo dir");
    fs::create_dir_all(&runtime).expect("runtime dir");

    let profile_path = root.join("profile.toml");
    fs::write(
        &profile_path,
        format!(
            r#"
            [[mount]]
            path = "{}/.config/niri"
            mode = "rw"

            [[mount]]
            path = "{}"
            mode = "rw"
            "#,
            home.display(),
            projects.display()
        ),
    )
    .expect("profile");

    let output = Command::new(env!("CARGO_BIN_EXE_agent-sandbox"))
        .args([
            "agent",
            "run-plan",
            "codex",
            "--profile",
            profile_path.to_str().expect("profile path"),
            "--",
            "--model",
            "gpt-5",
        ])
        .env("HOME", &home)
        .env("AGENT_REPO", &repo)
        .env("AGENT_PROJECTS_DIR", &projects)
        .env("XDG_RUNTIME_DIR", &runtime)
        .output()
        .expect("run agent-sandbox");

    let _ = fs::remove_dir_all(&root);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(stdout.contains("image: localhost/agent-sandbox:latest"));
    assert!(stdout.contains("sandbox: strict"));
    assert!(stdout.contains("read-only: yes"));
    assert!(stdout.contains("command: codex --model gpt-5"));
    assert!(stdout.contains(&format!(
        "mount: {} -> {} rw",
        home.join(".config/niri").display(),
        home.join(".config/niri").display()
    )));
    assert!(stdout.contains(&format!(
        "blocked: {} (mount covers protected launcher repo)",
        projects.display()
    )));
}
