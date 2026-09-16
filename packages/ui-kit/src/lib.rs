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
//!    inputs, listes, onglets (variants par enums, style par thème).
//! 5. **`containers`** — conteneurs agnostiques (`Section`, `Card`,
//!    `Stack`, `Panel`, `Split`).
//! 6. **`layout`** — état du workspace (`PanelId`, `WorkspaceState`,
//!    persistence JSON via `preferences`).
//! 7. **`i18n`** — clés de traduction stables + catalogue.
//! 8. **`widgets`** — composants historiques génériques
//!    (`CygnusButton`, `CygnusSlider`, …, `CygnusIcon` — seul contact
//!    avec `egui_material_icons` — `ReorderableList`).
//! 9. **`panels`** — conteneurs historiques (panneau titré, split,
//!    onglets, repliable, toolbar).
//! 10. **`viewport`** — état zoom/pan générique + affichage texture.
//! 11. **`dialogs`** — modales, sélecteurs de fichiers, progression.
//! 12. **`utils`** — état de drag & drop générique (index).
//!
//! INTERDIT ici : toute référence aux types métier des apps et aux
//! engines. Les widgets métier vivent dans `apps/*/src/ui/`. Le flux
//! reste Application → Domain UI → UI Kit → egui, et les actions
//! UI → UiCommand → Application → Engine.

pub mod components;
pub mod containers;
pub mod dialogs;
pub mod icons;
pub mod i18n;
pub mod layout;
pub mod panels;
pub mod primitives;
pub mod theme;
pub mod utils;
pub mod viewport;
pub mod widgets;
