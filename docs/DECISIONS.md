# Décisions (ADR) — Cygnus Photo

> Chaque décision actée ici est **définitive** : on ne la rouvre que via une
> nouvelle entrée (statut `remplacée par`). Ça évite de revenir dessus.
> Format : 5 lignes max par décision.

| ID | Date | Décision | Contexte | Statut |
|----|------|----------|----------|--------|
| D001 | 2026-10-05 | Live-filters façon Affinity, pas 70 filtres Photoshop | 3 filtres actuels (`brightness_contrast`, `blur`, `color_correct`) déjà non-destructifs | actée |
| D002 | 2026-10-05 | `.cygp` = JSON versionné + PNG base64, `FORMAT_VERSION` + migration | `engines/photo-engine/src/project.rs` v4 | actée |
| D003 | 2026-10-05 | Pas de parité PSD byte-exact (Photocraft vise 307/309) | import/export correct suffit, pas de round-trip parfait | actée |
| D004 | 2026-10-05 | Personas + StudioLink-like plutôt que parité Photoshop | `PhotoEditMode::{Pixel,Vector,Layout}` déjà en place, moteurs dormants à brancher | actée |
| D005 | 2026-10-05 | Pas d'IA générative / plugins `.8bf` avant v1.0 | focus retouche + vector + layout d'abord | actée |
| D006 | 2026-10-05 | Accusés/erreurs worker émis APRÈS l'état dans `apply_batch` | un `LayersChanged` efface le statut ; l'accusé (save/export, erreur) doit survivre | actée |

## Modèle pour une nouvelle décision

```md
| D006 | AAAA-MM-JJ | Titre | 1 phrase de contexte + fichiers concernés | actée |
- Conséquence : ...
- Remplace : D00X (le cas échéant).
```
