---
covers: []
---
# Cygnus architecture

## Overview

Cygnus is a professional creative suite made of three independent
applications (Photo, Video, Audio) sharing a common foundation: business
engines, a generic node graph, widgets and utility libraries.

## Folder structure

```
apps/       Final applications (independent binaries)
engines/    PURE business engines — zero UI dependency
core/       Shared foundation: datatypes
cygnus-ui/  Design system (egui): theme, components, viewport
cygnus-i18n/ Shared translations (all catalogs, no per-app duplicates)
packages/   Reusable libraries: app-shell, math-utils, file-utils
assets/     Shared resources (fonts)
```

### cygnus-ui/ + cygnus-i18n/ + packages/
Shared libraries reusable across all applications.
- `cygnus-i18n` (crate `cygnus_i18n`): shared translations —
  generic `TextKey` + `Catalog`/`Language` (en/fr tables, zero
  allocation). All catalogs (generic + photo/video/audio domains) live here; apps hold no translation tables.
- `cygnus-ui` (crate `cygnus_ui`): layered egui design system — `theme`
  (only source of colors/sizes, tokens in `theme/`),
  generic `components` (incl. `icon_button`, `ReorderableList`),
  `containers`, generic pan/zoom `viewport`, `dialogs`. Strictly
  domain-agnostic: no business types (verified by
  `scripts/check_uikit_domain_agnostic.sh`).
- `app-shell` (crate `app_shell`): generic app skeleton —
  `ActionQueue<A>` (action queue drained per frame) and generic
  `egui_tiles` dock logic over the `DockTab` trait (`has_tab`,
  `ensure_tab`, `push/remove_canvas_tab`, `reconcile_canvases`,
  `default_tree`, `set_linear_shares`). Depends only on `egui`,
  `egui_tiles`, `serde`, `uuid`: never on engines or apps, no
  business types (the `Behavior` — titles, contents — stays in each
  app).
- `math-utils`: canonical 2D affine transform (`Transform2D`);
  the canonical `Vec2` stays `datatypes::Vec2`, re-exported.
- `file-utils`: file errors, drag & drop types and dialogs.

These packages must NEVER depend on engines or apps.

### engines/
Domain-specific business engines, strictly pure:
no knowledge of egui or its types. Document-model buffers stay
pure (`RgbaBuf`, `Arc<[u8]>`); apps send commands via `mpsc` to a
worker owning the `Document` and receive snapshots + composite
preview (texture conversion app-side).
- `photo-engine`: document, CPU/GPU compositing, history, `.cygp`
  project (raster only; `BlendMode`/`RgbaBuf` come from `datatypes`).
- `vector-engine`: Bézier paths, shapes, styles, booleans (data) +
  `VectorBackend` trait (Vello or other behind the trait — no `wgpu`).
- `layout-engine`: frames, constraints, pages, row/column algorithm +
  `apply_to_scene` (layout computes, scene places — no `wgpu`/`egui`).
- `text-engine`: model → layout → glyph runs (shaping behind
  `FontProvider`; measurement for `layout-engine`, never direct rendering).
- `video-engine`: clips, editorial timeline, transitions,
  `FrameDecoder` trait, described compositing (shared `BlendMode`) — no player.
- `audio-engine`: timeline + DSP graph on `graph` (cycles rejected),
  `DspBackend` trait — OUTSIDE the Scene Graph, own business nodes.

They may depend on `core/*` and `packages/*` (non-UI); no
dependencies between engines (single documented exception:
`layout-engine` dev-dependency for tests on `text-engine`, never at runtime).

### core/
Cross-cutting foundation: `datatypes` (nodes, sockets, `Vec2`/`Rect`,
`BlendMode`, `RgbaBuf`), `ids`
(stable `EntityId` + shared monotonic `Revision`) and `scene`
(semantic Scene Graph: hierarchy, local transforms, derived world,
revisions — no wgpu or egui, no renderer state), `graph` (generic
dependency graph: nodes, edges, dirty propagation, topological
order — pure CPU, no domain logic) and `render-graph`
(scene-derived operations: `Source → Transform → Effect → Mask →
Blend → Output`, incremental sync by scope, abstract `Backend` trait —
no wgpu, egui, or UI) and `tiles` (spatial invalidation: `TileGrid`
rect ↔ tiles, `DirtyTiles` as bitset, separate CPU/GPU `TileCache`,
progressive `TileScheduler` visible/nearby/coarse first — pure CPU).

### apps/
Final applications combining packages, core and engines. Split by role
(`main.rs` eframe boot, `app.rs` state + channels, `layout.rs`
app-specific layout, `ui/` domain widgets). Each app is an independent
binary; photo is complete, video/audio are foundations awaiting their engines.

## Data flow (overview)

```
┌─────────────────────────────┐
│  eframe App (photo/video/   │
│  audio) — egui, UI thread   │
└──────────────┬──────────────┘
                │ PhotoCommandQueue
                │ (ActionQueue<A>, drained per frame)
                ▼
┌─────────────────────────────┐      ┌───────────────────────────┐
│  PhotoApp / Behavior        │──────│  PhotoUiContext           │
│  - ActionQueue              │      │  (egui Context, theme,    │
│  - docs: Vec<Arc<Document>> │      │   icons, Catalog<Fr>)     │
│  - dock layout              │      └───────────────────────────┘
└───────┬─────────────────────┘
         │ PhotoAction
         │
         ▼
┌─────────────────────────────┐     ┌──────────────────────────────┐
│  ui/ (domain widgets)       │────▶│  engine_bridge (worker)      │
│  - draw_menu_bar → Catalog  │     │  (PhotoEngineCommand)        │
│  - canvas (state-only draw) │     │  - sync / apply / composite  │
│  - dock/panels              │     │  - undo-redo, history        │
│                             │     └──────────────┬───────────────┘
└─────────────────────────────┘                    │
                                                    │ mpsc (LayersChanged,
                                                    │  ProjectSaved, PerfMetrics)
                                                    ▼
                                           ┌─────────────────────────────┐
                                           │ photo-engine                  │
                                           │  - Document / layers          │
                                           │  - CPU or wgpu compositing    │
                                           │  - RenderGraph                │
                                           │  - TileCache (GPU)            │
                                           └─────────────────────────────┘
```

- `app-shell` provides the generic `ActionQueue<A>` + `DockTab`
  model (queue + docks), reused by photo / video / audio.
- The worker replies over `mpsc`; the app reads non-blocking
  (`try_recv`) frame by frame and updates its state (state-only: no
  regenerated pixels).

## Dependency rules

1. `packages/`, `cygnus-ui/` and `cygnus-i18n/` NEVER depend on `engines/` or `apps/`
2. `engines/` may depend on `core/` and non-UI `packages/`; `datatypes` depends only on `serde`
3. `cygnus-ui/` may depend on `cygnus-i18n/` (panel titles, translator); never the reverse
4. `apps/` may depend on `core/`, `engines/`, `cygnus-ui/`, `cygnus-i18n/` and `packages/`
4. No circular dependencies
5. No dependencies between apps

Check: `cargo tree -p <crate> --depth 1`.

## Rendering model

- **State-only**: a setting (opacity, position…) never regenerates pixels;
  it applies at GPU draw.
- **Rendering**: CPU composite preview computed worker-side (background
  thread, capped size) and uploaded as an egui texture app-side; native
  zero-copy wgpu path (`register_native_texture`) planned.
- **Engine→UI boundary**: the worker replies `LayersChanged { layers,
  preview, can_undo, can_redo }`, polled non-blocking (`try_recv`)
  each frame — single conversion point.

## Building

```bash
cargo build --workspace --release   # whole workspace
cargo build -p photo --release      # a single app
cargo run -p photo                  # launch an app
cargo test -p photo-engine          # photo engine tests (golden compositing)
```
