# Roadmap — Cygnus Photo (v0.8 → v1.0+)

> Milestones + definitions of done (DoD). Fine-grained tracking lives in `docs/SUIVI.md`.
> Before coding a milestone: `graft ask "<topic>" --source`.

## v0.8 "Usable daily"

- [x] **v0.8.1 Persistence + shortcuts + history**
  Scope: Save/Save As/Open `.cygp` menus (`apps/photo/src/ui/menubar.rs`,
  `apps/photo/src/persistence/`), `shortcuts.rs` → `app.rs::handle_action` wiring,
  minimal History panel (reads `engines/photo-engine/src/history.rs`).
  DoD: open → edit → save → reopen without loss.
- [x] **v0.8.2 Basic transforms**: crop, H/V flip, 90° rotation (`ui/viewport/canvas.rs`, `math-utils`).
  DoD: every transform goes through PRE-mutation `push_coalesced`.
- [x] **v0.8.3 Adjustments**: Levels + Curves + HSL, param editing in the inspector
  (`engines/photo-engine/src/nodes/`, `apps/photo/src/ui/features/inspector/`).
  DoD: presets + re-editing after save/load.
- [x] **v0.8.4 Export v1 + histogram**: WebP/TIFF (`export.rs`), size estimate, histogram on `preview_buf`.
  DoD: export with no PNG/JPEG regression.

## v0.9 "Photo Persona"

- [x] Selections: rectangle/ellipse (done, O007), lasso + wand (done, O009) / pen.
- [ ] Paintable masks: density/feather/invert (`paint.rs`, `LayerMask`).
- [ ] Live Filters + 1st Layer Style (drop shadow/glow).
- [ ] Brush v2: hardness/flow/spacing/stabilization (done, O008), stamp/clone, bucket/gradient.
- [ ] Layer multi-select + align/distribute; 12–15 blend modes.
  DoD: masked portrait composite + local retouch, re-editable after reload.

## v0.10 "Vector + Text" (makes the modes useful)

- [ ] Wire `vector-engine` to `Vector` mode: shapes, pen, booleans.
- [ ] Wire `text-engine`: point/paragraph text, Character/Paragraph panels.
- [ ] Vector masks + guides/snapping.
  DoD: photo poster + shapes + title, all re-editable.

## v0.11 "Layout / StudioLink-like" (differentiator)

- [ ] Wire `layout-engine` to `Layout` mode: pages, frames, `apply_to_scene`.
- [ ] Unified `.cygp` document (`FORMAT_VERSION` bump + migration).
- [ ] Named export slices.
  DoD: same file editable in Pixel/Vector/Layout without conversion.

## v1.0 "Pro"

- [ ] Develop Persona (non-destructive RAW), Liquify, Tone Mapping (HDR), Export Persona (batch).
- [ ] Pro color: soft-proofing, 16-bit.
- [ ] Macros/Actions (record `PhotoAction` → replay).
