//! Fail-fast checks run before a capture so missing tools are reported up front
//! — with an install hint — instead of after every frame has been captured.

use miette::{Diagnostic, Result};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("`{tool}` was not found on your PATH")]
#[diagnostic(code(sepia::preflight::missing_tool), help("{hint}"))]
pub struct MissingTool {
    tool: String,
    hint: String,
}

/// Ensure an external tool resolves on `PATH`, returning a diagnostic with an
/// install hint otherwise.
pub fn ensure_binary(tool: &str, hint: impl Into<String>) -> Result<()> {
    if which::which(tool).is_ok() {
        Ok(())
    } else {
        Err(MissingTool {
            tool: tool.to_owned(),
            hint: hint.into(),
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tool_is_reported_with_hint() {
        let err = ensure_binary("sepia-definitely-not-a-real-tool", "install it").unwrap_err();
        let rendered = format!("{err:?}");
        assert!(rendered.contains("sepia-definitely-not-a-real-tool"));
        assert!(rendered.contains("install it"));
    }

    #[test]
    fn present_tool_passes() {
        // `cargo` is always present in the test environment.
        assert!(ensure_binary("cargo", "unused").is_ok());
    }
}
