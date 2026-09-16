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

//! Contexte UI partagé : dépendances réellement communes.
//!
//! Ni GodContext ni état métier : uniquement le contexte egui, le
//! thème, le registre d'icônes et le traducteur. Les documents,
//! moteurs et commandes applicatives restent côté apps.

use crate::i18n::Catalog;
use crate::icons::IconRegistry;
use crate::theme::CygnusTheme;

/// Dépendances communes à toute frame UI.
#[derive(Debug, Clone)]
pub struct UiContext {
    /// Contexte egui (clonable, peu coûteux).
    ctx: egui::Context,
    /// Thème actif.
    theme: CygnusTheme,
    /// Registre d'icônes.
    icons: IconRegistry,
    /// Traducteur actif.
    translator: Catalog,
}

impl UiContext {
    /// Assemble le contexte (thème sombre + registre + catalogue par
    /// défaut si non fournis : voir [`UiContext::dark`]).
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

    /// Contexte standard : thème sombre, registre et catalogue par défaut.
    pub fn dark(ctx: egui::Context) -> Self {
        Self::new(
            ctx,
            CygnusTheme::dark(),
            IconRegistry::new(),
            Catalog::default(),
        )
    }

    /// Contexte egui.
    pub fn ctx(&self) -> &egui::Context {
        &self.ctx
    }

    /// Thème actif.
    pub fn theme(&self) -> &CygnusTheme {
        &self.theme
    }

    /// Registre d'icônes.
    pub fn icons(&self) -> IconRegistry {
        self.icons
    }

    /// Traducteur actif.
    pub fn translator(&self) -> Catalog {
        self.translator
    }

    /// Change le traducteur (changement de langue).
    pub fn set_translator(&mut self, translator: Catalog) {
        self.translator = translator;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, TextKey};

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
