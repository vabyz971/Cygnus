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

//! État d'un panneau : visibilité + dimension.

use super::panel_id::PanelId;
use serde::{Deserialize, Serialize};

/// Largeur / hauteur par défaut d'un panneau en points.
pub const DEFAULT_PANEL_SIZE: f32 = 280.0;

/// État sérialisable d'un panneau.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelState {
    /// Panneau concerné.
    pub id: PanelId,
    /// Panneau visible.
    #[serde(default = "default_visible")]
    pub visible: bool,
    /// Largeur (régions latérales) ou hauteur (région basse).
    #[serde(default = "default_size")]
    pub size: f32,
}

/// Visibilité par défaut : visible.
fn default_visible() -> bool {
    true
}

/// Dimension par défaut.
fn default_size() -> f32 {
    DEFAULT_PANEL_SIZE
}

impl PanelState {
    /// État par défaut d'un panneau (visible, taille standard).
    pub fn new(id: PanelId) -> Self {
        Self {
            id,
            visible: true,
            size: DEFAULT_PANEL_SIZE,
        }
    }

    /// Bascule la visibilité.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}
