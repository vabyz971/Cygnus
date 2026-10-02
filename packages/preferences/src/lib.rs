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

//! Crate partagé `preferences` — modèle de préférences persistant,
//! détection matérielle et résolveur de raccourcis clavier.
//!
//! Utilisable par les trois apps de la suite :
//! - [`model`] : [`Preferences`] sérialisable (JSON dans le dossier config),
//!   sections Général / Rendu / Raccourcis ;
//! - [`hardware`] : rapport CPU / RAM / GPU via wgpu (adaptateurs réels) ;
//! - [`keybindings`] : parsing « Ctrl+Shift+S » et résolution
//!   générique d'événements clavier vers identifiants d'action
//!   (les enums d'actions vivent côté apps, ex. `PhotoShortcut`).

pub mod hardware;
pub mod keybindings;
pub mod model;

pub use hardware::{CpuInfo, GpuInfo, HardwareReport, RamInfo};
pub use keybindings::parse_combo;
pub use keybindings::{
    AppKey, AppModifiers, KeyCombo, KeybindingResolver, NamedKey, key_to_string,
};
pub use model::{
    GeneralPreferences, KeybindingPreferences, Preferences, PreferencesError, RenderApi,
    RenderPreferences, RenderQuality, Theme,
};
