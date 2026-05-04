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

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::GET;
    use httpmock::MockServer;

    fn client_at(server: &MockServer) -> GitHubClient {
        GitHubClient::new_with_root("test-token".into(), server.base_url())
    }

    // ── constructors / auth_header ──────────────────────────────────────────

    #[test]
    fn new_uses_default_api_root() {
        let c = GitHubClient::new("t".into());
        assert_eq!(c.api_root, "https://api.github.com");
        assert_eq!(c.token, "t");
    }

    #[test]
    fn new_with_root_uses_custom_url() {
        let c = GitHubClient::new_with_root("t".into(), "http://x".into());
        assert_eq!(c.api_root, "http://x");
    }

    #[test]
    fn auth_header_formats_bearer() {
        let c = GitHubClient::new("abc123".into());
        assert_eq!(c.auth_header(), "Bearer abc123");
    }

    // ── get_json ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_json_returns_parsed_value_on_200() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/foo")
                    .header("authorization", "Bearer test-token");
                then.status(200).json_body(serde_json::json!({"k": "v"}));
            })
            .await;
        let c = client_at(&server);

        let v = c.get_json("/foo").await.unwrap();
        assert_eq!(v["k"], "v");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn get_json_maps_401_to_auth_error() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET).path("/x");
                then.status(401);
            })
            .await;
        let c = client_at(&server);

        let err = c.get_json("/x").await.unwrap_err().to_string();
        assert!(err.contains("auth failed"), "got: {err}");
    }

    #[tokio::test]
    async fn get_json_maps_5xx_with_body() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET).path("/y");
                then.status(503).body("upstream down");
            })
            .await;
        let c = client_at(&server);

        let err = c.get_json("/y").await.unwrap_err().to_string();
        assert!(err.contains("503"), "got: {err}");
        assert!(err.contains("upstream down"), "got: {err}");
    }

    // ── get_paginated ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_paginated_aggregates_until_empty_page() {
        let server = MockServer::start_async().await;
        let _p1 = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/list")
                    .query_param("page", "1")
                    .query_param("per_page", "2");
                then.status(200)
                    .json_body(serde_json::json!([{"n": 1}, {"n": 2}]));
            })
            .await;
        let _p2 = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/list")
                    .query_param("page", "2")
                    .query_param("per_page", "2");
                then.status(200).json_body(serde_json::json!([{"n": 3}]));
            })
            .await;
        let _p3 = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/list")
                    .query_param("page", "3")
                    .query_param("per_page", "2");
                then.status(200).json_body(serde_json::json!([]));
            })
            .await;
        let c = client_at(&server);

        let items = c.get_paginated("/list", &[], 2).await.unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["n"], 1);
        assert_eq!(items[2]["n"], 3);
    }

    #[tokio::test]
    async fn get_paginated_breaks_when_response_is_not_array() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET).path("/scalar");
                then.status(200)
                    .json_body(serde_json::json!({"not": "list"}));
            })
            .await;
        let c = client_at(&server);

        let items = c.get_paginated("/scalar", &[], 50).await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn get_paginated_appends_pagination_to_existing_query_string() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/q")
                    .query_param("filter", "issues")
                    .query_param("state", "open")
                    .query_param("page", "1");
                then.status(200).json_body(serde_json::json!([]));
            })
            .await;
        let c = client_at(&server);

        let items = c
            .get_paginated("/q?filter=issues", &[("state", "open")], 50)
            .await
            .unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn get_paginated_maps_401() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET).path("/p");
                then.status(401);
            })
            .await;
        let c = client_at(&server);

        let err = c
            .get_paginated("/p", &[], 50)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("auth failed"), "got: {err}");
    }

    #[tokio::test]
    async fn get_paginated_maps_5xx() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET).path("/p");
                then.status(502).body("bad gateway");
            })
            .await;
        let c = client_at(&server);

        let err = c
            .get_paginated("/p", &[], 50)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("502"));
        assert!(err.contains("bad gateway"));
    }

    // ── fetch_open_prs ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn fetch_open_prs_maps_response_shape() {
        let server = MockServer::start_async().await;
        let _p1 = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/repos/o/r/pulls")
                    .query_param("state", "open")
                    .query_param("page", "1");
                then.status(200).json_body(serde_json::json!([
                    {"number": 1, "title": "first", "mergeable_state": "clean"},
                    {"number": 2, "title": "second"} // mergeable_state absent → "unknown"
                ]));
            })
            .await;
        let _p2 = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/repos/o/r/pulls")
                    .query_param("page", "2");
                then.status(200).json_body(serde_json::json!([]));
            })
            .await;
        let c = client_at(&server);

        let prs = c.fetch_open_prs("o", "r").await.unwrap();
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].number, 1);
        assert_eq!(prs[0].title, "first");
        assert_eq!(prs[0].merge_state, "clean");
        assert_eq!(prs[1].merge_state, "unknown");
    }

    // ── fetch_open_issues_count ─────────────────────────────────────────────
    //
    // The implementation does TWO requests: the paginated one (which we
    // satisfy with a single empty page) plus the per_page=1 sniff that reads
    // the Link header. Each test mocks both calls.

    fn empty_paginated_mock(server: &MockServer, path: &str) {
        server.mock(|when, then| {
            when.method(GET)
                .path(path)
                .query_param("page", "1")
                .query_param("per_page", "1");
            then.status(200).json_body(serde_json::json!([]));
        });
    }

    #[tokio::test]
    async fn fetch_open_issues_count_parses_last_from_link_header() {
        let server = MockServer::start_async().await;
        empty_paginated_mock(&server, "/repos/o/r/issues");
        // The sniff request: per_page=1 + Link header pointing to last=42.
        let _sniff = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/repos/o/r/issues")
                    .query_param("state", "open")
                    .query_param("filter", "issues")
                    .query_param("per_page", "1");
                then.status(200).json_body(serde_json::json!([])).header(
                    "link",
                    "<http://x/issues?page=2&per_page=1>; rel=\"next\", \
                     <http://x/issues?page=42&per_page=1>; rel=\"last\"",
                );
            })
            .await;
        let c = client_at(&server);

        assert_eq!(c.fetch_open_issues_count("o", "r").await.unwrap(), 42);
    }

    #[tokio::test]
    async fn fetch_open_issues_count_handles_link_with_only_close_bracket() {
        // Link last token shaped without `&` — the second branch in the
        // parser (find('>') instead of find('&')).
        let server = MockServer::start_async().await;
        empty_paginated_mock(&server, "/repos/o/r/issues");
        let _sniff = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/repos/o/r/issues")
                    .query_param("per_page", "1");
                then.status(200)
                    .json_body(serde_json::json!([]))
                    .header("link", "<http://x?page=7>; rel=\"last\"");
            })
            .await;
        let c = client_at(&server);

        assert_eq!(c.fetch_open_issues_count("o", "r").await.unwrap(), 7);
    }

    #[tokio::test]
    async fn fetch_open_issues_count_falls_back_to_one_when_no_link() {
        let server = MockServer::start_async().await;
        empty_paginated_mock(&server, "/repos/o/r/issues");
        let _sniff = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/repos/o/r/issues")
                    .query_param("per_page", "1");
                then.status(200).json_body(serde_json::json!([]));
            })
            .await;
        let c = client_at(&server);

        assert_eq!(c.fetch_open_issues_count("o", "r").await.unwrap(), 1);
    }

    // ── fetch_milestones ────────────────────────────────────────────────────

    #[tokio::test]
    async fn fetch_milestones_maps_response_shape() {
        let server = MockServer::start_async().await;
        let _p1 = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/repos/o/r/milestones")
                    .query_param("page", "1");
                then.status(200).json_body(serde_json::json!([
                    {
                        "title": "v0.1",
                        "due_on": "2026-06-01T00:00:00Z",
                        "open_issues": 3,
                        "closed_issues": 7,
                        "description": "first cut"
                    },
                    {
                        "title": "v0.2",
                        "open_issues": 0,
                        "closed_issues": 0
                        // due_on + description absent
                    }
                ]));
            })
            .await;
        let _p2 = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/repos/o/r/milestones")
                    .query_param("page", "2");
                then.status(200).json_body(serde_json::json!([]));
            })
            .await;
        let c = client_at(&server);

        let ms = c.fetch_milestones("o", "r").await.unwrap();
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].title, "v0.1");
        assert_eq!(ms[0].due_on.as_deref(), Some("2026-06-01T00:00:00Z"));
        assert_eq!(ms[0].open, 3);
        assert_eq!(ms[0].closed, 7);
        assert_eq!(ms[0].description.as_deref(), Some("first cut"));
        assert_eq!(ms[1].due_on, None);
        assert_eq!(ms[1].description, None);
    }
}
