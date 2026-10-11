# ui-kit

egui design system for Cygnus — layered architecture (bottom to top):

## Layers

1. **`theme`** — ONLY source of colors, sizes, radii, typography.
   - See `packages/ui-kit/src/theme/` (tokens `colors`, `spacing`,
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
   JSON persistence via `preferences`). No UI dependency.

7. **`i18n`** — stable translation keys + catalog (`Catalog`,
   `Language`, `TextKey`, `PhotoCatalog`). Zero allocation at
   render time.

8. **`viewport`** — zoom/pan state (`ViewportState`), overlays.

9. **`dialogs`** — modals, file picker, progress.

10. **`utils`** — generic drag & drop state.

11. **`context`** — shared dependencies (`UiContext`: egui,
    theme, icons, translation). No engine or app dependency.

## Golden rules

- No component hardcodes a color/size/enum: it references
  the theme tokens.
- `unwrap()` / `expect()` forbidden outside tests.
- No emoji in code or commits.
- Domain widgets live in `apps/*/src/ui/` — ui-kit stays
  domain-agnostic (verified by `scripts/check_uikit_domain_agnostic.sh`).

## Usage

```rust
use ui_kit::theme::CygnusTheme;
use ui_kit::components::Button;
use ui_kit::i18n::Catalog;
```
