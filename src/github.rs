use std::{io::Write, path::Path, process::Command};

use miette::{Result, bail};
use serde::Deserialize;

use crate::ResultContextExt;

#[derive(Debug)]
pub struct PullRequest {
    pub url: String,
    pub body: String,
}

#[derive(Deserialize)]
struct PullRequestResponse {
    url: String,
    body: Option<String>,
}

pub fn get_pull_request(repo: Option<&str>, number: Option<u64>) -> Result<PullRequest> {
    let mut command = Command::new("gh");
    command.args(["pr", "view"]);
    if let Some(number) = number {
        command.arg(number.to_string());
    }
    if let Some(repo) = repo {
        command.args(["--repo", repo]);
    }
    let output = command.args(["--json", "url,body"]).output().context(
        "failed to run `gh pr view`; install GitHub CLI and authenticate with `gh auth login`",
    )?;
    if !output.status.success() {
        bail!(
            "`gh pr view` failed ({}): {}\nPass --repo OWNER/REPO and --pr NUMBER, or run from a branch with a pull request.",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let response: PullRequestResponse = serde_json::from_slice(&output.stdout)
        .context("failed to decode `gh pr view --json url,body` response")?;
    Ok(PullRequest {
        url: response.url,
        body: response.body.unwrap_or_default(),
    })
}

/// Publishes an absolute video path referenced in `body`, after human approval.
/// Native gh may upload or update the PR before failing; callers must not retry automatically.
pub fn attach_recording(pr_url: &str, body: &str, video: &Path) -> Result<()> {
    // A body file avoids a piped-stdin writer racing the child's stderr or exit.
    let mut body_file =
        tempfile::NamedTempFile::new().context("failed to create temporary PR description file")?;
    body_file
        .write_all(body.as_bytes())
        .context("failed to write temporary PR description file")?;

    let output = Command::new("gh")
        .args(["pr", "edit", pr_url, "--body-file"])
        .arg(body_file.path())
        .arg("--attach")
        .arg(video)
        .output()
        .context(
            "failed to run `gh pr edit --attach`; install GitHub CLI 2.99.0 or newer and authenticate with `gh auth login`. If gh started, attachments or the PR may already have changed; inspect before retrying",
        )?;
    if !output.status.success() {
        bail!(
            "`gh pr edit --attach` failed ({}): {}\nNative attachment publishing requires GitHub CLI 2.99.0 or newer, repository write access, and OAuth/PAT authentication. Attachments or the PR may already have changed; inspect before retrying.",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
