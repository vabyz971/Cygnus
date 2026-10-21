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

Status: done (O007–O018). Follow-ups noted in `docs/SUIVI.md` (GPU layer
styles, Bezier pen selection, multi-layer move / delete / grouping).

- [x] **Selections**
  - [x] Rectangle / ellipse marquee, select all / deselect / invert, mask from selection (O007).
  - [x] Lasso + magic wand (tolerance, contiguous) (O009).
  - [x] Add / subtract / intersect with Shift / Alt (O013).
  - [x] Pen (polygonal) selection (O015).
  - [x] Undoable selection changes (O016).
- [x] **Paintable masks**: density / feather / invert, non-destructive, `.cygp` v7 (O010).
- [x] **Live Filters + 1st Layer Style**
  - [x] Live filters as sub-layers with masks and presets (v0.8.3).
  - [x] Drop shadow + outer glow layer styles, `.cygp` v9 (O017).
- [x] **Brush v2**
  - [x] Hardness / flow / spacing / stabilizer (O008).
  - [x] Paint bucket + linear gradient (O011).
  - [x] Clone stamp (O014).
- [x] **Layers**
  - [x] 14 blend modes, CPU and GPU, `.cygp` v8 (O012).
  - [x] Multi-select + align / distribute (O018).
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
