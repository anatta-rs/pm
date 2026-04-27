//! Tiny in-memory `polystore::GraphStore<PmNode, PmEdge>` for tests.

use crate::node::{PmEdge, PmNode};
use async_trait::async_trait;
use polystore::{Direction, EntityId, GraphStore, Result, Scope};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct InMemoryGraph {
    scope: Scope,
    nodes: Mutex<HashMap<EntityId, PmNode>>,
    out_edges: Mutex<HashMap<EntityId, Vec<(EntityId, PmEdge)>>>,
    in_edges: Mutex<HashMap<EntityId, Vec<(EntityId, PmEdge)>>>,
}

impl InMemoryGraph {
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            scope: Scope::new(namespace, "_", "_"),
            nodes: Mutex::new(HashMap::new()),
            out_edges: Mutex::new(HashMap::new()),
            in_edges: Mutex::new(HashMap::new()),
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.lock().expect("lock").len()
    }
}

#[async_trait]
impl GraphStore<PmNode, PmEdge> for InMemoryGraph {
    fn scope(&self) -> &Scope {
        &self.scope
    }

    async fn upsert_node(&self, id: &EntityId, node: PmNode) -> Result<()> {
        self.nodes.lock().expect("lock").insert(id.clone(), node);
        Ok(())
    }

    async fn get_node(&self, id: &EntityId) -> Result<Option<PmNode>> {
        Ok(self.nodes.lock().expect("lock").get(id).cloned())
    }

    async fn delete_node(&self, id: &EntityId) -> Result<()> {
        self.nodes.lock().expect("lock").remove(id);
        Ok(())
    }

    async fn add_edge(&self, from: &EntityId, to: &EntityId, edge: PmEdge) -> Result<()> {
        self.out_edges
            .lock()
            .expect("lock")
            .entry(from.clone())
            .or_default()
            .push((to.clone(), edge.clone()));
        self.in_edges
            .lock()
            .expect("lock")
            .entry(to.clone())
            .or_default()
            .push((from.clone(), edge));
        Ok(())
    }

    async fn neighbors(
        &self,
        id: &EntityId,
        direction: Direction,
    ) -> Result<Vec<(EntityId, PmEdge)>> {
        let out = self
            .out_edges
            .lock()
            .expect("lock")
            .get(id)
            .cloned()
            .unwrap_or_default();
        let inc = self
            .in_edges
            .lock()
            .expect("lock")
            .get(id)
            .cloned()
            .unwrap_or_default();
        Ok(match direction {
            Direction::Outgoing => out,
            Direction::Incoming => inc,
            Direction::Both => {
                let mut all = out;
                all.extend(inc);
                all
            }
        })
    }

    async fn list_by_kind(&self, kind: &str) -> Result<Vec<EntityId>> {
        Ok(self
            .nodes
            .lock()
            .expect("lock")
            .iter()
            .filter(|(_, n)| node_kind(n) == kind)
            .map(|(id, _)| id.clone())
            .collect())
    }

    async fn search_by_name(&self, query: &str, top_k: usize) -> Result<Vec<(EntityId, PmNode)>> {
        let matches: Vec<_> = self
            .nodes
            .lock()
            .expect("lock")
            .iter()
            .filter(|(_, n)| node_name(n).contains(query))
            .take(top_k)
            .map(|(id, n)| (id.clone(), n.clone()))
            .collect();
        Ok(matches)
    }

    async fn reverse_path(&self, _from: &EntityId, _hops: u8) -> Result<Vec<Vec<EntityId>>> {
        Ok(vec![])
    }
}

fn node_kind(n: &PmNode) -> &'static str {
    match n {
        PmNode::Repo { .. } => "repo",
        PmNode::Milestone { .. } => "milestone",
        PmNode::Issue { .. } => "issue",
        PmNode::Label { .. } => "label",
    }
}

fn node_name(n: &PmNode) -> &str {
    match n {
        PmNode::Milestone { title, .. } | PmNode::Issue { title, .. } => title,
        PmNode::Repo { name, .. } | PmNode::Label { name, .. } => name,
    }
}
