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

//! One panel's state: visibility + size.

use super::panel_id::PanelId;
use serde::{Deserialize, Serialize};

/// Default panel width / height in points.
pub const DEFAULT_PANEL_SIZE: f32 = 280.0;

/// Serializable panel state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelState {
    /// Concerned panel.
    pub id: PanelId,
    /// Visible panel.
    #[serde(default = "default_visible")]
    pub visible: bool,
    /// Width (side regions) or height (bottom region).
    #[serde(default = "default_size")]
    pub size: f32,
}

/// Default visibility: visible.
fn default_visible() -> bool {
    true
}

/// Default size.
fn default_size() -> f32 {
    DEFAULT_PANEL_SIZE
}

impl PanelState {
    /// A panel's default state (visible, standard size).
    pub fn new(id: PanelId) -> Self {
        Self {
            id,
            visible: true,
            size: DEFAULT_PANEL_SIZE,
        }
    }

    /// Toggles visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}
