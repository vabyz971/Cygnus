# cygnus-ui

egui design system for Cygnus — layered architecture (bottom to top):

## Layers

1. **`theme`** — ONLY source of colors, sizes, radii, typography.
   - See `cygnus-ui/src/theme/` (tokens `colors`, `spacing`,
     `radius`, `typography`). Never hardcoded values elsewhere.

2. **`primitives`** — generic building blocks (`Text`, `Icon`, `divider`,
   `Surface`), domain-independent.

3. **`icons`** — `CygnusIcon` enum + registry (`IconRegistry`,
   `ALL_ICONS`). Only `egui_material_icons` import source.

4. **`components`** — generic components (`Button`, `Slider`,
   `Select`, `Tabs`, `input`, `Toggle`, `IconButton`, `reorderable_list`,
   `menu`). Styling only via theme tokens.

5. **`containers`** — agnostic containers (`Section`, `Card`,
   `Stack`, `Panel`, `Split` + `SplitState`, `Collapsible`,
   `Toolbar`). No UI dependency.

6. **`layout`** — workspace state (`PanelId`, `WorkspaceState`,
   JSON persistence via `preferences`). Panel titles resolve via
   the shared `cygnus-i18n` catalog (no per-app duplicates).

7. **`viewport`** — zoom/pan state (`ViewportState`), overlays.

8. **`dialogs`** — modals, file picker, progress.

9. **`utils`** — generic drag & drop state.

10. **`context`** — shared dependencies (`UiContext`: egui,
    theme, icons, translation via `cygnus-i18n`). No engine or
    app dependency.

Shared translations live in `cygnus-i18n/` at the workspace root
(`Catalog`, `Language`, `TextKey` — one catalog for photo /
video / audio). All catalogs (generic + photo/video/audio domains) live in `cygnus-i18n`; apps hold no translation tables.

## Golden rules

- No component hardcodes a color/size/enum: it references
  the theme tokens.
- `unwrap()` / `expect()` forbidden outside tests.
- No emoji in code or commits.
- Domain widgets live in `apps/*/src/ui/` — cygnus-ui stays
  domain-agnostic (verified by `scripts/check_uikit_domain_agnostic.sh`).

## Usage

```rust
use cygnus_ui::theme::CygnusTheme;
use cygnus_ui::components::Button;
use cygnus_i18n::Catalog;
```
