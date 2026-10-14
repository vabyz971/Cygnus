//! cygnus-ui — packageless `use` for apps.
//!
//! When a single app needs the component and style suite defined in
//! cygnus-ui (e.g. generic button, main slider, sliding panel),
//! it goes through a single `use cygnus_ui::prelude::*`.
//! Shared translations live in `cygnus_i18n` (workspace root):
//! import them separately via `use cygnus_i18n::{Catalog, …}`.
//! No domain module is exported in cygnus-ui.

pub use crate::components::{
    Button, Checkbox, IconButton, NumberInput, Select, Slider, Tabs, reorderable_list,
};

pub use crate::containers::{Card, Collapsible, Panel, Section, Split, Stack, Toolbar};

pub use crate::icons::Icon;

pub use crate::primitives::{Icon as PrimitiveIcon, Surface, Text};

pub use crate::theme::{CygnusTheme, UiThemeExt, apply_cygnus_theme, install_theme, setup_fonts};

pub use crate::viewport::ViewportState;

pub use crate::context::UiContext;
