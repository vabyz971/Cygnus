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

//! Photo dockable tiles via `egui_tiles` (rerun engine).
//!
//! ```text
//! PhotoWorkspace (top/bottom fixed)
//!   ↓ CentralPanel (Tree)
//! PhotoDockTab (Tools / Canvas(id) / Inspector / Layers+History)
//!   ↓
//! Domain contents (left_sidebar / central_view / right_sidebar)
//! ```
//!
//! The top (title, personas, context) and bottom (status) bars stay
//! fixed `egui::Panel`s. Layout, left to right: the tool rail (bare
//! pane, pinned width), the document tabs, then the side column —
//! Histogram and Adjustments (bare panes drawing their own section
//! header) above the Layers / History tabs. Bare panes have no tab
//! bar: their header overflow menu hides them, the Window menu shows
//! them again ([`PhotoAction::ShowDockTab`](crate::commands::PhotoAction)).
//! Each open document has its own closable canvas tab (glyph + name,
//! accent top edge when active): activating it activates the document,
//! closing it closes the document (back to welcome if it was the last
//! one). Styling inherits from egui (hence the Cygnus theme): no
//! hardcoded color here.

use super::{central_view, left_sidebar, right_sidebar};
use crate::app::{PhotoApp, remove_document};
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::state::OpenDocument;
use app_shell::dock::DockTab;
use app_shell::dock::set_linear_shares;
use cygnus_i18n::TextKey;
use cygnus_ui::containers::{DockResizeState, DockStyle, DockTabState};
use cygnus_ui::icons::{Icon, IconRegistry};
use cygnus_ui::theme::CygnusTheme;
use egui_tiles::{Behavior, TileId, Tiles, Tree, UiResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Generic dock mechanics (lookup, canvas tabs,
/// reconciliation): see `app-shell`.
pub use app_shell::dock::{ensure_tab, push_canvas_tab, reconcile_canvases, remove_canvas_tab};

/// Dockable Photo workspace panel (stable across versions:
/// serialized into preferences, never rename without
/// migration — like [`cygnus_ui::layout::PanelId`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PhotoDockTab {
    /// Compact tool rail.
    Tools,
    /// Canvas of document `Uuid` (title = name + zoom).
    Canvas(Uuid),
    /// Inspector of the selected layer.
    Inspector,
    /// Layer list.
    Layers,
    /// Undo/redo step history (tabbed with Layers).
    History,
    /// Tone histogram of the composite (O006, tabbed with Layers).
    Histogram,
}

impl PhotoDockTab {
    /// Title translation key (except canvas: name + zoom).
    pub fn title_key(self) -> TextKey {
        match self {
            Self::Tools => TextKey::Tools,
            Self::Canvas(_) => TextKey::Canvas,
            Self::Inspector => TextKey::Inspector,
            Self::Layers => TextKey::Layers,
            Self::History => TextKey::History,
            Self::Histogram => TextKey::Histogram,
        }
    }
}

impl DockTab for PhotoDockTab {
    fn canvas_id(&self) -> Option<Uuid> {
        match self {
            Self::Canvas(id) => Some(*id),
            _ => None,
        }
    }

    fn canvas(id: Uuid) -> Self {
        Self::Canvas(id)
    }
}

/// Width shares of the root row: rail (re-pinned to a fixed pixel
/// width every frame, see [`pin_tools_width`]), canvases, side column.
const SHARE_TOOLS: f32 = 1.0;
/// Canvas share of the root row.
const SHARE_CANVAS: f32 = 23.0;
/// Side column share of the root row (~1/5 of the window).
const SHARE_SIDE: f32 = 6.0;

/// Default photo layout: tool rail on the left, document tabs in the
/// center, and the side column — Histogram, Adjustments (inspector),
/// then Layers tabbed with History.
pub fn default_tree(canvases: Vec<PhotoDockTab>) -> Tree<PhotoDockTab> {
    debug_assert!(
        canvases.iter().all(|tab| tab.canvas_id().is_some()),
        "la racine ne porte que des canevas"
    );
    let mut tiles = Tiles::default();
    let tools = tiles.insert_pane(PhotoDockTab::Tools);
    let canvas_ids: Vec<TileId> = canvases
        .into_iter()
        .map(|tab| tiles.insert_pane(tab))
        .collect();
    let canvas_tabs = tiles.insert_tab_tile(canvas_ids);
    let histogram = tiles.insert_pane(PhotoDockTab::Histogram);
    let inspector = tiles.insert_pane(PhotoDockTab::Inspector);
    let layers = tiles.insert_pane(PhotoDockTab::Layers);
    let history = tiles.insert_pane(PhotoDockTab::History);
    let layer_tabs = tiles.insert_tab_tile(vec![layers, history]);
    let side = tiles.insert_vertical_tile(vec![histogram, inspector, layer_tabs]);
    let root = tiles.insert_horizontal_tile(vec![tools, canvas_tabs, side]);
    set_linear_shares(
        &mut tiles,
        root,
        &[
            (tools, SHARE_TOOLS),
            (canvas_tabs, SHARE_CANVAS),
            (side, SHARE_SIDE),
        ],
    );
    set_linear_shares(
        &mut tiles,
        side,
        &[(histogram, 2.1), (inspector, 3.4), (layer_tabs, 4.5)],
    );
    Tree::new("cygnus-tree", root, tiles)
}

/// Rail width in points: one tool button plus side margins.
fn tools_width(theme: &CygnusTheme) -> f32 {
    theme.sizes.tool_button + theme.spacing.md * 2.0 - theme.spacing.xxs
}

/// Keeps the tool rail at a fixed pixel width inside its row (shares
/// are proportional, so the rail would otherwise grow and shrink with
/// the window). No-op when the rail was docked elsewhere.
fn pin_tools_width(tree: &mut Tree<PhotoDockTab>, total_width: f32, rail: f32) {
    let Some(tools) = tree.tiles.find_pane(&PhotoDockTab::Tools) else {
        return;
    };
    let Some(parent) = tree.tiles.parent_of(tools) else {
        return;
    };
    let Some(egui_tiles::Tile::Container(egui_tiles::Container::Linear(linear))) =
        tree.tiles.get_mut(parent)
    else {
        return;
    };
    if linear.dir != egui_tiles::LinearDir::Horizontal || total_width <= rail {
        return;
    }
    let others: f32 = linear
        .children
        .iter()
        .filter(|child| **child != tools)
        .map(|child| linear.shares[*child])
        .sum();
    let fraction = rail / total_width;
    linear
        .shares
        .set_share(tools, others * fraction / (1.0 - fraction));
}

/// Histogram tabbed next to History (O006).
///
/// Idempotent like [`ensure_history_tabbed`]: trees restored from an
/// earlier JSON gain the tab; an existing one is left where the user
/// put it. Unexpected container around History: nothing touched.
pub fn ensure_histogram_tabbed(tree: &mut Tree<PhotoDockTab>) {
    if app_shell::dock::has_tab(tree, PhotoDockTab::Histogram) {
        return;
    }
    let Some(history) = tree.tiles.find_pane(&PhotoDockTab::History) else {
        return;
    };
    let Some(parent) = tree.tiles.parent_of(history) else {
        return;
    };
    if !matches!(
        tree.tiles.get(parent),
        Some(egui_tiles::Tile::Container(egui_tiles::Container::Tabs(_)))
    ) {
        return;
    }
    let histogram = tree.tiles.insert_pane(PhotoDockTab::Histogram);
    if let Some(egui_tiles::Tile::Container(egui_tiles::Container::Tabs(tabs))) =
        tree.tiles.get_mut(parent)
    {
        tabs.add_child(histogram);
    }
}

/// History tabbed with Layers (bottom of the right column).
///
/// Idempotent: no effect if the tab already exists (trees restored
/// from an earlier JSON: see `load_tree_or_default`).
pub fn ensure_history_tabbed(tree: &mut Tree<PhotoDockTab>) {
    if app_shell::dock::has_tab(tree, PhotoDockTab::History) {
        return;
    }
    let Some(layers_tile) = tree.tiles.find_pane(&PhotoDockTab::Layers) else {
        return;
    };
    let Some(parent) = tree.tiles.parent_of(layers_tile) else {
        return;
    };
    // Position read BEFORE any mutation: on an unexpected container,
    // touch nothing (never an orphaned tile).
    let position = match tree.tiles.get(parent) {
        Some(egui_tiles::Tile::Container(egui_tiles::Container::Linear(linear))) => linear
            .children
            .iter()
            .position(|child| *child == layers_tile),
        _ => None,
    };
    let Some(position) = position else {
        return;
    };
    let history = tree.tiles.insert_pane(PhotoDockTab::History);
    let tabs = tree.tiles.insert_tab_tile(vec![layers_tile, history]);
    if let Some(egui_tiles::Tile::Container(egui_tiles::Container::Linear(linear))) =
        tree.tiles.get_mut(parent)
    {
        linear.children[position] = tabs;
    }
}

/// Draws the tree in the central panel and returns the panels'
/// domain actions.
///
/// Disjoint borrows (`docs` / `active` vs `shell.tree`): the
/// behavior never takes the whole `PhotoApp`, otherwise `Tree::ui`
/// (which borrows the tree) would refuse to compile.
pub fn show_tree(ui: &mut egui::Ui, app: &mut PhotoApp, ctx: &PhotoUiContext) -> Vec<PhotoAction> {
    pin_tools_width(
        &mut app.shell.tree,
        ui.available_width(),
        tools_width(ctx.shared.theme()),
    );
    let mut behavior = PhotoTreeBehavior {
        docs: &mut app.docs,
        active: &mut app.active,
        filter_types: &app.runtime.filter_types,
        ctx,
        actions: Vec::new(),
        active_canvas_tabs: Default::default(),
    };
    app.shell.tree.ui(&mut behavior, ui);
    behavior.actions
}

/// egui_tiles tab state → the dock style's own (cygnus-ui stays free
/// of the tiling crate).
fn dock_tab_state(state: &egui_tiles::TabState) -> DockTabState {
    DockTabState {
        active: state.active,
        dragged: state.is_being_dragged,
    }
}

/// Side-panel tile: raised surface (`bg_secondary`) with inner `md`
/// horizontal / `xs` vertical padding; styling stays 100% theme tokens.
fn padded_tile<R>(
    ui: &mut egui::Ui,
    ctx: &PhotoUiContext,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let theme = ctx.shared.theme();
    ui.painter()
        .rect_filled(ui.max_rect(), 0.0, theme.colors.bg_secondary);
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(
            theme.spacing.md as i8,
            theme.spacing.xs as i8,
        ))
        .show(ui, add_contents)
        .inner
}

/// Tab title made of a glyph then a label (one galley, so the tab
/// text color applies to both).
fn glyph_title(
    theme: &CygnusTheme,
    icon: Icon,
    label: &str,
    suffix: Option<String>,
) -> egui::WidgetText {
    let style = egui::Style::default();
    let mut job = egui::text::LayoutJob::default();
    IconRegistry::new()
        .sized(icon, theme.typography.icon_size)
        .append_to(
            &mut job,
            &style,
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    egui::RichText::new(format!("  {label}"))
        .size(theme.typography.body_size)
        .append_to(
            &mut job,
            &style,
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    if let Some(suffix) = suffix {
        egui::RichText::new(format!("  {suffix}"))
            .size(theme.typography.body_size)
            .color(theme.colors.fg_subdued)
            .append_to(
                &mut job,
                &style,
                egui::FontSelection::Default,
                egui::Align::Center,
            );
    }
    job.into()
}

/// Plain (glyph-less) tab title with an optional subdued suffix
/// ("Layers  4").
fn counted_title(theme: &CygnusTheme, label: &str, suffix: Option<String>) -> egui::WidgetText {
    let Some(suffix) = suffix else {
        return label.into();
    };
    let style = egui::Style::default();
    let mut job = egui::text::LayoutJob::default();
    egui::RichText::new(label)
        .size(theme.typography.body_size)
        .append_to(
            &mut job,
            &style,
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    egui::RichText::new(format!("  {suffix}"))
        .size(theme.typography.body_size)
        .color(theme.colors.fg_subdued)
        .append_to(
            &mut job,
            &style,
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    job.into()
}

/// True when `tile` holds a canvas pane.
fn is_canvas_tile(tiles: &Tiles<PhotoDockTab>, tile: TileId) -> bool {
    matches!(
        tiles.get(tile),
        Some(egui_tiles::Tile::Pane(PhotoDockTab::Canvas(_)))
    )
}

/// `egui_tiles` behavior: translated titles + domain contents.
///
/// Owns only the documents (never the tree) to honor the
/// borrow split of [`show_tree`]. Tiles are only
/// drawn with at least one document (the workspace shows
/// the welcome screen otherwise); defensive arms avoid any panic.
struct PhotoTreeBehavior<'a> {
    /// Open documents (non-empty when the tree is drawn).
    docs: &'a mut Vec<OpenDocument>,
    /// Active document index.
    active: &'a mut usize,
    /// Filter catalog (engine registry: name, `type_id`) for
    /// the Layers panel context menu.
    filter_types: &'a [(String, String)],
    /// Display dependencies (theme, translation).
    ctx: &'a PhotoUiContext,
    /// Domain actions collected during the frame.
    actions: Vec<PhotoAction>,
    /// Canvas tabs painted active this frame (see `tab_bg_color`).
    active_canvas_tabs: std::cell::RefCell<Vec<TileId>>,
}

impl Behavior<PhotoDockTab> for PhotoTreeBehavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &PhotoDockTab) -> egui::WidgetText {
        match *pane {
            // Narrow rail: drag icon instead of text (DnD
            // goes through the tab bar, which must be kept).
            PhotoDockTab::Tools => IconRegistry::new()
                .sized(
                    Icon::DragHandle,
                    self.ctx.shared.theme().typography.body_size,
                )
                .into(),
            // Document title: glyph + name (resolution, depth and
            // zoom live in the status bar, never on the canvas).
            PhotoDockTab::Canvas(id) => {
                let theme = self.ctx.shared.theme();
                let title = self
                    .docs
                    .iter()
                    .find(|doc| doc.id == id)
                    .map(|doc| doc.title.clone())
                    .unwrap_or_else(|| {
                        self.ctx
                            .shared
                            .translator()
                            .get(TextKey::Canvas)
                            .to_string()
                    });
                glyph_title(theme, Icon::ImageIcon, &title, None)
            }
            // Layers: title + top-level count of the active document.
            PhotoDockTab::Layers => {
                let count = self
                    .docs
                    .get(*self.active)
                    .map(|doc| doc.ui.layers.len())
                    .filter(|count| *count > 0)
                    .map(|count| count.to_string());
                counted_title(
                    self.ctx.shared.theme(),
                    self.ctx.shared.translator().get(pane.title_key()),
                    count,
                )
            }
            _ => self.ctx.shared.translator().get(pane.title_key()).into(),
        }
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: TileId,
        pane: &mut PhotoDockTab,
    ) -> UiResponse {
        // Explicit disjoint borrows: `ctx` is copied (`Copy`
        // shared reference) before the mutable document borrow.
        let ctx: &PhotoUiContext = self.ctx;
        let translator = ctx.shared.translator();
        let texts = cygnus_i18n::PhotoCatalog::new(translator.language());
        match *pane {
            PhotoDockTab::Tools => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let Some(doc) = self.docs.get_mut(index) else {
                    ui.label(texts.get(cygnus_i18n::PhotoTextKey::NoDocument));
                    return UiResponse::None;
                };
                let translator = ctx.shared.translator();
                let texts = cygnus_i18n::PhotoCatalog::new(translator.language());
                self.actions.extend(left_sidebar::draw_tools_content(
                    ui,
                    doc,
                    ctx.shared.theme(),
                    texts,
                ));
            }
            PhotoDockTab::Canvas(id) => {
                // Activating the tab activates the document.
                if let Some(index) = self.docs.iter().position(|doc| doc.id == id) {
                    *self.active = index;
                    let doc: &mut OpenDocument = &mut self.docs[index];
                    self.actions
                        .extend(central_view::draw_canvas_content(ui, doc, ctx));
                } else {
                    ui.label(ctx.shared.translator().get(TextKey::Close));
                }
            }
            PhotoDockTab::Inspector => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let Some(doc) = self.docs.get(index) else {
                    ui.label(texts.get(cygnus_i18n::PhotoTextKey::NoDocument));
                    return UiResponse::None;
                };
                let inner = padded_tile(ui, ctx, |ui| {
                    right_sidebar::draw_inspector_content(ui, doc, ctx)
                });
                self.actions.extend(inner);
            }
            PhotoDockTab::Layers => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let Some(doc) = self.docs.get_mut(index) else {
                    ui.label(texts.get(cygnus_i18n::PhotoTextKey::NoDocument));
                    return UiResponse::None;
                };
                let inner = padded_tile(ui, ctx, |ui| {
                    right_sidebar::draw_layers_content(ui, doc, ctx, self.filter_types)
                });
                self.actions.extend(inner);
            }
            PhotoDockTab::History => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let Some(doc) = self.docs.get(index) else {
                    ui.label(texts.get(cygnus_i18n::PhotoTextKey::NoDocument));
                    return UiResponse::None;
                };
                let inner = padded_tile(ui, ctx, |ui| {
                    right_sidebar::draw_history_content(ui, doc, ctx)
                });
                self.actions.extend(inner);
            }
            PhotoDockTab::Histogram => {
                let index = (*self.active).min(self.docs.len().saturating_sub(1));
                let Some(doc) = self.docs.get(index) else {
                    ui.label(texts.get(cygnus_i18n::PhotoTextKey::NoDocument));
                    return UiResponse::None;
                };
                let inner = padded_tile(ui, ctx, |ui| {
                    right_sidebar::draw_histogram_content(ui, doc, ctx)
                });
                self.actions.extend(inner);
            }
        }
        UiResponse::None
    }

    /// Only canvas tabs carry a cross (it closes the document); side
    /// panels hide through their header overflow menu instead.
    fn is_tab_closable(&self, tiles: &Tiles<PhotoDockTab>, tile_id: TileId) -> bool {
        is_canvas_tile(tiles, tile_id)
    }

    fn on_tab_close(&mut self, tiles: &mut Tiles<PhotoDockTab>, tile_id: TileId) -> bool {
        // Canvas: close the DOCUMENT (removed from docs, worker stopped
        // when the channels drop) and destroy the tile (`true`).
        // Zero documents left = back to welcome (the workspace
        // only shows the tree while a document remains).
        if let Some(egui_tiles::Tile::Pane(PhotoDockTab::Canvas(id))) = tiles.get(tile_id) {
            let id = *id;
            remove_document(self.docs, self.active, id);
            return true;
        }
        // Other panel: hide instead of destroying to
        // keep its slot in the layout (`false` = the tile stays).
        tiles.set_visible(tile_id, false);
        false
    }

    // --- Rerun-style chrome, all from theme tokens (`DockStyle`) ---

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        DockStyle::new(self.ctx.shared.theme()).tab_bar_height()
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        DockStyle::new(self.ctx.shared.theme()).gap_width()
    }

    fn tab_title_spacing(&self, _visuals: &egui::Visuals) -> f32 {
        DockStyle::new(self.ctx.shared.theme()).tab_title_spacing()
    }

    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> egui::Color32 {
        DockStyle::new(self.ctx.shared.theme()).tab_bar_color()
    }

    /// The active document tab takes the canvas surface (it merges
    /// with the canvas below); side panel tabs stay text-only.
    fn tab_bg_color(
        &self,
        _visuals: &egui::Visuals,
        tiles: &Tiles<PhotoDockTab>,
        tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> egui::Color32 {
        let theme = self.ctx.shared.theme();
        if state.is_being_dragged {
            return DockStyle::new(theme).tab_bg_color(dock_tab_state(state));
        }
        if state.active && is_canvas_tile(tiles, tile_id) {
            // Remembered for `on_tab_button` (accent edge), which
            // gets no tab state.
            self.active_canvas_tabs.borrow_mut().push(tile_id);
            theme.colors.bg_canvas
        } else {
            egui::Color32::TRANSPARENT
        }
    }

    /// Accent top edge on the active document tab.
    fn on_tab_button(
        &mut self,
        tiles: &mut Tiles<PhotoDockTab>,
        tile_id: TileId,
        button_response: egui::Response,
    ) -> egui::Response {
        if is_canvas_tile(tiles, tile_id) && self.active_canvas_tabs.borrow().contains(&tile_id) {
            let theme = self.ctx.shared.theme();
            let rect = button_response.rect;
            let width = theme.borders.medium;
            button_response
                .ctx
                .layer_painter(button_response.layer_id)
                .hline(
                    rect.x_range(),
                    rect.top() + width / 2.0,
                    egui::Stroke::new(width, theme.colors.accent),
                );
        }
        button_response
    }

    /// Overflow menu at the right of side tab bars (hides the active
    /// panel); document tab bars get nothing.
    fn top_bar_right_ui(
        &mut self,
        tiles: &Tiles<PhotoDockTab>,
        ui: &mut egui::Ui,
        _tile_id: TileId,
        tabs: &egui_tiles::Tabs,
        _scroll_offset: &mut f32,
    ) {
        let Some(active) = tabs.active else {
            return;
        };
        let Some(egui_tiles::Tile::Pane(pane)) = tiles.get(active) else {
            return;
        };
        if pane.canvas_id().is_some() {
            return;
        }
        ui.add_space(self.ctx.shared.theme().spacing.sm);
        right_sidebar::panel_menu(ui, self.ctx, *pane, &mut self.actions);
    }

    fn tab_outline_stroke(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &Tiles<PhotoDockTab>,
        _tile_id: TileId,
        _state: &egui_tiles::TabState,
    ) -> egui::Stroke {
        egui::Stroke::NONE
    }

    fn tab_bar_hline_stroke(&self, _visuals: &egui::Visuals) -> egui::Stroke {
        DockStyle::new(self.ctx.shared.theme()).tab_bar_hline_stroke()
    }

    fn tab_text_color(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &Tiles<PhotoDockTab>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> egui::Color32 {
        DockStyle::new(self.ctx.shared.theme()).tab_text_color(dock_tab_state(state))
    }

    fn resize_stroke(
        &self,
        _style: &egui::Style,
        resize_state: egui_tiles::ResizeState,
    ) -> egui::Stroke {
        let state = match resize_state {
            egui_tiles::ResizeState::Idle => DockResizeState::Idle,
            egui_tiles::ResizeState::Hovering => DockResizeState::Hovering,
            egui_tiles::ResizeState::Dragging => DockResizeState::Dragging,
        };
        DockStyle::new(self.ctx.shared.theme()).resize_stroke(state)
    }

    fn drag_preview_color(&self, _visuals: &egui::Visuals) -> egui::Color32 {
        DockStyle::new(self.ctx.shared.theme()).drag_preview_color()
    }

    fn drag_preview_stroke(&self, _visuals: &egui::Visuals) -> egui::Stroke {
        DockStyle::new(self.ctx.shared.theme()).drag_preview_stroke()
    }

    /// Bare panes keep no tab bar (rail, histogram, adjustments draw
    /// their own header) while a lone document keeps its tab: single
    /// child tab containers are never pruned.
    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: false,
            prune_single_child_tabs: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_shell::dock::has_tab;
    use egui_tiles::Container;

    /// Finds the `TileId` of panel `tab`, if any.
    fn find_tile(tree: &Tree<PhotoDockTab>, tab: PhotoDockTab) -> Option<TileId> {
        tree.tiles.find_pane(&tab)
    }

    /// Fixed test panels.
    const FIXED_TABS: [PhotoDockTab; 5] = [
        PhotoDockTab::Tools,
        PhotoDockTab::Inspector,
        PhotoDockTab::Layers,
        PhotoDockTab::History,
        PhotoDockTab::Histogram,
    ];

    fn canvas(id: u128) -> PhotoDockTab {
        PhotoDockTab::Canvas(Uuid::from_u128(id))
    }

    #[test]
    fn default_layout_contains_tools_inspector_layers_and_canvases() {
        let tree = default_tree(vec![canvas(1), canvas(2)]);
        for tab in FIXED_TABS {
            assert!(has_tab(&tree, tab), "panneau manquant : {tab:?}");
        }
        assert!(has_tab(&tree, canvas(1)));
        assert!(has_tab(&tree, canvas(2)));
    }

    #[test]
    fn inspector_and_layers_share_the_right_column() {
        let tree = default_tree(vec![canvas(1)]);
        let inspector = find_tile(&tree, PhotoDockTab::Inspector).expect("inspecteur présent");
        let layers = find_tile(&tree, PhotoDockTab::Layers).expect("calques présents");
        let history = find_tile(&tree, PhotoDockTab::History).expect("historique présent");
        // Layers + History tabbed: same parent tile.
        let layers_parent = tree.tiles.parent_of(layers).expect("parent calques");
        let history_parent = tree.tiles.parent_of(history).expect("parent historique");
        assert_eq!(
            layers_parent, history_parent,
            "calques + historique en onglets"
        );
        assert!(
            matches!(
                tree.tiles.get(layers_parent),
                Some(egui_tiles::Tile::Container(Container::Tabs(_)))
            ),
            "conteneur d'onglets attendu"
        );
        // The tab shares the vertical right column with the inspector.
        let tabs_parent = tree.tiles.parent_of(layers_parent).expect("parent onglets");
        let inspector_parent = tree.tiles.parent_of(inspector).expect("parent inspecteur");
        assert_eq!(tabs_parent, inspector_parent, "même colonne droite");
        assert!(matches!(
            tree.tiles.get(tabs_parent),
            Some(egui_tiles::Tile::Container(Container::Linear(linear)))
                if linear.dir == egui_tiles::LinearDir::Vertical
        ));
    }

    #[test]
    fn tools_column_is_narrow() {
        let tree = default_tree(vec![canvas(1)]);
        let tools = find_tile(&tree, PhotoDockTab::Tools).expect("outils présents");
        let root = tree.root().expect("racine présente");
        let Some(egui_tiles::Tile::Container(Container::Linear(linear))) = tree.tiles.get(root)
        else {
            panic!("racine horizontale attendue");
        };
        let total: f32 = linear.shares.iter().map(|(_, share)| *share).sum();
        let tools_share = linear.shares[tools];
        assert!(
            tools_share / total < 0.1,
            "outils compacts : part {tools_share}/{total}"
        );
    }

    #[test]
    fn push_canvas_tab_groups_canvases_in_one_tabs_container() {
        let mut tree = default_tree(vec![canvas(1)]);
        push_canvas_tab(&mut tree, canvas(2));
        let first = find_tile(&tree, canvas(1)).expect("canevas 1 présent");
        let second = find_tile(&tree, canvas(2)).expect("canevas 2 présent");
        assert_eq!(
            tree.tiles.parent_of(first),
            tree.tiles.parent_of(second),
            "canevas en onglets côte à côte"
        );
    }

    #[test]
    fn closing_panel_hides_it_and_menu_reopens_it() {
        let mut tree = default_tree(vec![canvas(1)]);
        let inspector = find_tile(&tree, PhotoDockTab::Inspector).expect("inspecteur présent");
        // The cross hides without destroying (slot kept).
        tree.set_visible(inspector, false);
        assert!(has_tab(&tree, PhotoDockTab::Inspector));
        assert!(!tree.is_visible(inspector));
        ensure_tab(&mut tree, PhotoDockTab::Inspector);
        assert!(tree.is_visible(inspector));
    }

    #[test]
    fn only_canvas_tabs_are_closable() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        let behavior = PhotoTreeBehavior {
            docs: &mut app.docs,
            active: &mut app.active,
            filter_types: &[],
            ctx: &photo_ctx,
            actions: Vec::new(),
            active_canvas_tabs: Default::default(),
        };
        let tree = default_tree(vec![canvas(1)]);
        for tab in FIXED_TABS {
            let tile = find_tile(&tree, tab).expect("panneau présent");
            assert!(
                !behavior.is_tab_closable(&tree.tiles, tile),
                "{tab:?} masqué par son menu, sans croix"
            );
        }
        let canvas_tile = find_tile(&tree, canvas(1)).expect("canevas présent");
        assert!(
            behavior.is_tab_closable(&tree.tiles, canvas_tile),
            "canevas fermable (ferme son document)"
        );
    }

    #[test]
    fn closing_canvas_tab_closes_document_and_returns_home() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        app.open_sized_tab(64, 64);
        app.open_sized_tab(64, 64);
        let first = app.docs[0].id;
        let second = app.docs[1].id;
        let mut tree = default_tree(vec![
            PhotoDockTab::Canvas(first),
            PhotoDockTab::Canvas(second),
        ]);
        // Close the first canvas: document removed, tile destroyed
        // (`true` = egui_tiles destroys the tile — reproduced here with
        // `remove_recursively`), second document active.
        {
            let tile = find_tile(&tree, PhotoDockTab::Canvas(first)).expect("canevas 1");
            let mut behavior = PhotoTreeBehavior {
                docs: &mut app.docs,
                active: &mut app.active,
                filter_types: &[],
                ctx: &photo_ctx,
                actions: Vec::new(),
                active_canvas_tabs: Default::default(),
            };
            assert!(behavior.on_tab_close(&mut tree.tiles, tile));
            tree.remove_recursively(tile);
        }
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.active_doc_opt().expect("doc restant").id, second);
        assert!(!has_tab(&tree, PhotoDockTab::Canvas(first)));
        assert!(has_tab(&tree, PhotoDockTab::Canvas(second)));
        // Close the last one: zero documents = back to welcome.
        {
            let tile = find_tile(&tree, PhotoDockTab::Canvas(second)).expect("canevas 2");
            let mut behavior = PhotoTreeBehavior {
                docs: &mut app.docs,
                active: &mut app.active,
                filter_types: &[],
                ctx: &photo_ctx,
                actions: Vec::new(),
                active_canvas_tabs: Default::default(),
            };
            assert!(behavior.on_tab_close(&mut tree.tiles, tile));
            tree.remove_recursively(tile);
        }
        assert!(app.docs.is_empty(), "accueil : aucun document");
        assert!(!has_tab(&tree, PhotoDockTab::Canvas(second)));
        // Panels kept for the next opening.
        for tab in FIXED_TABS {
            assert!(has_tab(&tree, tab), "panneau perdu : {tab:?}");
        }
    }

    #[test]
    fn canvas_title_shows_document_name() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        app.open_sized_tab(800, 600);
        let doc_id = app.active_doc_opt().expect("doc actif").id;
        let expected = app
            .docs
            .iter()
            .find(|doc| doc.id == doc_id)
            .expect("document présent")
            .title
            .clone();
        let mut behavior = PhotoTreeBehavior {
            docs: &mut app.docs,
            active: &mut app.active,
            filter_types: &[],
            ctx: &photo_ctx,
            actions: Vec::new(),
            active_canvas_tabs: Default::default(),
        };
        let title = behavior.tab_title_for_pane(&PhotoDockTab::Canvas(doc_id));
        assert!(
            title.text().ends_with(expected.as_str()),
            "titre inattendu : {}",
            title.text()
        );
        assert!(!title.text().contains('%'), "zoom dans la barre d'état");
    }

    #[test]
    fn rail_width_is_pinned_in_pixels() {
        let mut tree = default_tree(vec![canvas(1)]);
        pin_tools_width(&mut tree, 1000.0, 50.0);
        let tools = find_tile(&tree, PhotoDockTab::Tools).expect("outils présents");
        let root = tree.root().expect("racine présente");
        let Some(egui_tiles::Tile::Container(Container::Linear(linear))) = tree.tiles.get(root)
        else {
            panic!("racine horizontale attendue");
        };
        let total: f32 = linear.shares.iter().map(|(_, share)| *share).sum();
        assert!((linear.shares[tools] / total - 0.05).abs() < 1e-4);
    }

    #[test]
    fn tools_tab_shows_graphic_title_for_dnd() {
        let ctx = egui::Context::default();
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut app = PhotoApp::new();
        let mut behavior = PhotoTreeBehavior {
            docs: &mut app.docs,
            active: &mut app.active,
            filter_types: &[],
            ctx: &photo_ctx,
            actions: Vec::new(),
            active_canvas_tabs: Default::default(),
        };
        let title = behavior.tab_title_for_pane(&PhotoDockTab::Tools);
        assert!(!title.text().is_empty(), "titre graphique attendu");
        assert_ne!(title.text(), "Outils", "pas de texte, juste l'icône");
    }

    #[test]
    fn reconcile_drops_stale_canvases_and_adds_missing_ones() {
        let mut tree = default_tree(vec![canvas(1)]);
        reconcile_canvases(&mut tree, &[Uuid::from_u128(2)]);
        assert!(!has_tab(&tree, canvas(1)));
        assert!(has_tab(&tree, canvas(2)));
        // Panels kept.
        for tab in FIXED_TABS {
            assert!(has_tab(&tree, tab), "panneau perdu : {tab:?}");
        }
    }

    #[test]
    fn tree_survives_json_roundtrip() {
        use crate::persistence::ui_state::{load_tree_state, save_tree_state};

        let mut app = PhotoApp::new();
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let ctx_clone = ctx.clone();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw(&ctx_clone, ui);
        })
        .drop_without_applying_deltas();
        let json = save_tree_state(&app.shell.tree).expect("serialisation des tuiles");
        let restored = load_tree_state(&json).expect("restauration des tuiles");
        for tab in FIXED_TABS {
            assert!(has_tab(&restored, tab), "panneau perdu : {tab:?}");
        }
        // Canvases carry the open documents' ids.
        for doc in &app.docs {
            assert!(has_tab(&restored, PhotoDockTab::Canvas(doc.id)));
        }
    }

    #[test]
    fn corrupt_or_missing_tree_json_falls_back_to_defaults() {
        use crate::persistence::load_tree_or_default;

        let ids = [Uuid::from_u128(7)];
        for tab in [PhotoDockTab::Tools, canvas(7), PhotoDockTab::Inspector] {
            assert!(has_tab(
                &load_tree_or_default(Some("pas du json"), &ids),
                tab
            ));
            assert!(has_tab(&load_tree_or_default(None, &ids), tab));
        }
    }

    #[test]
    fn histogram_sits_above_the_adjustments_once() {
        let mut tree = default_tree(vec![canvas(1)]);
        let inspector = find_tile(&tree, PhotoDockTab::Inspector).expect("réglages présents");
        let histogram = find_tile(&tree, PhotoDockTab::Histogram).expect("histogramme présent");
        assert_eq!(
            tree.tiles.parent_of(inspector),
            tree.tiles.parent_of(histogram),
            "même colonne latérale"
        );
        ensure_histogram_tabbed(&mut tree);
        let count = tree
            .tiles
            .iter()
            .filter(|(_, tile)| matches!(tile, egui_tiles::Tile::Pane(PhotoDockTab::Histogram)))
            .count();
        assert_eq!(count, 1, "idempotent");
    }
}
