// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Container of dockable panels, built on `egui_dock`.
//!
//! The panels live in a resizable column on the right of the window. Inside
//! the column they can be stacked, split and tabbed; they can also be
//! detached into floating windows anywhere over the application window,
//! closed, and reopened. The whole arrangement is a [`DockLayout`], which is
//! what a workspace file stores.

use egui::{Id, Rect, Ui};
use egui_dock::{DockArea, DockState, Node, TabViewer};
use serde::{Deserialize, Serialize, Serializer};

use crate::theme::{self, metrics};

/// Arrangement of the dockable panels: what a workspace stores.
#[derive(Clone, Serialize, Deserialize)]
pub struct DockLayout<T: Clone + Serialize> {
    /// Panels docked in the column and floating windows.
    #[serde(serialize_with = "serialize_state")]
    pub state: DockState<T>,
    /// Width of the panel column, in points.
    pub column_width: f32,
}

/// Serializes the dock state without the rectangles egui_dock computes on
/// every frame. Before the first frame they are infinite, which JSON cannot
/// represent (they would be written as `null` and fail to load back).
fn serialize_state<T, S>(state: &DockState<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Clone + Serialize,
    S: Serializer,
{
    let mut state = state.clone();
    let finite = |rect: Rect| rect.min.is_finite() && rect.max.is_finite();
    for (_, node) in state.iter_all_nodes_mut() {
        if node.rect().is_some_and(|rect| !finite(rect)) {
            node.set_rect(Rect::ZERO);
        }
        if let Node::Leaf(leaf) = node
            && !finite(leaf.viewport)
        {
            leaf.viewport = Rect::ZERO;
        }
    }
    state.serialize(serializer)
}

impl<T: Clone + PartialEq + Serialize> DockLayout<T> {
    /// Whether `panel` is open, docked or floating.
    pub fn is_open(&self, panel: &T) -> bool {
        self.state.find_tab(panel).is_some()
    }

    /// Checks that no panel appears twice, that every panel belongs to
    /// `known`, and that the column width is usable. A layout restored from
    /// disk is only used when it is valid.
    pub fn is_valid(&self, known: &[T]) -> bool {
        let tabs: Vec<&T> = self.state.iter_all_tabs().map(|(_, tab)| tab).collect();
        let unique = tabs
            .iter()
            .enumerate()
            .all(|(index, tab)| !tabs[..index].contains(tab));
        let width_ok =
            self.column_width.is_finite() && self.column_width >= metrics::DOCK_MIN_WIDTH;
        unique && width_ok && tabs.iter().all(|tab| known.contains(tab))
    }
}

/// Dockable panels of an application, with the layout to restore on reset.
pub struct DockContainer<T: Clone + Serialize> {
    layout: DockLayout<T>,
    default_layout: fn() -> DockLayout<T>,
    /// Incremented on reset so that egui forgets the previous column width.
    generation: u64,
}

impl<T: Clone + PartialEq + Serialize> DockContainer<T> {
    /// Creates a container showing `layout`; `default_layout` is used by
    /// [`Self::reset`].
    pub fn new(layout: DockLayout<T>, default_layout: fn() -> DockLayout<T>) -> Self {
        Self {
            layout,
            default_layout,
            generation: 0,
        }
    }

    /// Current layout, to be saved in a workspace.
    pub fn layout(&self) -> &DockLayout<T> {
        &self.layout
    }

    /// Whether `panel` is open, docked or floating.
    pub fn is_open(&self, panel: &T) -> bool {
        self.layout.is_open(panel)
    }

    /// Opens or closes `panel`. A reopened panel is docked at the bottom of
    /// the column.
    pub fn set_open(&mut self, panel: &T, open: bool) {
        let found = self.layout.state.find_tab(panel);
        match (open, found) {
            (true, None) => self.dock_at_bottom(panel.clone()),
            (false, Some(path)) => {
                self.layout.state.remove_tab(path);
            }
            _ => {}
        }
    }

    /// Restores the default layout.
    pub fn reset(&mut self) {
        self.layout = (self.default_layout)();
        self.generation += 1;
    }

    fn dock_at_bottom(&mut self, panel: T) {
        let tree = self.layout.state.main_surface_mut();
        if tree.is_empty() {
            tree.push_to_first_leaf(panel);
            return;
        }
        let root = egui_dock::NodeIndex::root();
        tree.split_below(root, 0.75, vec![panel]);
    }

    /// Shows the panel column on the right of `ui`, and the floating windows.
    pub fn show(&mut self, ui: &mut Ui, viewer: &mut impl TabViewer<Tab = T>) {
        // Floating windows may move anywhere over the application window.
        let bounds = ui.ctx().content_rect();
        let dock_style = theme::dock_style(ui.style());
        let id = Id::new("cygnus-dock").with(self.generation);

        // With every panel closed there is nothing to show. When panels are
        // open but all floating, the empty column stays as a drop target.
        if self.layout.state.iter_all_tabs().next().is_none() {
            return;
        }

        let response = egui::Panel::right(id.with("column"))
            .resizable(true)
            .default_size(self.layout.column_width)
            .size_range(metrics::DOCK_MIN_WIDTH..=f32::INFINITY)
            .frame(egui::Frame::new().fill(theme::palette::PANEL))
            .show(ui, |ui| {
                DockArea::new(&mut self.layout.state)
                    .id(id)
                    .style(dock_style)
                    .window_bounds(bounds)
                    .show_leaf_collapse_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_inside(ui, viewer);
            });
        self.layout.column_width = response.response.rect.width();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
    enum Panel {
        A,
        B,
        C,
    }

    const KNOWN: &[Panel] = &[Panel::A, Panel::B, Panel::C];

    fn default_layout() -> DockLayout<Panel> {
        let mut state = DockState::new(vec![Panel::A]);
        let tree = state.main_surface_mut();
        let [_, below] = tree.split_below(egui_dock::NodeIndex::root(), 0.5, vec![Panel::B]);
        tree.split_below(below, 0.5, vec![Panel::C]);
        DockLayout {
            state,
            column_width: 320.0,
        }
    }

    #[test]
    fn default_layout_is_valid() {
        assert!(default_layout().is_valid(KNOWN));
    }

    #[test]
    fn duplicate_or_unknown_panels_are_invalid() {
        let mut layout = default_layout();
        layout.state.push_to_first_leaf(Panel::A);
        assert!(!layout.is_valid(KNOWN));
        assert!(!default_layout().is_valid(&[Panel::A, Panel::B]));
    }

    #[test]
    fn unusable_width_is_invalid() {
        let mut layout = default_layout();
        layout.column_width = f32::NAN;
        assert!(!layout.is_valid(KNOWN));
        layout.column_width = 10.0;
        assert!(!layout.is_valid(KNOWN));
    }

    #[test]
    fn close_and_reopen_panels() {
        let mut dock = DockContainer::new(default_layout(), default_layout);
        dock.set_open(&Panel::B, false);
        assert!(!dock.is_open(&Panel::B));
        dock.set_open(&Panel::B, true);
        assert!(dock.is_open(&Panel::B));
        assert!(dock.layout().is_valid(KNOWN));
    }

    #[test]
    fn reopen_after_closing_everything() {
        let mut dock = DockContainer::new(default_layout(), default_layout);
        for panel in KNOWN {
            dock.set_open(panel, false);
        }
        assert!(dock.layout().state.main_surface().is_empty());
        dock.set_open(&Panel::C, true);
        assert!(dock.is_open(&Panel::C));
    }

    #[test]
    fn floating_panel_stays_open_and_valid() {
        let mut layout = default_layout();
        let path = layout.state.find_tab(&Panel::A).unwrap();
        layout.state.detach_tab(
            path,
            egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 150.0)),
        );
        assert!(layout.is_open(&Panel::A));
        assert!(layout.is_valid(KNOWN));
    }

    #[test]
    fn reset_restores_default() {
        let mut dock = DockContainer::new(default_layout(), default_layout);
        dock.set_open(&Panel::A, false);
        dock.reset();
        assert!(KNOWN.iter().all(|panel| dock.is_open(panel)));
    }
}
