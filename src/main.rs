use std::{fs, io, io::IsTerminal, path::PathBuf};

use clap::{Args, Parser, Subcommand};
use miette::{Result, bail};
use sepia::{
    ResultContextExt,
    browser::AgentBrowserBackend,
    config::DemoConfig,
    encoder::FfmpegCliEncoder,
    github::{attach_recording, get_pull_request},
    inspect::open_in_browser,
    metadata::read_session_metadata,
    pr::{render_pr_block, upsert_marked_block_at_top},
    progress::cli_reporter,
    runner::run_capture,
    session::{SessionPaths, default_output_root, read_latest},
    skill_installer::{
        SkillInstallRequest, install_embedded_skill, list_installed_skills, remove_embedded_skill,
        sepia_skill_install_tip,
    },
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
        sepia pr --repo owner/name --pr 12           # upload the recording and update the PR\n  \
        sepia pr --dry-run                          # preview the PR block without GitHub access\n  \
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
    /// Upload the latest recording and update its GitHub PR demo block.
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
    /// Print the local PR block without accessing GitHub.
    #[arg(long)]
    dry_run: bool,
    /// Explicit PR number. Auto-detects with `gh pr view` when omitted.
    #[arg(long)]
    pr: Option<u64>,
    /// Explicit GitHub repo as owner/name. Inferred by `gh pr view` when omitted.
    #[arg(long)]
    repo: Option<String>,
    /// Output root used to locate latest.json.
    #[arg(long)]
    output_root: Option<PathBuf>,
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
        Command::Pr(args) => pr(args)?,
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

fn pr(args: PrArgs) -> Result<()> {
    let output_root = args.output_root.map_or_else(default_output_root, Ok)?;
    let latest = read_latest(output_root)?;
    let paths = SessionPaths::from_root(latest.latest_session);
    let metadata = read_session_metadata(&paths)?;
    let video = metadata.video.canonicalize().with_context(|| {
        format!(
            "failed to locate recording at {}; complete a capture before publishing",
            metadata.video.display()
        )
    })?;
    let block = render_pr_block(&metadata, &video);
    fs::write(&paths.pr_comment_md, &block)
        .with_context(|| format!("failed to write {}", paths.pr_comment_md.display()))?;

    if args.dry_run {
        print!("{block}");
        return Ok(());
    }

    let pull_request = get_pull_request(args.repo.as_deref(), args.pr)?;
    let body = upsert_marked_block_at_top(&pull_request.body, &block);
    attach_recording(&pull_request.url, &body, &video)?;
    println!("Updated Sepia block on {}", pull_request.url);
    Ok(())
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
