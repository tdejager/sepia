use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn cli_run_uses_fake_agent_browser_and_ffmpeg() {
    let temp = tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let log = temp.path().join("commands.log");

    write_fake_agent_browser(&bin_dir.join("agent-browser"));
    write_fake_ffmpeg(&bin_dir.join("ffmpeg"));

    let output_root = temp.path().join("out");
    let fake_path = fake_path_with(&bin_dir);

    Command::cargo_bin("sepia")
        .unwrap()
        .args([
            "run",
            "examples/basilisk-windowed-browse.toml",
            "--output-root",
            output_root.to_str().unwrap(),
        ])
        .env("PATH", fake_path)
        .env("SEPIA_FAKE_LOG", &log)
        .assert()
        .success();

    let latest = fs::read_to_string(output_root.join("latest.json")).unwrap();
    let latest: serde_json::Value = serde_json::from_str(&latest).unwrap();
    let session = Path::new(latest["latest_session"].as_str().unwrap());

    assert!(session.join("demo.mp4").exists());
    assert!(session.join("inspect.html").exists());
    assert!(session.join("session.json").exists());
    assert!(session.join("timeline.json").exists());
    assert!(session.join("pr-comment.md").exists());

    let log = fs::read_to_string(log).unwrap();
    assert!(log.contains("agent-browser --session basilisk-demo open http://localhost:3001"));
    assert!(log.contains("ffmpeg -y -framerate 24"));
}

#[test]
fn cli_run_inspect_and_pr_dry_run_are_integrated_without_real_tools() {
    let temp = tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let log = temp.path().join("commands.log");
    let output_root = temp.path().join("out");
    let config = temp.path().join("demo.toml");

    fs::write(
        &config,
        r##"
name = "integrated-demo"
description = "Integrated CLI smoke test"
url = "http://localhost:3456"
session = "integrated-session"

[capture]
output_fps = 4
default_hold_ms = 0
default_action_ms = 0

[[steps]]
name = "Initial"
wait_ms = 0
hold_ms = 0
screenshot = true

[[steps]]
name = "Click tab"
eval = "document.body.dataset.sepia = 'ok'"
hold_ms = 0
screenshot = false
"##,
    )
    .unwrap();

    write_fake_agent_browser(&bin_dir.join("agent-browser"));
    write_fake_ffmpeg(&bin_dir.join("ffmpeg"));
    write_fake_opener(&bin_dir.join(opener_name()));
    write_executable(
        &bin_dir.join("gh"),
        r#"#!/bin/sh
echo "gh $@" >> "$SEPIA_FAKE_LOG"
exit 42
"#,
    );

    let fake_path = fake_path_with(&bin_dir);

    Command::cargo_bin("sepia")
        .unwrap()
        .args([
            "run",
            config.to_str().unwrap(),
            "--output-root",
            output_root.to_str().unwrap(),
        ])
        .env("PATH", &fake_path)
        .env("SEPIA_FAKE_LOG", &log)
        .assert()
        .success();

    Command::cargo_bin("sepia")
        .unwrap()
        .args(["inspect", "--output-root", output_root.to_str().unwrap()])
        .env("PATH", &fake_path)
        .env("SEPIA_FAKE_LOG", &log)
        .assert()
        .success();

    let pr_output = Command::cargo_bin("sepia")
        .unwrap()
        .args([
            "pr",
            "--dry-run",
            "--output-root",
            output_root.to_str().unwrap(),
        ])
        .env("PATH", &fake_path)
        .env("SEPIA_FAKE_LOG", &log)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let pr_output = String::from_utf8(pr_output).unwrap();

    let latest = fs::read_to_string(output_root.join("latest.json")).unwrap();
    let latest: serde_json::Value = serde_json::from_str(&latest).unwrap();
    let session = Path::new(latest["latest_session"].as_str().unwrap());

    assert!(session.join("demo.mp4").exists());
    assert!(session.join("inspect.html").exists());
    assert!(session.join("pr-comment.md").exists());
    assert!(pr_output.contains("<!-- sepia:pr-demo:start -->"));
    assert!(pr_output.contains("Integrated CLI smoke test"));
    assert!(pr_output.contains(&format!("file://{}", session.join("demo.mp4").display())));

    let log = wait_for_log_contains(&log, opener_name());
    assert!(log.contains("agent-browser --session integrated-session open http://localhost:3456"));
    assert!(log.contains(
        "agent-browser --session integrated-session eval document.body.dataset.sepia = 'ok'"
    ));
    assert!(log.contains("ffmpeg -y -framerate 4"));
    assert!(log.contains(&format!(
        "{} {}",
        opener_name(),
        session.join("inspect.html").display()
    )));
    assert!(
        !log.contains("gh "),
        "dry-run unexpectedly invoked gh: {log}"
    );
}

#[test]
fn cli_run_with_failing_ffmpeg_still_points_latest_at_new_session() {
    let temp = tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let log = temp.path().join("commands.log");

    write_fake_agent_browser(&bin_dir.join("agent-browser"));
    // Simulate the bundled ffmpeg crashing at encode time (e.g. a missing dylib).
    write_executable(
        &bin_dir.join("ffmpeg"),
        r#"#!/bin/sh
echo "ffmpeg $@" >> "$SEPIA_FAKE_LOG"
echo "dyld: Library not loaded: @rpath/libjxl.0.11.dylib" >&2
exit 1
"#,
    );

    let output_root = temp.path().join("out");
    let stale_session = output_root.join("2026-01-01-000000-older-demo");
    fs::create_dir_all(&stale_session).unwrap();
    fs::write(
        output_root.join("latest.json"),
        serde_json::json!({ "latest_session": stale_session }).to_string(),
    )
    .unwrap();

    Command::cargo_bin("sepia")
        .unwrap()
        .args([
            "run",
            "examples/basilisk-windowed-browse.toml",
            "--output-root",
            output_root.to_str().unwrap(),
        ])
        .env("PATH", fake_path_with(&bin_dir))
        .env("SEPIA_FAKE_LOG", &log)
        .assert()
        .failure();

    let latest = fs::read_to_string(output_root.join("latest.json")).unwrap();
    let latest: serde_json::Value = serde_json::from_str(&latest).unwrap();
    let session = Path::new(latest["latest_session"].as_str().unwrap());

    assert_ne!(
        session, stale_session,
        "latest.json still points at the older session"
    );
    // Everything except the video should survive a failed encode, so the
    // frames can be encoded manually afterwards.
    assert!(session.join("frames").exists());
    assert!(session.join("session.json").exists());
    assert!(session.join("pr-comment.md").exists());
    assert!(!session.join("demo.mp4").exists());
}

fn write_fake_agent_browser(path: &Path) {
    write_executable(
        path,
        r#"#!/bin/sh
echo "agent-browser $@" >> "$SEPIA_FAKE_LOG"
previous=""
for arg in "$@"; do
  if [ "$previous" = "screenshot" ]; then
    mkdir -p "$(dirname "$arg")"
    printf 'fake png' > "$arg"
    exit 0
  fi
  previous="$arg"
done
exit 0
"#,
    );
}

fn write_fake_ffmpeg(path: &Path) {
    write_executable(
        path,
        r#"#!/bin/sh
echo "ffmpeg $@" >> "$SEPIA_FAKE_LOG"
for last do :; done
mkdir -p "$(dirname "$last")"
printf 'fake mp4' > "$last"
exit 0
"#,
    );
}

fn write_fake_opener(path: &Path) {
    write_executable(
        path,
        r#"#!/bin/sh
name=${0##*/}
echo "$name $@" >> "$SEPIA_FAKE_LOG"
exit 0
"#,
    );
}

fn opener_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "xdg-open"
    }
}

fn fake_path_with(bin_dir: &Path) -> String {
    let existing_path = std::env::var("PATH").unwrap_or_default();
    format!("{}:{existing_path}", bin_dir.display())
}

fn wait_for_log_contains(path: &Path, needle: &str) -> String {
    let start = Instant::now();
    loop {
        let text = fs::read_to_string(path).unwrap_or_default();
        if text.contains(needle) || start.elapsed() > Duration::from_secs(2) {
            return text;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}
