//! `pm-status` — cross-repo project status snapshot as Markdown on stdout.
//!
//! ```text
//! pm-status                              # defaults to "anatta-rs,Lsh0x"
//! pm-status anatta-rs                    # all non-archived repos under anatta-rs
//! pm-status anatta-rs/pm                 # single repo
//! pm-status "anatta-rs/pm,Lsh0x/rs-stats" # multiple repos
//! ```

#![warn(missing_docs)]
#![deny(unsafe_code)]
#![allow(
    clippy::collapsible_if,
    clippy::single_match_else,
    clippy::cast_possible_truncation
)]

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use clap::Parser;
use pm_cli::client::GitHubClient;
use pm_cli::status::RepoSummary;
use std::process::Command;

/// Command-line arguments.
#[derive(Debug, Parser)]
#[command(
    name = "pm-status",
    version,
    about = "Cross-repo project status snapshot"
)]
struct Args {
    /// Comma-separated list of GitHub owners or owner/repo pairs.
    /// Defaults to "anatta-rs,Lsh0x".
    #[arg(default_value = "anatta-rs,Lsh0x")]
    scope: String,
}

/// Resolve a scope string to a list of (owner, repo) pairs.
/// - `owner/repo` → that repo
/// - `owner` → list all non-archived repos in that org
async fn resolve_scope(client: &GitHubClient, scope: &str) -> Result<Vec<(String, String)>> {
    let mut repos = Vec::new();

    for item in scope.split(',') {
        let item = item.trim();
        if item.contains('/') {
            let (owner, repo) = item.split_once('/').unwrap();
            repos.push((owner.to_string(), repo.to_string()));
        } else {
            // Fetch all repos for this owner/org
            let path = format!("/orgs/{item}/repos");
            match client
                .get_paginated(&path, &[("type", "sources")], 50)
                .await
            {
                Ok(items) => {
                    for repo_item in items {
                        if let (Some(name), Some(archived)) = (
                            repo_item.get("name").and_then(|v| v.as_str()),
                            repo_item
                                .get("archived")
                                .and_then(serde_json::Value::as_bool),
                        ) {
                            if !archived {
                                repos.push((item.to_string(), name.to_string()));
                            }
                        }
                    }
                }
                Err(_) => {
                    // Try as a user instead of org
                    let user_path = format!("/users/{item}/repos");
                    if let Ok(items) = client
                        .get_paginated(&user_path, &[("type", "owner")], 50)
                        .await
                    {
                        for repo_item in items {
                            if let (Some(name), Some(archived)) = (
                                repo_item.get("name").and_then(|v| v.as_str()),
                                repo_item
                                    .get("archived")
                                    .and_then(serde_json::Value::as_bool),
                            ) {
                                if !archived {
                                    repos.push((item.to_string(), name.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(repos)
}

/// Fetch a token from env or `gh auth token`.
fn get_token() -> Result<String> {
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        return Ok(token);
    }

    let output = Command::new("gh")
        .arg("auth")
        .arg("token")
        .output()
        .context("failed to run `gh auth token`")?;

    if !output.status.success() {
        return Err(anyhow!(
            "neither GITHUB_TOKEN env var nor `gh auth token` worked"
        ));
    }

    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_string())
        .context("failed to decode gh auth token output")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let token = get_token()?;
    let client = GitHubClient::new(token);

    let repos = resolve_scope(&client, &args.scope).await?;

    if repos.is_empty() {
        eprintln!("warning: no repos found for scope '{}'", args.scope);
    }

    // Fetch summaries in parallel using tokio::spawn tasks
    let mut handles = Vec::new();
    for (owner, repo) in repos {
        let client_clone = client.client.clone();
        let token = client.token.clone();
        let api_root = client.api_root.clone();
        handles.push(tokio::spawn(async move {
            let client = GitHubClient {
                client: client_clone,
                token,
                api_root,
            };
            match fetch_repo_summary(&client, &owner, &repo).await {
                Ok(summary) => Some(summary),
                Err(e) => {
                    eprintln!("warning: failed to fetch {owner}/{repo}: {e}");
                    None
                }
            }
        }));
    }

    let summaries: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .filter_map(Result::ok)
        .flatten()
        .collect();

    let now_utc = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let md = pm_cli::status::render_markdown(&summaries, &args.scope, &now_utc);
    println!("{md}");

    Ok(())
}

/// Fetch a complete summary for a single repo.
async fn fetch_repo_summary(client: &GitHubClient, owner: &str, repo: &str) -> Result<RepoSummary> {
    let (prs, issues, milestones) = tokio::join!(
        client.fetch_open_prs(owner, repo),
        client.fetch_open_issues_count(owner, repo),
        client.fetch_milestones(owner, repo),
    );

    Ok(RepoSummary {
        owner: owner.to_string(),
        repo: repo.to_string(),
        open_prs: prs?,
        open_issues: issues?,
        milestones: milestones?,
    })
}
