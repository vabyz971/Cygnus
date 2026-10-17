---
covers: []
---
# AGENTS.md

Rust creative suite: Cargo workspace, **egui 0.36 (eframe) + wgpu 30** apps, GPL-3.0 license. Docs: `README.md` (architecture, roadmap), `ARCHITECTURE.md` (dependency rules).

## Environment
- Rust 2024 edition → toolchain **1.89+** required.
- Linux: enter `nix develop` (or direnv via `.envrc`) — provides cargo/clippy/rustfmt + Vulkan/Wayland with `LD_LIBRARY_PATH` already set.
- Without Nix: install `pkg-config vulkan-loader libxkbcommon wayland`.

## Commands
- Main app: `cargo run --release -p photo`. Other apps (foundations): `-p video`, `-p audio`.
- Pre-commit check: `cargo fmt --all -- --check`, then `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace`. The GitHub CI (`.github/workflows/ci.yml`) replays fmt+clippy+tests on every PR.
- Targeted tests: `cargo test -p photo-engine` (compositing, history, project). The "golden" tests in `engines/photo-engine/src/document/tests.rs` verify blend modes pixel by pixel — do not weaken them to make a refactor pass.

## Workspace packages
The crate name sometimes differs from the folder — use `-p` with the crate name:
| Folder | Crate |
|---|---|
| `apps/photo` / `apps/video` / `apps/audio` | `photo` / `video` / `audio` (entrypoint `src/main.rs`) |
| `core/datatypes` | `datatypes` (nodes, sockets, Vec2/Rect, shared BlendMode, RgbaBuf) |
| `core/scene` | `scene` (semantic Scene Graph: hierarchy, derived world, revisions) |
| `core/ids` | `ids` (stable `EntityId` + shared monotonic `Revision`) |
| `core/graph` | `graph` (generic dependencies: dirty propagation, topo — pure CPU) |
| `core/render-graph` | `render-graph` (scene-derived operations: partial build/sync, abstract backend — no wgpu or UI) |
| `core/tiles` | `tiles` (spatial invalidation: grid, dirty bitset, CPU/GPU cache, progressive scheduler — pure CPU) |
| `core/engine-runtime` | `engine-runtime` (generic engine worker: snapshot undo/redo with gesture coalescing, latest-value-wins batch folding, background thread loop + UI wake — no domain, UI or GPU) |
| `engines/photo-engine` | `photo-engine` (document, CPU/GPU compositing, history, project; `BlendMode`/`RgbaBuf` re-exported from `datatypes`) |
| `engines/vector-engine` | `vector-engine` (paths, shapes, styles, booleans + `VectorBackend` trait — no wgpu) |
| `engines/layout-engine` | `layout-engine` (frames, constraints, algorithm + `apply_to_scene` — no wgpu/egui) |
| `engines/text-engine` | `text-engine` (model → layout → glyph runs; measurement for layout) |
| `engines/video-engine` | `video-engine` (clips, timeline, transitions, decoder trait; legacy nodal registry kept) |
| `engines/audio-engine` | `audio-engine` (timeline + DSP graph on `graph`, DSP backend; outside Scene; legacy registry kept) |
| `cygnus-ui` | `cygnus-ui` (lib `cygnus_ui`, egui design system: theme, components, containers, viewport, dialogs) |
| `cygnus-i18n` | `cygnus-i18n` (lib `cygnus_i18n`, shared translations: generic `TextKey` + `Catalog`/`Language`, no per-app duplicates; all catalogs (generic + photo/video/audio domains) live here, apps hold no tables) |
| `packages/app-shell` | `app-shell` (lib `app_shell`, app skeleton: `ActionQueue`, generic dock on `DockTab`) |
| `packages/math-utils` | `math-utils` (affine transform `Transform2D`; canonical Vec2 = datatypes) |
| `packages/file-utils` | `file-utils` (file errors, drag & drop, dialogs) |
| `packages/preferences` | `preferences` (persistent preferences, hardware, shortcuts) |

Allowed dependencies: `packages/*` + `cygnus-i18n` ← `cygnus-ui` + `core/*` ← `engines/*` ← `apps/*`. Packages, cygnus-ui and cygnus-i18n never depend on engines or apps; cygnus-ui may depend on cygnus-i18n (never the reverse); no dependencies between apps.

## Architecture rules (strict)
- Business logic → `engines/*` and `core/*`; widgets → `cygnus-ui`; apps = interface + orchestration only. No rendering logic in apps.
- **Engines are PURE: no UI dependency.** `photo-engine` knows neither egui nor its types; the document model carries pure buffers (`RgbaBuf`, `Arc<[u8]>`). The app sends commands via `mpsc` to a worker owning the `Document`, and receives snapshots + composite preview; the texture is uploaded app-side (`ViewportTextureCache`). Non-blocking egui loop (`try_recv` per frame).
- Engine→UI boundary: the worker replies `LayersChanged { layers, preview, can_undo, can_redo }`; the app converts the preview into an `egui::ColorImage` and feeds its `TextureHandle` (zero regeneration on zoom/pan — state-only).
- **"state-only"** rendering model: a setting (opacity, position…) never regenerates pixels/textures; it applies at GPU draw. Preserve this model at all costs.
- Rendering: CPU composite preview worker-side (capped, background thread); zero-copy native wgpu path (`register_native_texture`) planned app-side.

## History & persistence
- Undo/redo: full document snapshots (`engines/photo-engine/src/history.rs`) almost free thanks to shared `Arc<DynamicImage>`. Continuous gestures (sliders, rename, drag) go through `push_coalesced` — always push the PRE-mutation snapshot, never after.
- `.cygp` project format (`engines/photo-engine/src/project.rs`): versioned JSON, layers as PNG+base64. Any incompatible model change → bump `FORMAT_VERSION` and handle rejection cleanly.

## Photo app structure
Split by role (same pattern for future apps):
`main.rs` (eframe boot) · `app.rs` (PhotoApp + channels + `impl eframe::App`) · `layout.rs` (app-specific layout) · `ui/` (domain widgets: `layers/`, `canvas.rs`, `toolbar.rs`, `properties.rs`, `engine_bridge.rs`). Do not let it regrow into a monolithic main.rs.

## `cygnus-ui` architecture (layered, see lib.rs)
1. **`theme`** = ONLY source of colors/sizes/radii (tokens `colors`, `spacing`, `radius`, `typography`).
2. **`components`** = generic components (`Button`, `Slider`, `Select`, `Tabs`, inputs, `Toggle`, `IconButton`, `icon_button`/`Icon` — only contact with `egui_material_icons` via `icons::glyph` — `ReorderableList`). A component NEVER hardcodes a color/size: it references the tokens.
3. **Containers** (`containers`: `Panel`, `Split` + `SplitState`, `Collapsible`, `Toolbar`, `Section`, `Card`, `Stack`) → 4. **Generic viewport** (texture display + zoom/pan, canonical `ViewportState`) → 5. **`dialogs`** (modal, file picker, progress) → 6. **`utils`** (generic drag state).
- App-specific elements stay in `apps/<app>/src/ui/`. Promote to `cygnus-ui` only when a 2nd app needs it (and never domain types: cygnus-ui stays domain-agnostic, verified by `scripts/check_uikit_domain_agnostic.sh`).
- **No hardcoded colors outside `theme/`** — including canvases (selection uses `item_selected`, indicators `drop_indicator`). Text sizes: go through `typography`.

## Conventions
- `unwrap()`/`expect()` forbidden outside tests; no emoji in code or commits.
- Short commits prefixed by the app: `photo: fix blend-mode offset jump`.
- Each `.rs` file starts with the GPL v3 header (copy the one from `apps/photo/src/main.rs`).
- Theme: palette/tokens implemented in `cygnus-ui/src/theme/`; Hanken Grotesk font and Material icons loaded from `assets/fonts/`.

<!-- graft:start -->
## Graft — repo context graph

This repo is indexed in `graft/`: small linked markdown nodes that explain each
system and carry exact file:line spans, kept in sync with the code through git.

For ANY task here — understanding how something works, finding where code lives,
or scoping a change — get context from the graph before grepping or opening
source files. Re-ask freely (it's cheap) and reuse literal identifiers you
already have (symbol, error string, file name) as the query. New to this repo?
Run `graft map` first — a token-budgeted orientation (dir clusters, hubs,
hotspots), no LLM, no key.

- Run `graft ask "<your question>" --source` → ranked nodes with the relevant
  code spans inlined (each hit's ≤8-line crux by default; `--full` for whole
  definitions when the crux isn't enough). Match the tool to the task shape:
  for understanding or editing, the top node IS the answer — cite its
  `covers:` file:line spans and edit straight from `--source`. For
  exhaustive tasks ("every occurrence / every caller of this pattern"), ranked
  results are top-N, not complete — run `graft grep "<literal>"` instead
  (exhaustive over indexed files, grouped by enclosing symbol), falling back
  to raw `grep -rn` only for unindexed files.
- `graft skeleton <file>` → every definition's signature + span, ~10× cheaper
  than reading the file; use it to skim an API surface.
- `graft callers <symbol>` gives precomputed, exact edges — who calls this.
  Add `--direction out` for what it calls, or `--depth N` to walk
  transitively for the full blast radius. For structural questions, skip
  ranking and use this directly.
- Or browse: `graft/INDEX.md` lists every node; follow the links.
- Monorepos and folders of multiple repos rank fairly across sub-projects —
  hits carry `[scope/]` labels naming which one they're from. Narrow with
  `graft ask "<task>" --in <scope>/` once you know where you're working.

If a returned span is truncated ("+N more lines"), open the file at that exact
range before finalizing. Only open source files when a node genuinely lacks a
needed detail, and then at the exact file:line the node points to — never
re-read whole files.

After big code changes, refresh the graph with `graft build` (deterministic,
no API key, $0). The graph is a derived index: it may lag the source tree, so
the code and `Cargo.toml` remain the source of truth.
<!-- graft:end -->
