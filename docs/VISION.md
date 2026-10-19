# Vision — Cygnus Photo

> 1 page. Read this file before any scope decision.
> Code map: `graft map` / `graft ask "<question>" --source`.

## Positioning

Cygnus Photo targets the **Affinity (by Canva)** model, not a Photoshop clone:

- **Personas**: spaces tailored to each stage (Photo / Develop / Liquify / Tone Mapping / Export), not every tool on a single screen.
- **Non-destructive by default**: Live Filters, re-editable RAW layer, composable masks.
- **StudioLink-like**: photo + vector + text + layout in **a single `.cygp` document** (app `Pixel` / `Vector` / `Layout` modes, see `apps/photo/src/ui/modebar.rs`).

## Principles

1. Pure engines, apps = interface + orchestration (`ARCHITECTURE.md`).
2. **State-only** rendering: a setting never regenerates pixels.
3. Every destructive setting has a non-destructive equivalent wherever Affinity has one.
4. Quality > quantity: 10 finished live filters > 70 previews.

## Non-goals (decided, see `docs/DECISIONS.md`)

- No byte-exact PSD parity in the Photocraft style: correct import/export is enough.
- No `.8bf` plugins / ExtendScript / Adobe cloud.
- No generative AI before v1.0.
- No Lightroom-style RAW catalog (editor, not DAM).
