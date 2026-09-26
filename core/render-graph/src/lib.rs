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

//! Render Graph : opérations dérivées du Scene Graph, pas son remplacement.
//!
//! ```text
//! Scene Graph  ("que contient le document ?" — groupes, images, …)
//!   │ dérive (build / sync incrémental)
//! Render Graph ("quelles opérations pour produire l'image ?"
//!               Source → Transform → Effect → Mask → Blend → Output)
//!   │ ordonne (plan topologique, dirty seul si demandé)
//! Backend      (CPU aujourd'hui, wgpu demain — trait pur, aucune texture ici)
//! ```
//!
//! Règles de cette crate :
//!
//! - AUCUNE dépendance `egui`, UI, widgets, app, `wgpu`, texture ou buffer
//!   GPU. Les pixels sont résolus par le [`Backend`] au moment d'exécuter.
//! - Le graphe de dépendances ([`graph::Graph`]) est le moteur
//!   d'invalidation : propagation dirty, révisions, ordre topologique.
//!   Cette crate ajoute la DÉRIVATION depuis la scène (liaisons
//!   scène→rendu, reconstruction partielle par portée) et le vocabulaire
//!   d'opérations.
//! - Reconstruction partielle : un changement d'attribut (opacité,
//!   transform, visibilité…) ne reconstruit RIEN — il salit les nœuds
//!   dérivés. Un changement structurel ne reconstruit que la PORTÉE
//!   concernée (fratrie), jamais le graphe entier. Reconstruire le graphe
//!   ne recalcule aucun pixel : seuls les flags dirty pilotent le travail.
//!
//! # Examples
//!
//! ```
//! use render_graph::{Backend, NullBackend, RenderGraph, Scene};
//!
//! let mut scene = Scene::new();
//! let root = scene.create_node(scene::NodeKind::Group);
//! scene.create_child(root, scene::NodeKind::Image);
//!
//! let graph = RenderGraph::build(&scene, &Default::default());
//! let plan = graph.plan();
//! let mut backend = NullBackend::new();
//! graph.execute(&mut backend, &plan).expect("null backend never fails");
//! assert!(backend.executed() > 0);
//! ```

pub mod backend;
pub mod graph;
pub mod op;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

pub use backend::{Backend, NullBackend};
pub use graph::{ExecuteReport, ExecutionPlan, RenderGraph, SyncReport};
pub use ids::Revision;
pub use op::{EffectKey, RenderEdge, RenderNode, RenderNodeId, RenderOp, RenderPort};
pub use scene::{Scene, SceneNodeId};

/// Effets par nœud de scène : chaînes d'effets nommés (filtres photo,
/// effets vidéo…) dans l'ordre d'application, fournies par le moteur du
/// domaine au moment de dériver/synchroniser.
///
/// La scène ne porte pas d'effets (volontairement) ; cet overlay est le
/// SEUL point d'entrée des effets dans la dérivation — et le point
/// d'intégration des moteurs existants (voir `photo-engine`,
/// `render_overlay::effect_overlay`).
pub type EffectOverlay = HashMap<SceneNodeId, Vec<EffectKey>>;
