//! End-to-end integration test for pm-graph against a real Neo4j instance.
//!
//! Spins up neo4j:5-community via testcontainers, builds a minimal Neo4j-backed
//! `GraphStore<PmNode, PmEdge>`, and asserts that `project_*` functions produce
//! the expected nodes/edges/upserts.
//!
//! Run with: `cargo test -p pm-graph --features e2e-neo4j --tests`

#![cfg(feature = "e2e-neo4j")]

use neo4rs::{Graph, Query};
use pm_core::{Issue, IssueState, Label, Milestone, MilestoneState};
use pm_graph::{
    PmEdge, PmEdgeKind, PmNode, RepoCoord, project_issue, project_label, project_milestone,
    project_repo, project_spec,
};
use polystore::{Direction, EntityId, GraphStore, Result as PolystoreResult, Scope};
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;

/// A minimal Neo4j-backed `GraphStore<PmNode, PmEdge>` for testing.
/// Wraps `neo4rs::Graph` and implements the polystore trait.
struct Neo4jStore {
    graph: Arc<Graph>,
    scope: Scope,
}

impl Neo4jStore {
    fn new(graph: Arc<Graph>) -> Self {
        Self {
            graph,
            scope: Scope::new("neo4j-e2e-test", "_", "_"),
        }
    }
}

/// Convert neo4rs error to polystore error.
fn neo4j_to_poly(e: neo4rs::Error) -> polystore::PolystoreError {
    polystore::PolystoreError::Backend(Box::new(e))
}

/// Wrapper for row extraction errors.
#[derive(Debug)]
struct RowError(String);

impl std::fmt::Display for RowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Neo4j row extraction: {}", self.0)
    }
}

impl std::error::Error for RowError {}

/// Convert row extraction error to polystore error.
#[allow(clippy::needless_pass_by_value)]
fn row_error_to_poly(e: neo4rs::DeError) -> polystore::PolystoreError {
    polystore::PolystoreError::Backend(Box::new(RowError(e.to_string())))
}

#[async_trait::async_trait]
impl GraphStore<PmNode, PmEdge> for Neo4jStore {
    fn scope(&self) -> &Scope {
        &self.scope
    }

    async fn upsert_node(&self, id: &EntityId, node: PmNode) -> PolystoreResult<()> {
        let node_json = serde_json::to_string(&node)?;
        let node_kind = match &node {
            PmNode::Repo { .. } => "repo",
            PmNode::Milestone { .. } => "milestone",
            PmNode::Issue { .. } => "issue",
            PmNode::Label { .. } => "label",
        };

        let query = Query::new(
            "MERGE (n:PmNode { id: $id, kind: $kind }) SET n.data = $data RETURN n".to_string(),
        )
        .param("id", id.as_str())
        .param("kind", node_kind)
        .param("data", node_json);

        self.graph.run(query).await.map_err(neo4j_to_poly)?;
        Ok(())
    }

    async fn get_node(&self, id: &EntityId) -> PolystoreResult<Option<PmNode>> {
        let query = Query::new("MATCH (n:PmNode { id: $id }) RETURN n.data AS data".to_string())
            .param("id", id.as_str());

        let mut result = self.graph.execute(query).await.map_err(neo4j_to_poly)?;

        match result.next().await.map_err(neo4j_to_poly)? {
            Some(row) => {
                let data_str: String = row.get("data").map_err(row_error_to_poly)?;
                let node = serde_json::from_str(&data_str)?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    async fn delete_node(&self, id: &EntityId) -> PolystoreResult<()> {
        let query = Query::new("MATCH (n:PmNode { id: $id }) DETACH DELETE n".to_string())
            .param("id", id.as_str());

        self.graph.run(query).await.map_err(neo4j_to_poly)?;
        Ok(())
    }

    async fn add_edge(&self, from: &EntityId, to: &EntityId, edge: PmEdge) -> PolystoreResult<()> {
        let edge_kind = match edge.kind {
            PmEdgeKind::InRepo => "IN_REPO",
            PmEdgeKind::InMilestone => "IN_MILESTONE",
            PmEdgeKind::HasLabel => "HAS_LABEL",
            PmEdgeKind::AssignedTo => "ASSIGNED_TO",
        };
        let edge_json = serde_json::to_string(&edge)?;

        let query = Query::new(
            "MATCH (a:PmNode { id: $from_id }), (b:PmNode { id: $to_id }) \
             MERGE (a)-[r:PmEdge { kind: $kind }]->(b) \
             SET r.data = $data"
                .to_string(),
        )
        .param("from_id", from.as_str())
        .param("to_id", to.as_str())
        .param("kind", edge_kind)
        .param("data", edge_json);

        self.graph.run(query).await.map_err(neo4j_to_poly)?;
        Ok(())
    }

    async fn neighbors(
        &self,
        id: &EntityId,
        direction: Direction,
    ) -> PolystoreResult<Vec<(EntityId, PmEdge)>> {
        match direction {
            Direction::Outgoing => {
                let query = Query::new(
                    "MATCH (a:PmNode { id: $id })-[r:PmEdge]->(b:PmNode) \
                     RETURN b.id AS neighbor_id, r.data AS edge_data"
                        .to_string(),
                )
                .param("id", id.as_str());

                let mut result = self.graph.execute(query).await.map_err(neo4j_to_poly)?;
                let mut neighbors = Vec::new();

                while let Some(row) = result.next().await.map_err(neo4j_to_poly)? {
                    let neighbor_id: String = row.get("neighbor_id").map_err(row_error_to_poly)?;
                    let edge_json: String = row.get("edge_data").map_err(row_error_to_poly)?;
                    let edge: PmEdge = serde_json::from_str(&edge_json)?;
                    neighbors.push((EntityId::new(neighbor_id), edge));
                }

                Ok(neighbors)
            }
            Direction::Incoming => {
                let query = Query::new(
                    "MATCH (b:PmNode)-[r:PmEdge]->(a:PmNode { id: $id }) \
                     RETURN b.id AS neighbor_id, r.data AS edge_data"
                        .to_string(),
                )
                .param("id", id.as_str());

                let mut result = self.graph.execute(query).await.map_err(neo4j_to_poly)?;
                let mut neighbors = Vec::new();

                while let Some(row) = result.next().await.map_err(neo4j_to_poly)? {
                    let neighbor_id: String = row.get("neighbor_id").map_err(row_error_to_poly)?;
                    let edge_json: String = row.get("edge_data").map_err(row_error_to_poly)?;
                    let edge: PmEdge = serde_json::from_str(&edge_json)?;
                    neighbors.push((EntityId::new(neighbor_id), edge));
                }

                Ok(neighbors)
            }
            Direction::Both => {
                let out = self.neighbors(id, Direction::Outgoing).await?;
                let inc = self.neighbors(id, Direction::Incoming).await?;
                let mut combined = out;
                combined.extend(inc);
                Ok(combined)
            }
        }
    }

    async fn list_by_kind(&self, kind: &str) -> PolystoreResult<Vec<EntityId>> {
        let query = Query::new("MATCH (n:PmNode { kind: $kind }) RETURN n.id AS id".to_string())
            .param("kind", kind);

        let mut result = self.graph.execute(query).await.map_err(neo4j_to_poly)?;
        let mut ids = Vec::new();

        while let Some(row) = result.next().await.map_err(neo4j_to_poly)? {
            let id: String = row.get("id").map_err(row_error_to_poly)?;
            ids.push(EntityId::new(id));
        }

        Ok(ids)
    }

    async fn search_by_name(
        &self,
        _query: &str,
        _top_k: usize,
    ) -> PolystoreResult<Vec<(EntityId, PmNode)>> {
        // Not implemented for e2e test — can add if needed.
        Ok(vec![])
    }

    async fn reverse_path(
        &self,
        _from: &EntityId,
        _hops: u8,
    ) -> PolystoreResult<Vec<Vec<EntityId>>> {
        // Not implemented for e2e test — can add if needed.
        Ok(vec![])
    }
}

/// Helper to get the total node count from Neo4j.
async fn count_nodes(graph: &Graph) -> Result<i64, String> {
    let query = Query::new("MATCH (n:PmNode) RETURN count(n) AS count".to_string());
    let mut result = graph
        .execute(query)
        .await
        .map_err(|e| format!("Query execution failed: {e}"))?;
    if let Some(row) = result
        .next()
        .await
        .map_err(|e| format!("Result iteration failed: {e}"))?
    {
        row.get("count")
            .map_err(|e| format!("Row extraction failed: {e}"))
    } else {
        Ok(0)
    }
}

/// Helper to count edges by kind.
async fn count_edges_by_kind(graph: &Graph, kind: &str) -> Result<i64, String> {
    let query =
        Query::new("MATCH ()-[r:PmEdge { kind: $kind }]->() RETURN count(r) AS count".to_string())
            .param("kind", kind);
    let mut result = graph
        .execute(query)
        .await
        .map_err(|e| format!("Query execution failed: {e}"))?;
    if let Some(row) = result
        .next()
        .await
        .map_err(|e| format!("Result iteration failed: {e}"))?
    {
        row.get("count")
            .map_err(|e| format!("Row extraction failed: {e}"))
    } else {
        Ok(0)
    }
}

#[tokio::test]
#[ignore = "requires Docker / Neo4j container — run with `cargo test -p pm-graph --features e2e-neo4j -- --ignored`"]
async fn e2e_project_repo_upsert() {
    let container = testcontainers_modules::neo4j::Neo4j::default()
        .start()
        .await
        .expect("start container");
    let port = container.get_host_port_ipv4(7687).await.expect("get port");
    let uri = format!("bolt://127.0.0.1:{port}");

    let graph = Arc::new(
        Graph::new(&uri, "neo4j", "password")
            .await
            .expect("connect"),
    );
    let store = Neo4jStore::new(graph.clone());
    let coord = RepoCoord::new("anatta-rs", "anatta");

    // First projection.
    project_repo(&store, &coord).await.expect("project_repo");
    let count1 = count_nodes(graph.as_ref()).await.expect("count_nodes");
    assert_eq!(count1, 1, "first projection creates 1 repo node");

    // Second projection (upsert).
    project_repo(&store, &coord).await.expect("project_repo");
    let count2 = count_nodes(graph.as_ref()).await.expect("count_nodes");
    assert_eq!(count2, 1, "second projection is upsert (still 1 node)");
}

#[tokio::test]
#[ignore = "requires Docker / Neo4j container — run with `cargo test -p pm-graph --features e2e-neo4j -- --ignored`"]
async fn e2e_project_milestone_with_repo() {
    let container = testcontainers_modules::neo4j::Neo4j::default()
        .start()
        .await
        .expect("start container");
    let port = container.get_host_port_ipv4(7687).await.expect("get port");
    let uri = format!("bolt://127.0.0.1:{port}");

    let graph = Arc::new(
        Graph::new(&uri, "neo4j", "password")
            .await
            .expect("connect"),
    );
    let store = Neo4jStore::new(graph.clone());
    let coord = RepoCoord::new("anatta-rs", "anatta");
    let milestone = Milestone::new("v0.5");

    project_milestone(&store, &coord, &milestone)
        .await
        .expect("project_milestone");

    let count = count_nodes(graph.as_ref()).await.expect("count_nodes");
    assert_eq!(count, 2, "repo + milestone = 2 nodes");

    let in_repo_count = count_edges_by_kind(graph.as_ref(), "IN_REPO")
        .await
        .expect("count edges");
    assert_eq!(in_repo_count, 1, "milestone->repo IN_REPO edge exists");
}

#[tokio::test]
#[ignore = "requires Docker / Neo4j container — run with `cargo test -p pm-graph --features e2e-neo4j -- --ignored`"]
async fn e2e_project_issue_with_labels_and_milestone() {
    let container = testcontainers_modules::neo4j::Neo4j::default()
        .start()
        .await
        .expect("start container");
    let port = container.get_host_port_ipv4(7687).await.expect("get port");
    let uri = format!("bolt://127.0.0.1:{port}");

    let graph = Arc::new(
        Graph::new(&uri, "neo4j", "password")
            .await
            .expect("connect"),
    );
    let store = Neo4jStore::new(graph.clone());
    let coord = RepoCoord::new("anatta-rs", "anatta");

    // Pre-project labels + milestone.
    let label1 = Label::new("type:bug");
    let label2 = Label::new("area:graph");
    let milestone = Milestone::new("v0.5");

    project_label(&store, &coord, &label1)
        .await
        .expect("project_label");
    project_label(&store, &coord, &label2)
        .await
        .expect("project_label");
    project_milestone(&store, &coord, &milestone)
        .await
        .expect("project_milestone");

    // Project issue with both labels and milestone.
    let issue = Issue::new("Fix critical bug")
        .with_milestone("v0.5")
        .with_labels(["type:bug", "area:graph"])
        .with_state(IssueState::Open);

    project_issue(&store, &coord, &issue)
        .await
        .expect("project_issue");

    let node_count = count_nodes(graph.as_ref()).await.expect("count_nodes");
    // 1 repo + 2 labels + 1 milestone + 1 issue = 5 nodes
    assert_eq!(node_count, 5, "expected node count");

    let in_repo_count = count_edges_by_kind(graph.as_ref(), "IN_REPO")
        .await
        .expect("count IN_REPO");
    assert_eq!(
        in_repo_count, 4,
        "4 IN_REPO edges: labels(2) + milestone(1) + issue(1)"
    );

    let in_milestone_count = count_edges_by_kind(graph.as_ref(), "IN_MILESTONE")
        .await
        .expect("count IN_MILESTONE");
    assert_eq!(
        in_milestone_count, 1,
        "1 IN_MILESTONE edge: issue->milestone"
    );

    let has_label_count = count_edges_by_kind(graph.as_ref(), "HAS_LABEL")
        .await
        .expect("count HAS_LABEL");
    assert_eq!(
        has_label_count, 2,
        "2 HAS_LABEL edges: issue->label1, issue->label2"
    );
}

#[tokio::test]
#[ignore = "requires Docker / Neo4j container — run with `cargo test -p pm-graph --features e2e-neo4j -- --ignored`"]
async fn e2e_project_spec_full() {
    let container = testcontainers_modules::neo4j::Neo4j::default()
        .start()
        .await
        .expect("start container");
    let port = container.get_host_port_ipv4(7687).await.expect("get port");
    let uri = format!("bolt://127.0.0.1:{port}");

    let graph = Arc::new(
        Graph::new(&uri, "neo4j", "password")
            .await
            .expect("connect"),
    );
    let store = Neo4jStore::new(graph.clone());
    let coord = RepoCoord::new("anatta-rs", "anatta");

    // From the README example.
    let labels = vec![
        Label::new("type:bug").with_color("d73a4a"),
        Label::new("area:graph").with_color("0075ca"),
    ];
    let milestones = vec![
        Milestone::new("v0.5 — Multi-tenant")
            .with_description("GitHub-orgs style namespace model")
            .with_due_on("2026-06-01")
            .with_state(MilestoneState::Open),
    ];
    let issues = vec![
        Issue::new("I7: fix /api/v1/health 401")
            .with_body("Hook blocks before handler — health probe gets 401.\nAdd `/api/v1/health` to BOOTSTRAP_WRITE_PATHS.")
            .with_milestone("v0.5 — Multi-tenant")
            .with_labels(["type:bug"])
            .with_state(IssueState::Open),
    ];

    project_spec(&store, &coord, &labels, &milestones, &issues)
        .await
        .expect("project_spec");

    let node_count = count_nodes(graph.as_ref()).await.expect("count_nodes");
    // 1 repo + 2 labels + 1 milestone + 1 issue = 5 nodes
    assert_eq!(
        node_count, 5,
        "repo(1) + labels(2) + milestones(1) + issues(1) = 5"
    );

    let total_edges = count_edges_by_kind(graph.as_ref(), "IN_REPO")
        .await
        .expect("count IN_REPO")
        + count_edges_by_kind(graph.as_ref(), "IN_MILESTONE")
            .await
            .expect("count IN_MILESTONE")
        + count_edges_by_kind(graph.as_ref(), "HAS_LABEL")
            .await
            .expect("count HAS_LABEL");

    // IN_REPO: label1->repo, label2->repo, milestone->repo, issue->repo = 4
    // IN_MILESTONE: issue->milestone = 1
    // HAS_LABEL: issue->label1 = 1
    // Total = 6
    assert_eq!(total_edges, 6, "expected total edge count");
}
