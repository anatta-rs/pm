//! Node + edge payloads that pm-graph stores on the underlying graph.

use pm_core::{IssueState, MilestoneState};
use serde::{Deserialize, Serialize};

/// One node in the projection. The `kind` field is the discriminator; the
/// other fields are populated according to it. Backends that key on
/// `kind` (e.g. Anatta's Neo4j shape) can index directly by the value.
///
/// Storing this as a tagged enum keeps the node-payload-generic
/// `polystore::GraphStore<N, _>` happy — only one `N` type per store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PmNode {
    /// `(owner, repo)` root.
    Repo {
        /// e.g. `anatta-rs`.
        owner: String,
        /// e.g. `anatta`.
        name: String,
    },
    /// Release / planning bucket.
    Milestone {
        /// Display title — also the natural key.
        title: String,
        /// Optional description.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// `YYYY-MM-DD` due date.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        due_on: Option<String>,
        /// Open or closed.
        state: MilestoneState,
    },
    /// One issue.
    Issue {
        /// Title — also the natural key.
        title: String,
        /// Markdown body (may be empty).
        #[serde(default)]
        body: String,
        /// Assignee logins, in spec order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        assignees: Vec<String>,
        /// Open or closed.
        state: IssueState,
    },
    /// Taxonomy marker.
    Label {
        /// Label name.
        name: String,
        /// Optional hex colour.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        /// Optional description.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

/// Edge payload — just a kind for now (no attributes), but we wrap it in
/// a struct so we can grow it (`created_at`, `causal source`) without a
/// breaking change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PmEdge {
    /// What this edge means.
    pub kind: PmEdgeKind,
}

impl PmEdge {
    /// Construct an edge with just a kind.
    #[must_use]
    pub fn new(kind: PmEdgeKind) -> Self {
        Self { kind }
    }
}

/// The relations pm-graph emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PmEdgeKind {
    /// `Milestone → Repo` and `Issue → Repo` — every PM entity belongs to a repo.
    InRepo,
    /// `Issue → Milestone` — issue scheduled into a milestone.
    InMilestone,
    /// `Issue → Label` — multi-cardinality: one issue, many labels.
    HasLabel,
    /// `Issue → User` — assignee. Reserved; the assignee user-node is not
    /// emitted in v0.1 (we'd need a User type — see ROADMAP).
    AssignedTo,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn node_serde_round_trips() {
        let n = PmNode::Issue {
            title: "X".into(),
            body: "b".into(),
            assignees: vec!["me".into()],
            state: IssueState::Open,
        };
        let json = serde_json::to_string(&n).expect("ok");
        let back: PmNode = serde_json::from_str(&json).expect("ok");
        assert_eq!(n, back);
    }

    #[test]
    fn node_serde_tags_kind() {
        let json = serde_json::to_string(&PmNode::Repo {
            owner: "o".into(),
            name: "r".into(),
        })
        .expect("ok");
        assert!(json.contains(r#""kind":"repo""#), "kind tagged: {json}");
    }

    #[test]
    fn edge_kind_serializes_snake_case() {
        let e = PmEdge::new(PmEdgeKind::InMilestone);
        let json = serde_json::to_string(&e).expect("ok");
        assert!(json.contains("in_milestone"), "{json}");
    }

    #[test]
    fn edge_constructs_with_kind() {
        assert_eq!(PmEdge::new(PmEdgeKind::HasLabel).kind, PmEdgeKind::HasLabel);
    }

    #[test]
    fn milestone_node_omits_unset_optionals_in_serde() {
        let n = PmNode::Milestone {
            title: "v0.5".into(),
            description: None,
            due_on: None,
            state: MilestoneState::Open,
        };
        let json = serde_json::to_string(&n).expect("ok");
        assert!(!json.contains("description"), "absent: {json}");
        assert!(!json.contains("due_on"), "absent: {json}");
    }

    #[test]
    fn issue_node_omits_empty_assignees_in_serde() {
        let n = PmNode::Issue {
            title: "X".into(),
            body: String::new(),
            assignees: vec![],
            state: IssueState::Open,
        };
        let json = serde_json::to_string(&n).expect("ok");
        assert!(!json.contains("assignees"), "absent: {json}");
    }
}
