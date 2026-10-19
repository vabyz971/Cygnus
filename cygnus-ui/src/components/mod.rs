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

//! Configurable design-system components.
//!
//! Every component takes the [`CygnusTheme`](crate::theme::CygnusTheme)
//! as its `show` parameter: no hardcoded visual value. Variants
//! go through enums ([`ButtonVariant`], [`ButtonSize`]),
//! never through specialized types.

pub mod button;
pub mod checkbox;
pub mod icon_button;
pub mod input;
pub mod menu;
pub mod reorderable_list;
pub mod select;
pub mod slider;
pub mod tabs;
pub mod toggle;
pub mod value_field;

pub use button::{Button, ButtonSize, ButtonVariant, menu_row};
pub use checkbox::Checkbox;
pub use icon_button::IconButton;
pub use input::{NumberInput, TextInput};
pub use menu::menu_style;
pub use reorderable_list::{
    HierarchicalDrop, ReorderableList, compute_target_index, draw_drop_indicator, item_background,
};
pub use select::{Select, sanitize_selected};
pub use slider::{Slider, SliderValue};
pub use tabs::Tabs;
pub use toggle::Toggle;
pub use value_field::ValueField;
