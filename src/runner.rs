use std::{fs, path::PathBuf, thread, time::Duration};

use anyhow::{Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::{
    browser::BrowserBackend,
    config::{DemoConfig, StepConfig},
    encoder::VideoEncoder,
    inspect::write_inspect_html,
    metadata::{SessionMetadata, SessionScreenshot, write_session_metadata},
    pr::{pr_data_from_metadata, render_pr_comment},
    session::{SessionPaths, slugify, write_latest},
    timeline::{TimelineCompiler, frames_for_duration, render_timeline_markdown},
};

#[derive(Debug, Clone)]
pub struct RunOutput {
    pub paths: SessionPaths,
    pub frame_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureReport {
    pub frame_count: u32,
    pub video: PathBuf,
    pub inspect: PathBuf,
}

pub fn run_capture<B, E>(
    config: &DemoConfig,
    output_root: PathBuf,
    browser: &B,
    encoder: &E,
) -> Result<RunOutput>
where
    B: BrowserBackend,
    E: VideoEncoder,
{
    let created_at = Local::now();
    let paths = SessionPaths::create(output_root.clone(), &config.name, created_at)?;
    let plan = TimelineCompiler::compile(config);
    let mut screenshots = Vec::new();
    let mut capture = FrameCapture::new(&paths);

    browser.open(&config.url)?;
    capture
        .capture_frame(browser)
        .context("failed to capture initial frame")?;

    for (index, step) in config.steps.iter().enumerate() {
        execute_step(step, config, &mut capture, browser)
            .with_context(|| format!("failed while executing step `{}`", step.name))?;

        if step.screenshot {
            let screenshot_path =
                paths
                    .steps_dir
                    .join(format!("step-{:02}-{}.png", index + 1, slugify(&step.name)));
            browser
                .screenshot(&screenshot_path)
                .with_context(|| format!("failed to capture key screenshot for `{}`", step.name))?;
            screenshots.push(SessionScreenshot {
                label: step.name.clone(),
                path: screenshot_path,
            });
        }
    }

    encoder
        .encode(&paths.frames_dir, &paths.video, config.capture.output_fps)
        .context("failed to encode Sepia video")?;

    fs::write(&paths.timeline_json, serde_json::to_string_pretty(&plan)?)
        .with_context(|| format!("failed to write {}", paths.timeline_json.display()))?;
    fs::write(&paths.timeline_md, render_timeline_markdown(&plan))
        .with_context(|| format!("failed to write {}", paths.timeline_md.display()))?;
    fs::write(
        &paths.summary_md,
        render_summary(
            config,
            &CaptureReport {
                frame_count: capture.frame_count(),
                video: paths.video.clone(),
                inspect: paths.inspect_html.clone(),
            },
        ),
    )
    .with_context(|| format!("failed to write {}", paths.summary_md.display()))?;
    write_inspect_html(&paths, config, &plan)?;
    let frame_count = capture.frame_count();
    let metadata =
        SessionMetadata::from_capture(config, &paths, created_at, frame_count, screenshots);
    write_session_metadata(&paths, &metadata)?;
    fs::write(
        &paths.pr_comment_md,
        render_pr_comment(&pr_data_from_metadata(&metadata, None, &[])),
    )
    .with_context(|| format!("failed to write {}", paths.pr_comment_md.display()))?;
    write_latest(output_root, paths.root.clone())?;

    drop(capture);

    Ok(RunOutput { paths, frame_count })
}

fn execute_step<B>(
    step: &StepConfig,
    config: &DemoConfig,
    capture: &mut FrameCapture<'_>,
    browser: &B,
) -> Result<()>
where
    B: BrowserBackend,
{
    if let Some(wait_ms) = step.wait_ms {
        sleep_ms(wait_ms);
        capture.capture_frame(browser)?;
    } else if let Some(js) = &step.eval {
        browser.eval(js)?;
        sleep_ms(step.duration_ms.unwrap_or(config.capture.default_action_ms));
        capture.capture_frame(browser)?;
    } else if let Some(fill) = &step.fill {
        browser.fill(&fill.selector, &fill.text)?;
        sleep_ms(step.duration_ms.unwrap_or(config.capture.default_action_ms));
        capture.capture_frame(browser)?;
    } else if let Some(scroll) = &step.scroll {
        let duration_ms = step.duration_ms.unwrap_or(config.capture.default_action_ms);
        let frames = step
            .frames
            .unwrap_or_else(|| frames_for_duration(duration_ms, config.capture.output_fps).max(2));
        let per_frame_pixels = scroll.pixels as f64 / f64::from(frames);
        let per_frame_wait = if frames == 0 {
            0
        } else {
            duration_ms / u64::from(frames)
        }
        .min(100);

        for _ in 0..frames {
            browser.eval(&scroll_js(&scroll.selector, per_frame_pixels))?;
            sleep_ms(per_frame_wait);
            capture.capture_frame(browser)?;
        }
    } else {
        capture.capture_frame(browser)?;
    }

    let hold_ms = step.hold_ms.unwrap_or(config.capture.default_hold_ms);
    let hold_frames = frames_for_duration(hold_ms, config.capture.output_fps);
    capture.duplicate_last_frame(hold_frames)?;
    Ok(())
}

fn scroll_js(selector: &str, pixels: f64) -> String {
    let selector_json = serde_json::to_string(selector).expect("selector string should serialize");
    format!(
        r#"(() => {{
  const el = document.querySelector({selector_json});
  if (!el) throw new Error(`Sepia scroll target not found: ${{{selector_json}}}`);
  el.scrollTop += {pixels};
}})()"#
    )
}

fn sleep_ms(ms: u64) {
    if ms > 0 {
        thread::sleep(Duration::from_millis(ms));
    }
}

struct FrameCapture<'a> {
    paths: &'a SessionPaths,
    next_frame: u32,
    last_frame: Option<PathBuf>,
}

impl<'a> FrameCapture<'a> {
    fn new(paths: &'a SessionPaths) -> Self {
        Self {
            paths,
            next_frame: 1,
            last_frame: None,
        }
    }

    fn capture_frame<B>(&mut self, browser: &B) -> Result<PathBuf>
    where
        B: BrowserBackend,
    {
        let path = self.paths.frame_path(self.next_frame);
        browser.screenshot(&path)?;
        self.next_frame += 1;
        self.last_frame = Some(path.clone());
        Ok(path)
    }

    fn duplicate_last_frame(&mut self, count: u32) -> Result<()> {
        let Some(last_frame) = self.last_frame.clone() else {
            return Ok(());
        };
        for _ in 0..count {
            let path = self.paths.frame_path(self.next_frame);
            fs::copy(&last_frame, &path).with_context(|| {
                format!(
                    "failed to duplicate hold frame from {} to {}",
                    last_frame.display(),
                    path.display()
                )
            })?;
            self.next_frame += 1;
            self.last_frame = Some(path);
        }
        Ok(())
    }

    fn frame_count(&self) -> u32 {
        self.next_frame.saturating_sub(1)
    }
}

fn render_summary(config: &DemoConfig, report: &CaptureReport) -> String {
    format!(
        "# Sepia Summary\n\n- Demo: `{}`\n- URL: `{}`\n- Output FPS: `{}`\n- Frames: `{}`\n- Video: `{}`\n- Inspect: `{}`\n",
        config.name,
        config.url,
        config.capture.output_fps,
        report.frame_count,
        report.video.display(),
        report.inspect.display()
    )
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, path::Path};

    use tempfile::tempdir;

    use crate::{
        config::{CaptureConfig, FillConfig},
        encoder::VideoEncoder,
    };

    use super::*;

    #[derive(Default)]
    struct FakeBrowser {
        calls: RefCell<Vec<String>>,
    }

    impl BrowserBackend for FakeBrowser {
        fn open(&self, url: &str) -> Result<()> {
            self.calls.borrow_mut().push(format!("open {url}"));
            Ok(())
        }

        fn eval(&self, js: &str) -> Result<()> {
            self.calls.borrow_mut().push(format!("eval {js}"));
            Ok(())
        }

        fn fill(&self, selector: &str, text: &str) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("fill {selector} {text}"));
            Ok(())
        }

        fn screenshot(&self, path: &Path) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("screenshot {}", path.display()));
            fs::write(path, b"fake png")?;
            Ok(())
        }
    }

    struct FakeEncoder;

    impl VideoEncoder for FakeEncoder {
        fn encode(&self, _frames_dir: &Path, output: &Path, _output_fps: u32) -> Result<()> {
            fs::write(output, b"fake mp4")?;
            Ok(())
        }
    }

    #[test]
    fn runs_capture_with_fake_backends_and_writes_artifacts() {
        let output = tempdir().unwrap();
        let config = DemoConfig {
            name: "Fake Demo".into(),
            description: Some("Testing".into()),
            url: "http://localhost".into(),
            session: None,
            capture: CaptureConfig {
                output_fps: 10,
                default_hold_ms: 100,
                default_action_ms: 0,
            },
            steps: vec![StepConfig {
                name: "Search".into(),
                wait_ms: None,
                eval: None,
                fill: Some(FillConfig {
                    selector: "input".into(),
                    text: "zlib".into(),
                }),
                scroll: None,
                hold_ms: Some(100),
                duration_ms: Some(0),
                frames: None,
                screenshot: true,
            }],
        };

        let browser = FakeBrowser::default();
        let run =
            run_capture(&config, output.path().to_path_buf(), &browser, &FakeEncoder).unwrap();

        assert!(run.paths.video.exists());
        assert!(run.paths.inspect_html.exists());
        assert!(run.paths.timeline_json.exists());
        assert!(run.paths.pr_comment_md.exists());
        assert!(run.frame_count >= 3);
        assert!(
            browser
                .calls
                .borrow()
                .iter()
                .any(|call| call == "fill input zlib")
        );
    }
}
