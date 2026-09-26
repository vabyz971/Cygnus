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

//! Vocabulaire d'opérations : nœuds, ports, arêtes typées.
//!
//! Minimaliste et sans charge utile : un [`RenderNode`] dit QUOI faire
//! (opcode) et D'OÙ il vient (nœud de scène), jamais COMMENT (pixels,
//! shaders — affaire du [`crate::Backend`]).

use ids::EntityId;

use crate::SceneNodeId;

/// Identifiant stable d'un [`RenderNode`].
///
/// Frais à chaque (re)construction de portée — l'identité inter-`sync` des
/// portées intactes est préservée (leurs nœuds ne sont jamais touchés).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct RenderNodeId(EntityId);

impl RenderNodeId {
    /// Nouvel identifiant aléatoire.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Identifiant nul — sentinelle explicite, jamais attribué.
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// L'entité sous-jacente.
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Enveloppe une entité existante.
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }

    pub(crate) fn as_graph_id(self) -> graph::NodeId {
        graph::NodeId::from_entity(self.0)
    }

    pub(crate) fn from_graph_id(id: graph::NodeId) -> Self {
        Self(id.entity())
    }
}

impl Default for RenderNodeId {
    /// Identifiant nul — même valeur que [`RenderNodeId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for RenderNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r{}", self.0.as_uuid())
    }
}

/// Clé d'effet nommée, fournie par le moteur du domaine via l'overlay
/// ([`crate::EffectOverlay`]) : `"brightness_contrast"`, `"blur"`…
/// Le Render Graph ne connaît ni les paramètres ni l'implémentation —
/// seulement l'identité ordonnée, suffisante pour dériver et invalider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct EffectKey(pub String);

impl EffectKey {
    /// Nouvelle clé depuis un nom d'effet du domaine.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

/// Opération dérivée — le « quoi faire » d'un [`RenderNode`].
///
/// Une feuille de scène dérive `Source → Transform → [Effect…] → [Mask] →
/// Blend` ; un groupe dérive un `Blend` qui plie ses enfants ; les racines
/// se plient en chaîne et alimentent l'unique `Output`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderOp {
    /// Fournit le contenu d'une feuille (pixels lus par le backend).
    Source,
    /// Place le contenu (le backend lit le transform local de la scène).
    Transform,
    /// Applique un effet nommé du domaine (chaîne overlay, dans l'ordre).
    Effect(EffectKey),
    /// Applique la couverture combinée des masques sémantiques.
    Mask,
    /// Composite le premier plan sur le fond (ports [`RenderPort`]).
    Blend,
    /// Puits final unique (présentation / export).
    Output,
}

/// Port d'entrée d'une arête : le `Blend` distingue le fond (accumulé) du
/// premier plan (contenu du calque). Les autres opcodes n'acceptent que
/// [`RenderPort::Foreground`] (chaîne linéaire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderPort {
    /// Contenu principal (chaîne de la feuille, dernier top précédent…).
    Foreground,
    /// Accumulé à recouvrir (fond du `Blend`).
    Background,
}

/// Arête typée `from → to` sur `port` : « `to` lit `from` via `port` ».
/// Miroir exact (ports inclus) de l'arête interne du graphe de dépendances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RenderEdge {
    /// Nœud lu (amont).
    pub from: RenderNodeId,
    /// Nœud lecteur (aval).
    pub to: RenderNodeId,
    /// Port de lecture de `to`.
    pub port: RenderPort,
}

impl RenderEdge {
    /// Nouvelle arête typée.
    pub fn new(from: RenderNodeId, to: RenderNodeId, port: RenderPort) -> Self {
        Self { from, to, port }
    }
}

/// Nœud d'opération : opcode + origine scène.
///
/// Ni révision stockée (voir [`graph::Graph::node_revision`], source unique
/// des révisions), ni pixels : le backend résout le contenu via
/// [`RenderNode::scene`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RenderNode {
    /// Identifiant stable (à portée intacte préservée).
    pub id: RenderNodeId,
    /// Opération à exécuter.
    pub op: RenderOp,
    /// Nœud de scène d'origine (`None` = puits `Output` synthétique).
    pub scene: Option<SceneNodeId>,
}

impl RenderNode {
    pub(crate) fn new(id: RenderNodeId, op: RenderOp, scene: Option<SceneNodeId>) -> Self {
        Self { id, op, scene }
    }
}
