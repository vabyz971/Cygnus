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

//! DSP graph: business nodes wired onto the shared dependency graph.
//!
//! [`AudioGraph`] owns the [`AudioNode`]s and mirrors their dependencies
//! in `graph::Graph`: dirty propagation, revisions, topo order and
//! CYCLE DETECTION (a non-delayed feedback loop is rejected at
//! insertion — DSP requires it). Disabled nodes stay wired
//! (bypass decided by the backend, not by the graph).

use std::collections::HashMap;

use graph::{Graph as DepGraph, GraphError, NodeId as GraphNodeId};
use ids::Revision;

use crate::{AudioId, AudioNode};

/// Audio graph: business + shared dependencies.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioGraph {
    inner: DepGraph,
    nodes: HashMap<AudioId, AudioNode>,
}

impl AudioGraph {
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

    /// True if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// True if tracked.
    #[must_use]
    pub fn contains(&self, id: AudioId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Read-only node.
    #[must_use]
    pub fn find(&self, id: AudioId) -> Option<&AudioNode> {
        self.nodes.get(&id)
    }

    /// Last emitted revision.
    #[must_use]
    pub fn tree_revision(&self) -> Revision {
        self.inner.tree_revision()
    }

    /// Revision of a node (`None` if unknown).
    #[must_use]
    pub fn node_revision(&self, id: AudioId) -> Option<Revision> {
        self.inner.node_revision(graph_id(id))
    }

    /// Dirtiness of a node (`None` if unknown).
    #[must_use]
    pub fn is_dirty(&self, id: AudioId) -> Option<bool> {
        self.inner.is_dirty(graph_id(id))
    }

    /// Adds a (clean) node and returns its id.
    pub fn add_node(&mut self, node: AudioNode) -> AudioId {
        let id = node.id;
        self.inner.insert(graph_id(id));
        self.nodes.insert(id, node);
        id
    }

    /// Removes a node and its edges (false if unknown).
    pub fn remove_node(&mut self, id: AudioId) -> bool {
        if self.nodes.remove(&id).is_none() {
            return false;
        }
        self.inner.remove_node(graph_id(id));
        true
    }

    /// Wires `from → to` (`to` depends on `from`). Cycles, loops and
    /// unknown ids rejected ([`GraphError`], graph unchanged — the ids of
    /// errors are the internal mirrors, same entity).
    ///
    /// # Errors
    ///
    /// Returns [`GraphError`] if the edge is rejected.
    pub fn connect(&mut self, from: AudioId, to: AudioId) -> Result<Revision, GraphError> {
        self.inner.add_edge(graph_id(from), graph_id(to))
    }

    /// Disconnects (false if edge absent).
    pub fn disconnect(&mut self, from: AudioId, to: AudioId) -> bool {
        self.inner.remove_edge(graph_id(from), graph_id(to))
    }

    /// Direct dependencies of `to` (`None` if unknown).
    #[must_use]
    pub fn dependencies(&self, to: AudioId) -> Option<Vec<AudioId>> {
        self.inner.dependencies(graph_id(to)).map(|deps| {
            deps.iter()
                .map(|id| AudioId::from_entity(id.entity()))
                .collect()
        })
    }

    /// Processing order (upstream first — the DSP driver follows this order).
    #[must_use]
    pub fn process_order(&self) -> Vec<AudioId> {
        self.inner
            .topo_order()
            .into_iter()
            .filter_map(|id| {
                let audio = AudioId::from_entity(id.entity());
                self.nodes.contains_key(&audio).then_some(audio)
            })
            .collect()
    }

    /// Marks a node and its downstream dirty (modified parameter).
    pub fn mark_dirty(&mut self, id: AudioId) -> Option<Revision> {
        self.inner.mark_dirty(graph_id(id))
    }

    /// Marks a node recomputed (no bump — see `graph`).
    pub fn clear_dirty(&mut self, id: AudioId) -> Option<bool> {
        self.inner.clear_dirty(graph_id(id))
    }
}

/// Projection `AudioId → graph::NodeId` (same entity, other world).
fn graph_id(id: AudioId) -> GraphNodeId {
    GraphNodeId::from_entity(id.entity())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioNodeKind;

    fn chaine() -> (AudioGraph, AudioId, AudioId, AudioId) {
        let mut graph = AudioGraph::new();
        let src = graph.add_node(AudioNode::oscillator("osc", 440.0, crate::Wave::Sine));
        let gain = graph.add_node(AudioNode::gain("g", -6.0));
        let out = graph.add_node(AudioNode::mixer("mix"));
        graph.connect(src, gain).expect("acyclic");
        graph.connect(gain, out).expect("acyclic");
        (graph, src, gain, out)
    }

    #[test]
    fn cablage_topo_et_dependances() {
        let (graph, src, gain, out) = chaine();
        assert_eq!(graph.process_order(), vec![src, gain, out]);
        assert_eq!(graph.dependencies(out), Some(vec![gain]));
        assert_eq!(graph.dependencies(src), Some(vec![]));
    }

    #[test]
    fn boucle_de_feedback_rejetee() {
        let (mut graph, src, _, out) = chaine();
        assert!(matches!(
            graph.connect(out, src),
            Err(GraphError::Cycle { .. })
        ));
        assert!(matches!(
            graph.connect(src, src),
            Err(GraphError::SelfLoop(_))
        ));
        // Graph intact: the order still covers everything.
        assert_eq!(graph.process_order().len(), 3);
    }

    #[test]
    fn salissure_propagee_aux_dependants() {
        let (mut graph, src, gain, out) = chaine();
        let _ = (gain, out);
        graph.mark_dirty(src).expect("exists");
        for id in [src, gain, out] {
            assert!(graph.is_dirty(id).expect("known"));
        }
        assert!(graph.clear_dirty(gain).expect("known"));
        assert!(!graph.is_dirty(gain).expect("known"));
        assert!(graph.mark_dirty(AudioId::nil()).is_none());
        let _ = AudioNodeKind::Mixer;
    }
}
