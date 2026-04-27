//! `pm-graph` — project [`pm_core`] issues/milestones/labels onto a
//! [`polystore::GraphStore`].
//!
//! The trait/types in `pm-core` describe **what** an issue is; this crate
//! decides **how** that maps into a graph: nodes for the entities and
//! typed edges for `IN_REPO`, `IN_MILESTONE`, `HAS_LABEL`. Anatta plugs
//! its `Neo4jGraphStore` underneath and the issues become first-class
//! graph entities you can query alongside hypotheses, decisions, code,
//! etc.
//!
//! ## Idempotency
//!
//! IDs are derived from the natural keys (`pm:repo:owner/name`,
//! `pm:milestone:owner/name#title`, …) so re-projecting a spec is an
//! upsert — no duplicates. This matches the `pm-core::IssueTracker`
//! contract: `pm apply` is idempotent against the tracker AND against
//! the graph.
//!
//! ## Example
//!
//! ```no_run
//! use pm_core::{Issue, Label, Milestone};
//! use pm_graph::{PmEdge, PmNode, RepoCoord, project_label, project_milestone, project_issue};
//!
//! async fn populate<G>(graph: &G) -> polystore::Result<()>
//! where
//!     G: polystore::GraphStore<PmNode, PmEdge>,
//! {
//!     let coord = RepoCoord::new("anatta-rs", "anatta");
//!     project_label(graph, &coord, &Label::new("type:bug")).await?;
//!     project_milestone(graph, &coord, &Milestone::new("v0.5")).await?;
//!     project_issue(graph, &coord,
//!         &Issue::new("Fix it").with_milestone("v0.5").with_labels(["type:bug"])
//!     ).await?;
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![deny(unsafe_code)]

mod ids;
mod node;
mod project;

pub use ids::RepoCoord;
pub use node::{PmEdge, PmEdgeKind, PmNode};
pub use project::{project_issue, project_label, project_milestone, project_repo, project_spec};

#[cfg(test)]
mod test_store;
