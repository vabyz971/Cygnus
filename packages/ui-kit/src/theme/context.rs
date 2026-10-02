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

//! Thème via le contexte egui : installation et lecture.
//!
//! [`install_theme`] stocke le [`CygnusTheme`] dans le
//! `egui::Context` (donnée temporaire sous un [`egui::Id`] dédié) ;
//! [`UiThemeExt::cygnus_theme`] le relit depuis n'importe quel `Ui` ou
//! `Context`, en retombant sur [`CygnusTheme::dark`] si rien n'est
//! installé (les tests sans installation continuent de marcher).
//!
//! Règle : aucun widget hors `theme/` n'appelle
//! `CygnusTheme::dark()` — il lit `ui.cygnus_theme()` ou
//! `ctx.cygnus_theme()`.

use super::theme::CygnusTheme;

/// Identifiant dédié (et constant) du thème dans `ctx.data_mut`.
pub const THEME_DATA_ID: &str = "cygnus_theme";

/// Id egui correspondant à [`THEME_DATA_ID`].
fn theme_id() -> egui::Id {
    egui::Id::new(THEME_DATA_ID)
}

/// Installe le thème dans le contexte egui (à appeler au démarrage
/// de chaque app, via [`apply_cygnus_theme`](super::visuals::apply_cygnus_theme)).
pub fn install_theme(ctx: &egui::Context, theme: CygnusTheme) {
    ctx.data_mut(|data| {
        data.insert_temp(theme_id(), theme);
    });
}

/// Lit le thème installé dans le contexte egui (repli sombre).
fn installed_or_dark(ctx: &egui::Context) -> CygnusTheme {
    ctx.data(|data| data.get_temp::<CygnusTheme>(theme_id()))
        .unwrap_or_else(CygnusTheme::dark)
}

/// Accès au [`CygnusTheme`] depuis egui (`Ui` ou `Context`).
///
/// Nommé `cygnus_theme` (et non `theme`) car egui 0.36 fournit déjà
/// des méthodes natives `theme()` (retournant `egui::Theme`) :
/// une méthode de trait homonyme serait masquée par la méthode
/// native et rendrait l'appel ambigu.
pub trait UiThemeExt {
    /// Thème installé, ou [`CygnusTheme::dark`] par défaut.
    fn cygnus_theme(&self) -> CygnusTheme;
}

impl UiThemeExt for egui::Ui {
    /// Lit le thème du contexte parent.
    fn cygnus_theme(&self) -> CygnusTheme {
        installed_or_dark(self.ctx())
    }
}

impl UiThemeExt for egui::Context {
    /// Lit le thème installé sur ce contexte.
    fn cygnus_theme(&self) -> CygnusTheme {
        installed_or_dark(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_is_dark_without_install() {
        let ctx = egui::Context::default();
        assert_eq!(UiThemeExt::cygnus_theme(&ctx), CygnusTheme::dark());
        ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(UiThemeExt::cygnus_theme(ui), CygnusTheme::dark());
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn installed_theme_is_read_back() {
        let ctx = egui::Context::default();
        let theme = CygnusTheme::dark();
        install_theme(&ctx, theme);
        assert_eq!(UiThemeExt::cygnus_theme(&ctx), theme);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(UiThemeExt::cygnus_theme(ui), theme);
        })
        .drop_without_applying_deltas();
    }
}
