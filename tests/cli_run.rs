use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn cli_run_uses_fake_agent_browser_and_ffmpeg() {
    let temp = tempdir().unwrap();
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let log = temp.path().join("commands.log");

    write_executable(
        &bin_dir.join("agent-browser"),
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

    write_executable(
        &bin_dir.join("ffmpeg"),
        r#"#!/bin/sh
echo "ffmpeg $@" >> "$SEPIA_FAKE_LOG"
for last do :; done
mkdir -p "$(dirname "$last")"
printf 'fake mp4' > "$last"
exit 0
"#,
    );

    let output_root = temp.path().join("out");
    let existing_path = std::env::var("PATH").unwrap_or_default();
    let fake_path = format!("{}:{existing_path}", bin_dir.display());

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

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}
