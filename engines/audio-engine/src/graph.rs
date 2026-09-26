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

//! Graphe DSP : nœuds métier câblés sur le graphe de dépendances partagé.
//!
//! [`AudioGraph`] possède les [`AudioNode`] et miroite leurs dépendances
//! dans `graph::Graph` : propagation dirty, révisions, ordre topo et
//! DÉTECTION DE CYCLES (une boucle de feedback non retardée est rejetée à
//! l'insertion — le DSP l'exige). Les nœuds désactivés restent câblés
//! (bypass décidé par le backend, pas par le graphe).

use std::collections::HashMap;

use graph::{Graph as DepGraph, GraphError, NodeId as GraphNodeId};
use ids::Revision;

use crate::{AudioId, AudioNode};

/// Graphe audio : métier + dépendances partagées.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioGraph {
    inner: DepGraph,
    nodes: HashMap<AudioId, AudioNode>,
}

impl AudioGraph {
    /// Graphe vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nombre de nœuds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Vrai si vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Vrai si suivi.
    #[must_use]
    pub fn contains(&self, id: AudioId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Nœud en lecture seule.
    #[must_use]
    pub fn find(&self, id: AudioId) -> Option<&AudioNode> {
        self.nodes.get(&id)
    }

    /// Dernière révision émise.
    #[must_use]
    pub fn tree_revision(&self) -> Revision {
        self.inner.tree_revision()
    }

    /// Révision d'un nœud (`None` si inconnu).
    #[must_use]
    pub fn node_revision(&self, id: AudioId) -> Option<Revision> {
        self.inner.node_revision(graph_id(id))
    }

    /// Saleté d'un nœud (`None` si inconnu).
    #[must_use]
    pub fn is_dirty(&self, id: AudioId) -> Option<bool> {
        self.inner.is_dirty(graph_id(id))
    }

    /// Ajoute un nœud (propre) et retourne son id.
    pub fn add_node(&mut self, node: AudioNode) -> AudioId {
        let id = node.id;
        self.inner.insert(graph_id(id));
        self.nodes.insert(id, node);
        id
    }

    /// Retire un nœud et ses arêtes (faux si inconnu).
    pub fn remove_node(&mut self, id: AudioId) -> bool {
        if self.nodes.remove(&id).is_none() {
            return false;
        }
        self.inner.remove_node(graph_id(id));
        true
    }

    /// Câble `from → to` (`to` dépend de `from`). Cycles, boucles et
    /// inconnus rejetés ([`GraphError`], graphe inchangé — les ids des
    /// erreurs sont les miroirs internes, même entité).
    ///
    /// # Errors
    ///
    /// Retourne [`GraphError`] si l'arête est rejetée.
    pub fn connect(&mut self, from: AudioId, to: AudioId) -> Result<Revision, GraphError> {
        self.inner.add_edge(graph_id(from), graph_id(to))
    }

    /// Déconnecte (faux si arête absente).
    pub fn disconnect(&mut self, from: AudioId, to: AudioId) -> bool {
        self.inner.remove_edge(graph_id(from), graph_id(to))
    }

    /// Dépendances directes de `to` (`None` si inconnu).
    #[must_use]
    pub fn dependencies(&self, to: AudioId) -> Option<Vec<AudioId>> {
        self.inner.dependencies(graph_id(to)).map(|deps| {
            deps.iter()
                .map(|id| AudioId::from_entity(id.entity()))
                .collect()
        })
    }

    /// Ordre de traitement (amont d'abord — le driver DSP suit cet ordre).
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

    /// Salit un nœud et son aval (paramètre modifié).
    pub fn mark_dirty(&mut self, id: AudioId) -> Option<Revision> {
        self.inner.mark_dirty(graph_id(id))
    }

    /// Marque un nœud recalculé (sans bump — voir `graph`).
    pub fn clear_dirty(&mut self, id: AudioId) -> Option<bool> {
        self.inner.clear_dirty(graph_id(id))
    }
}

/// Projection `AudioId → graph::NodeId` (même entité, autre monde).
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
        // Graphe intact : l'ordre couvre toujours tout.
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
