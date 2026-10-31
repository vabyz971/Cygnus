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

//! Dockable panels of Cygnus Photo and the workspace that arranges them.

use std::path::PathBuf;

use cygnus_ui_kit::dock::DockLayout;
use cygnus_ui_kit::i18n::{Key, tr};
use cygnus_ui_kit::theme::metrics;
use cygnus_ui_kit::{config, workspace};
use egui::{Id, Ui, WidgetText};
use egui_dock::{DockState, NodeIndex, TabViewer};
use serde::{Deserialize, Serialize};

use crate::keys;
use crate::placeholder;

/// A dockable panel. The Channels and Paths tabs of the mockup are left out:
/// they are not part of the beta.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Panel {
    /// Histogram of the image.
    Histogram,
    /// Parameters of the selected adjustment.
    Adjustments,
    /// Layer tree.
    Layers,
    /// Undo history.
    History,
}

impl Panel {
    /// Every panel, in the order of the Window menu.
    pub const ALL: [Self; 4] = [
        Self::Histogram,
        Self::Adjustments,
        Self::Layers,
        Self::History,
    ];

    /// Translation key of the panel title.
    pub fn title(self) -> Key {
        match self {
            Self::Histogram => keys::PANEL_HISTOGRAM,
            Self::Adjustments => keys::PANEL_ADJUSTMENTS,
            Self::Layers => keys::PANEL_LAYERS,
            Self::History => keys::PANEL_HISTORY,
        }
    }
}

/// Version of the workspace file format. Increment it whenever [`Panel`] or
/// the layout structure changes in a way older files cannot be read with.
pub const LAYOUT_VERSION: u32 = 1;

/// Default arrangement: the four panels stacked in the right column, as in
/// the mockup.
pub fn default_layout() -> DockLayout<Panel> {
    let mut state = DockState::new(vec![Panel::Histogram]);
    let tree = state.main_surface_mut();
    let [_, rest] = tree.split_below(NodeIndex::root(), 0.2, vec![Panel::Adjustments]);
    let [_, rest] = tree.split_below(rest, 0.37, vec![Panel::Layers]);
    tree.split_below(rest, 0.8, vec![Panel::History]);
    DockLayout {
        state,
        column_width: metrics::DOCK_DEFAULT_WIDTH,
    }
}

/// Path of the workspace file, if the system has a configuration directory.
pub fn workspace_path() -> Option<PathBuf> {
    config::app_config_dir("photo").map(|dir| dir.join("workspace.json"))
}

/// Chooses the layout to start with from the result of reading the
/// workspace file: the saved layout when it is readable and valid, the
/// default layout otherwise. Problems are reported on stderr.
pub fn restore_layout(
    loaded: Result<Option<DockLayout<Panel>>, workspace::LoadError>,
) -> DockLayout<Panel> {
    match loaded {
        Ok(Some(layout)) if layout.is_valid(&Panel::ALL) => layout,
        Ok(Some(_)) => {
            eprintln!("cygnus-photo: saved workspace is inconsistent, using the default layout");
            default_layout()
        }
        Ok(None) => default_layout(),
        Err(error) => {
            eprintln!("cygnus-photo: {error}; using the default layout");
            default_layout()
        }
    }
}

/// Reads the saved workspace, or returns the default layout.
pub fn load_layout() -> DockLayout<Panel> {
    match workspace_path() {
        Some(path) => restore_layout(workspace::load(&path, LAYOUT_VERSION)),
        None => default_layout(),
    }
}

/// Saves `layout` to the workspace file. Failures are reported on stderr:
/// losing the layout must never prevent the application from closing.
pub fn save_layout(layout: &DockLayout<Panel>) {
    let Some(path) = workspace_path() else {
        eprintln!("cygnus-photo: no configuration directory, workspace not saved");
        return;
    };
    if let Err(error) = workspace::save(&path, LAYOUT_VERSION, layout) {
        eprintln!("cygnus-photo: {error}");
    }
}

/// Draws the content of each panel.
pub struct PanelViewer<'a> {
    /// Placeholder values shown until the document model exists (J3).
    pub placeholder: &'a mut placeholder::State,
}

impl TabViewer for PanelViewer<'_> {
    type Tab = Panel;

    fn id(&mut self, tab: &mut Panel) -> Id {
        Id::new(("cygnus-panel", *tab))
    }

    fn title(&mut self, tab: &mut Panel) -> WidgetText {
        tr(tab.title()).into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Panel) {
        match tab {
            Panel::Histogram => placeholder::histogram(ui),
            Panel::Adjustments => placeholder::adjustments(ui, self.placeholder),
            Panel::Layers => placeholder::layers(ui, self.placeholder),
            Panel::History => placeholder::history(ui),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(layout: &DockLayout<Panel>) -> DockLayout<Panel> {
        let json = workspace::to_json(LAYOUT_VERSION, layout).unwrap();
        restore_layout(workspace::from_json(LAYOUT_VERSION, &json).map(Some))
    }

    fn open_panels(layout: &DockLayout<Panel>) -> Vec<Panel> {
        Panel::ALL
            .into_iter()
            .filter(|panel| layout.is_open(panel))
            .collect()
    }

    #[test]
    fn default_layout_opens_every_panel() {
        let layout = default_layout();
        assert!(layout.is_valid(&Panel::ALL));
        assert_eq!(open_panels(&layout), Panel::ALL);
    }

    #[test]
    fn layout_survives_serialization() {
        let mut layout = default_layout();
        // Close a panel, detach another into a floating window, resize.
        let history = layout.state.find_tab(&Panel::History).unwrap();
        layout.state.remove_tab(history);
        let histogram = layout.state.find_tab(&Panel::Histogram).unwrap();
        let rect = egui::Rect::from_min_size(egui::pos2(40.0, 60.0), egui::vec2(300.0, 200.0));
        layout.state.detach_tab(histogram, rect);
        layout.column_width = 410.0;

        let restored = round_trip(&layout);
        assert_eq!(
            open_panels(&restored),
            vec![Panel::Histogram, Panel::Adjustments, Panel::Layers]
        );
        assert_eq!(restored.column_width, 410.0);
        assert_eq!(
            restored.state.surfaces_count(),
            layout.state.surfaces_count()
        );
        let floating = restored.state.find_tab(&Panel::Histogram).unwrap();
        assert_ne!(
            floating.node_path().surface,
            egui_dock::SurfaceIndex::main()
        );
        assert_eq!(
            workspace::to_json(LAYOUT_VERSION, &restored).unwrap(),
            workspace::to_json(LAYOUT_VERSION, &layout).unwrap()
        );
    }

    #[test]
    fn missing_workspace_gives_default_layout() {
        let layout = restore_layout(Ok(None));
        assert_eq!(open_panels(&layout), Panel::ALL);
    }

    #[test]
    fn invalid_workspace_gives_default_layout() {
        for json in [
            "",
            "{ broken",
            r#"{"version": 1, "layout": {"column_width": 300}}"#,
            r#"{"version": 1, "layout": 42}"#,
        ] {
            let layout = restore_layout(workspace::from_json(LAYOUT_VERSION, json));
            assert_eq!(open_panels(&layout), Panel::ALL, "input: {json}");
        }
    }

    #[test]
    fn older_version_gives_default_layout() {
        let mut layout = default_layout();
        let path = layout.state.find_tab(&Panel::Layers).unwrap();
        layout.state.remove_tab(path);
        let json = workspace::to_json(LAYOUT_VERSION - 1, &layout).unwrap();
        let loaded = workspace::from_json(LAYOUT_VERSION, &json);
        assert!(matches!(
            loaded,
            Err(workspace::LoadError::UnsupportedVersion { .. })
        ));
        assert_eq!(open_panels(&restore_layout(loaded)), Panel::ALL);
    }

    #[test]
    fn inconsistent_workspace_gives_default_layout() {
        let mut layout = default_layout();
        layout.state.push_to_first_leaf(Panel::Layers);
        let json = workspace::to_json(LAYOUT_VERSION, &layout).unwrap();
        let restored = restore_layout(workspace::from_json(LAYOUT_VERSION, &json).map(Some));
        assert!(restored.is_valid(&Panel::ALL));
        assert_eq!(restored.state.iter_all_tabs().count(), Panel::ALL.len());
    }

    #[test]
    fn unknown_panel_gives_default_layout() {
        let json = workspace::to_json(LAYOUT_VERSION, &default_layout())
            .unwrap()
            .replace("\"Layers\"", "\"Channels\"");
        let layout = restore_layout(workspace::from_json(LAYOUT_VERSION, &json));
        assert_eq!(open_panels(&layout), Panel::ALL);
    }
}
