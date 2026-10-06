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

//! Sections de l'inspecteur (une par domaine du calque).
//!
//! Aujourd'hui : `attachment` (pièce jointe focalisée depuis
//! l'arbre des calques : nom, type, on/off). Les futures sections
//! (`transform`, `blend`, `effects`) suivront le même contrat :
//! `(ui, ctx, …) -> Vec<InspectorAction>`.
//!
//! Visibilité / opacité / fusion vivent dans le panneau Calques
//! (en-tête de sélection) : jamais dupliquées ici.

pub mod attachment;

pub use attachment::draw_attachment_section;
