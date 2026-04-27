//! `pm-core` — abstract project-management interface.
//!
//! Defines the [`IssueTracker`] trait that backends ([`pm-github`](https://crates.io/crates/pm-github),
//! …) implement, plus the data types ([`Issue`], [`Milestone`], [`Label`])
//! that flow across it. Trackers are upsert-by-natural-key — re-running the
//! same plan against the same backend is a no-op.
//!
//! ## Example
//!
//! ```no_run
//! use pm_core::{IssueTracker, Issue, IssueState};
//!
//! async fn ship<T: IssueTracker>(tracker: &T) -> pm_core::Result<()> {
//!     let issue = Issue::new("Fix the auth middleware")
//!         .with_body("It returns 401 on /health.")
//!         .with_labels(["type:bug"])
//!         .with_milestone("v0.5");
//!     let upserted = tracker.upsert_issue(&issue).await?;
//!     assert_eq!(upserted.state, IssueState::Open);
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![deny(unsafe_code)]

pub mod error;
pub mod issue;
pub mod label;
pub mod milestone;
pub mod traits;

pub use error::{PmError, Result};
pub use issue::{Issue, IssueRef, IssueState};
pub use label::Label;
pub use milestone::{Milestone, MilestoneRef, MilestoneState};
pub use traits::IssueTracker;
