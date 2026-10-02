---
covers: []
---
# Architecture de Cygnus

## Vue d'ensemble

Cygnus est une suite créative professionnelle composée de trois applications
indépendantes (Photo, Vidéo, Audio) qui partagent un socle commun : moteurs métier,
graphe nodal générique, widgets et bibliothèques utilitaires.

## Structure des dossiers

```
apps/       Applications finales (binaires indépendants)
engines/    Moteurs métier PURS — zéro dépendance UI
core/       Socle commun : datatypes
packages/   Bibliothèques réutilisables : ui-kit, app-shell, math-utils, file-utils
assets/     Ressources partagées (polices)
```

### packages/
Bibliothèques partagées réutilisables entre toutes les applications.
- `ui-kit` (crate `ui_kit`) : design system egui en couches — `theme`
  (seule source des couleurs/tailles, tokens dans `theme/`),
  `components` génériques (dont `icon_button`, `ReorderableList`),
  `containers`, `viewport` pan/zoom générique, `dialogs`. Strictement
  domain-agnostic : aucun type métier (vérifié par
  `scripts/check_uikit_domain_agnostic.sh`).
- `app-shell` (crate `app_shell`) : squelette d'app générique —
  `ActionQueue<A>` (file d'actions drainée par frame) et logique de
  dock `egui_tiles` générique sur le trait `DockTab` (`has_tab`,
  `ensure_tab`, `push/remove_canvas_tab`, `reconcile_canvases`,
  `default_tree`, `set_linear_shares`). Dépend uniquement de `egui`,
  `egui_tiles`, `serde`, `uuid` : jamais d'engines ni d'apps, aucun
  type métier (le `Behavior` — titres, contenus — reste dans chaque
  app).
- `math-utils` : transformation affine 2D canonique (`Transform2D`) ;
  le `Vec2` canonique reste `datatypes::Vec2`, réexporté.
- `file-utils` : erreurs fichiers, types drag & drop et dialogues.

Ces packages ne doivent JAMAIS dépendre des engines ni des apps.

### engines/
Moteurs métier spécifiques à chaque domaine, strictement purs :
aucune connaissance d'egui ou de ses types. Les buffers portés par le modèle
document restent purs (`RgbaBuf`, `Arc<[u8]>`) ; les apps envoient des
commandes via `mpsc` à un worker propriétaire du `Document` et reçoivent
snapshots + aperçu composite (conversion texture côté app).
- `photo-engine` : document, compositing CPU/GPU, historique, projet `.cygp`
  (raster uniquement ; `BlendMode`/`RgbaBuf` viennent de `datatypes`).
- `vector-engine` : paths Bézier, formes, styles, booléens (données) +
  trait `VectorBackend` (Vello ou autre derrière le trait — sans `wgpu`).
- `layout-engine` : frames, contraintes, pages, algo ligne/colonne +
  `apply_to_scene` (le layout calcule, la scène place — sans `wgpu`/`egui`).
- `text-engine` : modèle → layout → glyph runs (façonnage derrière
  `FontProvider` ; mesure pour `layout-engine`, jamais de rendu direct).
- `video-engine` : clips, timeline éditoriale, transitions, trait
  `FrameDecoder`, compositing décrit (`BlendMode` partagé) — pas de lecteur.
- `audio-engine` : timeline + graphe DSP sur `graph` (cycles rejetés),
  trait `DspBackend` — HORS Scene Graph, nœuds métier propres.

Ils peuvent dépendre de `core/*` et de `packages/*` (hors UI) ; pas de
dépendances entre engines (seule exception documentée : `layout-engine`
en dev-dependency de test vers `text-engine`, jamais au runtime).

### core/
Socle transverse : `datatypes` (nœuds, sockets, `Vec2`/`Rect`, `BlendMode`,
`RgbaBuf`), `ids`
(`EntityId` stable + `Revision` monotone partagés) et `scene`
(Scene Graph sémantique : hiérarchie, transforms locaux, monde dérivé,
révisions — sans wgpu ni egui, sans état renderer), `graph` (graphe de
dépendances générique : nœuds, arêtes, propagation dirty, ordre
topologique — CPU pur, sans logique de domaine) et `render-graph`
(opérations dérivées de la scène : `Source → Transform → Effect → Mask →
Blend → Output`, sync incrémental par portée, trait `Backend` abstrait —
sans wgpu, ni egui, ni UI) et `tiles` (invalidation spatiale : `TileGrid`
rect ↔ tuiles, `DirtyTiles` en bitset, `TileCache` CPU/GPU séparés,
`TileScheduler` progressif visible/proche/grossier d'abord — CPU pur).

### apps/
Applications finales qui combinent packages, core et engines. Découpage par rôle
(`main.rs` boot eframe, `app.rs` état + channels, `layout.rs` disposition
propre, `ui/` widgets métier). Chaque app est un binaire indépendant ;
photo est complète, video/audio sont des bases en attendant leurs moteurs.

## Flux de données (vue d'ensemble)

```
┌─────────────────────────────┐
│  App eframe (photo/video/   │
│  audio) — egui, thread UI   │
└──────────────┬──────────────┘
               │ PhotoCommandQueue
               │ (ActionQueue<A>, drainée frame)
               ▼
┌─────────────────────────────┐      ┌───────────────────────────┐
│  PhotoApp / Behavior        │──────│  PhotoUiContext           │
│  - ActionQueue              │      │  (egui Context, theme,    │
│  - docs: Vec<Arc<Document>> │      │   icons, Catalog<Fr>)     │
│  - dock layout              │      └───────────────────────────┘
└───────┬─────────────────────┘
        │ PhotoAction
        │
        ▼
┌─────────────────────────────┐     ┌──────────────────────────────┐
│  ui/ (widgets métier)       │────▶│  engine_bridge (worker)      │
│  - draw_menu_bar → Catalog  │     │  (PhotoEngineCommand)        │
│  - canvas (state-only draw) │     │  - sync / apply / composite  │
│  - dock/panels              │     │  - undo-redo, history        │
│                             │     └──────────────┬───────────────┘
└─────────────────────────────┘                    │
                                                   │ mpsc (LayersChanged,
                                                   │  ProjectSaved, PerfMetrics)
                                                   ▼
                                          ┌─────────────────────────────┐
                                          │ photo-engine                  │
                                          │  - Document / layers          │
                                          │  - compositing CPU ou wgpu      │
                                          │  - RenderGraph                  │
                                          │  - TileCache (GPU)            │
                                          └─────────────────────────────┘
```

- `app-shell` apporte le modèle générique `ActionQueue<A>` + `DockTab`
  (file + docks), réutilisé par photo / video / audio.
- Le worker répond par `mpsc` ; l'app lit en non bloquant (`try_recv`)
  frame par frame et met à jour son état (state-only : pas de pixels
  régénérés).

## Règles de dépendances

1. `packages/` ne dépend JAMAIS de `engines/` ni de `apps/`
2. `engines/` peut dépendre de `core/` et des `packages/` non-UI ; `datatypes` ne dépend que de `serde`
3. `apps/` peuvent dépendre de `core/`, `engines/` et `packages/`
4. Pas de dépendances circulaires
5. Pas de dépendances entre apps

Vérification : `cargo tree -p <crate> --depth 1`.

## Modèle de rendu

- **State-only** : un réglage (opacité, position…) ne régénère jamais les pixels ;
  il s'applique au draw GPU.
- **Rendu** : aperçu composite CPU calculé côté worker (thread background,
  taille plafonnée) et téléversé en texture egui côté app ; chemin natif
  wgpu zéro-copie (`register_native_texture`) prévu.
- **Frontière moteur→UI** : le worker répond `LayersChanged { layers,
  preview, can_undo, can_redo }`, pollé en non bloquant (`try_recv`) à
  chaque frame — point unique de conversion.

## Compilation

```bash
cargo build --workspace --release   # tout le workspace
cargo build -p photo --release      # une seule app
cargo run -p photo                  # lancer une app
cargo test -p photo-engine          # tests du moteur photo (golden compositing)
```
