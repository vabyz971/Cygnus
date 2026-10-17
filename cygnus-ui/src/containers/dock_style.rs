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

//! Dock (tiled panels) styling from the theme, Rerun-style.
//!
//! Plain values only (colors, strokes, sizes): every app's dock
//! `Behavior` (egui_tiles) reads them, so cygnus-ui stays free of the
//! tiling crate and photo / video / audio docks look identical.
//! Look: tab bar on the bar surface, active tab merged with its panel
//! (same fill, strong text), inactive tabs flat with secondary text,
//! 1 px dividers between tiles, accent only while resizing / dropping.

use crate::theme::CygnusTheme;

/// Tab state as seen by the dock (decoupled from egui_tiles' type).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockTabState {
    /// Tab currently shown in its container.
    pub active: bool,
    /// Tab being dragged to another slot.
    pub dragged: bool,
}

/// Resize-handle state between tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockResizeState {
    /// Nothing happening.
    Idle,
    /// Pointer over the divider.
    Hovering,
    /// Divider being dragged.
    Dragging,
}

/// Dock styling derived from a theme.
#[derive(Debug, Clone, Copy)]
pub struct DockStyle<'a> {
    theme: &'a CygnusTheme,
}

impl<'a> DockStyle<'a> {
    /// Style for `theme`.
    #[must_use]
    pub fn new(theme: &'a CygnusTheme) -> Self {
        Self { theme }
    }

    /// Tab bar height (title-bar token).
    #[must_use]
    pub fn tab_bar_height(&self) -> f32 {
        self.theme.sizes.tab_height
    }

    /// Gap between tiles (a divider line).
    #[must_use]
    pub fn gap_width(&self) -> f32 {
        self.theme.borders.thin
    }

    /// Horizontal padding around tab titles.
    #[must_use]
    pub fn tab_title_spacing(&self) -> f32 {
        self.theme.spacing.sm
    }

    /// Tab bar background.
    #[must_use]
    pub fn tab_bar_color(&self) -> egui::Color32 {
        self.theme.colors.bg_secondary
    }

    /// Tab background: the active tab merges with its panel.
    #[must_use]
    pub fn tab_bg_color(&self, state: DockTabState) -> egui::Color32 {
        if state.active {
            self.theme.colors.bg_primary
        } else if state.dragged {
            self.theme.colors.item_hover
        } else {
            egui::Color32::TRANSPARENT
        }
    }

    /// Tab title color.
    #[must_use]
    pub fn tab_text_color(&self, state: DockTabState) -> egui::Color32 {
        if state.active || state.dragged {
            self.theme.colors.fg_strong
        } else {
            self.theme.colors.fg_secondary
        }
    }

    /// Line under the tab bar.
    #[must_use]
    pub fn tab_bar_hline_stroke(&self) -> egui::Stroke {
        egui::Stroke::new(self.theme.borders.thin, self.theme.colors.border)
    }

    /// Divider between tiles.
    #[must_use]
    pub fn resize_stroke(&self, state: DockResizeState) -> egui::Stroke {
        match state {
            DockResizeState::Idle => {
                egui::Stroke::new(self.theme.borders.thin, self.theme.colors.border)
            }
            DockResizeState::Hovering => {
                egui::Stroke::new(self.theme.borders.medium, self.theme.colors.accent_hover)
            }
            DockResizeState::Dragging => {
                egui::Stroke::new(self.theme.borders.medium, self.theme.colors.accent)
            }
        }
    }

    /// Drop-target preview fill while dragging a tab.
    #[must_use]
    pub fn drag_preview_color(&self) -> egui::Color32 {
        self.theme.colors.drop_indicator.gamma_multiply(0.25)
    }

    /// Drop-target preview outline.
    #[must_use]
    pub fn drag_preview_stroke(&self) -> egui::Stroke {
        egui::Stroke::new(self.theme.borders.medium, self.theme.colors.drop_indicator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_tab_merges_with_its_panel() {
        let theme = CygnusTheme::dark();
        let style = DockStyle::new(&theme);
        let active = DockTabState {
            active: true,
            dragged: false,
        };
        let idle = DockTabState {
            active: false,
            dragged: false,
        };
        assert_eq!(style.tab_bg_color(active), theme.colors.bg_primary);
        assert_eq!(style.tab_bg_color(idle), egui::Color32::TRANSPARENT);
        assert_eq!(style.tab_text_color(active), theme.colors.fg_strong);
        assert_eq!(style.tab_text_color(idle), theme.colors.fg_secondary);
        assert_eq!(style.tab_bar_height(), theme.sizes.tab_height);
        assert!(
            style.resize_stroke(DockResizeState::Dragging).width
                > style.resize_stroke(DockResizeState::Idle).width
        );
    }
}
