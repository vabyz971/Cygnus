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

//! Cygnus theme control sizes.
//!
//! SINGLE source of widget heights (buttons, inputs, tabs):
//! components reference these tokens instead of `f32` literals.
//! Spacing stays in [`super::spacing`].

/// Standard control heights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusSizes {
    /// Extra-extra-small button height (12px).
    pub button_xxs: f32,
    /// Extra-small button height (18px).
    pub button_xs: f32,
    /// Small button height (24px).
    pub button_sm: f32,
    /// Standard button height (32px).
    pub button_md: f32,
    /// Large button height (40px).
    pub button_lg: f32,
    /// Text-input field height (30px).
    pub input_height: f32,
    /// Square icon-button size (28px).
    pub icon_button: f32,
    /// Photo layer-row height (44px).
    pub layer_row: f32,
    /// Attachment-row height (30px).
    pub attachment_row: f32,
    /// Square layer thumbnail (28px).
    pub layer_thumb: f32,
    /// Square toolbar tool button (28px).
    pub tool_button: f32,
    /// Creation-dialog preset card (90px).
    pub preset_card: f32,
    /// Photo selection-bar height (40px).
    pub bar_height: f32,
    /// Tab height (28px).
    pub tab_height: f32,
    /// Minimum touch target (24px).
    pub min_touch: f32,
}

impl CygnusSizes {
    /// Dark-theme sizes.
    pub fn dark() -> Self {
        Self {
            button_xxs: 12.0,
            button_xs: 18.0,
            button_sm: 24.0,
            button_md: 32.0,
            button_lg: 40.0,
            input_height: 30.0,
            icon_button: 28.0,
            layer_row: 44.0,
            attachment_row: 30.0,
            layer_thumb: 28.0,
            tool_button: 28.0,
            preset_card: 90.0,
            bar_height: 40.0,
            tab_height: 28.0,
            min_touch: 24.0,
        }
    }
}

impl Default for CygnusSizes {
    /// Default sizes.
    fn default() -> Self {
        Self::dark()
    }
}
