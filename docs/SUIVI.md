# Suivi des objectifs — Cygnus Photo

> **Le fichier anti-retour-en-arrière.** Un objectif `fait` ne se rouvre jamais :
> on crée un nouvel objectif qui le référence.
> Règle : `statut ∈ {à faire, en cours, fait, gelé}` — un seul `en cours` à la fois.
> Vérification : `cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings`
> + `cargo test --workspace` + `bash scripts/check_theme_rules.sh`.

| ID | Objectif | Jalon | Statut | Critère de sortie (DoD) | Points graft / code | Vérifié le |
|----|----------|-------|--------|--------------------------|---------------------|------------|
| O001 | Menus Save/Save As/Open `.cygp` | v0.8.1 | fait | round-trip sans perte, refus propre si version inconnue | `apps/photo/src/ui/menubar.rs`, `apps/photo/src/persistence/`, `engines/photo-engine/src/project.rs` | 2026-10-05 |
| O002 | Raccourcis câblés | v0.8.1 | à faire | les 23 `PhotoShortcut` déclenchent `handle_action` | `apps/photo/src/shortcuts.rs`, `apps/photo/src/app.rs` | — |
| O003 | Panneau Historique minimal | v0.8.1 | à faire | liste undo/redo, clic = saut d'état | `engines/photo-engine/src/history.rs` | — |
| O004 | Crop + flip + rotation 90° | v0.8.2 | à faire | `push_coalesced` PRÉ-mutation, golden tests OK | `apps/photo/src/ui/viewport/canvas.rs`, `packages/math-utils` | — |
| O005 | Niveaux + Courbes + HSL éditables | v0.8.3 | à faire | presets + ré-édition après reload | `engines/photo-engine/src/nodes/`, `apps/photo/src/ui/features/inspector/` | — |
| O006 | Export WebP/TIFF + histogramme | v0.8.4 | à faire | Ø régression PNG/JPEG | `engines/photo-engine/src/export.rs` | — |

## Journal (ne jamais effacer, ajouter une ligne)

| Date | ID | Événement |
|------|----|-----------|
| 2026-10-05 | — | Création des squelettes VISION / ROADMAP / DECISIONS / SUIVI. |
| 2026-10-05 | O001 | Fait : `SaveProject`/`LoadProject` worker + `ProjectSaved`, pickers `.cygp` ui-kit, menus Fichier, `project_path` par doc, D006 (accusés en dernier dans `apply_batch`). |
