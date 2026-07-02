//! Live progress feedback for a capture run.
//!
//! The runner depends only on the [`ProgressReporter`] trait, so concrete
//! reporters (currently the kdam-backed [`CliReporter`]) stay quarantined in
//! submodules and can be swapped, restyled, or split into their own crate
//! later without touching the capture pipeline.

mod cli;
mod tree;

pub use cli::cli_reporter;

/// Receives capture progress events. Every method defaults to a no-op so
/// implementors override only what they care about — and `()` is a fully silent
/// reporter, used by tests and library callers that want no output.
pub trait ProgressReporter {
    /// Called once before anything is driven.
    fn started(&self, _name: &str, _steps: usize, _fps: u32) {}
    /// Called just before opening the initial URL.
    fn opening(&self, _url: &str) {}
    /// Called before each step runs (`index` is 1-based).
    fn step(&self, _index: usize, _total: usize, _kind: &str, _name: &str) {}
    /// Called on every captured or held frame, so animated reporters stay
    /// smooth during the slow parts of a step (screenshots, scroll frames).
    fn tick(&self) {}
    /// Called just before the frames are encoded into the video.
    fn encoding(&self, _output: &str) {}
    /// Called after the video is written.
    fn finished(&self, _frames: u32, _seconds: f64) {}
}

/// No-op reporter for tests and silent library use.
impl ProgressReporter for () {}
