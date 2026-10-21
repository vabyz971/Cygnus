# Roadmap — Cygnus Photo (v0.8 → v1.0+)

> Milestones + definitions of done (DoD). Fine-grained tracking lives in `docs/SUIVI.md`.
> Before coding a milestone: `graft ask "<topic>" --source`.
>
> Target: the Affinity model (`docs/VISION.md`) — ONE application and ONE `.cygp`
> document edited through three studios: **Pixel** (Photo), **Vector** (Designer)
> and **Layout** (Publisher). Objectives get their `Oxxx` id in `docs/SUIVI.md`
> when they start; within a milestone, the shortest ones go first.

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

## v0.10 "Unified document" (foundation for the three studios)

Everything the Vector and Layout studios stand on: one layer tree that holds
pixel, vector, text and frame layers, real document units, and the shared
editing aids (rulers, guides, snapping).

- [ ] **One layer tree for every kind**: Pixel / Vector / Text / Frame / Group / Adjustment
  nodes in the same `Document`, each with transform, opacity, blend mode, masks, styles
  (`engines/photo-engine/src/document/`, `core/scene`). `.cygp` bump + migration.
- [ ] **Document setup**: units (px / mm / cm / in / pt), DPI, size presets, margins and
  bleed (stored, shown as overlays), resize document / canvas dialogs.
- [ ] **Rulers, guides, grid**: rulers in document units, drag guides from the rulers, guide
  manager, configurable grid (spacing, subdivisions).
- [ ] **Snapping engine** (shared, pure CPU): to guides, grid, document edges and centers,
  other layers' bounds and centers, with on-canvas hints.
- [ ] **Transform panel**: numeric X / Y / W / H / rotation / shear with an anchor picker,
  lock aspect ratio; works on one or several layers.
- [ ] **Multi-selection editing**: move, delete, duplicate, group / ungroup, lock / unlock,
  hide / show the whole selection; marquee-select layers on the canvas.
- [ ] **Navigator + view tools**: navigator panel, rotate view, fit / fill / 100 %,
  split view (before / after).
  DoD: one `.cygp` holding a photo, a shape and a guide-aligned group, measured in mm,
  survives save / reload unchanged.

## v0.11 "Vector studio" (Affinity Designer)

Wire `vector-engine` to the Vector mode: drawing, editing and styling paths, rendered
on the GPU (Vello) with a CPU reference for exports.

- [ ] **Pen tool** (Bézier): click / drag to place nodes and handles, close, continue a
  path, rubber band preview.
- [ ] **Node tool v2**: sharp / smooth / symmetric / smart nodes, convert, add / delete /
  break / join nodes, select several nodes, align nodes.
- [ ] **Pencil + vector brush**: freehand curves fitted to Béziers (smoothing amount).
- [ ] **Live shapes**: rectangle (per-corner radius), ellipse, polygon, star, line, arrow,
  with editable parameters kept non-destructive until "convert to curves".
- [ ] **Fill & stroke**: solid, linear / radial / conical gradients with an on-canvas
  gradient editor, stroke width / caps / joins / dashes / alignment / arrowheads,
  Fill-Stroke panel, swatches panel (document + app palettes).
- [ ] **Boolean operations**: add, subtract, intersect, exclude, divide; compound shapes
  that stay editable (`vector-engine` booleans as data).
- [ ] **Clipping and vector masks**: clip any layer into a vector shape, vector masks on
  pixel layers.
- [ ] **Symbols / linked copies** and repeat (duplicate with the last transform).
- [ ] **Vector export**: SVG import / export, vector PDF export, export of selected
  layers at 1x / 2x / 3x (PNG, SVG).
  DoD: an icon set and a poster drawn only with vector tools, re-editable after reload,
  exported to SVG / PDF that reopen in other editors.

## v0.12 "Text"

Give `text-engine` real fonts and put text everywhere: artistic text, frame text, text
on a path, in every studio.

- [ ] **Font backend**: system and bundled font discovery, real shaping (ligatures,
  kerning, bidi), font fallback, behind the `FontProvider` trait.
- [ ] **Artistic text** (point text): click to type, scales with its box.
- [ ] **Frame text**: text flowing in a rectangle or any vector shape, overflow marker.
- [ ] **Text on a path**: along any curve, with start / end handles.
- [ ] **Character and Paragraph panels**: family, weight, size, tracking, leading,
  baseline shift, case, alignment, indents, spacing before / after, hyphenation.
- [ ] **Text styles**: character and paragraph styles, applied and updated document-wide.
- [ ] **Text editing on canvas**: caret, selection, clipboard, IME, spell check hooks.
- [ ] **Convert text to curves**; text keeps live filters, masks and styles like any layer.
  DoD: a poster title, a text block flowing in a shape and text on a circle, all still
  editable after reload, rendered identically on screen and in exports.

## v0.13 "Layout studio" (Affinity Publisher)

Wire `layout-engine` to the Layout mode: pages, frames and long documents.

- [ ] **Pages and spreads**: multi-page documents, facing pages, Pages panel (add /
  remove / reorder / duplicate), page size per page.
- [ ] **Master pages**: margins, columns, recurring items, page numbers and sections.
- [ ] **Picture frames**: place images and vector files in frames, fit / fill / center
  modes, crop inside the frame, linked or embedded resources.
- [ ] **Linked text frames**: text flowing across frames and pages, columns, gutters,
  baseline grid, text wrap around objects.
- [ ] **Resource manager**: list of placed images (linked / embedded, missing, DPI),
  relink, embed.
- [ ] **Tables** (basic): rows, columns, cell text, borders and fills.
- [ ] **Print-ready PDF export**: bleed, crop marks, page ranges, spreads, embedded fonts;
  PDF/X profiles later with pro color.
  DoD: an 8-page magazine (master pages, linked text across pages, picture frames)
  exported to a PDF that prints with bleed and crop marks.

## v0.14 "StudioLink" (one document, three studios)

Make switching studios seamless, Affinity-style.

- [ ] **Studio switch on the same document**: Pixel / Vector / Layout change tools,
  context bar and panels, never the document (no import, no conversion).
- [ ] **Per-studio workspaces**: each studio remembers its dock layout and tool.
- [ ] **Edit in place**: double-click a placed photo in Layout to retouch it in Pixel,
  a shape to edit it in Vector, then return.
- [ ] **Shared panels**: Layers, History, Swatches, Styles, Assets, Transform stay
  available in every studio.
- [ ] **Assets panel**: reusable groups, shapes and images across documents.
  DoD: a flyer made in one sitting — photo retouched in Pixel, logo drawn in Vector,
  page laid out in Layout — in one `.cygp`, with no export between studios.

## v0.15 "Pixel pro" (close the gap with Affinity Photo)

- [ ] **GPU layer styles** (shadow, glow) and new styles: inner shadow, inner / outer
  stroke, bevel, color overlay, gradient overlay.
- [ ] **More live filters**: Gaussian / motion / radial / lens blur, unsharp mask, clarity,
  noise add / reduce, vignette, black and white, color balance, channel mixer.
- [ ] **Retouch tools**: healing brush, patch, spot removal, dodge / burn, sponge, smudge,
  blur / sharpen brushes, red-eye.
- [ ] **Selections v2**: Bézier pen selection, selection brush, refine edges (hair),
  color range, select by subject later.
- [ ] **Crop tool v2**: interactive crop with ratios, straighten, perspective crop.
- [ ] **Liquify persona**: push, twirl, pinch, bloat, freeze / thaw, mesh.
- [ ] **Brush engine v3**: texture / shape dynamics, pressure and tilt (tablets),
  brush library import (`.abr` basics).
  DoD: a full portrait retouch (healing, dodge / burn, liquify, frequency separation)
  re-editable after reload.

## v1.0 "Pro"

- [ ] **Develop persona**: non-destructive RAW development (exposure, white balance,
  lens corrections, noise), re-editable RAW layer.
- [ ] **Tone Mapping persona** (HDR merge and tone mapping) and **Export persona**
  (slices, batch export, presets).
- [ ] **Pro color**: ICC color management, soft-proofing, CMYK documents, 16-bit and
  32-bit float pipelines.
- [ ] **Interchange**: PSD import / export (layers, masks, text as best effort),
  PDF / SVG / AI-compatible import, EPS export.
- [ ] **Macros / Actions**: record `PhotoAction`s, replay, batch over files.
- [ ] **Performance and robustness**: 100+ layers and 100 MP documents stay fluid,
  autosave and crash recovery, large-document memory budget.
- [ ] **Release quality**: installers (Linux, Windows, macOS), help, onboarding,
  complete translations, accessibility pass.
  DoD: a professional can deliver a photo retouch, a logo and a printed brochure
  without leaving Cygnus.
