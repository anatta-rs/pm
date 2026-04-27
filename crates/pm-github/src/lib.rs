//! `pm-github` — `IssueTracker` backend for GitHub Issues.
//!
//! Hits `api.github.com` (or any compatible endpoint set via the builder)
//! using the [GitHub REST API v3](https://docs.github.com/rest). Each
//! upsert is two calls — one `GET …?per_page=…` to find the existing
//! match by natural key, then `POST` (create) or `PATCH` (update). That
//! makes the whole tracker idempotent without any local cache.
//!
//! ## Auth
//!
//! Pass either a fine-grained PAT or a classic token via
//! [`GitHubTrackerBuilder::token`]. The token needs `issues: write` on
//! the target repo (and `metadata: read` for reads).
//!
//! ## Example
//!
//! ```no_run
//! use pm_core::{Issue, IssueTracker};
//! use pm_github::GitHubTracker;
//!
//! # async fn run() -> pm_core::Result<()> {
//! let tracker = GitHubTracker::builder()
//!     .repo("anatta-rs", "anatta")
//!     .token(std::env::var("GITHUB_TOKEN").expect("set"))
//!     .build()
//!     .expect("ok");
//! let issue = Issue::new("Fix the auth middleware").with_labels(["type:bug"]);
//! let r = tracker.upsert_issue(&issue).await?;
//! println!("→ #{} {}", r.number, r.url);
//! # Ok(()) }
//! ```

#![warn(missing_docs)]
#![deny(unsafe_code)]

mod tracker;

pub use tracker::{GitHubTracker, GitHubTrackerBuilder};
