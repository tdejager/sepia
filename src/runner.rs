use std::{fs, path::PathBuf, thread, time::Duration};

use miette::{Result, WrapErr};

use crate::ResultContextExt;
use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::{
    browser::{BrowserBackend, Recording},
    config::{DemoConfig, StepConfig},
    encoder::VideoEncoder,
    inspect::write_inspect_html,
    metadata::{SessionMetadata, SessionScreenshot, SessionStep, write_session_metadata},
    pr::render_pr_block,
    progress::ProgressReporter,
    session::{SessionPaths, slugify, write_latest},
    timeline::{
        CLICK_CUE_MS, FILL_CUE_MS, TimelineCompiler, action_frames, cue_frames,
        frames_for_duration, render_timeline_markdown,
    },
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
    progress: &dyn ProgressReporter,
) -> Result<RunOutput>
where
    B: BrowserBackend,
    E: VideoEncoder,
{
    browser.preflight()?;
    encoder.preflight()?;

    let created_at = Local::now();
    let paths = SessionPaths::create(output_root.clone(), &config.name, created_at)?;
    // Point latest.json at this session immediately: a run that fails partway
    // must not leave tooling (`sepia pr`, `sepia inspect`) silently resolving
    // to an older session's video.
    write_latest(output_root, paths.root.clone())?;
    let plan = TimelineCompiler::compile(config);
    let mut screenshots = Vec::new();

    progress.started(&config.name, config.steps.len(), config.capture.output_fps);
    browser
        .set_viewport(config.browser.width, config.browser.height)
        .with_context(|| {
            format!(
                "failed to set browser viewport to {}x{}",
                config.browser.width, config.browser.height
            )
        })?;
    progress.opening(&config.url);
    browser.open(&config.url)?;
    let recording = browser
        .start_recording(&paths, config.capture.output_fps)
        .context("failed to start recording the browser")?;
    let mut capture = FrameCapture::new(recording, progress);
    capture.roll(1).context("failed to capture initial frame")?;

    let mut steps = Vec::new();
    let total_steps = config.steps.len();
    for (index, step) in config.steps.iter().enumerate() {
        progress.step(index + 1, total_steps, step.kind().label(), &step.name);
        let start_frame = capture.next_frame;
        execute_step(index + 1, total_steps, step, config, &mut capture, browser)
            .with_context(|| format!("failed while executing step `{}`", step.name))?;
        let hold_frames =
            frames_for_duration(step.hold_ms(&config.capture), config.capture.output_fps);
        let captured_frames = capture
            .next_frame
            .saturating_sub(start_frame)
            .saturating_sub(hold_frames);

        let mut screenshot = None;
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
                path: screenshot_path.clone(),
            });
            screenshot = Some(screenshot_path);
        }

        steps.push(SessionStep {
            name: step.name.clone(),
            kind: step.kind(),
            start_frame,
            captured_frames,
            hold_frames,
            screenshot,
        });
    }
    // Every frame is on disk; stop the screencast before the slower bookkeeping.
    let frame_count = capture.finish();

    let timeline_json =
        serde_json::to_string_pretty(&plan).context("failed to encode timeline JSON")?;
    fs::write(&paths.timeline_json, timeline_json)
        .with_context(|| format!("failed to write {}", paths.timeline_json.display()))?;
    fs::write(&paths.timeline_md, render_timeline_markdown(&plan))
        .with_context(|| format!("failed to write {}", paths.timeline_md.display()))?;
    fs::write(
        &paths.summary_md,
        render_summary(
            config,
            &CaptureReport {
                frame_count,
                video: paths.video.clone(),
                inspect: paths.inspect_html.clone(),
            },
        ),
    )
    .with_context(|| format!("failed to write {}", paths.summary_md.display()))?;
    let metadata =
        SessionMetadata::from_capture(config, &paths, created_at, frame_count, screenshots, steps);
    // The inspect page embeds the PR block, so the comment file must exist
    // before the page is rendered.
    let video =
        std::path::absolute(&metadata.video).context("failed to resolve recording path")?;
    fs::write(
        &paths.pr_comment_md,
        render_pr_block(&metadata, &video),
    )
    .with_context(|| format!("failed to write {}", paths.pr_comment_md.display()))?;
    write_inspect_html(&paths, &metadata)?;
    write_session_metadata(&paths, &metadata)?;

    // Encode last: if ffmpeg fails, the session still has its frames, metadata,
    // and the latest.json pointer, so the video can be re-encoded afterwards.
    progress.encoding(
        paths
            .video
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("demo.mp4"),
    );
    encoder
        .encode(&paths.frames_dir, &paths.video, config.capture.output_fps)
        .context("failed to encode Sepia video")?;

    progress.finished(frame_count, metadata.duration_seconds());
    Ok(RunOutput { paths, frame_count })
}

fn execute_step<B>(
    step_number: usize,
    total_steps: usize,
    step: &StepConfig,
    config: &DemoConfig,
    capture: &mut FrameCapture<'_>,
    browser: &B,
) -> Result<()>
where
    B: BrowserBackend,
{
    if let Some(wait_for) = &step.wait_for {
        browser
            .wait_for_selector(&wait_for.selector)
            .with_context(|| format!("timed out waiting for `{}`", wait_for.selector))?;
    }
    show_step_label(browser, config, step_number, total_steps, step);

    if let Some(wait_ms) = step.wait_ms {
        // A wait is a pause in the script, not footage: it stays off camera.
        sleep_ms(wait_ms);
        show_step_label(browser, config, step_number, total_steps, step);
        capture.roll(1)?;
    } else if let Some(js) = &step.eval {
        browser.eval(js)?;
        roll_action(step_number, total_steps, step, config, capture, browser)?;
    } else if let Some(fill) = &step.fill {
        browser
            .wait_for_selector(&fill.selector)
            .with_context(|| format!("timed out waiting for fill target `{}`", fill.selector))?;
        // Ring the input so viewers see where the text lands, then fill it.
        browser.eval(&fill_cue_js(&fill.selector))?;
        capture.roll(cue_frames(FILL_CUE_MS, config.capture.output_fps))?;
        browser.fill(&fill.selector, &fill.text)?;
        roll_action(step_number, total_steps, step, config, capture, browser)?;
    } else if let Some(click) = &step.click {
        browser
            .wait_for_selector(&click.selector)
            .with_context(|| format!("timed out waiting for click target `{}`", click.selector))?;
        // Pulse a ripple at the target and record it, so the click is visible
        // in the video, then fire the (fire-and-forget) click.
        let index = click.index.unwrap_or(0);
        browser.eval(&click_cue_js(&click.selector, index))?;
        capture.roll(cue_frames(CLICK_CUE_MS, config.capture.output_fps))?;
        browser.eval(&click_js(&click.selector, index))?;
        roll_action(step_number, total_steps, step, config, capture, browser)?;
    } else if let Some(scroll) = &step.scroll {
        browser
            .wait_for_selector(&scroll.selector)
            .with_context(|| {
                format!("timed out waiting for scroll target `{}`", scroll.selector)
            })?;
        let duration_ms = step.action_ms(&config.capture);
        let frames = step
            .frames
            .unwrap_or_else(|| frames_for_duration(duration_ms, config.capture.output_fps).max(2));
        let per_frame_pixels = scroll.pixels as f64 / f64::from(frames);

        // One scroll increment per video frame.
        for _ in 0..frames {
            browser.eval(&scroll_js(&scroll.selector, per_frame_pixels))?;
            capture.roll(1)?;
        }
    } else {
        show_step_label(browser, config, step_number, total_steps, step);
        capture.roll(1)?;
    }

    let hold_frames = frames_for_duration(step.hold_ms(&config.capture), config.capture.output_fps);
    capture.roll(hold_frames)?;
    Ok(())
}

/// How often the step label is redrawn while an action's footage rolls.
const LABEL_REFRESH_MS: u64 = 250;

/// Record the footage of a step's action. The action may navigate, which wipes
/// the step label, so redraw it between short rolls; the redraw happens off
/// camera and costs no video time.
fn roll_action<B>(
    step_number: usize,
    total_steps: usize,
    step: &StepConfig,
    config: &DemoConfig,
    capture: &mut FrameCapture<'_>,
    browser: &B,
) -> Result<()>
where
    B: BrowserBackend,
{
    let fps = config.capture.output_fps;
    let refresh_frames = frames_for_duration(LABEL_REFRESH_MS, fps).max(1);
    let mut remaining = action_frames(step.action_ms(&config.capture), fps);
    while remaining > 0 {
        let frames = remaining.min(refresh_frames);
        capture.roll(frames)?;
        remaining -= frames;
        show_step_label(browser, config, step_number, total_steps, step);
    }
    Ok(())
}

fn show_step_label<B>(
    browser: &B,
    config: &DemoConfig,
    step_number: usize,
    total_steps: usize,
    step: &StepConfig,
) where
    B: BrowserBackend,
{
    if config.capture.show_step_labels {
        // This is context for the reviewer, not a capture precondition. Never
        // fail the run just because a page navigated while we were drawing it.
        let _ = browser.eval(&step_label_js(
            step_number,
            total_steps,
            step.kind().label(),
            &step.name,
        ));
    }
}

fn step_label_js(step_number: usize, total_steps: usize, kind: &str, name: &str) -> String {
    let progress_json = serde_json::to_string(&format!("{step_number:02}/{total_steps:02}"))
        .expect("step progress should serialize");
    let kind_json = serde_json::to_string(kind).expect("step kind should serialize");
    let name_json = serde_json::to_string(name).expect("step name should serialize");
    format!(
        r#"(() => {{
  const id = 'sepia-step-label';
  let root = document.getElementById(id);
  if (!root) {{
    root = document.createElement('div');
    root.id = id;
    Object.assign(root.style, {{
      position: 'fixed', left: '18px', bottom: '18px', zIndex: '2147483647',
      maxWidth: 'min(620px, calc(100vw - 36px))', padding: '10px 13px',
      borderRadius: '12px', border: '2px solid rgba(60,56,54,.72)',
      background: 'rgba(251,241,199,.94)', color: '#3c3836',
      boxShadow: '0 10px 30px rgba(60,56,54,.28)', pointerEvents: 'none',
      fontFamily: 'ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
      lineHeight: '1.25', backdropFilter: 'blur(3px)'
    }});
    root.innerHTML = '<div data-sepia-meta></div><div data-sepia-name></div>';
    document.body.appendChild(root);
  }}
  const meta = root.querySelector('[data-sepia-meta]');
  const title = root.querySelector('[data-sepia-name]');
  Object.assign(meta.style, {{
    marginBottom: '3px', fontSize: '11px', fontWeight: '800', letterSpacing: '.08em',
    textTransform: 'uppercase', color: '#af3a03'
  }});
  Object.assign(title.style, {{
    fontSize: '17px', fontWeight: '800', color: '#3c3836'
  }});
  meta.textContent = {progress_json} + ' · ' + {kind_json};
  title.textContent = {name_json};
}})()"#
    )
}

fn scroll_js(selector: &str, pixels: f64) -> String {
    let selector_json = serde_json::to_string(selector).expect("selector string should serialize");
    // Scrolling `html`/`body` is a trap: which one actually scrolls depends on
    // the page's mode. Redirect those to `document.scrollingElement` (the real
    // page scroller), scroll inner containers directly, and fall back to
    // `window.scrollBy` if the page element reports no movement.
    format!(
        r#"(() => {{
  const el = document.querySelector({selector_json});
  if (!el) throw new Error(`Sepia scroll target not found: ${{{selector_json}}}`);
  const page = document.scrollingElement || document.documentElement;
  const target = (el === document.documentElement || el === document.body) ? page : el;
  const before = target.scrollTop;
  target.scrollTop += {pixels};
  if (target.scrollTop === before && target === page) window.scrollBy(0, {pixels});
}})()"#
    )
}

fn click_js(selector: &str, index: u32) -> String {
    let selector_json = serde_json::to_string(selector).expect("selector string should serialize");
    format!(
        r#"(() => {{
  const el = document.querySelectorAll({selector_json})[{index}];
  if (!el) throw new Error(`Sepia click target not found: ${{{selector_json}}} #{index}`);
  el.click();
}})()"#
    )
}

/// Scroll the target into view and pulse a ripple over it, so the click is
/// visible in the recording. Uses the Web Animations API (no injected
/// stylesheet) to stay within strict Content-Security-Policy pages.
fn click_cue_js(selector: &str, index: u32) -> String {
    let selector_json = serde_json::to_string(selector).expect("selector string should serialize");
    format!(
        r#"(() => {{
  const el = document.querySelectorAll({selector_json})[{index}];
  if (!el) throw new Error(`Sepia click target not found: ${{{selector_json}}} #{index}`);
  el.scrollIntoView({{block: 'center', inline: 'center'}});
  const r = el.getBoundingClientRect();
  const dot = document.createElement('div');
  Object.assign(dot.style, {{
    position: 'fixed',
    left: (r.left + r.width / 2) + 'px',
    top: (r.top + r.height / 2) + 'px',
    width: '46px', height: '46px', borderRadius: '50%',
    border: '3px solid #d65d0e', background: 'rgba(214, 93, 14, 0.28)',
    zIndex: '2147483647', pointerEvents: 'none',
    transform: 'translate(-50%, -50%)'
  }});
  document.body.appendChild(dot);
  dot.animate(
    [{{transform: 'translate(-50%, -50%) scale(0.4)', opacity: 1}},
     {{transform: 'translate(-50%, -50%) scale(2.4)', opacity: 0}}],
    {{duration: 700, easing: 'ease-out'}}
  );
  setTimeout(() => dot.remove(), 750);
}})()"#
    )
}

/// Scroll an input into view and draw a fading ring around it, so viewers see
/// where the text is about to be typed.
fn fill_cue_js(selector: &str) -> String {
    let selector_json = serde_json::to_string(selector).expect("selector string should serialize");
    format!(
        r#"(() => {{
  const el = document.querySelector({selector_json});
  if (!el) throw new Error(`Sepia fill target not found: ${{{selector_json}}}`);
  el.scrollIntoView({{block: 'center', inline: 'center'}});
  const r = el.getBoundingClientRect();
  const ring = document.createElement('div');
  Object.assign(ring.style, {{
    position: 'fixed',
    left: r.left + 'px', top: r.top + 'px',
    width: r.width + 'px', height: r.height + 'px',
    border: '3px solid #d65d0e', borderRadius: '6px', boxSizing: 'border-box',
    zIndex: '2147483647', pointerEvents: 'none'
  }});
  document.body.appendChild(ring);
  ring.animate(
    [{{opacity: 0, transform: 'scale(1.08)'}}, {{opacity: 1, transform: 'scale(1)'}}],
    {{duration: 300, easing: 'ease-out'}}
  );
  setTimeout(() => ring.remove(), 1600);
}})()"#
    )
}

fn sleep_ms(ms: u64) {
    if ms > 0 {
        thread::sleep(Duration::from_millis(ms));
    }
}

/// The live recording plus the runner's frame bookkeeping.
struct FrameCapture<'a> {
    recording: Box<dyn Recording>,
    progress: &'a dyn ProgressReporter,
    /// 1-based index of the next video frame.
    next_frame: u32,
}

impl<'a> FrameCapture<'a> {
    fn new(recording: Box<dyn Recording>, progress: &'a dyn ProgressReporter) -> Self {
        Self {
            recording,
            progress,
            next_frame: 1,
        }
    }

    /// Record `frames` frames of live footage, which takes `frames / fps`
    /// seconds. Anything between rolls stays off camera.
    fn roll(&mut self, frames: u32) -> Result<()> {
        let Self {
            recording,
            progress,
            next_frame,
        } = self;
        recording.roll(frames, &mut || {
            *next_frame += 1;
            progress.tick();
        })
    }

    /// Stop recording and return the number of frames captured.
    fn finish(self) -> u32 {
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
        config::{BrowserConfig, CaptureConfig, FillConfig},
        encoder::VideoEncoder,
        timeline::plan_tree,
    };

    use super::*;

    #[derive(Default)]
    struct FakeBrowser {
        calls: RefCell<Vec<String>>,
    }

    /// Hands out every requested frame at once: no browser, no real time.
    struct FakeRecording;

    impl Recording for FakeRecording {
        fn roll(&mut self, frames: u32, on_frame: &mut dyn FnMut()) -> Result<()> {
            for _ in 0..frames {
                on_frame();
            }
            Ok(())
        }
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
            fs::write(path, b"fake png").context("failed to write fake screenshot")?;
            Ok(())
        }

        fn start_recording(&self, _paths: &SessionPaths, _fps: u32) -> Result<Box<dyn Recording>> {
            self.calls.borrow_mut().push("start recording".into());
            Ok(Box::new(FakeRecording))
        }

        fn set_viewport(&self, width: u32, height: u32) -> Result<()> {
            self.calls
                .borrow_mut()
                .push(format!("set viewport {width} {height}"));
            Ok(())
        }

        fn wait_for_selector(&self, selector: &str) -> Result<()> {
            self.calls.borrow_mut().push(format!("wait {selector}"));
            Ok(())
        }
    }

    struct FakeEncoder;

    impl VideoEncoder for FakeEncoder {
        fn encode(&self, _frames_dir: &Path, output: &Path, _output_fps: u32) -> Result<()> {
            fs::write(output, b"fake mp4").context("failed to write fake video")?;
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
                show_step_labels: true,
            },
            browser: BrowserConfig {
                width: 1200,
                height: 800,
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
                click: None,
                wait_for: None,
                hold_ms: Some(100),
                duration_ms: Some(0),
                frames: None,
                screenshot: true,
            }],
        };

        let browser = FakeBrowser::default();
        let run = run_capture(
            &config,
            output.path().to_path_buf(),
            &browser,
            &FakeEncoder,
            &(),
        )
        .unwrap();

        assert!(run.paths.video.exists());
        assert!(run.paths.inspect_html.exists());
        assert!(run.paths.timeline_json.exists());
        assert!(run.paths.pr_comment_md.exists());
        // The plan drives the `--plan` preview and the live progress tree, so
        // it has to predict exactly what a run records.
        let plan = plan_tree(&config, &TimelineCompiler::compile(&config));
        assert_eq!(run.frame_count, plan.total_frames);
        assert!(
            browser
                .calls
                .borrow()
                .iter()
                .any(|call| call == "set viewport 1200 800")
        );
        assert!(
            browser
                .calls
                .borrow()
                .iter()
                .any(|call| call == "wait input")
        );
        assert!(
            browser
                .calls
                .borrow()
                .iter()
                .any(|call| call == "fill input zlib")
        );
    }
}
