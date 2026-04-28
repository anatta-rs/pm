//! `pm` — apply YAML/JSON specs of issues + milestones to a tracker.
//!
//! ```text
//! pm apply spec.yaml                 # uses GITHUB_TOKEN
//! pm apply spec.yaml --token $GH_PAT
//! pm list issues --repo owner/name
//! pm list milestones --repo owner/name
//! ```

#![warn(missing_docs)]
#![deny(unsafe_code)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use pm::GitHubTracker;
use pm::IssueTracker;
use pm::Spec;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "pm",
    version,
    about = "Apply YAML specs of issues + milestones to a tracker"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,

    /// Override the GitHub API root (used for tests / GitHub Enterprise).
    #[arg(long, env = "PM_GITHUB_API_ROOT", global = true)]
    api_root: Option<String>,

    /// Auth token. Falls back to `$GITHUB_TOKEN`.
    #[arg(long, env = "GITHUB_TOKEN", global = true, hide_env_values = true)]
    token: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Read a spec file and upsert every label/milestone/issue.
    Apply {
        /// Path to a YAML or JSON spec file.
        path: PathBuf,
    },
    /// List existing entities (labels, milestones, issues) on a repo.
    List {
        /// What to list.
        kind: ListKind,
        /// `owner/name`.
        #[arg(long)]
        repo: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ListKind {
    /// All labels defined on the repo.
    Labels,
    /// Open + closed milestones.
    Milestones,
    /// Open + closed issues (PRs filtered out).
    Issues,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Apply { path } => cmd_apply(path, cli.token, cli.api_root).await,
        Cmd::List { kind, repo } => cmd_list(kind, &repo, cli.token, cli.api_root).await,
    }
}

async fn cmd_apply(path: PathBuf, token: Option<String>, api_root: Option<String>) -> Result<()> {
    let token = token.context("set $GITHUB_TOKEN or pass --token")?;
    let spec = Spec::from_path(&path).with_context(|| format!("load {}", path.display()))?;
    let (owner, repo) = spec
        .split_repo()
        .with_context(|| format!("invalid repo {:?}", spec.repo))?;
    let mut builder = GitHubTracker::builder().repo(owner, repo).token(token);
    if let Some(root) = api_root {
        builder = builder.api_root(root);
    }
    let tracker = builder.build().context("build GitHubTracker")?;

    tracing::info!(repo = %spec.repo, labels = spec.labels.len(), milestones = spec.milestones.len(), issues = spec.issues.len(), "applying spec");
    let report = pm::apply(&spec, &tracker).await.context("apply failed")?;
    println!(
        "✓ applied {repo}: {l} label(s), {m} milestone(s), {i} issue(s)",
        repo = spec.repo,
        l = report.labels,
        m = report.milestones,
        i = report.issues,
    );
    Ok(())
}

async fn cmd_list(
    kind: ListKind,
    repo: &str,
    token: Option<String>,
    api_root: Option<String>,
) -> Result<()> {
    let token = token.context("set $GITHUB_TOKEN or pass --token")?;
    let (owner, name) = repo
        .split_once('/')
        .with_context(|| format!("invalid --repo {repo:?} (expected owner/name)"))?;
    let mut builder = GitHubTracker::builder().repo(owner, name).token(token);
    if let Some(root) = api_root {
        builder = builder.api_root(root);
    }
    let tracker = builder.build().context("build GitHubTracker")?;

    match kind {
        ListKind::Labels => {
            for l in tracker.list_labels().await.context("list labels")? {
                println!("{}\t{}", l.name, l.color.as_deref().unwrap_or(""));
            }
        }
        ListKind::Milestones => {
            for m in tracker.list_milestones().await.context("list milestones")? {
                println!("{}\t#{}\t{:?}", m.title, m.id, m.state);
            }
        }
        ListKind::Issues => {
            for i in tracker.list_issues().await.context("list issues")? {
                println!("#{}\t{:?}\t{}\t{}", i.number, i.state, i.title, i.url);
            }
        }
    }
    Ok(())
}
