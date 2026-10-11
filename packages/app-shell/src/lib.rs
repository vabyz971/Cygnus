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

//! Generic app skeleton: action queue and dock logic.
//!
//! `app-shell` carries what the photo, video and audio apps share
//! without knowing any domain: an action queue drained once
//! per frame ([`ActionQueue`]) and the dockable-tile mechanics of
//! `egui_tiles` ([`dock`]). Panel contents (titles,
//! domain widgets) stay in each app.
//!
//! FORBIDDEN here: any dependency on engines and apps, and any
//! domain type (documents, layers, clips, tracks).

pub mod actions;
pub mod dock;

pub use actions::ActionQueue;
pub use dock::DockTab;
