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

//! Frontière d'exécution : le trait [`Backend`] et son backend nul.
//!
//! Séparation visée :
//!
//! ```text
//! RenderGraph ──plan──▶ Backend ──spécialisation──▶ wgpu (plus tard)
//! ```
//!
//! Le backend reçoit les nœuds en ordre topologique avec leurs entrées
//! typées et résout lui-même les contenus (scène, pixels, caches). Cette
//! crate ne transporte ni textures ni handles GPU — seulement des ids.

use crate::{RenderEdge, RenderNode, RenderNodeId};

/// Backend d'exécution du Render Graph (CPU aujourd'hui, GPU demain).
///
/// Contrat : `execute` est appelé avec un plan topologique
/// ([`crate::RenderGraph::plan`]) ; le backend peut supposer chaque entrée
/// déjà produite. Aucune méthode n'alloue côté graphe : `inputs` est une
/// tranche sur les arêtes stockées.
pub trait Backend {
    /// Ouvre une passe d'exécution (cadre, cible — sémantique du backend).
    fn begin_frame(&mut self) {}

    /// Exécute UN nœud. `inputs` liste `(from, port)` dans un ordre
    /// quelconque — le backend distingue fond/premier plan par le port.
    ///
    /// # Errors
    ///
    /// Retourne un message en cas d'échec d'exécution (le graphe remonte
    /// l'erreur sans état partiel observable).
    fn execute_node(&mut self, node: &RenderNode, inputs: &[RenderEdge]) -> Result<(), String>;

    /// Ferme la passe (présentation — sémantique du backend).
    fn end_frame(&mut self) {}
}

/// Backend nul : enregistre l'ordre d'exécution, ne produit rien.
///
/// Sert aux tests, aux mesures d'invalidation et de gabarit aux futurs
/// backends (CPU pixels, puis wgpu).
#[derive(Debug, Default)]
pub struct NullBackend {
    order: Vec<RenderNodeId>,
}

impl NullBackend {
    /// Nouveau backend nul.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nœuds exécutés, dans l'ordre.
    #[must_use]
    pub fn order(&self) -> &[RenderNodeId] {
        &self.order
    }

    /// Nombre de nœuds exécutés.
    #[must_use]
    pub fn executed(&self) -> usize {
        self.order.len()
    }

    /// Remet l'enregistrement à zéro (passes successives comparables).
    pub fn reset(&mut self) {
        self.order.clear();
    }
}

impl Backend for NullBackend {
    fn execute_node(&mut self, node: &RenderNode, _inputs: &[RenderEdge]) -> Result<(), String> {
        self.order.push(node.id);
        Ok(())
    }
}
