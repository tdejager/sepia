//! CLI reporter selection and the plain-text fallback.
//!
//! [`cli_reporter`] picks the live animated tree when stderr is a terminal and
//! the tree fits, and otherwise the plain line-by-line reporter used for agents,
//! CI, pipes, and terminals too short for the tree.

use std::io::{IsTerminal, stderr};

use terminal_size::{Height, Width, terminal_size_of};

use super::ProgressReporter;
use super::tree::TreeReporter;
use crate::config::DemoConfig;
use crate::timeline::{TimelinePlan, plan_tree};

/// Choose the best progress reporter for the current environment.
#[must_use]
pub fn cli_reporter(config: &DemoConfig, plan: &TimelinePlan) -> Box<dyn ProgressReporter> {
    let out = stderr();
    if out.is_terminal()
        && let Some((Width(w), Height(h))) = terminal_size_of(&out)
    {
        let tree = plan_tree(config, plan);
        // Leave a couple of lines of headroom for the summary line.
        if TreeReporter::height(&tree) + 2 <= usize::from(h) {
            return Box::new(TreeReporter::new(tree, usize::from(w)));
        }
    }
    Box::new(PlainReporter)
}

/// Line-by-line reporter: no cursor control, safe for any output target.
pub struct PlainReporter;

impl ProgressReporter for PlainReporter {
    fn started(&self, name: &str, steps: usize, fps: u32) {
        eprintln!("\nSepia · recording \"{name}\" — {steps} steps @ {fps} fps");
    }

    fn opening(&self, url: &str) {
        eprintln!("  opening {url}");
    }

    fn step(&self, index: usize, total: usize, kind: &str, name: &str) {
        eprintln!("  [{index}/{total}] {kind:<6}  {name}");
    }

    fn encoding(&self, output: &str) {
        eprintln!("  encoding {output} …");
    }

    fn finished(&self, frames: u32, seconds: f64) {
        eprintln!("  ✓ {frames} frames · {seconds:.1}s\n");
    }
}
