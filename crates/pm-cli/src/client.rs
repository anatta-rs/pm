//! GitHub REST API client for fetching repo data.

use anyhow::{Context, Result, anyhow};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, StatusCode};
use serde_json::Value;

/// GitHub REST client for fetching repo data.
pub struct GitHubClient {
    /// HTTP client for requests.
    pub client: Client,
    /// GitHub API token.
    pub token: String,
    /// GitHub API root URL.
    pub api_root: String,
}

impl GitHubClient {
    /// Create a new client with a token.
    pub fn new(token: String) -> Self {
        Self {
            client: Client::new(),
            token,
            api_root: "https://api.github.com".to_string(),
        }
    }

    /// Create a new client with a custom API root (for testing).
    pub fn new_with_root(token: String, api_root: String) -> Self {
        Self {
            client: Client::new(),
            token,
            api_root,
        }
    }

    /// Get the bearer token header value.
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// Fetch JSON from a GitHub endpoint.
    pub async fn get_json(&self, path: &str) -> Result<Value> {
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

    /// Fetch paginated results with query parameters.
    ///
    /// Takes a base path and a slice of query parameters.
    /// Automatically adds `page` and `per_page` parameters.
    pub async fn get_paginated(
        &self,
        path: &str,
        params: &[(&str, &str)],
        per_page: u32,
    ) -> Result<Vec<Value>> {
        let mut results = Vec::new();
        let mut page = 1u32;

        loop {
            // Build URL with base path and all query params + pagination
            let mut url = format!("{}{path}", self.api_root);

            // Check if path already has a query string
            let mut first_param = !path.contains('?');

            // Add user-provided query params
            for (key, value) in params {
                url.push(if first_param { '?' } else { '&' });
                url.push_str(key);
                url.push('=');
                url.push_str(value);
                first_param = false;
            }

            // Add pagination params
            url.push(if first_param { '?' } else { '&' });
            url.push_str("page=");
            url.push_str(&page.to_string());
            url.push('&');
            url.push_str("per_page=");
            url.push_str(&per_page.to_string());

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
                s if s.is_success() => {
                    let items = resp
                        .json::<Value>()
                        .await
                        .context("failed to parse JSON response")?;

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
                StatusCode::UNAUTHORIZED => {
                    return Err(anyhow!("GitHub auth failed: invalid or expired token"));
                }
                s => {
                    return Err(anyhow!("GitHub returned {s}: {}", resp.text().await?));
                }
            }
        }

        Ok(results)
    }

    /// Fetch open PRs for a repo.
    pub async fn fetch_open_prs(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<super::status::PrSummary>> {
        let path = format!("/repos/{owner}/{repo}/pulls");
        let items = self.get_paginated(&path, &[("state", "open")], 50).await?;

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

                Some(super::status::PrSummary {
                    number,
                    title,
                    merge_state,
                })
            })
            .collect();

        Ok(prs)
    }

    /// Fetch open issues count (excluding PRs).
    #[allow(clippy::collapsible_if)]
    pub async fn fetch_open_issues_count(&self, owner: &str, repo: &str) -> Result<u32> {
        let path = format!("/repos/{owner}/{repo}/issues");
        let params = [("state", "open"), ("filter", "issues")];
        let _items = self.get_paginated(&path, &params, 1).await?;

        // Simple approach: fetch one page and count from Link header.
        let url = format!(
            "{}{path}?state=open&filter=issues&per_page=1",
            self.api_root
        );
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
    #[allow(clippy::cast_possible_truncation)]
    pub async fn fetch_milestones(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<super::status::MilestoneSummary>> {
        let path = format!("/repos/{owner}/{repo}/milestones");
        let items = self.get_paginated(&path, &[], 50).await?;

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

                Some(super::status::MilestoneSummary {
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
