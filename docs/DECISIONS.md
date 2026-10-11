# Decisions (ADR) — Cygnus Photo

> Each decision recorded here is **final**: it is only reopened via a
> new entry (`superseded by` status). This avoids relitigating it.
> Format: 5 lines max per decision.

| ID | Date | Decision | Context | Status |
|----|------|----------|----------|--------|
| D001 | 2026-10-05 | Affinity-style live filters, not 70 Photoshop filters | 3 current filters (`brightness_contrast`, `blur`, `color_correct`) already non-destructive | accepted |
| D002 | 2026-10-05 | `.cygp` = versioned JSON + base64 PNG, `FORMAT_VERSION` + migration | `engines/photo-engine/src/project.rs` v4 | accepted |
| D003 | 2026-10-05 | No byte-exact PSD parity (Photocraft targets 307/309) | correct import/export is enough, no perfect round-trip | accepted |
| D004 | 2026-10-05 | Personas + StudioLink-like over Photoshop parity | `PhotoEditMode::{Pixel,Vector,Layout}` already in place, dormant engines to wire up | accepted |
| D005 | 2026-10-05 | No generative AI / `.8bf` plugins before v1.0 | retouch + vector + layout focus first | accepted |
| D006 | 2026-10-05 | Worker acks/errors emitted AFTER state in `apply_batch` | a `LayersChanged` clears the status; the ack (save/export, error) must survive | accepted |
| D007 | 2026-10-05 | Shortcuts suspended while typing (`egui_wants_keyboard_input`) | typing `b` while renaming must not switch tools; no double undo with fields | accepted |
| D008 | 2026-10-06 | Layer tree: selectable + collapsible attachments, no DnD for them | click = focus (carrier selected), eye on/off, per-carrier chevron; layer DnD unchanged | accepted |
| D009 | 2026-10-06 | Duplication-free inspector (opacity/visibility/blend = Layers panel) | single source of settings; inspector = identity + focused attachment | accepted |
| D010 | 2026-10-06 | I18n: generic verbs → ui-kit `TextKey`, domain → `PhotoTextKey` | transient statuses, worker/engine messages and layer/file names excluded (data or no language plumbing) | accepted |

## Template for a new decision

```md
| D006 | YYYY-MM-DD | Title | 1 context sentence + affected files | accepted |
- Consequence: ...
- Supersedes: D00X (if applicable).
```
