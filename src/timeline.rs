use serde::{Deserialize, Serialize};

use crate::config::{DemoConfig, StepConfig};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SegmentKind {
    Wait,
    Eval,
    Fill,
    Scroll,
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
        } else if step.eval.is_some() {
            Some(SegmentKind::Eval)
        } else if step.wait_ms.is_some() {
            Some(SegmentKind::Wait)
        } else {
            None
        };

        if let Some(kind) = action_kind {
            let duration_ms = step
                .duration_ms
                .or(step.wait_ms)
                .unwrap_or(config.capture.default_action_ms);
            let frames_to_capture = step.frames.unwrap_or_else(|| match kind {
                SegmentKind::Scroll => frames_for_duration(duration_ms, fps).max(2),
                SegmentKind::Wait => 1,
                _ => 2,
            });

            segments.push(TimelineSegment {
                step_name: step.name.clone(),
                kind,
                duration_ms,
                frames_to_capture,
                hold_frames: 0,
                key_screenshot: step.screenshot,
            });
        }

        let hold_ms = step.hold_ms.unwrap_or(config.capture.default_hold_ms);
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

pub fn render_timeline_markdown(plan: &TimelinePlan) -> String {
    let mut out = String::from("# Sepia Timeline\n\n");
    for (idx, segment) in plan.segments.iter().enumerate() {
        out.push_str(&format!(
            "{}. **{}** — {:?}, {}ms, capture {}, hold {}\n",
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
}
