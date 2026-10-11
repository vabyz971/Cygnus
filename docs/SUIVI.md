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
| O005 | Editable Levels + Curves + HSL | v0.8.3 | todo | presets + re-editing after reload | `engines/photo-engine/src/nodes/`, `apps/photo/src/ui/features/inspector/` | — |
| O006 | WebP/TIFF export + histogram | v0.8.4 | todo | no PNG/JPEG regression | `engines/photo-engine/src/export.rs` | — |

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
