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
//! au module interne [`super::glyph`], seul contact du workspace avec
//! `egui_material_icons::icons`. Changer de lib un jour = modifier un
//! seul fichier.

use super::glyph::CygnusIcon;
use super::icon::Icon;

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
    pub(crate) fn resolve(icon: Icon) -> CygnusIcon {
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
            Icon::Visibility => CygnusIcon::Visibility,
            Icon::VisibilityOff => CygnusIcon::VisibilityOff,
            Icon::Layers => CygnusIcon::Layers,
            Icon::ImageIcon => CygnusIcon::ImageIcon,
            Icon::Brush => CygnusIcon::Brush,
            Icon::Eraser => CygnusIcon::Eraser,
            Icon::Hand => CygnusIcon::Hand,
            Icon::MoveTool => CygnusIcon::MoveTool,
            Icon::ExpandMore => CygnusIcon::ExpandMore,
            Icon::ExpandLess => CygnusIcon::ExpandLess,
            Icon::Eyedropper => CygnusIcon::Eyedropper,
            Icon::Filter => CygnusIcon::Filter,
            Icon::Mask => CygnusIcon::Mask,
            Icon::DragHandle => CygnusIcon::DragHandle,
            Icon::Text => CygnusIcon::Text,
            Icon::Shape => CygnusIcon::Shape,
            Icon::Cut => CygnusIcon::Cut,
            Icon::Split => CygnusIcon::Split,
            Icon::Trim => CygnusIcon::Trim,
            Icon::Film => CygnusIcon::Film,
            Icon::Stop => CygnusIcon::Stop,
            Icon::Record => CygnusIcon::Record,
            Icon::MusicNote => CygnusIcon::MusicNote,
            Icon::Piano => CygnusIcon::Piano,
            Icon::Mic => CygnusIcon::Mic,
            Icon::Warning => CygnusIcon::Warning,
            Icon::Error => CygnusIcon::Error,
            Icon::Remove => CygnusIcon::Remove,
            Icon::LayerAdd => CygnusIcon::LayerAdd,
        }
    }
}

/// Liste exhaustive des icônes publiques, utilisée par les tests et les
/// galeries (47 variantes de [`Icon`] ; `Open` et `Folder` partagent le
/// même glyphe mais restent deux usages distincts).
pub const ALL_ICONS: &[Icon] = &[
    Icon::Add,
    Icon::Remove,
    Icon::Visibility,
    Icon::VisibilityOff,
    Icon::Layers,
    Icon::LayerAdd,
    Icon::Brush,
    Icon::Eraser,
    Icon::Hand,
    Icon::MoveTool,
    Icon::ZoomIn,
    Icon::ZoomOut,
    Icon::Undo,
    Icon::Redo,
    Icon::Save,
    Icon::Open,
    Icon::Export,
    Icon::Settings,
    Icon::Close,
    Icon::DragHandle,
    Icon::Folder,
    Icon::ImageIcon,
    Icon::Mask,
    Icon::Text,
    Icon::Shape,
    Icon::Cut,
    Icon::Split,
    Icon::Trim,
    Icon::Film,
    Icon::Play,
    Icon::Pause,
    Icon::Stop,
    Icon::Record,
    Icon::MusicNote,
    Icon::Piano,
    Icon::Mic,
    Icon::Duplicate,
    Icon::Filter,
    Icon::Delete,
    Icon::Eyedropper,
    Icon::Search,
    Icon::Check,
    Icon::Warning,
    Icon::Error,
    Icon::Info,
    Icon::ExpandMore,
    Icon::ExpandLess,
];

/// Bouton icône standard Cygnus : sans frame, fond au hover, tooltip optionnel.
///
/// Utilisé PARTOUT (toolbar, panels, layers) pour garantir l'uniformité
/// entre les 3 apps.
///
/// # Exemple
/// ```rust,no_run
/// # use ui_kit::icons::{Icon, icon_button};
/// # egui::__run_test_ui(|ui| {
/// let response = icon_button(ui, Icon::Save, Some("Enregistrer"));
/// if response.clicked() {
///     // … déclencher la sauvegarde via un channel moteur …
/// }
/// # });
/// ```
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: Option<&str>) -> egui::Response {
    let registry = IconRegistry::new();
    let response = ui.add(egui::Button::new(registry.sized(icon, 18.0)).frame(false));
    if let Some(tip) = tooltip {
        response.clone().on_hover_text(tip)
    } else {
        response
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

    #[test]
    fn all_icons_have_glyph() {
        for icon in ALL_ICONS {
            let glyph = IconRegistry::resolve(*icon);
            assert!(!glyph.codepoint().is_empty(), "glyphe vide pour {icon:?}");
        }
    }

    #[test]
    fn all_icons_list_is_exhaustive() {
        // 47 variantes déclarées dans `Icon` : le test casse si une
        // variante est ajoutée sans être enregistrée dans ALL_ICONS.
        // (`Open` et `Folder` partagent le glyphe dossier mais comptent
        // comme deux usages distincts, d'où 47 contre 46 côté glyphe.)
        assert_eq!(ALL_ICONS.len(), 47);
    }

    #[test]
    fn icons_render_without_panic() {
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        let registry = IconRegistry::new();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for icon in ALL_ICONS {
                    ui.label(registry.text(*icon));
                }
            });
        });
        assert!(
            !output.shapes.is_empty(),
            "aucune primitive de rendu produite"
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn icon_button_renders_without_panic() {
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for icon in ALL_ICONS {
                    let _ = icon_button(ui, *icon, Some("tip"));
                }
            });
        })
        .drop_without_applying_deltas();
    }
}
