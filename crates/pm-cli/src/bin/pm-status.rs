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
use pm_cli::status::{MilestoneSummary, PrSummary, RepoSummary};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::process::Command;

/// GitHub REST client for fetching repo data.
struct GitHubClient {
    client: Client,
    token: String,
    api_root: String,
}

impl GitHubClient {
    /// Create a new client with a token.
    fn new(token: String) -> Self {
        Self {
            client: Client::new(),
            token,
            api_root: "https://api.github.com".to_string(),
        }
    }

    /// Get the bearer token header value.
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// Fetch JSON from a GitHub endpoint.
    async fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{path}", self.api_root);
        let resp = self
            .client
            .get(&url)
            .header(ACCEPT, "application/vnd.github+json")
            .header(USER_AGENT, "pm-status/0.1")
            .header(AUTHORIZATION, self.auth_header())
            .send()
            .await
            .context("failed to fetch from GitHub")?;

        match resp.status() {
            s if s.is_success() => resp.json().await.context("failed to parse JSON response"),
            StatusCode::UNAUTHORIZED => {
                Err(anyhow!("GitHub auth failed: invalid or expired token"))
            }
            s => Err(anyhow!("GitHub returned {s}: {}", resp.text().await?)),
        }
    }

    /// Fetch paginated results (up to `per_page` per request).
    async fn get_paginated(&self, path: &str, per_page: u32) -> Result<Vec<Value>> {
        let mut results = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!("{path}?page={page}&per_page={per_page}");
            let items = self.get_json(&url).await?;

            if let Value::Array(arr) = items {
                if arr.is_empty() {
                    break;
                }
                results.extend(arr);
                page += 1;
            } else {
                break;
            }
        }

        Ok(results)
    }

    /// Fetch open PRs for a repo.
    async fn fetch_open_prs(&self, owner: &str, repo: &str) -> Result<Vec<PrSummary>> {
        let path = format!("/repos/{owner}/{repo}/pulls?state=open");
        let items = self.get_paginated(&path, 50).await?;

        let prs = items
            .iter()
            .filter_map(|item| {
                let number = item.get("number")?.as_u64()?;
                let title = item.get("title")?.as_str()?.to_string();
                let merge_state = item
                    .get("mergeable_state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                Some(PrSummary {
                    number,
                    title,
                    merge_state,
                })
            })
            .collect();

        Ok(prs)
    }

    /// Fetch open issues count (excluding PRs).
    async fn fetch_open_issues_count(&self, owner: &str, repo: &str) -> Result<u32> {
        let path = format!("/repos/{owner}/{repo}/issues?state=open&filter=issues");
        let _items = self.get_paginated(&path, 1).await?;

        // Simple approach: fetch one page and count.
        let url = format!("{}{path}&per_page=1", self.api_root);
        let resp = self
            .client
            .get(url)
            .header(ACCEPT, "application/vnd.github+json")
            .header(USER_AGENT, "pm-status/0.1")
            .header(AUTHORIZATION, self.auth_header())
            .send()
            .await?;

        let link_header = resp
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // Parse the Link header to find the last page; format: <...?page=N&...>; rel="last"
        if let Some(last_link) = link_header.split(',').find(|s| s.contains("rel=\"last\"")) {
            if let Some(start) = last_link.find("page=") {
                if let Some(end) = last_link[start + 5..].find('&') {
                    if let Ok(last_page) = last_link[start + 5..start + 5 + end].parse::<u32>() {
                        return Ok(last_page);
                    }
                } else if let Some(end) = last_link[start + 5..].find('>') {
                    if let Ok(last_page) = last_link[start + 5..start + 5 + end].parse::<u32>() {
                        return Ok(last_page);
                    }
                }
            }
        }

        // Fallback: assume 1 page = 1 issue
        Ok(1)
    }

    /// Fetch milestones.
    async fn fetch_milestones(&self, owner: &str, repo: &str) -> Result<Vec<MilestoneSummary>> {
        let path = format!("/repos/{owner}/{repo}/milestones");
        let items = self.get_paginated(&path, 50).await?;

        let milestones = items
            .iter()
            .filter_map(|item| {
                let title = item.get("title")?.as_str()?.to_string();
                let due_on = item
                    .get("due_on")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let open = item.get("open_issues")?.as_u64()? as u32;
                let closed = item.get("closed_issues")?.as_u64()? as u32;
                let description = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);

                Some(MilestoneSummary {
                    title,
                    due_on,
                    open,
                    closed,
                    description,
                })
            })
            .collect();

        Ok(milestones)
    }
}

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
            let path = format!("/orgs/{item}/repos?type=sources&per_page=50");
            match client.get_json(&path).await {
                Ok(Value::Array(items)) => {
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
                _ => {
                    // Try as a user instead of org
                    let user_path = format!("/users/{item}/repos?type=owner&per_page=50");
                    if let Ok(Value::Array(items)) = client.get_json(&user_path).await {
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
