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

//! Shared `preferences` crate — persistent preferences model,
//! hardware detection and keyboard-shortcut resolver.
//!
//! Usable by the suite's three apps:
//! - [`model`]: serializable [`Preferences`] (JSON in the config folder),
//!   General / Render / Shortcuts sections;
//! - [`hardware`]: CPU / RAM / GPU report via wgpu (real adapters);
//! - [`keybindings`]: "Ctrl+Shift+S" parsing and generic
//!   resolution of keyboard events to action ids
//!   (action enums live on the apps' side, e.g. `PhotoShortcut`).

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
