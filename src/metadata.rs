use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::{config::DemoConfig, session::SessionPaths};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionMetadata {
    pub name: String,
    pub description: Option<String>,
    pub url: String,
    pub created_at: DateTime<Local>,
    pub output_fps: u32,
    pub frame_count: u32,
    pub video: PathBuf,
    pub inspect: PathBuf,
    pub screenshots: Vec<SessionScreenshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionScreenshot {
    pub label: String,
    pub path: PathBuf,
}

impl SessionMetadata {
    #[must_use]
    pub fn from_capture(
        config: &DemoConfig,
        paths: &SessionPaths,
        created_at: DateTime<Local>,
        frame_count: u32,
        screenshots: Vec<SessionScreenshot>,
    ) -> Self {
        Self {
            name: config.name.clone(),
            description: config.description.clone(),
            url: config.url.clone(),
            created_at,
            output_fps: config.capture.output_fps,
            frame_count,
            video: paths.video.clone(),
            inspect: paths.inspect_html.clone(),
            screenshots,
        }
    }

    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        if self.output_fps == 0 {
            0.0
        } else {
            f64::from(self.frame_count) / f64::from(self.output_fps)
        }
    }
}

pub fn write_session_metadata(paths: &SessionPaths, metadata: &SessionMetadata) -> Result<()> {
    fs::write(&paths.session_json, serde_json::to_string_pretty(metadata)?)
        .with_context(|| format!("failed to write {}", paths.session_json.display()))
}

pub fn read_session_metadata(paths: &SessionPaths) -> Result<SessionMetadata> {
    let text = fs::read_to_string(&paths.session_json)
        .with_context(|| format!("failed to read {}", paths.session_json.display()))?;
    serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", paths.session_json.display()))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn computes_duration_from_frame_count_and_fps() {
        let metadata = SessionMetadata {
            name: "demo".into(),
            description: None,
            url: "http://localhost".into(),
            created_at: Local.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
            output_fps: 24,
            frame_count: 48,
            video: PathBuf::from("demo.mp4"),
            inspect: PathBuf::from("inspect.html"),
            screenshots: vec![],
        };
        assert_eq!(metadata.duration_seconds(), 2.0);
    }
}
