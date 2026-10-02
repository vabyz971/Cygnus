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

//! Crate `ui_kit` — design system egui de la suite Cygnus.
//!
//! Architecture en couches (bas vers haut) :
//!
//! 1. **`theme`** — SEULE source des couleurs, espacements, rayons,
//!    typo, tailles et bordures. Aucun autre module ne code de valeur
//!    en dur.
//! 2. **`primitives`** — briques génériques (`Text`, `Icon`,
//!    `divider`, `Surface`), indépendantes du domaine.
//! 3. **`icons`** — enum d'icônes stable + registre (aucune lib
//!    externe exposée aux apps).
//! 4. **`components`** — boutons, cases, interrupteurs, curseurs,
//!    inputs, listes, onglets, liste réordonnable (variants par enums,
//!    style par thème ; `sanitize_selected` unique dans `select`).
//! 5. **`containers`** — conteneurs agnostiques (`Section`, `Card`,
//!    `Stack`, `Panel`, `Split` + `SplitState`, `Collapsible`,
//!    `Toolbar`).
//! 6. **`layout`** — état du workspace (`PanelId`, `WorkspaceState`,
//!    persistence JSON via `preferences`).
//! 7. **`i18n`** — clés de traduction stables + catalogue.
//! 8. **`viewport`** — état zoom/pan (`ViewportState`), grille,
//!    overlays + affichage texture.
//! 9. **`dialogs`** — modales, sélecteurs de fichiers, progression.
//! 10. **`utils`** — état de drag & drop générique (index).
//! 11. **`context`** — dépendances communes (`UiContext` : egui,
//!     thème, icônes, traduction — ni moteurs ni documents).
//!
//! INTERDIT ici : toute référence aux types métier des apps et aux
//! engines. Les widgets métier vivent dans `apps/*/src/ui/`. Le flux
//! reste Application → Domain UI → UI Kit → egui, et les actions
//! UI → UiCommand → Application → Engine.

pub mod components;
pub mod containers;
pub mod context;
pub mod dialogs;
pub mod i18n;
pub mod icons;
pub mod layout;
pub mod primitives;
pub mod theme;
pub mod utils;
pub mod viewport;
