//! `pm` — bulk project management library and CLI.
//!
//! Applies YAML/JSON specs of issues, milestones, and labels to GitHub
//! (or any backend implementing [`IssueTracker`]). Fully idempotent
//! — re-running the same spec is a no-op.
//!
//! # Features
//!
//! - `github` (default): GitHub REST API backend support.

#![warn(missing_docs)]
#![deny(unsafe_code)]

pub mod apply;
pub mod client;
pub mod error;
pub mod issue;
pub mod label;
pub mod milestone;
pub mod spec;
pub mod status;
pub mod traits;

#[cfg(feature = "github")]
pub mod github;

// Re-export the public API.
pub use apply::{ApplyReport, apply};
pub use error::{PmError, Result};
pub use issue::{Issue, IssueRef, IssueState};
pub use label::Label;
pub use milestone::{Milestone, MilestoneRef, MilestoneState};
pub use spec::Spec;
pub use traits::IssueTracker;

// GitHub backend is optional but enabled by default.
#[cfg(feature = "github")]
pub use github::{GitHubTracker, GitHubTrackerBuilder};
