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

//! Composants configurables du design system.
//!
//! Chaque composant prend le [`CygnusTheme`](crate::theme::CygnusTheme)
//! en paramètre de `show` : aucune valeur visuelle en dur. Les
//! variantes passent par des enums ([`ButtonVariant`], [`ButtonSize`]),
//! jamais par des types spécialisés.

pub mod button;
pub mod checkbox;
pub mod icon_button;
pub mod input;
pub mod select;
pub mod slider;
pub mod tabs;
pub mod toggle;

pub use button::{Button, ButtonSize, ButtonVariant};
pub use checkbox::Checkbox;
pub use icon_button::IconButton;
pub use input::TextInput;
pub use select::{Select, sanitize_selected};
pub use slider::Slider;
pub use tabs::Tabs;
pub use toggle::Toggle;
