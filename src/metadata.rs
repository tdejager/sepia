use std::{fs, path::PathBuf};

use miette::Result;

use crate::ResultContextExt;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::{
    config::{DemoConfig, StepKind},
    session::SessionPaths,
};

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
    #[serde(default)]
    pub steps: Vec<SessionStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionScreenshot {
    pub label: String,
    pub path: PathBuf,
}

/// One script step as it was actually captured, with enough timing to drive the
/// inspect UI (per-step video seek points and thumbnails).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionStep {
    pub name: String,
    pub kind: StepKind,
    /// 1-based index of the first video frame belonging to this step.
    pub start_frame: u32,
    pub captured_frames: u32,
    pub hold_frames: u32,
    #[serde(default)]
    pub screenshot: Option<PathBuf>,
}

impl SessionMetadata {
    #[must_use]
    pub fn from_capture(
        config: &DemoConfig,
        paths: &SessionPaths,
        created_at: DateTime<Local>,
        frame_count: u32,
        screenshots: Vec<SessionScreenshot>,
        steps: Vec<SessionStep>,
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
            steps,
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
    let text =
        serde_json::to_string_pretty(metadata).context("failed to encode session metadata")?;
    fs::write(&paths.session_json, text)
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
            steps: vec![],
        };
        assert_eq!(metadata.duration_seconds(), 2.0);
    }
}
