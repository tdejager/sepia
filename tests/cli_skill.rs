use std::{fs, path::Path};

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn skill_install_list_and_remove_use_detected_agents_in_temp_home() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let xdg = temp.path().join("xdg");
    let project = temp.path().join("project");
    fs::create_dir_all(home.join(".pi/agent")).unwrap();
    fs::create_dir_all(&xdg).unwrap();
    fs::create_dir_all(&project).unwrap();

    sepia_skill_command(&home, &xdg, &project)
        .args(["skill", "install"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Installed Sepia skill for: pi"));

    let skill_md = home.join(".pi/agent/skills/sepia/SKILL.md");
    assert!(skill_md.exists());
    assert!(
        fs::read_to_string(&skill_md)
            .unwrap()
            .contains("name: sepia")
    );

    sepia_skill_command(&home, &xdg, &project)
        .args(["skill", "list", "--agent", "pi"])
        .assert()
        .success()
        .stdout(predicates::str::contains("sepia"));

    sepia_skill_command(&home, &xdg, &project)
        .args(["skill", "remove", "--agent", "pi"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Removed Sepia skill"));

    assert!(!home.join(".pi/agent/skills/sepia").exists());
}

#[test]
fn normal_commands_suggest_skill_install_when_detected_agent_is_missing_it() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let xdg = temp.path().join("xdg");
    let project = temp.path().join("project");
    fs::create_dir_all(home.join(".pi/agent")).unwrap();
    fs::create_dir_all(&xdg).unwrap();
    fs::create_dir_all(&project).unwrap();

    sepia_skill_command(&home, &xdg, &project)
        .args([
            "pr",
            "--dry-run",
            "--output-root",
            temp.path().join("out").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Tip: install the Sepia agent skill for pi with `sepia skill install`.",
        ));
}

#[test]
fn skill_install_honors_explicit_agent_filter_without_detection() {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let xdg = temp.path().join("xdg");
    let project = temp.path().join("project");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&xdg).unwrap();
    fs::create_dir_all(&project).unwrap();

    sepia_skill_command(&home, &xdg, &project)
        .args(["skill", "install", "--agent", "pi"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Installed Sepia skill for: pi"));

    assert!(home.join(".pi/agent/skills/sepia/SKILL.md").exists());
}

fn sepia_skill_command(home: &Path, xdg: &Path, project: &Path) -> Command {
    let mut command = Command::cargo_bin("sepia").unwrap();
    command
        .current_dir(project)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg)
        .env("CODEX_HOME", home.join(".codex"))
        .env("CLAUDE_CONFIG_DIR", home.join(".claude"));
    command
}
