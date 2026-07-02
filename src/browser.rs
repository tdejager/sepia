use std::{path::Path, process::Command};

use miette::{Result, bail};

use crate::ResultContextExt;

pub trait BrowserBackend {
    fn open(&self, url: &str) -> Result<()>;
    fn eval(&self, js: &str) -> Result<()>;
    fn fill(&self, selector: &str, text: &str) -> Result<()>;
    fn screenshot(&self, path: &Path) -> Result<()>;
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
        Ok(())
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
