use std::{fs, io, path::PathBuf, process::Command as ProcessCommand};

use anyhow::{Context, Result, bail};
use arboard::Clipboard;
use clap::{Args, Parser, Subcommand};
use regex::Regex;
use sepia::{
    browser::AgentBrowserBackend,
    config::DemoConfig,
    encoder::FfmpegCliEncoder,
    github::{
        current_pr_number, current_repo_name_with_owner, get_pr_body, github_token, repo_info,
        update_pr_body,
    },
    inspect::open_in_browser,
    metadata::read_session_metadata,
    pr::{pr_data_from_metadata, render_pr_comment, upsert_marked_block_at_top},
    runner::run_capture,
    session::{SessionPaths, default_output_root, read_latest},
    skill_installer::{
        SkillInstallRequest, install_embedded_skill, list_installed_skills, remove_embedded_skill,
        sepia_skill_install_tip,
    },
    uploader::{ArtifactUploader, DryRunUploader},
};

#[derive(Debug, Parser)]
#[command(name = "sepia", version, about = "Agent-native PR demo capture")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Capture a scripted browser demo.
    Run(RunArgs),
    /// Open the inspection UI for a session, or latest when omitted.
    Inspect(InspectArgs),
    /// Generate or update GitHub PR demo evidence.
    Pr(PrArgs),
    /// Install/list/remove the bundled agent skill.
    Skill(SkillArgs),
}

#[derive(Debug, Args)]
struct RunArgs {
    config: PathBuf,
    /// Output root. Defaults to ~/Downloads/sepia.
    #[arg(long)]
    output_root: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct InspectArgs {
    session_dir: Option<PathBuf>,
    /// Output root used to locate latest.json when session-dir is omitted.
    #[arg(long)]
    output_root: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct PrArgs {
    /// Generate markdown only; do not upload or post.
    #[arg(long)]
    dry_run: bool,
    /// Explicit PR number. Auto-detects with `gh pr view` when omitted.
    #[arg(long)]
    pr: Option<u64>,
    /// Explicit GitHub repo as owner/name. Auto-detects with `gh repo view` when omitted.
    #[arg(long)]
    repo: Option<String>,
    /// Output root used to locate latest.json.
    #[arg(long)]
    output_root: Option<PathBuf>,
    /// Copy the latest MP4 path to the clipboard, prompt for a GitHub
    /// user-attachments URL, and place the Sepia block at the top of the PR description.
    #[arg(long)]
    attach: bool,
    /// Use an existing GitHub user-attachments video URL.
    #[arg(long)]
    video_url: Option<String>,
}

#[derive(Debug, Args)]
struct SkillArgs {
    #[command(subcommand)]
    command: SkillCommand,
}

#[derive(Debug, Subcommand)]
enum SkillCommand {
    /// Install the bundled Sepia skill globally by default.
    Install(SkillInstallArgs),
    /// List installed skills known to the skill ecosystem.
    List(SkillListArgs),
    /// Remove the bundled Sepia skill.
    Remove(SkillRemoveArgs),
}

#[derive(Debug, Args)]
struct SkillInstallArgs {
    /// Agent id to install for. May be repeated. Defaults to detected installed agents.
    #[arg(long)]
    agent: Vec<String>,
    /// Install project-locally instead of globally.
    #[arg(long)]
    project: bool,
}

#[derive(Debug, Args)]
struct SkillListArgs {
    #[arg(long)]
    agent: Vec<String>,
    #[arg(long)]
    project: bool,
}

#[derive(Debug, Args)]
struct SkillRemoveArgs {
    #[arg(long)]
    agent: Vec<String>,
    #[arg(long)]
    project: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if !matches!(cli.command, Command::Skill(_)) {
        maybe_print_skill_install_tip().await;
    }
    match cli.command {
        Command::Run(args) => run(args)?,
        Command::Inspect(args) => inspect(args)?,
        Command::Pr(args) => pr(args).await?,
        Command::Skill(args) => skill(args).await?,
    }
    Ok(())
}

async fn maybe_print_skill_install_tip() {
    if let Some(tip) = sepia_skill_install_tip().await {
        eprintln!("{tip}");
    }
}

fn run(args: RunArgs) -> Result<()> {
    let config = DemoConfig::from_path(&args.config)?;
    let output_root = args.output_root.map_or_else(default_output_root, Ok)?;
    let session = config
        .session
        .clone()
        .unwrap_or_else(|| format!("sepia-{}", sepia::session::slugify(&config.name)));
    let browser = AgentBrowserBackend::new(session);
    let encoder = FfmpegCliEncoder::default();
    let output = run_capture(&config, output_root, &browser, &encoder)?;

    println!("Session: {}", output.paths.root.display());
    println!("Video:   {}", output.paths.video.display());
    println!("Inspect: {}", output.paths.inspect_html.display());
    println!("Frames:  {}", output.frame_count);
    Ok(())
}

fn inspect(args: InspectArgs) -> Result<()> {
    let session_root = if let Some(session_dir) = args.session_dir {
        session_dir
    } else {
        let output_root = args.output_root.map_or_else(default_output_root, Ok)?;
        read_latest(output_root)?.latest_session
    };
    let paths = SessionPaths::from_root(session_root);
    if !paths.inspect_html.exists() {
        bail!(
            "No inspect UI found at {}. Run `sepia run <demo.toml>` first or pass a valid session directory.",
            paths.inspect_html.display()
        );
    }
    open_in_browser(&paths.inspect_html)
}

async fn pr(args: PrArgs) -> Result<()> {
    let output_root = args.output_root.map_or_else(default_output_root, Ok)?;
    let latest = read_latest(output_root)?;
    let paths = SessionPaths::from_root(latest.latest_session);
    let metadata = read_session_metadata(&paths)?;

    let video_url = if let Some(video_url) = args.video_url.clone() {
        Some(video_url)
    } else if args.attach {
        Some(prompt_for_attachment_url(
            &metadata.video,
            args.repo.as_deref(),
            args.pr,
        )?)
    } else if args.dry_run {
        Some(DryRunUploader.upload(&metadata.video).await?.url)
    } else {
        None
    };

    let data = pr_data_from_metadata(&metadata, video_url.clone(), &[]);
    let body = render_pr_comment(&data);
    fs::write(&paths.pr_comment_md, &body)
        .with_context(|| format!("failed to write {}", paths.pr_comment_md.display()))?;

    if args.dry_run {
        println!("{body}");
        return Ok(());
    }

    let Some(video_url) = video_url else {
        bail!(
            "Sepia PR updates now require an inline GitHub attachment URL. Use `sepia pr --attach` or pass `--video-url https://github.com/user-attachments/assets/...`."
        );
    };
    validate_attachment_url(&video_url)?;

    let token = github_token()?;
    let (owner, repo_name) = if let Some(repo) = &args.repo {
        parse_repo(repo)?
    } else {
        current_repo_name_with_owner()?
    };
    let pr_number = current_pr_number(args.pr)?;
    let client = reqwest::Client::new();
    let repo = repo_info(&client, &token, &owner, &repo_name).await?;

    let current_body = get_pr_body(&client, &token, &repo, pr_number).await?;
    let updated_body = upsert_marked_block_at_top(&current_body, &body);
    update_pr_body(&client, &token, &repo, pr_number, &updated_body).await?;
    println!("Updated Sepia block at the top of PR description on {owner}/{repo_name}#{pr_number}");
    Ok(())
}

fn prompt_for_attachment_url(
    video_path: &std::path::Path,
    repo: Option<&str>,
    pr: Option<u64>,
) -> Result<String> {
    copy_to_clipboard(&video_path.display().to_string())?;
    reveal_file(video_path);
    if let (Some(repo), Some(pr)) = (repo, pr) {
        open_url(&format!("https://github.com/{repo}/pull/{pr}"));
    }

    println!("\nSepia copied the demo MP4 path to your clipboard:");
    println!("  {}", video_path.display());
    println!("\nUpload it to GitHub by pasting or dragging it into the PR description editor.");
    println!("After GitHub finishes uploading, copy the generated URL that starts with:");
    println!("  https://github.com/user-attachments/assets/");
    println!("\nPaste that URL here and press Enter:");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    extract_attachment_url(&input).context("no GitHub user-attachments URL found in input")
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("failed to access system clipboard")?;
    clipboard
        .set_text(text.to_string())
        .context("failed to copy video path to clipboard")
}

fn reveal_file(path: &std::path::Path) {
    if cfg!(target_os = "macos") {
        let _ = ProcessCommand::new("open").arg("-R").arg(path).spawn();
    }
}

fn open_url(url: &str) {
    if cfg!(target_os = "macos") {
        let _ = ProcessCommand::new("open").arg(url).spawn();
    }
}

fn extract_attachment_url(input: &str) -> Option<String> {
    let re = Regex::new(r#"https://github\.com/user-attachments/assets/[A-Za-z0-9_-]+"#).ok()?;
    re.find(input).map(|m| m.as_str().to_string())
}

fn validate_attachment_url(url: &str) -> Result<()> {
    if extract_attachment_url(url).as_deref() == Some(url) {
        Ok(())
    } else {
        bail!("expected a GitHub user-attachments URL, got `{url}`")
    }
}

fn parse_repo(repo: &str) -> Result<(String, String)> {
    let Some((owner, name)) = repo.split_once('/') else {
        bail!("--repo must be in owner/name form, got `{repo}`");
    };
    Ok((owner.to_string(), name.to_string()))
}

async fn skill(args: SkillArgs) -> Result<()> {
    match args.command {
        SkillCommand::Install(args) => {
            let summary = install_embedded_skill(SkillInstallRequest {
                agents: args.agent,
                global: !args.project,
            })
            .await?;
            println!(
                "Installed Sepia skill for: {}",
                summary.installed_agents.join(", ")
            );
        }
        SkillCommand::List(args) => {
            for line in list_installed_skills(!args.project, args.agent).await? {
                println!("{line}");
            }
        }
        SkillCommand::Remove(args) => {
            remove_embedded_skill(!args.project, args.agent).await?;
            println!("Removed Sepia skill");
        }
    }
    Ok(())
}
