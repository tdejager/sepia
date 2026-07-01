use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use directories::UserDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct SessionPaths {
    pub root: PathBuf,
    pub frames_dir: PathBuf,
    pub steps_dir: PathBuf,
    pub video: PathBuf,
    pub session_json: PathBuf,
    pub timeline_json: PathBuf,
    pub timeline_md: PathBuf,
    pub summary_md: PathBuf,
    pub inspect_html: PathBuf,
    pub pr_comment_md: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatestSession {
    pub latest_session: PathBuf,
}

impl SessionPaths {
    pub fn create(output_root: PathBuf, name: &str, now: DateTime<Local>) -> Result<Self> {
        let session_name = format!("{}-{}", now.format("%Y-%m-%d-%H%M%S"), slugify(name));
        let root = output_root.join(session_name);
        let paths = Self::from_root(root);
        fs::create_dir_all(&paths.frames_dir).with_context(|| {
            format!("failed to create frames dir {}", paths.frames_dir.display())
        })?;
        fs::create_dir_all(&paths.steps_dir)
            .with_context(|| format!("failed to create steps dir {}", paths.steps_dir.display()))?;
        Ok(paths)
    }

    #[must_use]
    pub fn from_root(root: PathBuf) -> Self {
        Self {
            frames_dir: root.join("frames"),
            steps_dir: root.join("steps"),
            video: root.join("demo.mp4"),
            session_json: root.join("session.json"),
            timeline_json: root.join("timeline.json"),
            timeline_md: root.join("timeline.md"),
            summary_md: root.join("summary.md"),
            inspect_html: root.join("inspect.html"),
            pr_comment_md: root.join("pr-comment.md"),
            root,
        }
    }

    #[must_use]
    pub fn frame_path(&self, frame_number: u32) -> PathBuf {
        self.frames_dir.join(format!("frame-{frame_number:06}.png"))
    }
}

pub fn default_output_root() -> Result<PathBuf> {
    let user_dirs = UserDirs::new().context("could not locate user directories")?;
    Ok(user_dirs
        .download_dir()
        .unwrap_or(user_dirs.home_dir())
        .join("sepia"))
}

pub fn latest_file(output_root: PathBuf) -> PathBuf {
    output_root.join("latest.json")
}

pub fn write_latest(output_root: PathBuf, session_root: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_root)
        .with_context(|| format!("failed to create output root {}", output_root.display()))?;
    let latest = LatestSession {
        latest_session: session_root,
    };
    let text = serde_json::to_string_pretty(&latest)?;
    fs::write(latest_file(output_root), text).context("failed to write latest session pointer")
}

pub fn read_latest(output_root: PathBuf) -> Result<LatestSession> {
    let path = latest_file(output_root);
    let text = fs::read_to_string(&path).with_context(|| {
        format!(
            "failed to read latest session pointer at {}",
            path.display()
        )
    })?;
    serde_json::from_str(&text).context("failed to parse latest session pointer")
}

#[must_use]
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "demo".into()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn slugifies_human_names() {
        assert_eq!(slugify("Windowed Browse Demo!"), "windowed-browse-demo");
        assert_eq!(slugify("---"), "demo");
    }

    #[test]
    fn writes_and_reads_latest_pointer() {
        let dir = tempdir().unwrap();
        let session = dir.path().join("2026-demo");
        write_latest(dir.path().to_path_buf(), session.clone()).unwrap();
        assert_eq!(
            read_latest(dir.path().to_path_buf())
                .unwrap()
                .latest_session,
            session
        );
    }

    #[test]
    fn creates_predictable_session_paths() {
        let dir = tempdir().unwrap();
        let now = Local.with_ymd_and_hms(2026, 7, 1, 12, 30, 0).unwrap();
        let paths = SessionPaths::create(dir.path().to_path_buf(), "My Demo", now).unwrap();
        assert!(paths.root.ends_with("2026-07-01-123000-my-demo"));
        assert_eq!(paths.frame_path(7).file_name().unwrap(), "frame-000007.png");
    }
}
