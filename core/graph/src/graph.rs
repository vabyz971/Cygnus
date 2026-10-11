// Cygnus — Suite créative professionnelle open source
// Copyright (C) 2026 vabyz971
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Dependency graph: structure, propagation and topological order.
//!
//! [`Graph`] owns the nodes and both adjacencies (upstream `deps`,
//! downstream `dependents`): every mutation goes through it, with no public
//! `find_mut` — revisions stay consistent by construction.
//!
//! Mutation contract: `add_edge` returns `Result<Revision,
//! GraphError>` (`Ok` = tree revision after the call, fresh iff
//! the structure changed); removals and `clear_dirty` return
//! `bool`/`Option<bool>`; `mark_dirty` returns `Option<Revision>`
//! (`None` = unknown node).

use std::collections::{BTreeSet, HashMap, HashSet};

use ids::Revision;

use crate::{Edge, GraphError, Node, NodeId};

/// Acyclic dependency graph (cycles rejected at insertion).
///
/// Pure CPU, no hidden allocation beyond the adjacency tables:
/// propagation visits each node of the closure once (a
/// `HashSet` set), without cloning nodes or edges.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Graph {
    nodes: HashMap<NodeId, Node>,
    /// `node → direct dependencies` (upstream). Empty entry = no dependency.
    deps: HashMap<NodeId, Vec<NodeId>>,
    /// `node → direct dependents` (downstream). Empty entry = consumed leaf.
    dependents: HashMap<NodeId, Vec<NodeId>>,
    next: u64,
}

impl Graph {
    /// Empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True if the graph holds no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// True if `id` is tracked by the graph.
    #[must_use]
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Number of edges (each edge counted once, upstream side).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.deps.values().map(Vec::len).sum()
    }

    /// All identifiers (order not guaranteed — see [`Graph::topo_order`]).
    #[must_use]
    pub fn all_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    /// All edges (`from → to` = `to` depends on `from`).
    #[must_use]
    pub fn all_edges(&self) -> Vec<Edge> {
        let mut edges = Vec::with_capacity(self.edge_count());
        for (to, froms) in &self.deps {
            for from in froms {
                edges.push(Edge::new(*from, *to));
            }
        }
        edges
    }

    /// Last emitted revision ([`Revision::NONE`] if no mutation).
    #[must_use]
    pub fn tree_revision(&self) -> Revision {
        Revision(self.next)
    }

    /// Revision of a node (`None` if unknown).
    #[must_use]
    pub fn node_revision(&self, id: NodeId) -> Option<Revision> {
        self.nodes.get(&id).map(|n| n.revision)
    }

    /// Read-only node (`None` if unknown — no `find_mut`:
    /// mutations go through `Graph`).
    #[must_use]
    pub fn find(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Dirtiness of a node (`None` if unknown).
    #[must_use]
    pub fn is_dirty(&self, id: NodeId) -> Option<bool> {
        self.nodes.get(&id).map(|n| n.dirty)
    }

    /// Direct dependencies, in insertion order (`None` if unknown).
    #[must_use]
    pub fn dependencies(&self, id: NodeId) -> Option<&[NodeId]> {
        self.deps.get(&id).map(Vec::as_slice)
    }

    /// Direct dependents, in insertion order (`None` if unknown).
    #[must_use]
    pub fn dependents(&self, id: NodeId) -> Option<&[NodeId]> {
        self.dependents.get(&id).map(Vec::as_slice)
    }

    // -- Structural mutations -------------------------------------------

    /// Creates a clean node with a fresh identifier.
    pub fn add_node(&mut self) -> NodeId {
        let id = NodeId::new();
        let revision = self.alloc();
        self.nodes.insert(id, Node::new(id, revision));
        self.deps.insert(id, Vec::new());
        self.dependents.insert(id, Vec::new());
        id
    }

    /// Inserts a node with an imposed stable identifier (restore, migration
    /// from another ID system). False if already present (no effect).
    pub fn insert(&mut self, id: NodeId) -> bool {
        if self.nodes.contains_key(&id) {
            return false;
        }
        let revision = self.alloc();
        self.nodes.insert(id, Node::new(id, revision));
        self.deps.insert(id, Vec::new());
        self.dependents.insert(id, Vec::new());
        true
    }

    /// Removes a node and all its incident edges. False if unknown.
    pub fn remove_node(&mut self, id: NodeId) -> bool {
        if !self.nodes.contains_key(&id) {
            return false;
        }
        self.nodes.remove(&id);
        if let Some(froms) = self.deps.remove(&id) {
            for from in froms {
                if let Some(children) = self.dependents.get_mut(&from) {
                    children.retain(|&c| c != id);
                }
            }
        }
        if let Some(tos) = self.dependents.remove(&id) {
            for to in tos {
                if let Some(parents) = self.deps.get_mut(&to) {
                    parents.retain(|&p| p != id);
                }
            }
        }
        self.advance();
        true
    }

    /// Declares that `to` depends on `from` (data `from → to`).
    ///
    /// Rejected without effect: unknown endpoint ([`GraphError::UnknownNode`]),
    /// self-loop ([`GraphError::SelfLoop`]), cycle closure
    /// ([`GraphError::Cycle`]). A duplicate is an idempotent no-op (`Ok`
    /// with the current revision, no allocation).
    ///
    /// # Errors
    ///
    /// Returns [`GraphError`] if the edge is rejected (graph unchanged).
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) -> Result<Revision, GraphError> {
        if !self.nodes.contains_key(&from) {
            return Err(GraphError::UnknownNode(from));
        }
        if !self.nodes.contains_key(&to) {
            return Err(GraphError::UnknownNode(to));
        }
        if from == to {
            return Err(GraphError::SelfLoop(from));
        }
        if self
            .deps
            .get(&to)
            .is_some_and(|froms| froms.contains(&from))
        {
            return Ok(self.tree_revision());
        }
        if self.reachable(to, from) {
            return Err(GraphError::Cycle { from, to });
        }
        if let Some(froms) = self.deps.get_mut(&to) {
            froms.push(from);
        }
        if let Some(tos) = self.dependents.get_mut(&from) {
            tos.push(to);
        }
        Ok(self.alloc())
    }

    /// Removes the `from → to` edge. False if absent or nodes unknown.
    pub fn remove_edge(&mut self, from: NodeId, to: NodeId) -> bool {
        let present = self
            .deps
            .get(&to)
            .is_some_and(|froms| froms.contains(&from));
        if !present {
            return false;
        }
        if let Some(froms) = self.deps.get_mut(&to) {
            froms.retain(|&f| f != from);
        }
        if let Some(tos) = self.dependents.get_mut(&from) {
            tos.retain(|&t| t != to);
        }
        self.advance();
        true
    }

    /// True if the `from → to` edge would close a cycle (or is a loop).
    /// False if an endpoint is unknown (`add_edge` will answer
    /// [`GraphError::UnknownNode`] before any cycle test).
    #[must_use]
    pub fn would_create_cycle(&self, from: NodeId, to: NodeId) -> bool {
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return false;
        }
        from == to || self.reachable(to, from)
    }

    // -- Dirtiness / revisions ------------------------------------------------

    /// Marks `id` and ALL its transitive downstream dirty with ONE fresh revision
    /// (one logical change = one stamp, including for already-
    /// dirty nodes whose revision advances to the most recent change).
    /// `None` if the node is unknown.
    pub fn mark_dirty(&mut self, id: NodeId) -> Option<Revision> {
        if !self.nodes.contains_key(&id) {
            return None;
        }
        let mut seen = HashSet::new();
        let mut stack = vec![id];
        let mut closure = Vec::new();
        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }
            closure.push(current);
            if let Some(children) = self.dependents.get(&current) {
                stack.extend(children.iter().copied());
            }
        }
        let revision = self.alloc();
        for dirty in closure {
            if let Some(node) = self.nodes.get_mut(&dirty) {
                node.revision = revision;
                node.dirty = true;
            }
        }
        Some(revision)
    }

    /// Marks `id` as recomputed (clean). Returns the PREVIOUS dirty state
    /// (`None` if unknown). Does NOT bump the revision: it
    /// identifies the computed state, and bumping would falsely invalidate
    /// caches comparing revisions.
    pub fn clear_dirty(&mut self, id: NodeId) -> Option<bool> {
        let node = self.nodes.get_mut(&id)?;
        let was_dirty = node.dirty;
        node.dirty = false;
        Some(was_dirty)
    }

    // -- Scheduling ------------------------------------------------------------

    /// Topological order: each dependency precedes its dependents
    /// (upstream first — the evaluation order of a future scheduler).
    /// Deterministic: same graph, same order (tie-broken by `NodeId`).
    /// The graph being acyclic by construction, the result always covers
    /// exactly all nodes.
    #[must_use]
    pub fn topo_order(&self) -> Vec<NodeId> {
        let mut indegree: HashMap<NodeId, usize> = HashMap::with_capacity(self.nodes.len());
        for id in self.nodes.keys() {
            indegree.insert(*id, self.deps.get(id).map_or(0, Vec::len));
        }
        let mut ready: BTreeSet<NodeId> = indegree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut order = Vec::with_capacity(self.nodes.len());
        while let Some(id) = ready.pop_first() {
            order.push(id);
            if let Some(children) = self.dependents.get(&id) {
                for &child in children {
                    if let Some(degree) = indegree.get_mut(&child) {
                        *degree -= 1;
                        if *degree == 0 {
                            ready.insert(child);
                        }
                    }
                }
            }
        }
        order
    }

    // -- Internals ------------------------------------------------------------

    /// Allocates the next revision.
    fn alloc(&mut self) -> Revision {
        self.next = self.next.wrapping_add(1);
        Revision(self.next)
    }

    /// Advances the counter without touching any node (removals).
    fn advance(&mut self) {
        self.next = self.next.wrapping_add(1);
    }

    /// Is `target` reachable from `start` following the
    /// `from → to` edges (downstream)? Heart of cycle detection.
    fn reachable(&self, start: NodeId, target: NodeId) -> bool {
        let mut seen = HashSet::new();
        let mut stack = vec![start];
        while let Some(current) = stack.pop() {
            if current == target {
                return true;
            }
            if !seen.insert(current) {
                continue;
            }
            if let Some(children) = self.dependents.get(&current) {
                stack.extend(children.iter().copied());
            }
        }
        false
    }
}
