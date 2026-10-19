// Cygnus — Professional open-source creative suite
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

//! Bottom status bar: read-only (no actions).
//!
//! Left: document facts (color mode / depth, size, true zoom) then the
//! status message, or the active tool hint when there is none. Right:
//! application version ("Alpha" prefix). cygnus-ui-only styling
//! (`typography`).

use crate::app::PhotoApp;
use crate::commands::PhotoUiContext;
use crate::state::OpenDocument;
use cygnus_i18n::{PhotoCatalog, PhotoTextKey};
use cygnus_ui::theme::typography::caption_text;

/// Application version (from `Cargo.toml`).
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Document facts: "RGB / 8-bit | 1920 x 1080 px | 45 %" (size once
/// the first preview arrived; the engine is RGBA 8-bit, no profile).
fn document_facts(doc: &OpenDocument, texts: PhotoCatalog) -> String {
    let mut parts = vec![format!(
        "{} / {}",
        texts.get(PhotoTextKey::HistogramRgb),
        texts.get(PhotoTextKey::BitDepth8)
    )];
    if let Some(geom) = doc.ui.preview_geom {
        parts.push(format!(
            "{} \u{d7} {} px",
            geom.doc_size.x as u32, geom.doc_size.y as u32
        ));
    }
    // True zoom: screen pixels per document pixel (the viewport zoom
    // is relative to the fitted preview).
    let zoom = match (doc.ui.last_dest, doc.ui.preview_geom) {
        (Some(dest), Some(geom)) if geom.full_size.x > 0.0 => {
            (dest.width() / geom.full_size.x * 100.0).round()
        }
        _ => (doc.ui.viewport.zoom() * 100.0).round(),
    };
    parts.push(format!("{zoom} %"));
    parts.join("  |  ")
}

/// Bottom bar (emits no action).
pub fn show(ui: &mut egui::Ui, app: &PhotoApp, ctx: &PhotoUiContext) {
    let theme = ctx.shared.theme();
    let translator = ctx.shared.translator();
    let texts = PhotoCatalog::new(translator.language());
    egui::Panel::bottom("photo_status")
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(theme.colors.bg_primary)
                .inner_margin(egui::Margin::symmetric(
                    theme.spacing.md as i8,
                    theme.spacing.xs as i8,
                )),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = theme.spacing.lg;
                match app.active_doc_opt() {
                    Some(doc) => {
                        ui.label(
                            egui::RichText::new(document_facts(doc, texts))
                                .size(theme.typography.caption_size)
                                .color(theme.colors.fg_primary),
                        );
                        let message = doc
                            .ui
                            .status
                            .message(texts)
                            .unwrap_or_else(|| doc.ui.tool.hint(texts).to_owned());
                        ui.label(caption_text(theme, &message));
                    }
                    None => {
                        ui.label(caption_text(theme, texts.get(PhotoTextKey::NoDocumentOpen)));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("Alpha {APP_VERSION}"))
                            .size(theme.typography.caption_size)
                            .color(theme.colors.fg_subdued),
                    );
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facts_carry_mode_size_and_zoom() {
        let mut app = PhotoApp::new();
        app.open_sized_tab(800, 600);
        let doc = app.active_doc_mut_opt().expect("doc actif");
        doc.ui.preview_geom = Some(crate::state::PreviewGeom {
            thumb_size: egui::vec2(800.0, 600.0),
            full_size: egui::vec2(800.0, 600.0),
            origin: egui::Vec2::ZERO,
            doc_size: egui::vec2(800.0, 600.0),
        });
        let facts = document_facts(doc, PhotoCatalog::new(cygnus_i18n::Language::Fr));
        assert!(facts.starts_with("RVB / 8 bits"), "{facts}");
        assert!(facts.contains("800 \u{d7} 600 px"), "{facts}");
        assert!(facts.ends_with('%'), "{facts}");
    }
}
