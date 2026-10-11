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

//! Theme via the egui context: install and read.
//!
//! [`install_theme`] stores the [`CygnusTheme`] in the
//! `egui::Context` (temporary data under a dedicated [`egui::Id`]);
//! [`UiThemeExt::cygnus_theme`] re-reads it from any `Ui` or
//! `Context`, falling back to [`CygnusTheme::dark`] when nothing is
//! installed (tests without installation keep working).
//!
//! Rule: no widget outside `theme/` calls
//! `CygnusTheme::dark()` — it reads `ui.cygnus_theme()` or
//! `ctx.cygnus_theme()`.

use super::theme::CygnusTheme;

/// Dedicated (constant) id of the theme in `ctx.data_mut`.
pub const THEME_DATA_ID: &str = "cygnus_theme";

/// egui id matching [`THEME_DATA_ID`].
fn theme_id() -> egui::Id {
    egui::Id::new(THEME_DATA_ID)
}

/// Installs the theme in the egui context (call at each
/// app's startup, via [`apply_cygnus_theme`](super::visuals::apply_cygnus_theme)).
pub fn install_theme(ctx: &egui::Context, theme: CygnusTheme) {
    ctx.data_mut(|data| {
        data.insert_temp(theme_id(), theme);
    });
}

/// Reads the theme installed in the egui context (dark fallback).
fn installed_or_dark(ctx: &egui::Context) -> CygnusTheme {
    ctx.data(|data| data.get_temp::<CygnusTheme>(theme_id()))
        .unwrap_or_else(CygnusTheme::dark)
}

/// Access to the [`CygnusTheme`] from egui (`Ui` or `Context`).
///
/// Named `cygnus_theme` (not `theme`) because egui 0.36 already provides
/// native `theme()` methods (returning `egui::Theme`):
/// a homonymous trait method would be shadowed by the native
/// method and make the call ambiguous.
pub trait UiThemeExt {
    /// Installed theme, or [`CygnusTheme::dark`] by default.
    fn cygnus_theme(&self) -> CygnusTheme;
}

impl UiThemeExt for egui::Ui {
    /// Reads the parent context's theme.
    fn cygnus_theme(&self) -> CygnusTheme {
        installed_or_dark(self.ctx())
    }
}

impl UiThemeExt for egui::Context {
    /// Reads the theme installed on this context.
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
