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

//! Identifiants stables des panneaux du workspace.
//!
//! Les ids sont sérialisés dans les préférences : ne jamais renommer
//! une variante sans gérer la migration (voir [`super::persistence`]).

use crate::i18n::TextKey;
use serde::{Deserialize, Serialize};

/// Panneau ancrable du workspace (stable entre versions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelId {
    /// Barre d'outils.
    Tools,
    /// Liste des calques / clips / pistes.
    Layers,
    /// Inspecteur de propriétés.
    Inspector,
    /// Navigateur / miniature.
    Navigator,
    /// Historique des actions.
    History,
    /// Ligne temporelle (video / audio).
    Timeline,
}

impl PanelId {
    /// Clé de traduction du titre du panneau.
    pub fn title_key(self) -> TextKey {
        match self {
            Self::Tools => TextKey::Tools,
            Self::Layers => TextKey::Layers,
            Self::Inspector => TextKey::Inspector,
            Self::Navigator => TextKey::Navigator,
            Self::History => TextKey::History,
            Self::Timeline => TextKey::Timeline,
        }
    }
}
