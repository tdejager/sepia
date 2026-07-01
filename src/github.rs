use std::{env, fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Client, StatusCode, multipart};
use serde::{Deserialize, Serialize};

use crate::pr::{ExistingComment, content_type_for};

const GITHUB_API_VERSION: &str = "2022-11-28";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepo {
    pub owner: String,
    pub name: String,
    pub id: u64,
    pub default_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadedAsset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct RepoResponse {
    id: u64,
    default_branch: String,
}

#[derive(Debug, Deserialize)]
struct UploadPolicyResponse {
    upload_url: String,
    form: std::collections::BTreeMap<String, String>,
    asset: UploadPolicyAsset,
}

#[derive(Debug, Deserialize)]
struct UploadPolicyAsset {
    href: String,
}

#[derive(Debug, Serialize)]
struct UploadPolicyRequest<'a> {
    name: &'a str,
    size: u64,
    content_type: &'static str,
    repository_id: u64,
}

pub fn github_token() -> Result<String> {
    if let Ok(token) = env::var("GH_TOKEN")
        && !token.trim().is_empty()
    {
        return Ok(token.trim().to_string());
    }
    if let Ok(token) = env::var("GITHUB_TOKEN")
        && !token.trim().is_empty()
    {
        return Ok(token.trim().to_string());
    }

    let output = Command::new("gh")
        .args(["auth", "token"])
        .output()
        .context(
            "failed to run `gh auth token`; install/authenticate GitHub CLI or set GH_TOKEN",
        )?;
    if !output.status.success() {
        bail!(
            "could not get GitHub token from `gh auth token`: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn current_repo_name_with_owner() -> Result<(String, String)> {
    let output = Command::new("gh")
        .args([
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ])
        .output()
        .context("failed to run `gh repo view`; run from a GitHub repository")?;
    if !output.status.success() {
        bail!(
            "could not determine GitHub repository: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let nwo = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let Some((owner, name)) = nwo.split_once('/') else {
        bail!("unexpected GitHub repository name `{nwo}`");
    };
    Ok((owner.to_string(), name.to_string()))
}

pub fn current_pr_number(explicit: Option<u64>) -> Result<u64> {
    if let Some(number) = explicit {
        return Ok(number);
    }
    let output = Command::new("gh")
        .args(["pr", "view", "--json", "number", "-q", ".number"])
        .output()
        .context("failed to run `gh pr view`; pass --pr or run from a branch with a PR")?;
    if !output.status.success() {
        bail!(
            "could not determine PR number: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse::<u64>()
        .with_context(|| format!("unexpected PR number `{}`", text.trim()))
}

pub async fn repo_info(
    client: &Client,
    token: &str,
    owner: &str,
    repo: &str,
) -> Result<GitHubRepo> {
    let response = github_request(
        client.get(format!("https://api.github.com/repos/{owner}/{repo}")),
        token,
    )
    .send()
    .await?;
    ensure_success(response)
        .await?
        .json::<RepoResponse>()
        .await
        .map(|repo_response| GitHubRepo {
            owner: owner.to_string(),
            name: repo.to_string(),
            id: repo_response.id,
            default_branch: repo_response.default_branch,
        })
        .context("failed to decode GitHub repo response")
}

pub async fn upload_user_attachment(
    client: &Client,
    token: &str,
    repo_id: u64,
    path: &Path,
) -> Result<UploadedAsset> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("upload path has no valid file name")?;
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let request = UploadPolicyRequest {
        name,
        size: bytes.len() as u64,
        content_type: content_type_for(path),
        repository_id: repo_id,
    };

    let policy = client
        .post("https://github.com/upload/policies/assets")
        .header("Accept", "application/json")
        .header("Authorization", format!("token {token}"))
        .json(&request)
        .send()
        .await
        .context("failed to request GitHub upload policy")?;
    let policy = ensure_success(policy)
        .await?
        .json::<UploadPolicyResponse>()
        .await
        .context("failed to decode GitHub upload policy")?;

    let mut form = multipart::Form::new();
    for (key, value) in policy.form {
        form = form.text(key, value);
    }
    let part = multipart::Part::bytes(bytes)
        .file_name(name.to_string())
        .mime_str(content_type_for(path))?;
    form = form.part("file", part);

    let upload = client
        .post(policy.upload_url)
        .multipart(form)
        .send()
        .await
        .context("failed to upload asset to GitHub storage")?;
    if !upload.status().is_success() {
        bail!(
            "GitHub asset upload failed ({}): {}",
            upload.status(),
            upload.text().await.unwrap_or_default()
        );
    }

    Ok(UploadedAsset {
        name: name.to_string(),
        url: policy.asset.href,
    })
}

#[derive(Debug, Deserialize)]
struct GitRefResponse {
    object: GitRefObject,
}

#[derive(Debug, Deserialize)]
struct GitRefObject {
    sha: String,
}

#[derive(Debug, Serialize)]
struct CreateRefRequest {
    #[serde(rename = "ref")]
    reference: String,
    sha: String,
}

#[derive(Debug, Deserialize)]
struct ContentResponse {
    sha: String,
}

#[derive(Debug, Serialize)]
struct PutContentRequest<'a> {
    message: String,
    content: String,
    branch: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha: Option<String>,
}

pub async fn upload_repo_content(
    client: &Client,
    token: &str,
    repo: &GitHubRepo,
    artifacts_branch: &str,
    upload_root: &str,
    path: &Path,
) -> Result<UploadedAsset> {
    ensure_artifacts_branch(client, token, repo, artifacts_branch).await?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("upload path has no valid file name")?;
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let upload_path = format!("{}/{}", upload_root.trim_matches('/'), name);
    let encoded_path = encode_path(&upload_path);
    let existing_sha =
        existing_content_sha(client, token, repo, artifacts_branch, &encoded_path).await?;
    let body = PutContentRequest {
        message: format!("sepia: upload {upload_path}"),
        content: STANDARD.encode(bytes),
        branch: artifacts_branch,
        sha: existing_sha,
    };

    let response = github_request(
        client.put(format!(
            "https://api.github.com/repos/{}/{}/contents/{}",
            repo.owner, repo.name, encoded_path
        )),
        token,
    )
    .json(&body)
    .send()
    .await?;
    ensure_success(response).await?;

    Ok(UploadedAsset {
        name: name.to_string(),
        url: format!(
            "https://github.com/{}/{}/blob/{}/{}?raw=1",
            repo.owner,
            repo.name,
            urlencoding::encode(artifacts_branch),
            upload_path
                .split('/')
                .map(urlencoding::encode)
                .collect::<Vec<_>>()
                .join("/")
        ),
    })
}

async fn existing_content_sha(
    client: &Client,
    token: &str,
    repo: &GitHubRepo,
    branch: &str,
    encoded_path: &str,
) -> Result<Option<String>> {
    let response = github_request(
        client.get(format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            repo.owner,
            repo.name,
            encoded_path,
            urlencoding::encode(branch)
        )),
        token,
    )
    .send()
    .await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let content = ensure_success(response)
        .await?
        .json::<ContentResponse>()
        .await
        .context("failed to decode existing GitHub content response")?;
    Ok(Some(content.sha))
}

async fn ensure_artifacts_branch(
    client: &Client,
    token: &str,
    repo: &GitHubRepo,
    artifacts_branch: &str,
) -> Result<()> {
    let ref_url = format!(
        "https://api.github.com/repos/{}/{}/git/ref/heads/{}",
        repo.owner,
        repo.name,
        urlencoding::encode(artifacts_branch)
    );
    let existing = github_request(client.get(&ref_url), token).send().await?;
    if existing.status().is_success() {
        return Ok(());
    }
    if existing.status() != StatusCode::NOT_FOUND {
        ensure_success(existing).await?;
        return Ok(());
    }

    let default_ref = github_request(
        client.get(format!(
            "https://api.github.com/repos/{}/{}/git/ref/heads/{}",
            repo.owner,
            repo.name,
            urlencoding::encode(&repo.default_branch)
        )),
        token,
    )
    .send()
    .await?;
    let default_ref = ensure_success(default_ref)
        .await?
        .json::<GitRefResponse>()
        .await
        .context("failed to decode default branch ref")?;
    let create = CreateRefRequest {
        reference: format!("refs/heads/{artifacts_branch}"),
        sha: default_ref.object.sha,
    };
    let response = github_request(
        client.post(format!(
            "https://api.github.com/repos/{}/{}/git/refs",
            repo.owner, repo.name
        )),
        token,
    )
    .json(&create)
    .send()
    .await?;
    ensure_success(response).await?;
    Ok(())
}

fn encode_path(path: &str) -> String {
    path.split('/')
        .map(urlencoding::encode)
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug, Deserialize)]
struct IssueCommentResponse {
    id: u64,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PullResponse {
    body: Option<String>,
}

#[derive(Debug, Serialize)]
struct CommentBody<'a> {
    body: &'a str,
}

pub async fn get_pr_body(
    client: &Client,
    token: &str,
    repo: &GitHubRepo,
    pr_number: u64,
) -> Result<String> {
    let response = github_request(
        client.get(format!(
            "https://api.github.com/repos/{}/{}/pulls/{}",
            repo.owner, repo.name, pr_number
        )),
        token,
    )
    .send()
    .await?;
    let pull = ensure_success(response)
        .await?
        .json::<PullResponse>()
        .await
        .context("failed to decode PR response")?;
    Ok(pull.body.unwrap_or_default())
}

pub async fn update_pr_body(
    client: &Client,
    token: &str,
    repo: &GitHubRepo,
    pr_number: u64,
    body: &str,
) -> Result<()> {
    let response = github_request(
        client.patch(format!(
            "https://api.github.com/repos/{}/{}/pulls/{}",
            repo.owner, repo.name, pr_number
        )),
        token,
    )
    .json(&CommentBody { body })
    .send()
    .await?;
    ensure_success(response).await?;
    Ok(())
}

pub async fn list_pr_comments(
    client: &Client,
    token: &str,
    repo: &GitHubRepo,
    pr_number: u64,
) -> Result<Vec<ExistingComment>> {
    let response = github_request(
        client.get(format!(
            "https://api.github.com/repos/{}/{}/issues/{}/comments?per_page=100",
            repo.owner, repo.name, pr_number
        )),
        token,
    )
    .send()
    .await?;
    let comments = ensure_success(response)
        .await?
        .json::<Vec<IssueCommentResponse>>()
        .await
        .context("failed to decode PR comments")?;
    Ok(comments
        .into_iter()
        .map(|comment| ExistingComment {
            id: comment.id,
            body: comment.body.unwrap_or_default(),
        })
        .collect())
}

pub async fn create_pr_comment(
    client: &Client,
    token: &str,
    repo: &GitHubRepo,
    pr_number: u64,
    body: &str,
) -> Result<()> {
    let response = github_request(
        client.post(format!(
            "https://api.github.com/repos/{}/{}/issues/{}/comments",
            repo.owner, repo.name, pr_number
        )),
        token,
    )
    .json(&CommentBody { body })
    .send()
    .await?;
    ensure_success(response).await?;
    Ok(())
}

pub async fn update_pr_comment(
    client: &Client,
    token: &str,
    repo: &GitHubRepo,
    comment_id: u64,
    body: &str,
) -> Result<()> {
    let response = github_request(
        client.patch(format!(
            "https://api.github.com/repos/{}/{}/issues/comments/{}",
            repo.owner, repo.name, comment_id
        )),
        token,
    )
    .json(&CommentBody { body })
    .send()
    .await?;
    ensure_success(response).await?;
    Ok(())
}

fn github_request(builder: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    builder
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {token}"))
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .header("User-Agent", "sepia")
}

async fn ensure_success(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        Ok(response)
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("GitHub request failed ({status}): {body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_pr_number_does_not_require_gh() {
        assert_eq!(current_pr_number(Some(18)).unwrap(), 18);
    }
}
