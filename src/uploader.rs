use std::{future::Future, path::Path, pin::Pin};

use miette::Result;

use crate::ResultContextExt;
use reqwest::Client;

use crate::github::{GitHubRepo, UploadedAsset, upload_repo_content, upload_user_attachment};

pub type UploadFuture<'a> = Pin<Box<dyn Future<Output = Result<UploadedAsset>> + Send + 'a>>;

pub trait ArtifactUploader {
    fn upload<'a>(&'a self, path: &'a Path) -> UploadFuture<'a>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DryRunUploader;

impl ArtifactUploader for DryRunUploader {
    fn upload<'a>(&'a self, path: &'a Path) -> UploadFuture<'a> {
        Box::pin(async move {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .context("upload path has no valid file name")?
                .to_string();
            Ok(UploadedAsset {
                name,
                url: local_file_url(path)?,
            })
        })
    }
}

#[derive(Debug, Clone)]
pub struct GitHubUserAttachmentsUploader {
    client: Client,
    token: String,
    repo_id: u64,
}

impl GitHubUserAttachmentsUploader {
    #[must_use]
    pub fn new(client: Client, token: impl Into<String>, repo_id: u64) -> Self {
        Self {
            client,
            token: token.into(),
            repo_id,
        }
    }
}

impl ArtifactUploader for GitHubUserAttachmentsUploader {
    fn upload<'a>(&'a self, path: &'a Path) -> UploadFuture<'a> {
        Box::pin(async move {
            upload_user_attachment(&self.client, &self.token, self.repo_id, path).await
        })
    }
}

#[derive(Debug, Clone)]
pub struct GitHubRepoContentsUploader {
    client: Client,
    token: String,
    repo: GitHubRepo,
    artifacts_branch: String,
    upload_root: String,
}

impl GitHubRepoContentsUploader {
    #[must_use]
    pub fn new(
        client: Client,
        token: impl Into<String>,
        repo: GitHubRepo,
        artifacts_branch: impl Into<String>,
        upload_root: impl Into<String>,
    ) -> Self {
        Self {
            client,
            token: token.into(),
            repo,
            artifacts_branch: artifacts_branch.into(),
            upload_root: upload_root.into(),
        }
    }
}

impl ArtifactUploader for GitHubRepoContentsUploader {
    fn upload<'a>(&'a self, path: &'a Path) -> UploadFuture<'a> {
        Box::pin(async move {
            upload_repo_content(
                &self.client,
                &self.token,
                &self.repo,
                &self.artifacts_branch,
                &self.upload_root,
                path,
            )
            .await
        })
    }
}

fn local_file_url(path: &Path) -> Result<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("failed to read current directory")?
            .join(path)
    };
    Ok(format!("file://{}", absolute.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn dry_run_upload_returns_local_file_url() {
        let dir = tempdir().unwrap();
        let video = dir.path().join("demo.mp4");
        fs::write(&video, b"fake mp4").unwrap();

        let uploaded = DryRunUploader.upload(&video).await.unwrap();

        assert_eq!(uploaded.name, "demo.mp4");
        assert_eq!(uploaded.url, format!("file://{}", video.display()));
    }
}
