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

//! PHOTO personas, quick actions and context bar (domain widgets).
//!
//! [`draw_persona_switch`]: icon + label persona tabs (Pixel, Vector,
//! Layout), the active one on an accent pill. [`draw_quick_actions`]:
//! the right side of the persona row (history, view toggles, layout,
//! export). [`draw_photo_modebar`]: the context bar under them — tool
//! switcher then the selected tool's settings (delegated to
//! `super::optionsbar`). The mode restricts the rail tools (paint
//! tools live in Pixel; Vector and Layout expose move, hand, and zoom
//! until their own tools land): switching modes falls back to Move
//! when the current tool does not exist there.

use super::optionsbar::{ToolOptions, draw_tool_options};
use super::toolbar::tool_icon;
use super::viewport::PhotoCanvasTool;
use cygnus_i18n::{Catalog, PhotoCatalog, PhotoTextKey, TextKey};
use cygnus_ui::components::{IconButton, menu_style};
use cygnus_ui::icons::{Icon, IconRegistry};
use cygnus_ui::theme::CygnusTheme;

/// Document edit mode (persona-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoEditMode {
    /// Vector paths (move, hand, zoom for now).
    Vector,
    /// Pixel retouch (all tools).
    #[default]
    Pixel,
    /// Page layout (move, hand, zoom for now).
    Layout,
}

impl PhotoEditMode {
    /// Mode label (ASCII).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Vector => "Vector",
            Self::Pixel => "Pixel",
            Self::Layout => "Layout",
        }
    }

    /// Persona glyph.
    #[must_use]
    pub fn icon(self) -> Icon {
        match self {
            Self::Vector => Icon::Pen,
            Self::Pixel => Icon::Camera,
            Self::Layout => Icon::Stacks,
        }
    }

    /// Every mode, in display order.
    pub const ALL: &[Self] = &[Self::Pixel, Self::Vector, Self::Layout];

    /// Tools available in this mode.
    #[must_use]
    pub fn tools(self) -> &'static [PhotoCanvasTool] {
        match self {
            Self::Pixel => &[
                PhotoCanvasTool::Move,
                PhotoCanvasTool::Select,
                PhotoCanvasTool::Lasso,
                PhotoCanvasTool::Wand,
                PhotoCanvasTool::Brush,
                PhotoCanvasTool::Eraser,
                PhotoCanvasTool::Bucket,
                PhotoCanvasTool::Gradient,
                PhotoCanvasTool::Eyedropper,
                PhotoCanvasTool::Pan,
                PhotoCanvasTool::Zoom,
            ],
            Self::Vector => &[
                PhotoCanvasTool::Move,
                PhotoCanvasTool::Node,
                PhotoCanvasTool::Pan,
                PhotoCanvasTool::Zoom,
            ],
            Self::Layout => &[
                PhotoCanvasTool::Move,
                PhotoCanvasTool::Pan,
                PhotoCanvasTool::Zoom,
            ],
        }
    }

    /// True when the tool is available in this mode.
    #[must_use]
    pub fn supports(self, tool: PhotoCanvasTool) -> bool {
        self.tools().contains(&tool)
    }
}

/// Context bar outcome.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModeBarOutcome {
    /// Tool picked from the switcher, if any.
    pub tool: Option<PhotoCanvasTool>,
}

/// One persona tab: glyph + label, accent pill and dot when active.
fn persona_tab(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    mode: PhotoEditMode,
    active: bool,
) -> egui::Response {
    let (fg, icon_color) = if active {
        (theme.colors.fg_strong, theme.colors.accent)
    } else {
        (theme.colors.fg_secondary, theme.colors.fg_secondary)
    };
    let icon = egui::WidgetText::from(IconRegistry::new().colored(
        mode.icon(),
        theme.typography.icon_size,
        icon_color,
    ))
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    let label = egui::WidgetText::from(
        egui::RichText::new(mode.label())
            .size(theme.typography.body_size)
            .color(fg),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    let pad = theme.spacing.md;
    let gap = theme.spacing.sm;
    let dot = theme.spacing.xxs + 1.0;
    let dot_room = if active { gap + dot * 2.0 } else { 0.0 };
    let width = pad * 2.0 + icon.size().x + gap + label.size().x + dot_room;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, theme.sizes.button_md),
        egui::Sense::click(),
    );
    let fill = if active {
        theme.colors.item_selected
    } else if response.hovered() {
        theme.colors.item_hover
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, theme.radius.md, fill);
    let mut x = rect.left() + pad;
    let icon_size = icon.size();
    painter.galley(
        egui::pos2(x, rect.center().y - icon_size.y / 2.0),
        icon,
        icon_color,
    );
    x += icon_size.x + gap;
    let label_size = label.size();
    painter.galley(
        egui::pos2(x, rect.center().y - label_size.y / 2.0),
        label,
        fg,
    );
    if active {
        x += label_size.x + gap + dot;
        painter.circle_filled(egui::pos2(x, rect.center().y), dot, theme.colors.accent);
    }
    response
}

/// Persona tabs. Returns the newly chosen persona, if any.
pub fn draw_persona_switch(
    ui: &mut egui::Ui,
    mode: &mut PhotoEditMode,
    theme: &CygnusTheme,
) -> Option<PhotoEditMode> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing.xs;
        for candidate in PhotoEditMode::ALL {
            let active = *mode == *candidate;
            if persona_tab(ui, theme, *candidate, active).clicked() && !active {
                *mode = *candidate;
                chosen = Some(*candidate);
            }
        }
    });
    chosen
}

/// State shown by the quick actions (greyed history, lit toggles).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuickActionState {
    /// Undo available.
    pub can_undo: bool,
    /// Redo available.
    pub can_redo: bool,
    /// Canvas grid shown.
    pub show_grid: bool,
    /// Preview clipped to the document.
    pub preview_clip: bool,
}

/// Quick action clicked this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAction {
    /// Undo the last step.
    Undo,
    /// Redo.
    Redo,
    /// Toggle the canvas grid.
    ToggleGrid,
    /// Toggle the out-of-document mask.
    ToggleClip,
    /// Restore the default panel layout.
    ResetLayout,
    /// Open the export settings.
    Export,
}

/// Outlined "Export" button (glyph + label).
fn export_button(ui: &mut egui::Ui, theme: &CygnusTheme, label: &str) -> egui::Response {
    let icon = IconRegistry::new().colored(
        Icon::Export,
        theme.typography.icon_size,
        theme.colors.fg_primary,
    );
    let text = egui::RichText::new(label)
        .size(theme.typography.body_size)
        .color(theme.colors.fg_strong);
    ui.add(
        egui::Button::new((icon, text))
            .fill(theme.colors.bg_primary)
            .stroke(egui::Stroke::new(theme.borders.thin, theme.colors.border))
            .corner_radius(theme.radius.md)
            .min_size(egui::vec2(0.0, theme.sizes.button_md)),
    )
}

/// Right side of the persona row, laid out right-to-left (export
/// outermost). Returns the clicked action, if any.
pub fn draw_quick_actions(
    ui: &mut egui::Ui,
    state: QuickActionState,
    theme: &CygnusTheme,
    catalog: Catalog,
    texts: PhotoCatalog,
) -> Option<QuickAction> {
    let mut clicked = None;
    ui.spacing_mut().item_spacing.x = theme.spacing.xs;
    if export_button(ui, theme, catalog.get(TextKey::Export)).clicked() {
        clicked = Some(QuickAction::Export);
    }
    ui.add_space(theme.spacing.sm);
    if IconButton::new(Icon::Workspace)
        .tooltip(texts.get(PhotoTextKey::ResetLayout))
        .show(ui, theme)
        .clicked()
    {
        clicked = Some(QuickAction::ResetLayout);
    }
    if IconButton::new(Icon::SplitView)
        .tooltip(texts.get(PhotoTextKey::MaskView))
        .selected(state.preview_clip)
        .show(ui, theme)
        .clicked()
    {
        clicked = Some(QuickAction::ToggleClip);
    }
    if IconButton::new(Icon::Grid)
        .tooltip(texts.get(PhotoTextKey::GridLabel))
        .selected(state.show_grid)
        .show(ui, theme)
        .clicked()
    {
        clicked = Some(QuickAction::ToggleGrid);
    }
    ui.separator();
    let redo = ui
        .add_enabled_ui(state.can_redo, |ui| {
            IconButton::new(Icon::Redo)
                .tooltip(catalog.get(TextKey::Redo))
                .show(ui, theme)
        })
        .inner;
    if redo.clicked() {
        clicked = Some(QuickAction::Redo);
    }
    let undo = ui
        .add_enabled_ui(state.can_undo, |ui| {
            IconButton::new(Icon::Undo)
                .tooltip(catalog.get(TextKey::Undo))
                .show(ui, theme)
        })
        .inner;
    if undo.clicked() {
        clicked = Some(QuickAction::Undo);
    }
    clicked
}

/// Tool switcher: active tool glyph + name, popup listing the
/// persona's tools.
fn tool_switcher(
    ui: &mut egui::Ui,
    mode: PhotoEditMode,
    tool: PhotoCanvasTool,
    theme: &CygnusTheme,
    texts: PhotoCatalog,
) -> Option<PhotoCanvasTool> {
    let registry = IconRegistry::new();
    let (icon, name) = tool_icon(tool, texts);
    let response = ui
        .add(
            egui::Button::new((
                registry.colored(icon, theme.typography.icon_size, theme.colors.accent),
                egui::RichText::new(name)
                    .size(theme.typography.body_size)
                    .color(theme.colors.fg_strong),
                registry.colored(
                    Icon::ExpandMore,
                    theme.typography.icon_size,
                    theme.colors.fg_secondary,
                ),
            ))
            .frame(true)
            .frame_when_inactive(false)
            .min_size(egui::vec2(0.0, theme.sizes.input_height)),
        )
        .on_hover_text(texts.get(PhotoTextKey::ToolLabel));
    let mut picked = None;
    egui::Popup::menu(&response)
        .style(menu_style(theme))
        .show(|ui| {
            for candidate in mode.tools() {
                let (icon, name) = tool_icon(*candidate, texts);
                let color = if *candidate == tool {
                    theme.colors.accent
                } else {
                    theme.colors.fg_primary
                };
                if ui
                    .add(
                        egui::Button::new((
                            registry.colored(icon, theme.typography.icon_size, color),
                            egui::RichText::new(name).size(theme.typography.body_size),
                        ))
                        .frame_when_inactive(false),
                    )
                    .clicked()
                {
                    picked = Some(*candidate);
                    ui.close();
                }
            }
        });
    picked
}

/// Context bar: tool switcher, then the selected tool's settings.
pub fn draw_photo_modebar(
    ui: &mut egui::Ui,
    mode: PhotoEditMode,
    tool: PhotoCanvasTool,
    options: ToolOptions<'_>,
    theme: &CygnusTheme,
    texts: PhotoCatalog,
) -> ModeBarOutcome {
    let mut outcome = ModeBarOutcome::default();
    ui.horizontal(|ui| {
        outcome.tool = tool_switcher(ui, mode, tool, theme, texts);
        ui.separator();
        draw_tool_options(ui, tool, options, theme, texts);
    });
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::PhotoBrushSettings;

    #[test]
    fn pixel_mode_supports_every_tool() {
        for tool in [
            PhotoCanvasTool::Move,
            PhotoCanvasTool::Select,
            PhotoCanvasTool::Pan,
            PhotoCanvasTool::Zoom,
            PhotoCanvasTool::Brush,
            PhotoCanvasTool::Eraser,
            PhotoCanvasTool::Eyedropper,
        ] {
            assert!(PhotoEditMode::Pixel.supports(tool));
        }
    }

    #[test]
    fn vector_and_layout_are_navigation_only() {
        for mode in [PhotoEditMode::Vector, PhotoEditMode::Layout] {
            assert!(mode.supports(PhotoCanvasTool::Move));
            assert!(!mode.supports(PhotoCanvasTool::Brush));
            assert!(!mode.supports(PhotoCanvasTool::Eraser));
        }
    }

    #[test]
    fn modebar_renders_without_panic_and_idle() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let theme = cygnus_ui::theme::CygnusTheme::dark();
        let language = cygnus_i18n::Language::Fr;
        let texts = PhotoCatalog::new(language);
        let catalog = Catalog::new(language);
        let mut mode = PhotoEditMode::Pixel;
        let mut brush = PhotoBrushSettings::default();
        let mut kind = photo_engine::SelectionKind::default();
        let mut wand = crate::state::WandSettings::default();
        let mut bucket = crate::state::WandSettings::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert_eq!(draw_persona_switch(ui, &mut mode, &theme), None);
                let quick = ui
                    .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_quick_actions(ui, QuickActionState::default(), &theme, catalog, texts)
                    })
                    .inner;
                assert_eq!(quick, None);
                let outcome = draw_photo_modebar(
                    ui,
                    mode,
                    PhotoCanvasTool::Brush,
                    ToolOptions {
                        brush: &mut brush,
                        selection_kind: &mut kind,
                        wand: &mut wand,
                        bucket: &mut bucket,
                    },
                    &theme,
                    texts,
                );
                assert_eq!(outcome, ModeBarOutcome::default());
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(mode, PhotoEditMode::Pixel);
    }
}
