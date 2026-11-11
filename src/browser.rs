use std::{path::Path, process::Command};

use miette::{Result, bail};
use serde::Deserialize;

use crate::{ResultContextExt, screencast::ScreencastRecording, session::SessionPaths};

pub trait BrowserBackend {
    fn open(&self, url: &str) -> Result<()>;
    fn eval(&self, js: &str) -> Result<()>;
    fn fill(&self, selector: &str, text: &str) -> Result<()>;
    fn screenshot(&self, path: &Path) -> Result<()>;
    /// Start recording the active page as `paths.frame_path(n)` PNG frames at
    /// `fps`. Nothing is written until [`Recording::roll`] asks for frames.
    fn start_recording(&self, paths: &SessionPaths, fps: u32) -> Result<Box<dyn Recording>>;
    /// Set the browser viewport before navigation so responsive layouts are deterministic.
    fn set_viewport(&self, _width: u32, _height: u32) -> Result<()> {
        Ok(())
    }
    /// Wait for a selector before a step/action. Defaults to a no-op for tests/custom backends.
    fn wait_for_selector(&self, _selector: &str) -> Result<()> {
        Ok(())
    }
    /// Check the backend is ready before a capture starts. Defaults to a no-op.
    fn preflight(&self) -> Result<()> {
        Ok(())
    }
}

/// A live recording of the browser page. Dropping it stops the recording.
pub trait Recording {
    /// Record the next `frames` video frames of live footage (`frames / fps`
    /// seconds of real time), calling `on_frame` as each one lands on disk.
    /// Whatever the page does between rolls stays off camera.
    fn roll(&mut self, frames: u32, on_frame: &mut dyn FnMut()) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct AgentBrowserBackend {
    session: String,
    binary: String,
}

impl AgentBrowserBackend {
    #[must_use]
    pub fn new(session: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            binary: "agent-browser".into(),
        }
    }

    #[must_use]
    pub fn with_binary(mut self, binary: impl Into<String>) -> Self {
        self.binary = binary.into();
        self
    }

    fn run(&self, args: &[String]) -> Result<()> {
        self.output(args).map(drop)
    }

    /// Run an agent-browser command and return its stdout.
    fn output(&self, args: &[String]) -> Result<Vec<u8>> {
        let mut full_args = vec!["--session".to_string(), self.session.clone()];
        full_args.extend_from_slice(args);
        let output = Command::new(&self.binary)
            .args(&full_args)
            .output()
            .with_context(|| format!("failed to start `{}`", self.binary))?;

        if !output.status.success() {
            bail!(
                "agent-browser command failed\n\nCommand:\n  {} {}\n\nStderr:\n{}",
                self.binary,
                shellish_join(&full_args),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(output.stdout)
    }
}

impl BrowserBackend for AgentBrowserBackend {
    fn open(&self, url: &str) -> Result<()> {
        self.run(&["open".into(), url.into()])
    }

    fn eval(&self, js: &str) -> Result<()> {
        self.run(&["eval".into(), js.into()])
    }

    fn fill(&self, selector: &str, text: &str) -> Result<()> {
        self.run(&["fill".into(), selector.into(), text.into()])
    }

    fn screenshot(&self, path: &Path) -> Result<()> {
        self.run(&["screenshot".into(), path.display().to_string()])
    }

    fn start_recording(&self, paths: &SessionPaths, fps: u32) -> Result<Box<dyn Recording>> {
        let cdp_url = self.output(&["get".into(), "cdp-url".into()])?;
        let cdp_url = String::from_utf8_lossy(&cdp_url);
        let tabs = self.output(&["--json".into(), "tab".into(), "list".into()])?;
        let target_id = active_target_id(&tabs)?;
        let recording = ScreencastRecording::start(cdp_url.trim(), &target_id, paths, fps)?;
        Ok(Box::new(recording))
    }

    fn set_viewport(&self, width: u32, height: u32) -> Result<()> {
        self.run(&[
            "set".into(),
            "viewport".into(),
            width.to_string(),
            height.to_string(),
        ])
    }

    fn wait_for_selector(&self, selector: &str) -> Result<()> {
        self.run(&["wait".into(), selector.into()])
    }

    fn preflight(&self) -> Result<()> {
        crate::preflight::ensure_binary(
            &self.binary,
            "Install Sepia with Pixi to bundle it: \
             `pixi global install -c https://prefix.dev/tim -c conda-forge sepia`, \
             or install it on its own with `pixi global install agent-browser`.",
        )
    }
}

#[derive(Deserialize)]
struct TabList {
    data: TabListData,
}

#[derive(Deserialize)]
struct TabListData {
    tabs: Vec<Tab>,
}

#[derive(Deserialize)]
struct Tab {
    #[serde(default)]
    active: bool,
    #[serde(rename = "targetId")]
    target_id: Option<String>,
}

/// The DevTools target of the active tab, from `agent-browser --json tab list`.
fn active_target_id(tab_list_json: &[u8]) -> Result<String> {
    let list: TabList = serde_json::from_slice(tab_list_json)
        .context("failed to parse `agent-browser tab list --json`")?;
    list.data
        .tabs
        .into_iter()
        .find(|tab| tab.active)
        .and_then(|tab| tab.target_id)
        .context(
            "agent-browser did not report a DevTools target for the active tab; \
             Sepia needs agent-browser 0.39.1 or newer",
        )
}

#[must_use]
pub fn shellish_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_/.:=".contains(c))
            {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_join_quotes_human_unfriendly_args() {
        let args = vec!["eval".into(), "console.log('hello world')".into()];
        assert_eq!(
            shellish_join(&args),
            r#"eval 'console.log('\''hello world'\'')'"#
        );
    }
}
