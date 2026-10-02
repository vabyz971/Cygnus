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

//! Relais historique vers [`crate::icons`].
//!
//! La seule API publique pour les apps est désormais
//! [`crate::icons::{Icon, IconRegistry}`]. Ce module est conservé
//! temporairement (suppression prévue en phase 2 avec `widgets/`) et
//! réexporte l'API canonique sous les anciens noms.

pub use crate::icons::ALL_ICONS;
pub use crate::icons::Icon as CygnusIcon;
pub use crate::icons::icon_button;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::setup_fonts;

    #[test]
    fn all_icons_have_glyph() {
        let registry = crate::icons::IconRegistry::new();
        for icon in ALL_ICONS {
            let _ = registry.text(*icon);
        }
    }

    #[test]
    fn all_icons_list_is_exhaustive() {
        // Relais du test canonique de `crate::icons` : 47 variantes.
        assert_eq!(ALL_ICONS.len(), 47);
    }

    #[test]
    fn icons_render_without_panic() {
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        let registry = crate::icons::IconRegistry::new();
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
