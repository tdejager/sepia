use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub url: String,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub steps: Vec<StepConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    #[serde(default = "default_output_fps")]
    pub output_fps: u32,
    #[serde(default = "default_hold_ms")]
    pub default_hold_ms: u64,
    #[serde(default = "default_action_ms")]
    pub default_action_ms: u64,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            output_fps: default_output_fps(),
            default_hold_ms: default_hold_ms(),
            default_action_ms: default_action_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepConfig {
    pub name: String,
    #[serde(default)]
    pub wait_ms: Option<u64>,
    #[serde(default)]
    pub eval: Option<String>,
    #[serde(default)]
    pub fill: Option<FillConfig>,
    #[serde(default)]
    pub scroll: Option<ScrollConfig>,
    #[serde(default)]
    pub hold_ms: Option<u64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub frames: Option<u32>,
    #[serde(default)]
    pub screenshot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillConfig {
    pub selector: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrollConfig {
    pub selector: String,
    pub pixels: i64,
}

impl DemoConfig {
    pub fn from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read demo config at {}", path.display()))?;
        let config: Self = toml::from_str(&text)
            .with_context(|| format!("failed to parse TOML demo config at {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("demo config `name` must not be empty");
        }
        if self.url.trim().is_empty() {
            bail!("demo config `url` must not be empty");
        }
        if self.capture.output_fps == 0 {
            bail!("capture.output_fps must be greater than 0");
        }
        for step in &self.steps {
            step.validate()?;
        }
        Ok(())
    }
}

impl StepConfig {
    pub fn action_count(&self) -> usize {
        usize::from(self.wait_ms.is_some())
            + usize::from(self.eval.is_some())
            + usize::from(self.fill.is_some())
            + usize::from(self.scroll.is_some())
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("step name must not be empty");
        }
        if self.action_count() > 1 {
            bail!(
                "step `{}` has multiple actions; use exactly one of wait_ms, eval, fill, or scroll",
                self.name
            );
        }
        if matches!(self.frames, Some(0)) {
            bail!("step `{}` frames must be greater than 0", self.name);
        }
        Ok(())
    }
}

fn default_output_fps() -> u32 {
    24
}

fn default_hold_ms() -> u64 {
    700
}

fn default_action_ms() -> u64 {
    400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_readable_config_with_timeline_granularity() {
        let config: DemoConfig = toml::from_str(
            r#"
name = "windowed-browse"
description = "Windowed package and advisory browse demo"
url = "http://localhost:3001"

[capture]
output_fps = 24
default_hold_ms = 800

[[steps]]
name = "Scroll packages"
scroll = { selector = ".package-list", pixels = 900 }
duration_ms = 1600
frames = 32
screenshot = true
"#,
        )
        .unwrap();

        config.validate().unwrap();
        assert_eq!(config.capture.output_fps, 24);
        assert_eq!(config.steps[0].frames, Some(32));
    }

    #[test]
    fn rejects_multiple_actions_in_one_step() {
        let step = StepConfig {
            name: "too much".into(),
            wait_ms: Some(1),
            eval: Some("console.log(1)".into()),
            fill: None,
            scroll: None,
            hold_ms: None,
            duration_ms: None,
            frames: None,
            screenshot: false,
        };

        assert!(step.validate().is_err());
    }
}
