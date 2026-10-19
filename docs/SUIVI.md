# Objective tracking — Cygnus Photo

> **The anti-backsliding file.** A `done` objective never reopens:
> create a new objective referencing it.
> Rule: `status ∈ {todo, in progress, done, frozen}` — a single `in progress` at a time.
> Check: `cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings`
> + `cargo test --workspace` + `bash scripts/check_theme_rules.sh`.

| ID | Objective | Milestone | Status | Exit criterion (DoD) | Graft points / code | Verified on |
|----|----------|-------|--------|--------------------------|---------------------|------------|
| O001 | Save/Save As/Open menus `.cygp` | v0.8.1 | done | lossless round-trip, clean refusal on unknown version | `apps/photo/src/ui/menubar.rs`, `apps/photo/src/persistence/`, `engines/photo-engine/src/project.rs` | 2026-10-05 |
| O002 | Wired shortcuts | v0.8.1 | done | all 23 `PhotoShortcut` trigger `handle_action` | `apps/photo/src/shortcuts.rs`, `apps/photo/src/app.rs` | 2026-10-05 |
| O003 | Minimal History panel | v0.8.1 | done | undo/redo list, click = state jump | `apps/photo/src/ui/features/history/`, `apps/photo/src/layout/dock.rs` | 2026-10-05 |
| O004 | Crop + flip + 90° rotation | v0.8.2 | done | `push_coalesced` PRE-mutation, golden tests OK | `apps/photo/src/ui/engine_bridge/worker.rs`, `engines/photo-engine/src/document/tree.rs` | 2026-10-05 |
| O005 | Editable Levels + Curves + HSL | v0.8.3 | done | presets + re-editing after reload | `engines/photo-engine/src/nodes/{levels,curves,hsl}.rs`, `engines/photo-engine/src/presets.rs`, `apps/photo/src/ui/features/inspector/` | 2026-10-17 |
| O006 | WebP/TIFF export + histogram | v0.8.4 | done | no PNG/JPEG regression | `engines/photo-engine/src/{export,histogram}.rs`, `apps/photo/src/ui/dialogs.rs`, `apps/photo/src/ui/features/histogram/` | 2026-10-17 |
| O007 | Rectangle / ellipse selection | v0.9 | done | strokes and mask paint clipped by the selection (live preview included), mask from selection undoable, select all / deselect / invert | `engines/photo-engine/src/selection.rs`, `engines/photo-engine/src/session/worker.rs`, `apps/photo/src/layout/central_view.rs`, `apps/photo/src/ui/menubar.rs` | 2026-10-18 |
| O008 | Brush v2: hardness, flow, spacing, stabilizer | v0.9 | done | default dynamics bit-identical to the hard brush, soft edge + flow build-up identical in live overlay and commit, brush and mask strokes | `engines/photo-engine/src/paint.rs`, `engines/photo-engine/src/interaction.rs`, `apps/photo/src/ui/optionsbar.rs`, `apps/photo/src/ui/viewport/canvas.rs` | 2026-10-18 |

## Log (never erase, append a line)

| Date | ID | Event |
|------|----|-----------|
| 2026-10-05 | — | Created VISION / ROADMAP / DECISIONS / TRACKING skeletons. |
| 2026-10-05 | O001 | Done: `SaveProject`/`LoadProject` worker + `ProjectSaved`, `.cygp` pickers ui-kit, File menus, per-doc `project_path`, D006 (acks last in `apply_batch`). |
| 2026-10-05 | O002 | Done: keyboard dispatch → `PhotoAction` via resolver (21/23 mapped, `ZoomFit`/`OpenPreferences` with no documented target), D007 (suspended while typing), `key_to_string` case fix in `preferences`. |
| 2026-10-05 | O003 | Done: FR-labeled steps worker-side (`HistoryStep`), `UndoSteps`/`RedoSteps` (single render), History tab docked with Layers, `ensure_history_tabbed` for older JSON. |
| 2026-10-05 | O004 | Done: `Document::rotate` (center preserved) + `FlipHorizontal/Vertical`, `RotateClockwise/Counterclockwise`, `CropToDocument` (intersection, no-op if content, refused if transformed/out of frame), Layer menu, history labels. Interactive crop (mouse rect) = future objective. |
| 2026-10-06 | — | Cursor fix: `menu_item_enabled` in Small like `menu_item` (uniform highlights) + test. |
| 2026-10-06 | — | Layer tree (D008), worker+engine filter/mask on/off toggle, duplication-free inspector (D009), Transformations submenu (menubar + canvas right-click + row right-click), i18n sweep ~90 keys (D010). |
| 2026-10-06 | — | History fix: click = row becomes head (off-by-one), Initial-state row, filled scroll (`auto_shrink` off). Attachment chevron fix (`load_with_default_open`). Mask painting (`paint_mask` engine + `PaintMask` worker, white/black). Stroke smoothing (`resample_stroke` Chaikin on commit). |
| 2026-10-17 | O005 | Done: `levels` / `curves` / `hsl` live filters (CPU, rayon; Levels and Curves through one RGB LUT, Curves = monotone cubic Fritsch-Carlson, points as `Text` param `"x,y;…"` so no `FORMAT_VERSION` bump), engine presets (`presets_for`, `"default"` reset + 4-6 per type, stable ids translated app-side), `SetFilterText` (coalesced like `SetFilterParam`) + `ApplyFilterPreset` (one discrete step) worker commands, inspector preset picker ("Custom" when no match) + natural slider order + interactive curve editor, `.cygp` round-trip test. Curves are master RGB only (per-channel = future objective). |
| 2026-10-17 | O006 | Done: WebP (lossless, alpha kept: the bundled encoder has no lossy mode) + TIFF (uncompressed RGBA) in `export_image`, in-memory `encode_image`, `estimate_export_size` (downscale beyond 1 MP, area-scaled), `EstimateExportSize` worker command (no history) + debounced dialog request (250 ms), quality slider JPEG-only, Histogram dock tab (RGB / luminance, sampled `Histogram::from_rgba` on the displayed preview, cached per render revision), `channel_red/green/blue` theme tokens. PNG/JPEG export tests unchanged and green. |
| 2026-10-17 | — | Clippy green on the current toolchain (doc indentation, `as_chunks`, struct-literal transforms, `too_many_arguments` allowances like `paint.rs`). |
| 2026-10-17 | — | Perf: GPU drag proxy (stack split once per gesture, layer drawn as a textured quad, no worker traffic until release; 4K: 117 ms/frame → 0, split 11 ms) + display-scale compositing (appearances and composite at preview scale; 4K: opacity 146 → 13 ms/tick, visibility 103 → 10 ms, add filter 1350 → 725 ms). Exports stay full resolution. |
| 2026-10-17 | — | Fix: workers wake the UI after answering (renders used to appear only on the next input event). Fix: new canvas tab could land in the Layers tab group once the empty canvas slot was simplified away. |
| 2026-10-17 | — | UI: Rerun-style tokens and full egui style, DockStyle for tiles, visible Button variants, framed inputs, modal footer, personas in the context bar (2 top rows instead of 3), fit margin, true zoom % in the canvas tab, French accents. |
| 2026-10-18 | — | UI: workspace redesign after the atelier mockup (client-side title bar, persona tabs + quick actions, context bar scrub fields, headerless side column, status bar facts). |
| 2026-10-18 | O007 | Done: engine `Selection` (doc-space rect/ellipse, inversion, 1 px anti-aliased coverage, `clip_for_layer` through the layer transform), worker `SetSelection` (session state: no history, no render, not saved in `.cygp`) clipping `PaintStroke` / `PaintMask` and the live ink overlay, `AddMaskFromSelection` (one history step), Select tool (M) with rect/ellipse marquee and marching ants, Selection menu + Ctrl+A / Ctrl+D / Ctrl+Shift+I. Lasso, wand, pen and undoable selection changes = future objectives. |
| 2026-10-18 | O007 | Fix: the central view cleared the shared stroke buffer every frame for non-paint tools, collapsing each marquee (nothing drawn, release deselected); end-to-end drag test through `PhotoApp::draw`. |
| 2026-10-18 | O008 | Done: `BrushDynamics` (hardness smoothstep falloff, per-dab flow build-up, spacing as a fraction of the diameter; default = legacy hard path, bit-identical), `DabWalker` carrying dab spacing across packets so the live overlay and the commit place the same dabs, worker `SetBrushDynamics` session state (sent before each stroke) for layer and mask strokes, app-side stabilizer (lazy mouse feeding both overlay and commit), context-bar fields matching the mockup. Stamp/clone, bucket and gradient = future objectives. |
