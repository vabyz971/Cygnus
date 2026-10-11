//! UI Kit — packageless `use` for apps.
//!
//! When a single app needs the component and style suite defined in
//! ui-kit (e.g. generic button, main slider, sliding panel,
//! translation catalog), it goes through a single
//! `use ui_kit::prelude::*`. No domain module is exported in ui-kit.

pub use crate::components::{
    Button, Checkbox, IconButton, NumberInput, Select, Slider, Tabs, reorderable_list,
};

pub use crate::containers::{Card, Collapsible, Panel, Section, Split, Stack, Toolbar};

pub use crate::i18n::{Catalog, Language, TextKey};

pub use crate::icons::Icon;

pub use crate::primitives::{Icon as PrimitiveIcon, Surface, Text};

pub use crate::theme::{CygnusTheme, UiThemeExt, apply_cygnus_theme, install_theme, setup_fonts};

pub use crate::viewport::ViewportState;

pub use crate::context::UiContext;
