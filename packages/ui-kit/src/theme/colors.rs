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

//! Couleurs du thème Cygnus.
//!
//! SEULE source des couleurs de la suite : aucun composant, panneau
//! ni canvas ne doit coder un [`egui::Color32`] en dur. Les valeurs
//! du thème sombre vivent dans [`CygnusColors::dark`], repris par
//! [`Default`] pour un accès direct.

/// Couleurs du thème Cygnus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CygnusColors {
    /// Fond principal des panneaux.
    pub bg_primary: egui::Color32,
    /// Fond des fenêtres et barres.
    pub bg_secondary: egui::Color32,
    /// Fond des zones encastrées (inputs, canvas vide).
    pub bg_tertiary: egui::Color32,
    /// Texte principal.
    pub fg_primary: egui::Color32,
    /// Texte secondaire / désactivé.
    pub fg_secondary: egui::Color32,
    /// Accent (sélection, boutons primaires).
    pub accent: egui::Color32,
    /// Accent au survol.
    pub accent_hover: egui::Color32,
    /// Bordures.
    pub border: egui::Color32,
    /// Erreur (aussi utilisée pour les actions destructrices).
    pub error: egui::Color32,
    /// Succès.
    pub success: egui::Color32,
    /// Avertissement.
    pub warning: egui::Color32,
    /// Fond d'un item sélectionné (calque, clip, piste).
    pub item_selected: egui::Color32,
    /// Fond d'un item survolé.
    pub item_hover: egui::Color32,
    /// Indicateur de position de drop (drag & drop).
    pub drop_indicator: egui::Color32,
}

impl CygnusColors {
    /// Palette sombre par défaut de Cygnus.
    pub fn dark() -> Self {
        Self {
            bg_primary: egui::Color32::from_rgb(18, 18, 22),
            bg_secondary: egui::Color32::from_rgb(24, 24, 30),
            bg_tertiary: egui::Color32::from_rgb(32, 32, 40),
            fg_primary: egui::Color32::from_rgb(240, 240, 245),
            fg_secondary: egui::Color32::from_rgb(160, 160, 175),
            accent: egui::Color32::from_rgb(88, 124, 255),
            accent_hover: egui::Color32::from_rgb(108, 144, 255),
            border: egui::Color32::from_rgb(48, 48, 60),
            error: egui::Color32::from_rgb(255, 85, 85),
            success: egui::Color32::from_rgb(80, 220, 120),
            warning: egui::Color32::from_rgb(255, 190, 60),
            item_selected: egui::Color32::from_rgb(45, 55, 90),
            item_hover: egui::Color32::from_rgb(35, 35, 45),
            drop_indicator: egui::Color32::from_rgb(88, 124, 255),
        }
    }
}

impl Default for CygnusColors {
    /// Palette sombre par défaut.
    fn default() -> Self {
        Self::dark()
    }
}
