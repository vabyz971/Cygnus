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

//! Shared UI context: truly-shared dependencies.
//!
//! Neither God-context nor domain state: only the egui context, the
//! theme, the icon registry and the translator. Documents, engines
//! and app commands stay on the apps' side.

use crate::icons::IconRegistry;
use crate::theme::CygnusTheme;
use crate::theme::UiThemeExt;
use cygnus_i18n::Catalog;

/// Dependencies shared by every UI frame.
#[derive(Debug, Clone)]
pub struct UiContext {
    /// egui context (clonable, cheap).
    ctx: egui::Context,
    /// Active theme.
    theme: CygnusTheme,
    /// Icon registry.
    icons: IconRegistry,
    /// Active translator.
    translator: Catalog,
}

impl UiContext {
    /// Assembles the context (dark theme + registry + default catalog
    /// when not provided: see [`UiContext::dark`]).
    pub fn new(
        ctx: egui::Context,
        theme: CygnusTheme,
        icons: IconRegistry,
        translator: Catalog,
    ) -> Self {
        Self {
            ctx,
            theme,
            icons,
            translator,
        }
    }

    /// Standard context: installed theme (dark by default), default
    /// registry and catalog.
    pub fn dark(ctx: egui::Context) -> Self {
        let theme = ctx.cygnus_theme();
        Self::new(ctx, theme, IconRegistry::new(), Catalog::default())
    }

    /// egui context.
    pub fn ctx(&self) -> &egui::Context {
        &self.ctx
    }

    /// Active theme.
    pub fn theme(&self) -> &CygnusTheme {
        &self.theme
    }

    /// Icon registry.
    pub fn icons(&self) -> IconRegistry {
        self.icons
    }

    /// Active translator.
    pub fn translator(&self) -> Catalog {
        self.translator
    }

    /// Switches the translator (language change).
    pub fn set_translator(&mut self, translator: Catalog) {
        self.translator = translator;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cygnus_i18n::{Language, TextKey};

    #[test]
    fn context_exposes_shared_dependencies() {
        let ctx = egui::Context::default();
        let mut ui = UiContext::dark(ctx);
        assert_eq!(ui.theme(), &CygnusTheme::dark());
        assert_eq!(ui.translator().get(TextKey::Save), "Save");
        ui.set_translator(Catalog::new(Language::Fr));
        assert_eq!(ui.translator().get(TextKey::Save), "Enregistrer");
        let _ = ui.icons();
        let _ = ui.ctx();
    }
}
