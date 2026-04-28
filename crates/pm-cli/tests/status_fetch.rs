//! Integration tests for pm-cli GitHub client fetching.

use httpmock::prelude::*;
use pm_cli::client::GitHubClient;
use pm_cli::status::{PrSummary, RepoSummary};

#[tokio::test]
async fn test_get_paginated_with_existing_query_string() {
    // Start a mock server
    let server = MockServer::start();

    // Mock the first page response with ?state=open already in the path
    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/o/r/pulls")
            .query_param("state", "open")
            .query_param("page", "1")
            .query_param("per_page", "50");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([
                {
                    "number": 42,
                    "title": "Test PR",
                    "mergeable_state": "MERGEABLE"
                }
            ]));
    });

    // Mock the second page (empty, terminates pagination)
    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/o/r/pulls")
            .query_param("state", "open")
            .query_param("page", "2")
            .query_param("per_page", "50");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([]));
    });

    let client = GitHubClient::new_with_root("test-token".to_string(), server.base_url());

    // Test: calling get_paginated with a path and query params
    let results = client
        .get_paginated("/repos/o/r/pulls", &[("state", "open")], 50)
        .await
        .expect("get_paginated failed");

    // Verify we got the expected result
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["number"].as_u64(), Some(42));
    assert_eq!(results[0]["title"].as_str(), Some("Test PR"));
}

#[tokio::test]
async fn test_get_paginated_multiple_params() {
    let server = MockServer::start();

    // Mock with multiple query params
    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/o/r/issues")
            .query_param("state", "open")
            .query_param("filter", "issues")
            .query_param("page", "1")
            .query_param("per_page", "10");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([{"number": 1}]));
    });

    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/o/r/issues")
            .query_param("state", "open")
            .query_param("filter", "issues")
            .query_param("page", "2")
            .query_param("per_page", "10");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([]));
    });

    let client = GitHubClient::new_with_root("test-token".to_string(), server.base_url());

    let results = client
        .get_paginated(
            "/repos/o/r/issues",
            &[("state", "open"), ("filter", "issues")],
            10,
        )
        .await
        .expect("get_paginated failed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["number"].as_u64(), Some(1));
}

#[tokio::test]
async fn test_get_paginated_no_params() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/o/r/milestones")
            .query_param("page", "1")
            .query_param("per_page", "50");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([{"title": "v1.0"}]));
    });

    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/o/r/milestones")
            .query_param("page", "2")
            .query_param("per_page", "50");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([]));
    });

    let client = GitHubClient::new_with_root("test-token".to_string(), server.base_url());

    let results = client
        .get_paginated("/repos/o/r/milestones", &[], 50)
        .await
        .expect("get_paginated failed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["title"].as_str(), Some("v1.0"));
}

#[tokio::test]
async fn test_fetch_open_prs_end_to_end() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/anatta-rs/pm/pulls")
            .query_param("state", "open")
            .query_param("page", "1")
            .query_param("per_page", "50");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([
                {
                    "number": 1,
                    "title": "Add status binary",
                    "mergeable_state": "MERGEABLE"
                },
                {
                    "number": 2,
                    "title": "Fix pagination bug",
                    "mergeable_state": "CONFLICTING"
                }
            ]));
    });

    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/anatta-rs/pm/pulls")
            .query_param("state", "open")
            .query_param("page", "2")
            .query_param("per_page", "50");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([]));
    });

    let client = GitHubClient::new_with_root("test-token".to_string(), server.base_url());

    let prs = client
        .fetch_open_prs("anatta-rs", "pm")
        .await
        .expect("fetch_open_prs failed");

    assert_eq!(prs.len(), 2);
    assert_eq!(prs[0].number, 1);
    assert_eq!(prs[0].title, "Add status binary");
    assert_eq!(prs[0].merge_state, "MERGEABLE");
    assert_eq!(prs[1].number, 2);
    assert_eq!(prs[1].merge_state, "CONFLICTING");
}

#[tokio::test]
async fn test_fetch_milestones_end_to_end() {
    let server = MockServer::start();

    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/anatta-rs/pm/milestones")
            .query_param("page", "1")
            .query_param("per_page", "50");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([
                {
                    "title": "v0.1",
                    "due_on": "2026-05-01T00:00:00Z",
                    "open_issues": 2,
                    "closed_issues": 3,
                    "description": "Initial release"
                }
            ]));
    });

    server.mock(|when, then| {
        when.method(httpmock::Method::GET)
            .path("/repos/anatta-rs/pm/milestones")
            .query_param("page", "2")
            .query_param("per_page", "50");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(serde_json::json!([]));
    });

    let client = GitHubClient::new_with_root("test-token".to_string(), server.base_url());

    let milestones = client
        .fetch_milestones("anatta-rs", "pm")
        .await
        .expect("fetch_milestones failed");

    assert_eq!(milestones.len(), 1);
    assert_eq!(milestones[0].title, "v0.1");
    assert_eq!(milestones[0].open, 2);
    assert_eq!(milestones[0].closed, 3);
    assert_eq!(
        milestones[0].description,
        Some("Initial release".to_string())
    );
}

/// Smoke test that verifies fetch_repo_summary rendering works with mock data.
#[tokio::test]
async fn test_fetch_repo_summary_rendering() {
    let summary = RepoSummary {
        owner: "test".to_string(),
        repo: "repo".to_string(),
        open_prs: vec![PrSummary {
            number: 1,
            title: "Test PR".to_string(),
            merge_state: "MERGEABLE".to_string(),
        }],
        open_issues: 5,
        milestones: vec![],
    };

    // Verify rendering works
    let md = pm_cli::status::render_markdown(&[summary], "test", "2026-04-28T12:00Z");
    assert!(md.contains("# pm-status — 2026-04-28T12:00Z"));
    assert!(md.contains("| test/repo |"));
    assert!(md.contains("| 1 |")); // 1 PR
    assert!(md.contains("| 5 |")); // 5 issues
}
