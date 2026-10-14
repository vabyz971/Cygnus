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

//! `cygnus_ui` crate — the Cygnus suite's egui design system.
//!
//! Layered architecture (bottom to top):
//!
//! 1. **`theme`** — SINGLE source of colors, spacing, radii,
//!    typo, sizes and borders. No other module hardcodes a value
//!    directly.
//! 2. **`primitives`** — generic building blocks (`Text`, `Icon`,
//!    `divider`, `Surface`), domain-independent.
//! 3. **`icons`** — stable icon enum + registry (no external
//!    lib exposed to apps).
//! 4. **`components`** — buttons, checkboxes, switches, sliders,
//!    inputs, lists, tabs, reorderable list (enum variants,
//!    theme styling; single `sanitize_selected` in `select`).
//! 5. **`containers`** — agnostic containers (`Section`, `Card`,
//!    `Stack`, `Panel`, `Split` + `SplitState`, `Collapsible`,
//!    `Toolbar`).
//! 6. **`layout`** — workspace state (`PanelId`, `WorkspaceState`,
//!    JSON persistence via `preferences`). Panel titles resolve via
//!    the shared `cygnus-i18n` catalog (owned by apps).
//! 7. **`viewport`** — zoom/pan state (`ViewportState`), grid,
//!    overlays + texture display.
//! 8. **`dialogs`** — modals, file pickers, progress.
//! 9. **`utils`** — generic drag & drop state (index).
//! 10. **`context`** — shared dependencies (`UiContext`: egui,
//!     theme, icons, translation — neither engines nor documents).
//!
//! Shared translations live in the `cygnus-i18n` crate at the
//! workspace root (one catalog for photo / video / audio, no
//! per-app duplicates). All catalogs (generic + photo/video/audio domains) live in `cygnus-i18n`; apps hold no translation tables.
//!
//! FORBIDDEN here: any reference to apps' domain types and to
//! engines. Domain widgets live in `apps/*/src/ui/`. The flow
//! stays Application → Domain UI → cygnus-ui → egui, and actions
//! UI → UiCommand → Application → Engine.

pub mod components;
pub mod containers;
pub mod context;
pub mod dialogs;
pub mod icons;
pub mod layout;
pub mod prelude;
pub mod primitives;
pub mod theme;
pub mod utils;
pub mod viewport;
