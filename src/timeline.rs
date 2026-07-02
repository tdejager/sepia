use serde::{Deserialize, Serialize};

use crate::config::{DemoConfig, StepConfig, StepKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelinePlan {
    pub output_fps: u32,
    pub segments: Vec<TimelineSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineSegment {
    pub step_name: String,
    pub kind: SegmentKind,
    pub duration_ms: u64,
    pub frames_to_capture: u32,
    pub hold_frames: u32,
    pub key_screenshot: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SegmentKind {
    Wait,
    Eval,
    Fill,
    Scroll,
    Click,
    Hold,
}

pub struct TimelineCompiler;

impl TimelineCompiler {
    #[must_use]
    pub fn compile(config: &DemoConfig) -> TimelinePlan {
        let mut segments = Vec::new();
        for step in &config.steps {
            segments.extend(Self::compile_step(config, step));
        }
        TimelinePlan {
            output_fps: config.capture.output_fps,
            segments,
        }
    }

    fn compile_step(config: &DemoConfig, step: &StepConfig) -> Vec<TimelineSegment> {
        let fps = config.capture.output_fps;
        let mut segments = Vec::new();

        let action_kind = if step.scroll.is_some() {
            Some(SegmentKind::Scroll)
        } else if step.fill.is_some() {
            Some(SegmentKind::Fill)
        } else if step.click.is_some() {
            Some(SegmentKind::Click)
        } else if step.eval.is_some() {
            Some(SegmentKind::Eval)
        } else if step.wait_ms.is_some() {
            Some(SegmentKind::Wait)
        } else {
            None
        };

        if let Some(kind) = action_kind {
            let action_ms = step
                .duration_ms
                .or(step.wait_ms)
                .unwrap_or(config.capture.default_action_ms);
            // Match what `runner::execute_step` actually captures: fill and click
            // record a cue animation (see FILL_CUE_MS / CLICK_CUE_MS) plus one
            // final frame; only scroll honours an explicit `frames`.
            let (duration_ms, frames_to_capture) = match kind {
                SegmentKind::Scroll => (
                    action_ms,
                    step.frames
                        .unwrap_or_else(|| frames_for_duration(action_ms, fps).max(2)),
                ),
                SegmentKind::Fill => (FILL_CUE_MS + action_ms, cue_frames(FILL_CUE_MS, fps) + 1),
                SegmentKind::Click => (CLICK_CUE_MS + action_ms, cue_frames(CLICK_CUE_MS, fps) + 1),
                SegmentKind::Wait | SegmentKind::Eval | SegmentKind::Hold => (action_ms, 1),
            };

            segments.push(TimelineSegment {
                step_name: step.name.clone(),
                kind,
                duration_ms,
                frames_to_capture,
                hold_frames: 0,
                key_screenshot: step.screenshot,
            });
        }

        let hold_ms = step.hold_ms(&config.capture);
        if hold_ms > 0 {
            segments.push(TimelineSegment {
                step_name: step.name.clone(),
                kind: SegmentKind::Hold,
                duration_ms: hold_ms,
                frames_to_capture: 0,
                hold_frames: frames_for_duration(hold_ms, fps),
                key_screenshot: false,
            });
        }

        segments
    }
}

#[must_use]
pub fn frames_for_duration(duration_ms: u64, fps: u32) -> u32 {
    if duration_ms == 0 || fps == 0 {
        return 0;
    }
    ((duration_ms * u64::from(fps)).div_ceil(1000)) as u32
}

/// Milliseconds of cue animation captured before a fill / click action fires.
/// Shared with `runner` so the plan and the actual capture agree.
pub(crate) const FILL_CUE_MS: u64 = 300;
pub(crate) const CLICK_CUE_MS: u64 = 600;

/// Frames captured while a fill/click cue animates (mirrors
/// `runner::FrameCapture::capture_over`).
#[must_use]
pub(crate) fn cue_frames(ms: u64, fps: u32) -> u32 {
    frames_for_duration(ms, fps).max(3)
}

/// A step grouped into its capture phases, shared by the static `--plan` tree
/// and the live animated tree shown during a run.
pub struct PlanTree {
    pub name: String,
    pub fps: u32,
    pub total_frames: u32,
    pub steps: Vec<PlanStep>,
}

pub struct PlanStep {
    pub name: String,
    /// The step's action kind (or `Hold` for a no-action step).
    pub kind: SegmentKind,
    pub start_secs: f64,
    pub frames: u32,
    pub phases: Vec<PlanPhase>,
}

pub struct PlanPhase {
    pub kind: SegmentKind,
    pub frames: u32,
    pub secs: f64,
}

/// Group a compiled plan into per-step nodes with their phases.
#[must_use]
pub fn plan_tree(config: &DemoConfig, plan: &TimelinePlan) -> PlanTree {
    let fps = plan.output_fps.max(1);

    let mut groups: Vec<(&str, Vec<&TimelineSegment>)> = Vec::new();
    for segment in &plan.segments {
        match groups.last_mut() {
            Some(group) if group.0 == segment.step_name => group.1.push(segment),
            _ => groups.push((segment.step_name.as_str(), vec![segment])),
        }
    }

    let mut elapsed = 0u32;
    let mut steps = Vec::new();
    for (name, segments) in groups {
        let kind = segments
            .iter()
            .map(|s| s.kind)
            .find(|k| !matches!(k, SegmentKind::Hold))
            .unwrap_or(SegmentKind::Hold);
        let start_secs = f64::from(elapsed) / f64::from(fps);
        let mut frames = 0;
        let mut phases = Vec::new();
        for segment in segments {
            let f = segment_frames(segment);
            frames += f;
            elapsed += f;
            phases.push(PlanPhase {
                kind: segment.kind,
                frames: f,
                secs: f64::from(f) / f64::from(fps),
            });
        }
        steps.push(PlanStep {
            name: name.to_owned(),
            kind,
            start_secs,
            frames,
            phases,
        });
    }

    let total_frames = steps.iter().map(|s| s.frames).sum();
    PlanTree {
        name: config.name.clone(),
        fps,
        total_frames,
        steps,
    }
}

impl SegmentKind {
    /// The action kind for a segment, or `None` for a hold (a timeline-only phase).
    fn as_step_kind(self) -> Option<StepKind> {
        Some(match self {
            SegmentKind::Wait => StepKind::Wait,
            SegmentKind::Eval => StepKind::Eval,
            SegmentKind::Fill => StepKind::Fill,
            SegmentKind::Scroll => StepKind::Scroll,
            SegmentKind::Click => StepKind::Click,
            SegmentKind::Hold => return None,
        })
    }
}

/// Colour used for each segment in the plan tree — action colours come from
/// [`StepKind`]; holds get a neutral brown.
pub(crate) fn kind_rgb(kind: &SegmentKind) -> (u8, u8, u8) {
    kind.as_step_kind().map_or((146, 131, 116), StepKind::rgb)
}

/// A one-word label for the segment, used in the plan tree.
pub(crate) fn kind_word(kind: &SegmentKind) -> &'static str {
    kind.as_step_kind().map_or("hold", StepKind::label)
}

impl std::fmt::Display for SegmentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(kind_word(self))
    }
}

/// `"1 frame"` / `"3 frames"`.
pub(crate) fn frames_label(frames: u32) -> String {
    let unit = if frames == 1 { "frame" } else { "frames" };
    format!("{frames} {unit}")
}

/// The frames a single segment contributes to the video.
fn segment_frames(segment: &TimelineSegment) -> u32 {
    if matches!(segment.kind, SegmentKind::Hold) {
        segment.hold_frames
    } else {
        segment.frames_to_capture
    }
}

/// Render the compiled plan as a colored tree. `stream` selects which output
/// stream's terminal/`NO_COLOR` support governs coloring (stdout for `--plan`,
/// stderr for the preview shown at the start of a real run).
#[must_use]
pub fn render_plan_tree(
    config: &DemoConfig,
    plan: &TimelinePlan,
    stream: owo_colors::Stream,
) -> String {
    use owo_colors::OwoColorize;

    let fps = plan.output_fps.max(1);
    let secs = |frames: u32| f64::from(frames) / f64::from(fps);

    // Group consecutive segments that belong to the same step.
    let mut groups: Vec<(&str, Vec<&TimelineSegment>)> = Vec::new();
    for segment in &plan.segments {
        match groups.last_mut() {
            Some(group) if group.0 == segment.step_name => group.1.push(segment),
            _ => groups.push((segment.step_name.as_str(), vec![segment])),
        }
    }

    let total_frames: u32 = plan.segments.iter().map(segment_frames).sum();
    let mut out = String::new();

    out.push_str(&format!(
        "{}\n",
        config.name.if_supports_color(stream, |t| t.bold())
    ));
    out.push_str(&format!(
        "{}\n\n",
        format!(
            "{fps} fps · {} {} · ~{:.1}s · {}",
            groups.len(),
            if groups.len() == 1 { "step" } else { "steps" },
            secs(total_frames),
            frames_label(total_frames),
        )
        .if_supports_color(stream, |t| t.dimmed())
    ));

    let mut elapsed_frames = 0u32;
    for (gi, (name, segments)) in groups.iter().enumerate() {
        let last_group = gi + 1 == groups.len();
        let branch = if last_group { "└─" } else { "├─" };
        let child_stem = if last_group { "  " } else { "│ " };

        let action_kind = segments
            .iter()
            .map(|s| &s.kind)
            .find(|k| !matches!(k, SegmentKind::Hold))
            .unwrap_or(&SegmentKind::Hold);
        let rgb = kind_rgb(action_kind);

        out.push_str(&format!(
            "{branch} {}  {}  {}\n",
            format!("{:02}", gi + 1).if_supports_color(stream, |t| t.dimmed()),
            name.if_supports_color(stream, |t| t.bold()),
            format!("[{}]", kind_word(action_kind))
                .if_supports_color(stream, |t| t.truecolor(rgb.0, rgb.1, rgb.2)),
        ));
        out.push_str(&format!(
            "{child_stem}   {}\n",
            format!("starts at {:.1}s", secs(elapsed_frames))
                .if_supports_color(stream, |t| t.dimmed())
        ));

        for (si, segment) in segments.iter().enumerate() {
            let last_child = si + 1 == segments.len();
            let twig = if last_child { "└─" } else { "├─" };
            let frames = segment_frames(segment);
            let srgb = kind_rgb(&segment.kind);
            out.push_str(&format!(
                "{child_stem} {twig} {}  {}\n",
                format!("{:<7}", kind_word(&segment.kind))
                    .if_supports_color(stream, |t| t.truecolor(srgb.0, srgb.1, srgb.2)),
                format!("{} · {:.1}s", frames_label(frames), secs(frames))
                    .if_supports_color(stream, |t| t.dimmed()),
            ));
            elapsed_frames += frames;
        }
    }

    out
}

pub fn render_timeline_markdown(plan: &TimelinePlan) -> String {
    let mut out = String::from("# Sepia Timeline\n\n");
    for (idx, segment) in plan.segments.iter().enumerate() {
        out.push_str(&format!(
            "{}. **{}** — {}, {}ms, capture {}, hold {}\n",
            idx + 1,
            segment.step_name,
            segment.kind,
            segment.duration_ms,
            segment.frames_to_capture,
            segment.hold_frames
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::config::{CaptureConfig, DemoConfig, ScrollConfig, StepConfig};

    use super::*;

    #[test]
    fn rounds_duration_to_frame_count() {
        assert_eq!(frames_for_duration(1000, 24), 24);
        assert_eq!(frames_for_duration(1600, 24), 39);
        assert_eq!(frames_for_duration(1, 24), 1);
    }

    #[test]
    fn preserves_explicit_scroll_granularity() {
        let config = DemoConfig {
            name: "demo".into(),
            description: None,
            url: "http://localhost".into(),
            session: None,
            capture: CaptureConfig::default(),
            steps: vec![StepConfig {
                name: "Scroll packages".into(),
                wait_ms: None,
                eval: None,
                fill: None,
                scroll: Some(ScrollConfig {
                    selector: ".package-list".into(),
                    pixels: 900,
                }),
                click: None,
                hold_ms: Some(500),
                duration_ms: Some(1600),
                frames: Some(32),
                screenshot: true,
            }],
        };

        let plan = TimelineCompiler::compile(&config);
        assert_eq!(plan.segments[0].kind, SegmentKind::Scroll);
        assert_eq!(plan.segments[0].frames_to_capture, 32);
        assert_eq!(plan.segments[1].kind, SegmentKind::Hold);
    }

    #[test]
    fn plan_tree_lists_steps_and_phases() {
        let config = DemoConfig {
            name: "Tree demo".into(),
            description: None,
            url: "http://localhost".into(),
            session: None,
            capture: CaptureConfig::default(),
            steps: vec![StepConfig {
                name: "Scroll it".into(),
                wait_ms: None,
                eval: None,
                fill: None,
                scroll: Some(ScrollConfig {
                    selector: ".list".into(),
                    pixels: 400,
                }),
                click: None,
                hold_ms: Some(500),
                duration_ms: Some(1000),
                frames: Some(1),
                screenshot: false,
            }],
        };

        let plan = TimelineCompiler::compile(&config);
        let tree = render_plan_tree(&config, &plan, owo_colors::Stream::Stdout);
        assert!(tree.contains("Tree demo"));
        assert!(tree.contains("Scroll it"));
        assert!(tree.contains("scroll"));
        assert!(tree.contains("hold"));
        assert!(tree.contains("└─"));
        // Pluralization: a single-frame segment reads "1 frame", not "1 frames".
        assert!(tree.contains("1 frame ") || tree.contains("1 frame\n"));
        assert!(!tree.contains("1 frames"));
    }
}
