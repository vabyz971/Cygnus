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

//! Graphe de dépendances : structure, propagation et ordre topologique.
//!
//! [`Graph`] possède les nœuds et les deux adjacences (amont `deps`,
//! aval `dependents`) : toute mutation passe par lui, sans `find_mut`
//! public — les révisions restent cohérentes par construction.
//!
//! Contrat des mutations : `add_edge` retourne `Result<Revision,
//! GraphError>` (`Ok` = révision de l'arbre après l'appel, nouvelle ssi
//! la structure a changé) ; suppressions et `clear_dirty` retournent des
//! `bool`/`Option<bool>` ; `mark_dirty` retourne `Option<Revision>`
//! (`None` = nœud inconnu).

use std::collections::{BTreeSet, HashMap, HashSet};

use ids::Revision;

use crate::{Edge, GraphError, Node, NodeId};

/// Graphe de dépendances acyclique (cycles refusés à l'insertion).
///
/// CPU pur, aucune allocation cachée au-delà des tables d'adjacence :
/// la propagation visite chaque nœud de la fermeture une fois (ensemble
/// `HashSet`), sans cloner ni nœuds ni arêtes.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Graph {
    nodes: HashMap<NodeId, Node>,
    /// `node → dépendances directes` (amont). Entrée vide = sans dépendance.
    deps: HashMap<NodeId, Vec<NodeId>>,
    /// `node → dépendants directs` (aval). Entrée vide = feuille consommée.
    dependents: HashMap<NodeId, Vec<NodeId>>,
    next: u64,
}

impl Graph {
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

    /// Vrai si le graphe ne contient aucun nœud.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Vrai si `id` est suivi par le graphe.
    #[must_use]
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Nombre d'arêtes (chaque arête compte une fois, côté amont).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.deps.values().map(Vec::len).sum()
    }

    /// Tous les identifiants (ordre non garanti — voir [`Graph::topo_order`]).
    #[must_use]
    pub fn all_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    /// Toutes les arêtes (`from → to` = `to` dépend de `from`).
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

    /// Dernière révision émise ([`Revision::NONE`] si aucune mutation).
    #[must_use]
    pub fn tree_revision(&self) -> Revision {
        Revision(self.next)
    }

    /// Révision d'un nœud (`None` si inconnu).
    #[must_use]
    pub fn node_revision(&self, id: NodeId) -> Option<Revision> {
        self.nodes.get(&id).map(|n| n.revision)
    }

    /// Nœud en lecture seule (`None` si inconnu — pas de `find_mut` :
    /// les mutations passent par `Graph`).
    #[must_use]
    pub fn find(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Saleté d'un nœud (`None` si inconnu).
    #[must_use]
    pub fn is_dirty(&self, id: NodeId) -> Option<bool> {
        self.nodes.get(&id).map(|n| n.dirty)
    }

    /// Dépendances directes, dans l'ordre d'insertion (`None` si inconnu).
    #[must_use]
    pub fn dependencies(&self, id: NodeId) -> Option<&[NodeId]> {
        self.deps.get(&id).map(Vec::as_slice)
    }

    /// Dépendants directs, dans l'ordre d'insertion (`None` si inconnu).
    #[must_use]
    pub fn dependents(&self, id: NodeId) -> Option<&[NodeId]> {
        self.dependents.get(&id).map(Vec::as_slice)
    }

    // -- Mutations structurelles -------------------------------------------

    /// Crée un nœud propre avec identifiant frais.
    pub fn add_node(&mut self) -> NodeId {
        let id = NodeId::new();
        let revision = self.alloc();
        self.nodes.insert(id, Node::new(id, revision));
        self.deps.insert(id, Vec::new());
        self.dependents.insert(id, Vec::new());
        id
    }

    /// Insère un nœud d'identifiant stable imposé (restauration, migration
    /// depuis un autre système d'IDs). Faux si déjà présent (sans effet).
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

    /// Supprime un nœud et toutes ses arêtes incidentes. Faux si inconnu.
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

    /// Déclare que `to` dépend de `from` (données `from → to`).
    ///
    /// Rejeté sans effet : extrémité inconnue ([`GraphError::UnknownNode`]),
    /// boucle sur soi ([`GraphError::SelfLoop`]), fermeture de cycle
    /// ([`GraphError::Cycle`]). Un doublon est un no-op idempotent (`Ok`
    /// avec la révision courante, sans allocation).
    ///
    /// # Errors
    ///
    /// Retourne [`GraphError`] si l'arête est rejetée (graphe inchangé).
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

    /// Retire l'arête `from → to`. Faux si absente ou nœuds inconnus.
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

    /// Vrai si l'arête `from → to` fermerait un cycle (ou est une boucle).
    /// Faux si une extrémité est inconnue (`add_edge` répondra
    /// [`GraphError::UnknownNode`] avant tout test de cycle).
    #[must_use]
    pub fn would_create_cycle(&self, from: NodeId, to: NodeId) -> bool {
        if !self.nodes.contains_key(&from) || !self.nodes.contains_key(&to) {
            return false;
        }
        from == to || self.reachable(to, from)
    }

    // -- Saleté / révisions --------------------------------------------------

    /// Salit `id` et TOUT son aval transitif avec UNE révision fraîche
    /// (un changement logique = un timbre, y compris pour les nœuds déjà
    /// dirty dont la révision avance vers le changement le plus récent).
    /// `None` si le nœud est inconnu.
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

    /// Marque `id` comme recalculé (propre). Retourne l'état dirty
    /// PRÉCÉDENT (`None` si inconnu). Ne bumpe PAS la révision : elle
    /// identifie l'état calculé, et bumper invaliderait faussement les
    /// caches qui comparent les révisions.
    pub fn clear_dirty(&mut self, id: NodeId) -> Option<bool> {
        let node = self.nodes.get_mut(&id)?;
        let was_dirty = node.dirty;
        node.dirty = false;
        Some(was_dirty)
    }

    // -- Ordonnancement ------------------------------------------------------

    /// Ordre topologique : chaque dépendance précède ses dépendants
    /// (amont d'abord — l'ordre d'évaluation d'un futur scheduler).
    /// Déterministe : à graphe égal, même ordre (départage par `NodeId`).
    /// Le graphe étant acyclique par construction, le résultat couvre
    /// toujours exactement tous les nœuds.
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

    // -- Internes ------------------------------------------------------------

    /// Alloue la prochaine révision.
    fn alloc(&mut self) -> Revision {
        self.next = self.next.wrapping_add(1);
        Revision(self.next)
    }

    /// Avance le compteur sans toucher de nœud (suppressions).
    fn advance(&mut self) {
        self.next = self.next.wrapping_add(1);
    }

    /// `target` est-il atteignable depuis `start` en suivant les arêtes
    /// `from → to` (sens aval) ? Cœur de la détection de cycles.
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
