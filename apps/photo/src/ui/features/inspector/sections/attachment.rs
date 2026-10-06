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

//! Section Pièce jointe : la sélection secondaire de l'arbre des
//! calques (filtre live ou masque focalisé au clic). Nom, type et
//! interrupteur on/off (bascule sans perdre réglages ni pixels).
//! Paramètres détaillés : futur objectif (édition des réglages).

use super::super::InspectorAction;
use crate::commands::PhotoUiContext;
use crate::ui::features::layers::types::{AttachmentRef, PhotoSubLayerInfo};
use ui_kit::components::Toggle;
use ui_kit::primitives::{Text, divider};

/// Dessine la section et retourne les actions (routées au worker).
pub fn draw_attachment_section(
    ui: &mut egui::Ui,
    ctx: &PhotoUiContext,
    focus: AttachmentRef,
    sub: &PhotoSubLayerInfo,
) -> Vec<InspectorAction> {
    let theme = ctx.shared.theme();
    let texts = crate::i18n::PhotoCatalog::new(ctx.shared.translator().language());
    let mut actions = Vec::new();
    divider(ui, theme);
    Text::heading(theme, &sub.name).show(ui);
    Text::body(
        theme,
        if sub.is_filter {
            texts.get(crate::i18n::PhotoTextKey::FilterLiveLabel)
        } else {
            texts.get(crate::i18n::PhotoTextKey::MaskLabel)
        },
    )
    .show(ui);
    let mut enabled = sub.enabled;
    Toggle::new(if sub.is_filter {
        texts.get(crate::i18n::PhotoTextKey::FilterActive)
    } else {
        texts.get(crate::i18n::PhotoTextKey::MaskActive)
    })
    .show(ui, theme, &mut enabled);
    if enabled != sub.enabled {
        actions.push(InspectorAction::ToggleAttachment {
            owner: focus.owner,
            id: focus.id,
            is_filter: sub.is_filter,
        });
    }
    actions
}
