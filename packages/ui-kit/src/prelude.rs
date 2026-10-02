//! UI Kit — packageless `use` pour apps.
//!
//! Qu'une seule app ait besoin de la suite de composants
//! et styles definis dans ui-kit (par exemple, bouton
//! generique, curseur principal, panneau glissant,
//! catalogue de traduction) se fasse via un seul
//! `use ui_kit::prelude::*`. Aucun module metier
//! exporte dans ui-kit.

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
