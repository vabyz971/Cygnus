# Roadmap — Cygnus Photo (v0.8 → v1.0+)

> Jalons + définitions de fini (DoD). Le suivi fin vit dans `docs/SUIVI.md`.
> Avant de coder un jalon : `graft ask "<sujet>" --source`.

## v0.8 « Utilisable au quotidien »

- [ ] **v0.8.1 Persistance + raccourcis + historique**
  Périmètre : menus Save/Save As/Open `.cygp` (`apps/photo/src/ui/menubar.rs`,
  `apps/photo/src/persistence/`), câblage `shortcuts.rs` → `app.rs::handle_action`,
  panneau Historique minimal (lit `engines/photo-engine/src/history.rs`).
  DoD : ouvrir → éditer → sauver → rouvrir sans perte.
- [ ] **v0.8.2 Transform de base** : crop, flip H/V, rotation 90° (`ui/viewport/canvas.rs`, `math-utils`).
  DoD : chaque transform passe par `push_coalesced` PRÉ-mutation.
- [ ] **v0.8.3 Réglages** : Niveaux + Courbes + HSL, édition params dans l'inspecteur
  (`engines/photo-engine/src/nodes/`, `apps/photo/src/ui/features/inspector/`).
  DoD : presets + ré-édition après save/load.
- [ ] **v0.8.4 Export v1 + histogramme** : WebP/TIFF (`export.rs`), estimation taille, histogramme sur `preview_buf`.
  DoD : export Ø régression PNG/JPEG.

## v0.9 « Photo Persona »

- [ ] Sélections : rectangle/ellipse/lasso + baguette + plume.
- [ ] Masques peignables : densité/plume/inversion (`paint.rs`, `LayerMask`).
- [ ] Live Filters + 1er Layer Style (ombre portée/lueur).
- [ ] Pinceau v2 : dureté/espacement/stabilisation, tampon/clone, pot/dégradé.
- [ ] Multi-sélection calques + align/distribuer ; 12–15 blend modes.
  DoD : composite portrait masqué + retouche locale, ré-éditable après reload.

## v0.10 « Vector + Texte » (rend les modes utiles)

- [ ] Brancher `vector-engine` au mode `Vector` : formes, plume, booléens.
- [ ] Brancher `text-engine` : texte point/paragraphe, panneaux Caractère/Paragraphe.
- [ ] Masques vectoriels + guides/aimantation.
  DoD : affiche photo + formes + titre, tout ré-éditable.

## v0.11 « Layout / StudioLink-like » (différenciateur)

- [ ] Brancher `layout-engine` au mode `Layout` : pages, frames, `apply_to_scene`.
- [ ] Document unifié `.cygp` (incrément `FORMAT_VERSION` + migration).
- [ ] Slices d'export nommées.
  DoD : même fichier éditable en Pixel/Vector/Layout sans conversion.

## v1.0 « Pro »

- [ ] Develop Persona (RAW non-destructif), Liquify, Tone Mapping (HDR), Export Persona (batch).
- [ ] Couleur pro : soft-proofing, 16-bit.
- [ ] Macros/Actions (enregistre `PhotoAction` → rejoue).
