//! Projection helpers — write Spec entities into a [`polystore::GraphStore`].
//!
//! Each function is upsert + edge-write. We don't track edge dedup at
//! this layer (the `polystore::GraphStore` trait makes no claim about
//! duplicate-edge handling) — backends that care should normalise on
//! read or `(from, to, kind)` write.

use crate::ids::RepoCoord;
use crate::node::{PmEdge, PmEdgeKind, PmNode};
use pm_core::{Issue, Label, Milestone};
use polystore::{GraphStore, Result};

/// Write the repo root node. Idempotent.
pub async fn project_repo<G>(graph: &G, coord: &RepoCoord) -> Result<()>
where
    G: GraphStore<PmNode, PmEdge> + ?Sized,
{
    let node = PmNode::Repo {
        owner: coord.owner.clone(),
        name: coord.repo.clone(),
    };
    graph.upsert_node(&coord.repo_id(), node).await
}

/// Write the label node + a `Repo → Label` edge for grouping. Re-running
/// against the same repo+label is upsert + edge-add (the trait doesn't
/// dedup edges; backends that care should — Anatta's Neo4j MERGE does).
pub async fn project_label<G>(graph: &G, coord: &RepoCoord, label: &Label) -> Result<()>
where
    G: GraphStore<PmNode, PmEdge> + ?Sized,
{
    project_repo(graph, coord).await?;
    let id = coord.label_id(&label.name);
    graph
        .upsert_node(
            &id,
            PmNode::Label {
                name: label.name.clone(),
                color: label.color.clone(),
                description: label.description.clone(),
            },
        )
        .await?;
    graph
        .add_edge(&id, &coord.repo_id(), PmEdge::new(PmEdgeKind::InRepo))
        .await
}

/// Write the milestone node + a `Milestone → Repo` edge.
pub async fn project_milestone<G>(graph: &G, coord: &RepoCoord, m: &Milestone) -> Result<()>
where
    G: GraphStore<PmNode, PmEdge> + ?Sized,
{
    project_repo(graph, coord).await?;
    let id = coord.milestone_id(&m.title);
    graph
        .upsert_node(
            &id,
            PmNode::Milestone {
                title: m.title.clone(),
                description: m.description.clone(),
                due_on: m.due_on.clone(),
                state: m.state,
            },
        )
        .await?;
    graph
        .add_edge(&id, &coord.repo_id(), PmEdge::new(PmEdgeKind::InRepo))
        .await
}

/// Write the issue node + edges to its repo, milestone (if any), and
/// each label (which the caller is expected to have projected first).
pub async fn project_issue<G>(graph: &G, coord: &RepoCoord, issue: &Issue) -> Result<()>
where
    G: GraphStore<PmNode, PmEdge> + ?Sized,
{
    project_repo(graph, coord).await?;
    let id = coord.issue_id(&issue.title);
    graph
        .upsert_node(
            &id,
            PmNode::Issue {
                title: issue.title.clone(),
                body: issue.body.clone(),
                assignees: issue.assignees.clone(),
                state: issue.state,
            },
        )
        .await?;
    graph
        .add_edge(&id, &coord.repo_id(), PmEdge::new(PmEdgeKind::InRepo))
        .await?;
    if let Some(milestone_title) = &issue.milestone {
        graph
            .add_edge(
                &id,
                &coord.milestone_id(milestone_title),
                PmEdge::new(PmEdgeKind::InMilestone),
            )
            .await?;
    }
    for label_name in &issue.labels {
        graph
            .add_edge(
                &id,
                &coord.label_id(label_name),
                PmEdge::new(PmEdgeKind::HasLabel),
            )
            .await?;
    }
    Ok(())
}

/// Project a full plan in one go: labels → milestones → issues.
///
/// Errors short-circuit (we don't try to recover) so the caller sees the
/// first failure with full context.
pub async fn project_spec<G>(
    graph: &G,
    coord: &RepoCoord,
    labels: &[Label],
    milestones: &[Milestone],
    issues: &[Issue],
) -> Result<()>
where
    G: GraphStore<PmNode, PmEdge> + ?Sized,
{
    project_repo(graph, coord).await?;
    for l in labels {
        project_label(graph, coord, l).await?;
    }
    for m in milestones {
        project_milestone(graph, coord, m).await?;
    }
    for i in issues {
        project_issue(graph, coord, i).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_store::InMemoryGraph;
    use pm_core::{IssueState, MilestoneState};
    use polystore::Direction;
    use pretty_assertions::assert_eq;

    fn coord() -> RepoCoord {
        RepoCoord::new("anatta-rs", "anatta")
    }

    #[tokio::test]
    async fn project_repo_writes_root_node() {
        let g = InMemoryGraph::new("ns");
        project_repo(&g, &coord()).await.expect("ok");
        let node = g.get_node(&coord().repo_id()).await.expect("ok");
        assert!(matches!(node, Some(PmNode::Repo { .. })));
    }

    #[tokio::test]
    async fn project_label_creates_node_and_in_repo_edge() {
        let g = InMemoryGraph::new("ns");
        project_label(&g, &coord(), &Label::new("type:bug").with_color("d73a4a"))
            .await
            .expect("ok");

        let id = coord().label_id("type:bug");
        let node = g.get_node(&id).await.expect("ok").expect("present");
        assert!(matches!(node, PmNode::Label { color: Some(c), .. } if c == "d73a4a"));

        let nbrs = g.neighbors(&id, Direction::Outgoing).await.expect("ok");
        assert!(
            nbrs.iter().any(|(_, e)| e.kind == PmEdgeKind::InRepo),
            "label edges out: {nbrs:?}"
        );
    }

    #[tokio::test]
    async fn project_milestone_creates_node_and_in_repo_edge() {
        let g = InMemoryGraph::new("ns");
        let m = Milestone::new("v0.5")
            .with_description("multi-tenant")
            .with_due_on("2026-06-01")
            .with_state(MilestoneState::Open);
        project_milestone(&g, &coord(), &m).await.expect("ok");

        let id = coord().milestone_id("v0.5");
        let node = g.get_node(&id).await.expect("ok").expect("present");
        match node {
            PmNode::Milestone { title, due_on, .. } => {
                assert_eq!(title, "v0.5");
                assert_eq!(due_on.as_deref(), Some("2026-06-01"));
            }
            other => panic!("expected milestone, got {other:?}"),
        }

        let nbrs = g.neighbors(&id, Direction::Outgoing).await.expect("ok");
        assert_eq!(nbrs.len(), 1);
        assert_eq!(nbrs[0].1.kind, PmEdgeKind::InRepo);
    }

    #[tokio::test]
    async fn project_issue_creates_node_and_all_three_edge_kinds() {
        let g = InMemoryGraph::new("ns");
        // Pre-project label + milestone so the issue edges land on real nodes.
        project_label(&g, &coord(), &Label::new("type:bug"))
            .await
            .expect("ok");
        project_milestone(&g, &coord(), &Milestone::new("v0.5"))
            .await
            .expect("ok");

        let issue = Issue::new("Fix it")
            .with_body("body")
            .with_milestone("v0.5")
            .with_labels(["type:bug"])
            .with_state(IssueState::Open);
        project_issue(&g, &coord(), &issue).await.expect("ok");

        let id = coord().issue_id("Fix it");
        let node = g.get_node(&id).await.expect("ok").expect("present");
        assert!(matches!(node, PmNode::Issue { .. }));

        let mut edge_kinds: Vec<_> = g
            .neighbors(&id, Direction::Outgoing)
            .await
            .expect("ok")
            .into_iter()
            .map(|(_, e)| e.kind)
            .collect();
        edge_kinds.sort_by_key(|k| format!("{k:?}"));
        assert_eq!(
            edge_kinds,
            vec![
                PmEdgeKind::HasLabel,
                PmEdgeKind::InMilestone,
                PmEdgeKind::InRepo
            ],
        );
    }

    #[tokio::test]
    async fn project_issue_without_milestone_omits_in_milestone_edge() {
        let g = InMemoryGraph::new("ns");
        let issue = Issue::new("X");
        project_issue(&g, &coord(), &issue).await.expect("ok");

        let id = coord().issue_id("X");
        let nbrs = g.neighbors(&id, Direction::Outgoing).await.expect("ok");
        assert!(
            nbrs.iter().all(|(_, e)| e.kind != PmEdgeKind::InMilestone),
            "no milestone edge: {nbrs:?}"
        );
    }

    #[tokio::test]
    async fn project_spec_creates_everything_in_order() {
        let g = InMemoryGraph::new("ns");
        let labels = vec![Label::new("a"), Label::new("b")];
        let milestones = vec![Milestone::new("v1"), Milestone::new("v2")];
        let issues = vec![
            Issue::new("X").with_milestone("v1").with_labels(["a"]),
            Issue::new("Y").with_milestone("v2").with_labels(["b"]),
        ];
        project_spec(&g, &coord(), &labels, &milestones, &issues)
            .await
            .expect("ok");

        // 1 repo + 2 labels + 2 milestones + 2 issues = 7 nodes
        assert_eq!(g.node_count(), 7);
    }

    #[tokio::test]
    async fn project_repo_is_idempotent() {
        let g = InMemoryGraph::new("ns");
        project_repo(&g, &coord()).await.expect("ok");
        project_repo(&g, &coord()).await.expect("ok");
        // upsert overwrites — only one repo node despite two calls.
        assert_eq!(g.node_count(), 1);
    }

    #[tokio::test]
    async fn project_milestone_is_idempotent_on_node_count() {
        let g = InMemoryGraph::new("ns");
        let m = Milestone::new("v0.5");
        project_milestone(&g, &coord(), &m).await.expect("ok");
        project_milestone(&g, &coord(), &m).await.expect("ok");
        // 1 repo + 1 milestone = 2 nodes regardless of how many times we project.
        assert_eq!(g.node_count(), 2);
    }
}
