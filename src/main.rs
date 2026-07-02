use std::{fs, io, io::IsTerminal, path::PathBuf};

use clap::{Args, Parser, Subcommand};
use miette::{Result, bail};
use regex::Regex;
use sepia::{
    ResultContextExt,
    browser::AgentBrowserBackend,
    config::DemoConfig,
    encoder::FfmpegCliEncoder,
    github::{
        current_pr_number, current_repo_name_with_owner, get_pr_body, github_token, repo_info,
        update_pr_body,
    },
    inspect::open_in_browser,
    metadata::read_session_metadata,
    pr::{
        pr_data_from_metadata, remove_marked_block, render_pr_comment, upsert_marked_block_at_top,
    },
    progress::cli_reporter,
    runner::run_capture,
    session::{SessionPaths, default_output_root, read_latest},
    skill_installer::{
        SkillInstallRequest, install_embedded_skill, list_installed_skills, remove_embedded_skill,
        sepia_skill_install_tip,
    },
    uploader::{ArtifactUploader, DryRunUploader},
};

#[derive(Debug, Parser)]
#[command(
    name = "sepia",
    version,
    about = "Agent-native PR demo capture",
    after_help = "EXAMPLES:\n  \
        sepia run examples/hacker-news-browse.toml   # record a demo (opens inspect when interactive)\n  \
        sepia run demo.toml --plan                   # preview the capture plan as a tree, no recording\n  \
        sepia inspect                                # open the latest capture's inspect page\n  \
        sepia pr --attach --repo owner/name --pr 12  # attach the demo to a GitHub PR\n  \
        sepia completions fish > ~/.config/fish/completions/sepia.fish\n\n\
        More examples live in the examples/ directory."
)]
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
    /// Print a shell completion script (bash, zsh, fish, …).
    Completions(CompletionsArgs),
}

#[derive(Debug, Args)]
struct CompletionsArgs {
    /// Shell to generate completions for.
    shell: clap_complete::Shell,
}

#[derive(Debug, Args)]
struct RunArgs {
    config: PathBuf,
    /// Output root. Defaults to ~/Downloads/sepia.
    #[arg(long)]
    output_root: Option<PathBuf>,
    /// Do not open the inspect page after recording. By default `run` opens it
    /// automatically when stderr is an interactive terminal.
    #[arg(long)]
    no_open: bool,
    /// Print the compiled capture plan as a tree and exit without recording.
    #[arg(long)]
    plan: bool,
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
    /// Open a file window with the demo to drag into the PR editor, then prompt
    /// for the resulting GitHub user-attachments URL.
    #[arg(long)]
    attach: bool,
    /// Read the PR description, find the user-attachments URL you already dropped
    /// into it, and wrap it in the Sepia block — no copy-paste needed.
    #[arg(long)]
    grab: bool,
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
async fn main() -> miette::Result<()> {
    install_miette_hook();
    let cli = Cli::parse();
    if !matches!(cli.command, Command::Skill(_) | Command::Completions(_)) {
        maybe_print_skill_install_tip().await;
    }
    match cli.command {
        Command::Run(args) => run(args)?,
        Command::Inspect(args) => inspect(args)?,
        Command::Pr(args) => pr(args).await?,
        Command::Skill(args) => skill(args).await?,
        Command::Completions(args) => completions(args),
    }
    Ok(())
}

fn completions(args: CompletionsArgs) {
    use clap::CommandFactory;
    clap_complete::generate(args.shell, &mut Cli::command(), "sepia", &mut io::stdout());
}

fn install_miette_hook() {
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(false)
                .unicode(true)
                .build(),
        )
    }));
}

async fn maybe_print_skill_install_tip() {
    if let Some(tip) = sepia_skill_install_tip().await {
        eprintln!("{tip}");
    }
}

fn run(args: RunArgs) -> miette::Result<()> {
    let config = DemoConfig::from_path(&args.config)?;

    if args.plan {
        let plan = sepia::timeline::TimelineCompiler::compile(&config);
        print!(
            "{}",
            sepia::timeline::render_plan_tree(&config, &plan, owo_colors::Stream::Stdout)
        );
        return Ok(());
    }

    let output_root = args.output_root.map_or_else(default_output_root, Ok)?;
    let session = config
        .session
        .clone()
        .unwrap_or_else(|| format!("sepia-{}", sepia::session::slugify(&config.name)));

    // In an interactive terminal the reporter draws the plan as a live tree that
    // updates as each step runs; agents and CI get plain per-step lines.
    let plan = sepia::timeline::TimelineCompiler::compile(&config);
    let reporter = cli_reporter(&config, &plan);

    let browser = AgentBrowserBackend::new(session);
    let encoder = FfmpegCliEncoder::default();
    let output = run_capture(&config, output_root, &browser, &encoder, reporter.as_ref())?;

    println!("Session: {}", output.paths.root.display());
    println!("Video:   {}", output.paths.video.display());
    println!("Inspect: {}", output.paths.inspect_html.display());
    println!("Frames:  {}", output.frame_count);

    // Open the inspect page for a human at a terminal; stay quiet for agents,
    // CI, and piped runs so a browser never pops up unexpectedly.
    if !args.no_open
        && io::stderr().is_terminal()
        && let Err(err) = open_in_browser(&output.paths.inspect_html)
    {
        eprintln!("Could not open the inspect page automatically: {err}");
    }
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
    let output_root = args
        .output_root
        .clone()
        .map_or_else(default_output_root, Ok)?;
    let latest = read_latest(output_root)?;
    let paths = SessionPaths::from_root(latest.latest_session);
    let metadata = read_session_metadata(&paths)?;

    if args.grab {
        return pr_grab(&args, &metadata, &paths).await;
    }

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

/// Read the PR body, find the user-attachments URL that was dropped into it,
/// and consolidate it into the Sepia block — no manual copy-paste.
async fn pr_grab(
    args: &PrArgs,
    metadata: &sepia::metadata::SessionMetadata,
    paths: &SessionPaths,
) -> Result<()> {
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
    // Look for the freshly-dropped URL in the user's content, not our own block.
    let user_content = remove_marked_block(&current_body);
    let Some(video_url) = extract_attachment_url(&user_content) else {
        bail!(
            "No github.com/user-attachments/assets/… URL found in PR #{pr_number}'s description.\n\nDrag the demo.mp4 into the PR description and save, then rerun `sepia pr --grab`."
        );
    };
    // Drop the raw line(s) that hold the URL so the video only lives in our block.
    let cleaned = remove_lines_containing(&user_content, &video_url);

    let data = pr_data_from_metadata(metadata, Some(video_url), &[]);
    let block = render_pr_comment(&data);
    fs::write(&paths.pr_comment_md, &block)
        .with_context(|| format!("failed to write {}", paths.pr_comment_md.display()))?;

    let updated_body = upsert_marked_block_at_top(&cleaned, &block);
    update_pr_body(&client, &token, &repo, pr_number, &updated_body).await?;
    println!(
        "Grabbed the uploaded video and updated the Sepia block on {owner}/{repo_name}#{pr_number}"
    );
    Ok(())
}

/// Remove every line containing `needle`, then trim surrounding blank lines.
fn remove_lines_containing(content: &str, needle: &str) -> String {
    content
        .lines()
        .filter(|line| !line.contains(needle))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn prompt_for_attachment_url(
    video_path: &std::path::Path,
    repo: Option<&str>,
    pr: Option<u64>,
) -> Result<String> {
    // Open a file-manager window with the demo selected so it can be dragged
    // into the PR editor. (GitHub's upload takes the file itself, not a path,
    // so there is nothing useful to put on the clipboard.)
    reveal_file(video_path);
    if let (Some(repo), Some(pr)) = (repo, pr) {
        open_url(&format!("https://github.com/{repo}/pull/{pr}"));
    }

    println!("\nSepia opened a file window with your demo selected:");
    println!("  {}", video_path.display());
    println!("\nDrag that file into the PR description's editor — or use the editor's");
    println!("\"attach files\" area and choose it. GitHub uploads it and inserts a URL");
    println!("that starts with:");
    println!("  https://github.com/user-attachments/assets/");

    loop {
        println!("\nPaste that URL here and press Enter (leave empty to cancel):");
        let mut input = String::new();
        let read = io::stdin()
            .read_line(&mut input)
            .context("failed to read attachment URL from stdin")?;
        if read == 0 || input.trim().is_empty() {
            bail!("cancelled: no GitHub user-attachments URL provided");
        }
        if let Some(url) = extract_attachment_url(&input) {
            return Ok(url);
        }
        println!(
            "  That isn't a https://github.com/user-attachments/assets/… URL — let's try again."
        );
    }
}

fn reveal_file(path: &std::path::Path) {
    let _ = opener::reveal(path);
}

fn open_url(url: &str) {
    let _ = opener::open_browser(url);
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
