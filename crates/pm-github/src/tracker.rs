//! GitHub REST tracker implementation.

use async_trait::async_trait;
use pm_core::{
    Issue, IssueRef, IssueState, IssueTracker, Label, Milestone, MilestoneRef, MilestoneState,
    PmError, Result,
};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;

const ENGINE_NAME: &str = "github";
const DEFAULT_API_ROOT: &str = "https://api.github.com";
const DEFAULT_USER_AGENT: &str = "pm-github/0.1 (+https://github.com/anatta-rs/pm)";
const ACCEPT_VND: &str = "application/vnd.github+json";
const PER_PAGE: usize = 100;

/// GitHub Issues backend.
#[derive(Debug, Clone)]
pub struct GitHubTracker {
    client: Client,
    api_root: String,
    user_agent: String,
    owner: String,
    repo: String,
    token: String,
}

impl GitHubTracker {
    /// Start a builder. Required: `repo(owner, repo)` and `token(…)`.
    #[must_use]
    pub fn builder() -> GitHubTrackerBuilder {
        GitHubTrackerBuilder::default()
    }

    fn issues_url(&self) -> String {
        format!(
            "{}/repos/{}/{}/issues",
            self.api_root, self.owner, self.repo
        )
    }
    fn labels_url(&self) -> String {
        format!(
            "{}/repos/{}/{}/labels",
            self.api_root, self.owner, self.repo
        )
    }
    fn milestones_url(&self) -> String {
        format!(
            "{}/repos/{}/{}/milestones",
            self.api_root, self.owner, self.repo
        )
    }

    async fn http(
        &self,
        method: Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response> {
        let mut req = self
            .client
            .request(method, url)
            .header(ACCEPT, ACCEPT_VND)
            .header(USER_AGENT, &self.user_agent)
            .header(AUTHORIZATION, format!("Bearer {}", self.token));
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| PmError::Network(e.to_string()))?;
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(PmError::Auth(format!("GitHub returned HTTP {status}")));
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(PmError::RateLimited {
                retry_after_seconds: resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60),
            });
        }
        Ok(resp)
    }

    async fn http_ok_json<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T> {
        let resp = self.http(method, url, body).await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(PmError::Backend(
                format!("GitHub HTTP {status}: {text}").into(),
            ));
        }
        resp.json::<T>()
            .await
            .map_err(|e| PmError::Parse(e.to_string()))
    }
}

/// Configurable builder for [`GitHubTracker`].
#[derive(Debug, Clone, Default)]
pub struct GitHubTrackerBuilder {
    client: Option<Client>,
    api_root: Option<String>,
    user_agent: Option<String>,
    owner: Option<String>,
    repo: Option<String>,
    token: Option<String>,
}

impl GitHubTrackerBuilder {
    /// Override the underlying `reqwest` client.
    #[must_use]
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Override the API root — primarily for tests against a mock server.
    #[must_use]
    pub fn api_root(mut self, root: impl Into<String>) -> Self {
        self.api_root = Some(root.into());
        self
    }

    /// Override the User-Agent header.
    #[must_use]
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Set the owner + repo (required).
    #[must_use]
    pub fn repo(mut self, owner: impl Into<String>, repo: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self.repo = Some(repo.into());
        self
    }

    /// Set the auth token (required). Bearer-style.
    #[must_use]
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Finalise the tracker.
    ///
    /// # Errors
    ///
    /// Returns [`PmError::Auth`] if `token` is missing, [`PmError::InvalidInput`]
    /// if `repo()` was not called.
    pub fn build(self) -> Result<GitHubTracker> {
        let owner = self.owner.ok_or_else(|| {
            PmError::InvalidInput("owner is required (call .repo(owner, repo))".into())
        })?;
        let repo = self.repo.ok_or_else(|| {
            PmError::InvalidInput("repo is required (call .repo(owner, repo))".into())
        })?;
        let token = self
            .token
            .ok_or_else(|| PmError::Auth("token is required".into()))?;
        Ok(GitHubTracker {
            client: self.client.unwrap_or_default(),
            api_root: self.api_root.unwrap_or_else(|| DEFAULT_API_ROOT.into()),
            user_agent: self.user_agent.unwrap_or_else(|| DEFAULT_USER_AGENT.into()),
            owner,
            repo,
            token,
        })
    }
}

#[derive(Debug, Deserialize)]
struct GhLabel {
    name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

impl From<GhLabel> for Label {
    fn from(g: GhLabel) -> Self {
        Self {
            name: g.name,
            color: g.color,
            description: g.description,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GhMilestone {
    number: u64,
    title: String,
    state: String,
}

impl From<GhMilestone> for MilestoneRef {
    fn from(g: GhMilestone) -> Self {
        Self {
            id: g.number,
            title: g.title,
            state: parse_milestone_state(&g.state),
        }
    }
}

#[derive(Debug, Deserialize)]
struct GhIssue {
    number: u64,
    title: String,
    html_url: String,
    state: String,
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
}

impl From<GhIssue> for IssueRef {
    fn from(g: GhIssue) -> Self {
        Self {
            number: g.number,
            title: g.title,
            url: g.html_url,
            state: parse_issue_state(&g.state),
        }
    }
}

fn parse_issue_state(s: &str) -> IssueState {
    if s.eq_ignore_ascii_case("closed") {
        IssueState::Closed
    } else {
        IssueState::Open
    }
}

fn parse_milestone_state(s: &str) -> MilestoneState {
    if s.eq_ignore_ascii_case("closed") {
        MilestoneState::Closed
    } else {
        MilestoneState::Open
    }
}

fn issue_state_str(s: IssueState) -> &'static str {
    match s {
        IssueState::Open => "open",
        IssueState::Closed => "closed",
    }
}

fn milestone_state_str(s: MilestoneState) -> &'static str {
    match s {
        MilestoneState::Open => "open",
        MilestoneState::Closed => "closed",
    }
}

#[derive(Debug, Serialize)]
struct LabelPayload<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

#[async_trait]
impl IssueTracker for GitHubTracker {
    fn name(&self) -> &'static str {
        ENGINE_NAME
    }

    async fn list_labels(&self) -> Result<Vec<Label>> {
        let url = format!("{}?per_page={PER_PAGE}", self.labels_url());
        let raw: Vec<GhLabel> = self.http_ok_json(Method::GET, &url, None).await?;
        Ok(raw.into_iter().map(Label::from).collect())
    }

    async fn upsert_label(&self, label: &Label) -> Result<Label> {
        let payload = LabelPayload {
            name: &label.name,
            color: label.color.as_deref(),
            description: label.description.as_deref(),
        };
        let create_resp = self
            .http(
                Method::POST,
                &self.labels_url(),
                Some(serde_json::to_value(&payload).map_err(|e| PmError::Parse(e.to_string()))?),
            )
            .await?;
        let status = create_resp.status();
        if status.is_success() {
            return Ok(label.clone());
        }
        if status == StatusCode::UNPROCESSABLE_ENTITY {
            // Label already exists — PATCH it.
            let patch_url = format!("{}/{}", self.labels_url(), urlencoding(&label.name));
            let _: GhLabel = self
                .http_ok_json(
                    Method::PATCH,
                    &patch_url,
                    Some(
                        serde_json::to_value(&payload)
                            .map_err(|e| PmError::Parse(e.to_string()))?,
                    ),
                )
                .await?;
            return Ok(label.clone());
        }
        let text = create_resp.text().await.unwrap_or_default();
        Err(PmError::Backend(
            format!("create label failed: HTTP {status}: {text}").into(),
        ))
    }

    async fn list_milestones(&self) -> Result<Vec<MilestoneRef>> {
        let url = format!("{}?state=all&per_page={PER_PAGE}", self.milestones_url());
        let raw: Vec<GhMilestone> = self.http_ok_json(Method::GET, &url, None).await?;
        Ok(raw.into_iter().map(MilestoneRef::from).collect())
    }

    async fn upsert_milestone(&self, m: &Milestone) -> Result<MilestoneRef> {
        let existing = self.list_milestones().await?;
        let mut payload = json!({
            "title": m.title,
            "state": milestone_state_str(m.state),
        });
        if let Some(d) = &m.description {
            payload["description"] = json!(d);
        }
        if let Some(d) = &m.due_on {
            // GitHub wants ISO-8601 *datetime* — promote bare YYYY-MM-DD to midnight UTC.
            let datetime = if d.len() == 10 {
                format!("{d}T00:00:00Z")
            } else {
                d.clone()
            };
            payload["due_on"] = json!(datetime);
        }
        if let Some(found) = existing.iter().find(|x| x.title == m.title) {
            let url = format!("{}/{}", self.milestones_url(), found.id);
            let g: GhMilestone = self
                .http_ok_json(Method::PATCH, &url, Some(payload))
                .await?;
            return Ok(g.into());
        }
        let g: GhMilestone = self
            .http_ok_json(Method::POST, &self.milestones_url(), Some(payload))
            .await?;
        Ok(g.into())
    }

    async fn list_issues(&self) -> Result<Vec<IssueRef>> {
        let url = format!("{}?state=all&per_page={PER_PAGE}", self.issues_url());
        let raw: Vec<GhIssue> = self.http_ok_json(Method::GET, &url, None).await?;
        Ok(raw
            .into_iter()
            .filter(|g| g.pull_request.is_none())
            .map(IssueRef::from)
            .collect())
    }

    async fn upsert_issue(&self, issue: &Issue) -> Result<IssueRef> {
        if !issue.is_valid() {
            return Err(PmError::InvalidInput("issue title is empty".into()));
        }
        let milestone_number = if let Some(title) = issue.milestone.as_deref() {
            let ms = self.list_milestones().await?;
            ms.iter()
                .find(|x| x.title == title)
                .map(|x| x.id)
                .ok_or_else(|| {
                    PmError::NotFound(format!(
                        "milestone {title:?} (create it before referencing from an issue)"
                    ))
                })?
                .into()
        } else {
            None
        };

        let mut payload = json!({
            "title": issue.title,
            "body": issue.body,
            "state": issue_state_str(issue.state),
            "labels": issue.labels,
            "assignees": issue.assignees,
        });
        if let Some(n) = milestone_number {
            payload["milestone"] = json!(n);
        }

        let existing = self.list_issues().await?;
        if let Some(found) = existing.iter().find(|x| x.title == issue.title) {
            let url = format!("{}/{}", self.issues_url(), found.number);
            let g: GhIssue = self
                .http_ok_json(Method::PATCH, &url, Some(payload))
                .await?;
            return Ok(g.into());
        }
        let g: GhIssue = self
            .http_ok_json(Method::POST, &self.issues_url(), Some(payload))
            .await?;
        Ok(g.into())
    }
}

/// Tiny percent-encoder for label names in URL paths. GitHub labels can
/// contain spaces (`good first issue`), `:`, `/`, etc.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("%{b:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn builder_requires_repo_and_token() {
        let err = GitHubTracker::builder().build().expect_err("missing");
        assert!(matches!(err, PmError::InvalidInput(_)));
        let err = GitHubTracker::builder()
            .repo("o", "r")
            .build()
            .expect_err("missing token");
        assert!(matches!(err, PmError::Auth(_)));
    }

    #[test]
    fn builder_yields_tracker() {
        let t = GitHubTracker::builder()
            .repo("o", "r")
            .token("tok")
            .build()
            .expect("ok");
        assert_eq!(t.name(), "github");
        assert_eq!(t.owner, "o");
        assert_eq!(t.repo, "r");
    }

    #[test]
    fn urlencoding_passes_safe_chars_unchanged() {
        assert_eq!(urlencoding("type-bug"), "type-bug");
        assert_eq!(urlencoding("a.b_c~d"), "a.b_c~d");
    }

    #[test]
    fn urlencoding_escapes_special() {
        assert_eq!(urlencoding("type:bug"), "type%3Abug");
        assert_eq!(urlencoding("good first issue"), "good%20first%20issue");
        assert_eq!(urlencoding("scope/a"), "scope%2Fa");
    }

    #[test]
    fn parse_issue_state_handles_case() {
        assert_eq!(parse_issue_state("open"), IssueState::Open);
        assert_eq!(parse_issue_state("Closed"), IssueState::Closed);
        assert_eq!(parse_issue_state("WUT"), IssueState::Open);
    }

    #[test]
    fn parse_milestone_state_handles_case() {
        assert_eq!(parse_milestone_state("open"), MilestoneState::Open);
        assert_eq!(parse_milestone_state("CLOSED"), MilestoneState::Closed);
    }

    fn make_tracker(server: &mockito::Server) -> GitHubTracker {
        GitHubTracker::builder()
            .api_root(server.url())
            .repo("o", "r")
            .token("tok")
            .build()
            .expect("ok")
    }

    #[tokio::test]
    async fn list_labels_round_trips() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/repos/o/r/labels?per_page=100")
            .match_header("authorization", "Bearer tok")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                {"name":"bug","color":"d73a4a","description":"oops"},
                {"name":"area:graph","color":"0075ca"}
            ]"#,
            )
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let labels = tracker.list_labels().await.expect("ok");
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].name, "bug");
        assert_eq!(labels[1].color.as_deref(), Some("0075ca"));
    }

    #[tokio::test]
    async fn upsert_label_creates_when_absent() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/repos/o/r/labels")
            .with_status(201)
            .with_body(r#"{"name":"bug","color":"d73a4a"}"#)
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let l = tracker
            .upsert_label(&Label::new("bug").with_color("d73a4a"))
            .await
            .expect("ok");
        assert_eq!(l.name, "bug");
    }

    #[tokio::test]
    async fn upsert_label_falls_back_to_patch_on_422() {
        let mut server = mockito::Server::new_async().await;
        let _post = server
            .mock("POST", "/repos/o/r/labels")
            .with_status(422)
            .with_body("already exists")
            .create_async()
            .await;
        let _patch = server
            .mock("PATCH", "/repos/o/r/labels/bug")
            .with_status(200)
            .with_body(r#"{"name":"bug","color":"d73a4a"}"#)
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let l = tracker
            .upsert_label(&Label::new("bug").with_color("d73a4a"))
            .await
            .expect("ok");
        assert_eq!(l.color.as_deref(), Some("d73a4a"));
    }

    #[tokio::test]
    async fn upsert_milestone_creates_when_absent() {
        let mut server = mockito::Server::new_async().await;
        let _list = server
            .mock("GET", "/repos/o/r/milestones?state=all&per_page=100")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;
        let _post = server
            .mock("POST", "/repos/o/r/milestones")
            .with_status(201)
            .with_body(r#"{"number":1,"title":"v0.5","state":"open"}"#)
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let r = tracker
            .upsert_milestone(&Milestone::new("v0.5"))
            .await
            .expect("ok");
        assert_eq!(r.id, 1);
        assert_eq!(r.title, "v0.5");
    }

    #[tokio::test]
    async fn upsert_milestone_patches_when_match_exists() {
        let mut server = mockito::Server::new_async().await;
        let _list = server
            .mock("GET", "/repos/o/r/milestones?state=all&per_page=100")
            .with_status(200)
            .with_body(r#"[{"number":7,"title":"v0.5","state":"open"}]"#)
            .create_async()
            .await;
        let _patch = server
            .mock("PATCH", "/repos/o/r/milestones/7")
            .with_status(200)
            .with_body(r#"{"number":7,"title":"v0.5","state":"closed"}"#)
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let r = tracker
            .upsert_milestone(&Milestone::new("v0.5").with_state(MilestoneState::Closed))
            .await
            .expect("ok");
        assert_eq!(r.id, 7);
        assert_eq!(r.state, MilestoneState::Closed);
    }

    #[tokio::test]
    async fn list_issues_filters_pull_requests() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/repos/o/r/issues?state=all&per_page=100")
            .with_status(200)
            .with_body(r#"[
                {"number":1,"title":"real","html_url":"https://x.test/1","state":"open"},
                {"number":2,"title":"a PR","html_url":"https://x.test/2","state":"open","pull_request":{"url":"x"}}
            ]"#)
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let issues = tracker.list_issues().await.expect("ok");
        assert_eq!(issues.len(), 1, "PRs filtered out");
        assert_eq!(issues[0].number, 1);
    }

    #[tokio::test]
    async fn upsert_issue_creates_when_absent() {
        let mut server = mockito::Server::new_async().await;
        let _list = server
            .mock("GET", "/repos/o/r/issues?state=all&per_page=100")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;
        let _post = server
            .mock("POST", "/repos/o/r/issues")
            .with_status(201)
            .with_body(r#"{"number":3,"title":"X","html_url":"https://x.test/3","state":"open"}"#)
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let r = tracker.upsert_issue(&Issue::new("X")).await.expect("ok");
        assert_eq!(r.number, 3);
    }

    #[tokio::test]
    async fn upsert_issue_resolves_milestone_title_to_number() {
        let mut server = mockito::Server::new_async().await;
        let _ms = server
            .mock("GET", "/repos/o/r/milestones?state=all&per_page=100")
            .with_status(200)
            .with_body(r#"[{"number":5,"title":"v0.5","state":"open"}]"#)
            .create_async()
            .await;
        let _list = server
            .mock("GET", "/repos/o/r/issues?state=all&per_page=100")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;
        let post = server
            .mock("POST", "/repos/o/r/issues")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"milestone":5}"#.into(),
            ))
            .with_status(201)
            .with_body(r#"{"number":1,"title":"X","html_url":"https://x.test/1","state":"open"}"#)
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let r = tracker
            .upsert_issue(&Issue::new("X").with_milestone("v0.5"))
            .await
            .expect("ok");
        post.assert_async().await;
        assert_eq!(r.number, 1);
    }

    #[tokio::test]
    async fn upsert_issue_errors_on_missing_milestone() {
        let mut server = mockito::Server::new_async().await;
        let _ms = server
            .mock("GET", "/repos/o/r/milestones?state=all&per_page=100")
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let err = tracker
            .upsert_issue(&Issue::new("X").with_milestone("v0.5"))
            .await
            .expect_err("must err");
        assert!(matches!(err, PmError::NotFound(_)));
    }

    #[tokio::test]
    async fn upsert_issue_rejects_empty_title() {
        let server = mockito::Server::new_async().await;
        let tracker = make_tracker(&server);
        let err = tracker
            .upsert_issue(&Issue::new(""))
            .await
            .expect_err("err");
        assert!(matches!(err, PmError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn http_maps_401_to_auth() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", mockito::Matcher::Regex(".*".into()))
            .with_status(401)
            .with_body("nope")
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let err = tracker.list_labels().await.expect_err("err");
        assert!(matches!(err, PmError::Auth(_)));
    }

    #[tokio::test]
    async fn http_maps_429_to_rate_limited_with_retry_after() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", mockito::Matcher::Regex(".*".into()))
            .with_status(429)
            .with_header("retry-after", "120")
            .with_body("rate limit")
            .create_async()
            .await;
        let tracker = make_tracker(&server);
        let err = tracker.list_labels().await.expect_err("err");
        match err {
            PmError::RateLimited {
                retry_after_seconds,
            } => {
                assert_eq!(retry_after_seconds, 120);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }
}
