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

//! Registre d'icônes : point d'accès unique des apps aux glyphes.
//!
//! Le registre ne connaît aucune bibliothèque d'icônes : il délègue
//! à [`CygnusIcon`](crate::widgets::icon::CygnusIcon), seul contact
//! du workspace avec `egui_material_icons`. Changer de lib un jour =
//! modifier un seul fichier.

use super::icon::Icon;
use crate::widgets::icon::CygnusIcon;

/// Fournisseur de glyphes pour les apps (zéro dépendance externe).
#[derive(Debug, Clone, Copy, Default)]
pub struct IconRegistry;

impl IconRegistry {
    /// Crée le registre (statique, sans état).
    pub fn new() -> Self {
        Self
    }

    /// Glyphe prêt à afficher pour `icon`.
    pub fn text(self, icon: Icon) -> egui::RichText {
        Self::resolve(icon).text()
    }

    /// Glyphe à la taille demandée.
    pub fn sized(self, icon: Icon, size: f32) -> egui::RichText {
        Self::resolve(icon).sized(size)
    }

    /// Glyphe teinté à la taille demandée.
    pub fn colored(self, icon: Icon, size: f32, color: egui::Color32) -> egui::RichText {
        Self::resolve(icon).sized(size).color(color)
    }

    /// Résolution [`Icon`] → [`CygnusIcon`] (exhaustive : toute
    /// variante ajoutée à `Icon` casse ici jusqu'à son mapping).
    fn resolve(icon: Icon) -> CygnusIcon {
        match icon {
            Icon::Save => CygnusIcon::Save,
            Icon::Open => CygnusIcon::FolderOpen,
            Icon::Undo => CygnusIcon::Undo,
            Icon::Redo => CygnusIcon::Redo,
            Icon::Close => CygnusIcon::Close,
            Icon::ZoomIn => CygnusIcon::ZoomIn,
            Icon::ZoomOut => CygnusIcon::ZoomOut,
            Icon::Delete => CygnusIcon::Delete,
            Icon::Add => CygnusIcon::Add,
            Icon::Duplicate => CygnusIcon::Duplicate,
            Icon::Export => CygnusIcon::Export,
            Icon::Settings => CygnusIcon::Settings,
            Icon::Play => CygnusIcon::Play,
            Icon::Pause => CygnusIcon::Pause,
            Icon::Search => CygnusIcon::Search,
            Icon::Check => CygnusIcon::Check,
            Icon::Info => CygnusIcon::Info,
            Icon::Folder => CygnusIcon::FolderOpen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::setup_fonts;

    /// Toutes les variantes de `Icon` doivent se résoudre.
    #[test]
    fn all_icons_resolve() {
        let registry = IconRegistry::new();
        for icon in [
            Icon::Save,
            Icon::Open,
            Icon::Undo,
            Icon::Redo,
            Icon::Close,
            Icon::ZoomIn,
            Icon::ZoomOut,
            Icon::Delete,
            Icon::Add,
            Icon::Duplicate,
            Icon::Export,
            Icon::Settings,
            Icon::Play,
            Icon::Pause,
            Icon::Search,
            Icon::Check,
            Icon::Info,
            Icon::Folder,
        ] {
            let _ = registry.text(icon);
        }
    }

    #[test]
    fn registry_renders_without_panic() {
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        let registry = IconRegistry::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label(registry.sized(Icon::Save, 18.0));
            });
        })
        .drop_without_applying_deltas();
    }
}
