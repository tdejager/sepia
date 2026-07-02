//! Live, in-place animated plan tree shown during an interactive run.
//!
//! This is the only place that emits raw terminal control codes. It redraws a
//! fixed-height region on each tick: finished steps get a check, the active step
//! gets a spinner and a frame counter, pending steps stay dim. It is only ever
//! constructed when stderr is a terminal and the tree fits (see the factory in
//! `cli.rs`); otherwise the plain reporter is used.

use std::cell::{Cell, RefCell};
use std::io::{Write, stderr};
use std::time::Instant;

use owo_colors::{OwoColorize, Stream::Stderr};

use super::ProgressReporter;
use crate::timeline::{PlanTree, frames_label, kind_rgb, kind_word};

const SPINNER: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
const GB_GREEN: (u8, u8, u8) = (152, 151, 26);
const GB_YELLOW: (u8, u8, u8) = (215, 153, 33);
const MIN_REDRAW_MS: u128 = 40; // ~25fps ceiling for spinner redraws
// Synchronized-output mode: supporting terminals apply everything between these
// atomically (no intermediate cursor moves/clears shown); others ignore them.
const SYNC_BEGIN: &str = "\x1b[?2026h";
const SYNC_END: &str = "\x1b[?2026l";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pending,
    Active,
    Done,
}

pub struct TreeReporter {
    tree: PlanTree,
    width: usize,
    status: RefCell<Vec<Status>>,
    active: Cell<usize>,
    frame_in_step: Cell<u32>,
    spin: Cell<usize>,
    /// The exact lines drawn last frame, for differential redraws.
    prev: RefCell<Vec<String>>,
    started: Cell<bool>,
    last_draw: Cell<Option<Instant>>,
}

impl TreeReporter {
    /// Number of lines the tree occupies (constant for the whole run).
    #[must_use]
    pub fn height(tree: &PlanTree) -> usize {
        2 + tree.steps.iter().map(|s| 1 + s.phases.len()).sum::<usize>()
    }

    #[must_use]
    pub fn new(tree: PlanTree, width: usize) -> Self {
        let n = tree.steps.len();
        Self {
            tree,
            width,
            status: RefCell::new(vec![Status::Pending; n]),
            active: Cell::new(usize::MAX),
            frame_in_step: Cell::new(0),
            spin: Cell::new(0),
            prev: RefCell::new(Vec::new()),
            started: Cell::new(false),
            last_draw: Cell::new(None),
        }
    }

    fn set_all(&self, status: Status) {
        for slot in self.status.borrow_mut().iter_mut() {
            *slot = status;
        }
    }

    fn render(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(Self::height(&self.tree));
        let steps = &self.tree.steps;
        let secs = f64::from(self.tree.total_frames) / f64::from(self.tree.fps);

        lines.push(format!(
            "{}",
            self.tree.name.if_supports_color(Stderr, |t| t.bold())
        ));
        lines.push(format!(
            "{}",
            format!(
                "{} fps · {} {} · ~{secs:.1}s · {}",
                self.tree.fps,
                steps.len(),
                if steps.len() == 1 { "step" } else { "steps" },
                frames_label(self.tree.total_frames),
            )
            .if_supports_color(Stderr, |t| t.dimmed())
        ));

        let status = self.status.borrow();
        // Re-read the width every frame so a mid-run resize keeps lines from
        // wrapping (falling back to the width captured at construction).
        let width = terminal_size::terminal_size_of(std::io::stderr())
            .map_or(self.width, |(terminal_size::Width(w), _)| usize::from(w));
        // Budget the variable-length step name so lines never wrap.
        let name_budget = width.saturating_sub(34).max(8);

        for (i, step) in steps.iter().enumerate() {
            let last = i + 1 == steps.len();
            let branch = if last { "└─" } else { "├─" };
            let stem = if last { "  " } else { "│ " };
            let rgb = kind_rgb(&step.kind);

            let marker = match status[i] {
                Status::Done => format!(
                    "{}",
                    "✔".if_supports_color(Stderr, |t| t
                        .truecolor(GB_GREEN.0, GB_GREEN.1, GB_GREEN.2))
                ),
                Status::Active => format!(
                    "{}",
                    SPINNER[self.spin.get() % SPINNER.len()].if_supports_color(Stderr, |t| t
                        .truecolor(GB_YELLOW.0, GB_YELLOW.1, GB_YELLOW.2))
                ),
                Status::Pending => format!("{}", "·".if_supports_color(Stderr, |t| t.dimmed())),
            };

            let name = truncate(&step.name, name_budget);
            let name = match status[i] {
                Status::Pending => format!("{}", name.if_supports_color(Stderr, |t| t.dimmed())),
                _ => format!("{}", name.if_supports_color(Stderr, |t| t.bold())),
            };

            let extra = if status[i] == Status::Active {
                format!(
                    " {}",
                    format!(
                        "{}/{}f",
                        self.frame_in_step.get().min(step.frames),
                        step.frames
                    )
                    .if_supports_color(Stderr, |t| t.dimmed())
                )
            } else {
                String::new()
            };

            lines.push(format!(
                "{branch} {marker} {}  {}  {}{extra}",
                format!("{:02}", i + 1).if_supports_color(Stderr, |t| t.dimmed()),
                name,
                format!("[{}]", kind_word(&step.kind))
                    .if_supports_color(Stderr, |t| t.truecolor(rgb.0, rgb.1, rgb.2)),
            ));

            for (pi, phase) in step.phases.iter().enumerate() {
                let twig = if pi + 1 == step.phases.len() {
                    "└─"
                } else {
                    "├─"
                };
                let prgb = kind_rgb(&phase.kind);
                lines.push(format!(
                    "{stem} {twig} {}  {}",
                    format!("{:<7}", kind_word(&phase.kind))
                        .if_supports_color(Stderr, |t| t.truecolor(prgb.0, prgb.1, prgb.2)),
                    format!("{} · {:.1}s", frames_label(phase.frames), phase.secs)
                        .if_supports_color(Stderr, |t| t.dimmed()),
                ));
            }
        }

        lines
    }

    fn draw(&self) {
        // Advance the spinner once per rendered frame (not per tick) so it
        // cycles smoothly regardless of how many ticks the throttle coalesced.
        self.spin.set(self.spin.get().wrapping_add(1));
        let lines = self.render();
        let mut prev = self.prev.borrow_mut();

        let mut body = String::new();
        if !self.started.replace(true) {
            // First frame: hide the cursor and paint every line.
            body.push_str("\x1b[?25l");
            for line in &lines {
                body.push_str("\r\x1b[2K");
                body.push_str(line);
                body.push('\n');
            }
        } else {
            // Differential redraw: touch only the changed line range. The tree
            // is constant height, so `prev` and `lines` always align 1:1.
            let n = lines.len();
            let first = (0..n).find(|&i| prev.get(i) != Some(&lines[i]));
            let Some(first) = first else {
                return; // nothing changed — emit nothing (keeps throttled frames quiet)
            };
            let last = (0..n)
                .rev()
                .find(|&i| prev.get(i) != Some(&lines[i]))
                .unwrap();

            body.push_str(&format!("\x1b[{}A", n - first)); // up to the first changed line
            for line in &lines[first..=last] {
                body.push_str("\r\x1b[2K");
                body.push_str(line);
                body.push('\n');
            }
            let down = n - (last + 1);
            if down > 0 {
                body.push_str(&format!("\x1b[{down}B")); // back down below the region
            }
        }

        *prev = lines;
        drop(prev);

        let mut out = stderr();
        let _ = out.write_all(format!("{SYNC_BEGIN}{body}{SYNC_END}").as_bytes());
        let _ = out.flush();
    }

    fn throttled_draw(&self) {
        let now = Instant::now();
        if let Some(last) = self.last_draw.get()
            && now.duration_since(last).as_millis() < MIN_REDRAW_MS
        {
            return;
        }
        self.last_draw.set(Some(now));
        self.draw();
    }

    fn show_cursor(&self) {
        if self.started.get() {
            let mut out = stderr();
            let _ = out.write_all(b"\x1b[?25h");
            let _ = out.flush();
        }
    }
}

impl ProgressReporter for TreeReporter {
    fn started(&self, _name: &str, _steps: usize, _fps: u32) {
        self.draw();
    }

    fn step(&self, index: usize, _total: usize, _kind: &str, _name: &str) {
        let i = index - 1;
        {
            let mut status = self.status.borrow_mut();
            for slot in status.iter_mut().take(i) {
                *slot = Status::Done;
            }
            if let Some(slot) = status.get_mut(i) {
                *slot = Status::Active;
            }
        }
        self.active.set(i);
        self.frame_in_step.set(0);
        self.last_draw.set(Some(Instant::now()));
        self.draw();
    }

    fn tick(&self) {
        if self.active.get() != usize::MAX {
            self.frame_in_step.set(self.frame_in_step.get() + 1);
        }
        self.throttled_draw();
    }

    fn encoding(&self, _output: &str) {
        self.set_all(Status::Done);
        self.active.set(usize::MAX);
        self.last_draw.set(Some(Instant::now()));
        self.draw();
    }

    fn finished(&self, frames: u32, seconds: f64) {
        self.set_all(Status::Done);
        self.draw();
        self.show_cursor();
        let mut out = stderr();
        let _ = writeln!(
            out,
            "  {} {} · {seconds:.1}s\n",
            "✓".if_supports_color(Stderr, |t| t.truecolor(GB_GREEN.0, GB_GREEN.1, GB_GREEN.2)),
            frames_label(frames),
        );
        let _ = out.flush();
    }
}

impl Drop for TreeReporter {
    fn drop(&mut self) {
        // Restore the cursor even if the run errored before `finished`.
        self.show_cursor();
    }
}

/// Truncate to at most `max` characters, adding an ellipsis when clipped.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{PlanPhase, PlanStep, SegmentKind};

    fn sample_tree() -> PlanTree {
        PlanTree {
            name: "Demo run".into(),
            fps: 24,
            total_frames: 30,
            steps: vec![
                PlanStep {
                    name: "First step".into(),
                    kind: SegmentKind::Wait,
                    start_secs: 0.0,
                    frames: 13,
                    phases: vec![PlanPhase {
                        kind: SegmentKind::Hold,
                        frames: 13,
                        secs: 0.5,
                    }],
                },
                PlanStep {
                    name: "Second step".into(),
                    kind: SegmentKind::Scroll,
                    start_secs: 0.5,
                    frames: 17,
                    phases: vec![PlanPhase {
                        kind: SegmentKind::Scroll,
                        frames: 17,
                        secs: 0.7,
                    }],
                },
            ],
        }
    }

    #[test]
    fn render_marks_status_and_active_counter() {
        let reporter = TreeReporter::new(sample_tree(), 100);
        *reporter.status.borrow_mut() = vec![Status::Done, Status::Active];
        reporter.active.set(1);
        reporter.frame_in_step.set(3);

        let rendered = reporter.render().join("\n");
        assert!(rendered.contains("Demo run"));
        assert!(rendered.contains("First step"));
        assert!(rendered.contains("Second step"));
        assert!(rendered.contains('✔')); // first step marked done
        assert!(rendered.contains("3/17f")); // active step frame counter
        assert!(rendered.contains("scroll"));
    }

    #[test]
    fn long_names_are_truncated_to_fit() {
        assert_eq!(truncate("hello world", 5), "hell…");
        assert_eq!(truncate("hi", 5), "hi");
    }
}
