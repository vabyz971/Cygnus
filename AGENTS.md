---
covers: []
---
# AGENTS.md

Suite créative Rust : workspace Cargo, apps **egui 0.36 (eframe) + wgpu 30**, licence GPL-3.0. Docs : `README.md` (architecture, roadmap), `ARCHITECTURE.md` (règles de dépendances).

## Environnement
- Édition Rust 2024 → toolchain **1.85+** obligatoire.
- Linux : entrer dans `nix develop` (ou direnv via `.envrc`) — fournit cargo/clippy/rustfmt + Vulkan/Wayland avec `LD_LIBRARY_PATH` déjà réglé.
- Sans Nix : installer `pkg-config vulkan-loader libxkbcommon wayland`.

## Commandes
- App principale : `cargo run --release -p photo`. Autres apps (fondations) : `-p video`, `-p audio`.
- Vérification avant commit : `cargo fmt --all -- --check`, puis `cargo clippy --workspace --all-targets -- -D warnings`, puis `cargo test --workspace`. La CI GitHub (`.github/workflows/ci.yml`) rejoue fmt+clippy+tests sur chaque PR.
- Tests ciblés : `cargo test -p photo-engine` (compositing, historique, projet). Les tests « golden » de `engines/photo-engine/src/document/tests.rs` vérifient les modes de fusion pixel par pixel — ne pas les affaiblir pour faire passer un refactor.

## Packages du workspace
Le nom de crate diffère parfois du dossier — utiliser `-p` avec le nom de crate :
| Dossier | Crate |
|---|---|
| `apps/photo` / `apps/video` / `apps/audio` | `photo` / `video` / `audio` (entrypoint `src/main.rs`) |
| `core/datatypes` | `datatypes` (nœuds, sockets, Vec2/Rect, BlendMode, RgbaBuf partagés) |
| `core/scene` | `scene` (Scene Graph sémantique : hiérarchie, monde dérivé, révisions) |
| `core/ids` | `ids` (`EntityId` stable + `Revision` monotone partagés) |
| `core/graph` | `graph` (dépendances génériques : propagation dirty, topo — CPU pur) |
| `core/render-graph` | `render-graph` (opérations dérivées de la scène : build/sync partiel, backend abstrait — sans wgpu ni UI) |
| `core/tiles` | `tiles` (invalidation spatiale : grille, dirty bitset, cache CPU/GPU, scheduler progressif — CPU pur) |
| `engines/photo-engine` | `photo-engine` (document, compositing CPU/GPU, historique, projet ; `BlendMode`/`RgbaBuf` réexportés de `datatypes`) |
| `engines/vector-engine` | `vector-engine` (paths, formes, styles, booléens + trait `VectorBackend` — sans wgpu) |
| `engines/layout-engine` | `layout-engine` (frames, contraintes, algo + `apply_to_scene` — sans wgpu/egui) |
| `engines/text-engine` | `text-engine` (modèle → layout → glyph runs ; mesure pour layout) |
| `engines/video-engine` | `video-engine` (clips, timeline, transitions, decoder trait ; registre nodal legacy conservé) |
| `engines/audio-engine` | `audio-engine` (timeline + graphe DSP sur `graph`, backend DSP ; hors Scene ; registre legacy conservé) |
| `packages/ui-kit` | `ui-kit` (lib `ui_kit`, design system egui : theme, components, containers, viewport, dialogs) |
| `packages/app-shell` | `app-shell` (lib `app_shell`, squelette d'app : `ActionQueue`, dock générique sur `DockTab`) |
| `packages/math-utils` | `math-utils` (transformation affine `Transform2D` ; Vec2 canonique = datatypes) |
| `packages/file-utils` | `file-utils` (erreurs fichiers, drag & drop, dialogues) |
| `packages/preferences` | `preferences` (préférences persistantes, matériel, raccourcis) |

Dépendances autorisées : `packages/*` ← `core/*` ← `engines/*` ← `apps/*`. Les packages ne dépendent jamais des engines ni des apps ; pas de dépendances entre apps.

## Règles d'architecture (strictes)
- Logique métier → `engines/*` et `core/*` ; widgets → `packages/ui-kit` ; apps = interface + orchestration uniquement. Pas de logique de rendu dans les apps.
- **Les moteurs sont PURS : aucune dépendance UI.** `photo-engine` ne connaît ni egui ni ses types ; le modèle document porte des buffers purs (`RgbaBuf`, `Arc<[u8]>`). L'app envoie des commandes via `mpsc` à un worker qui possède le `Document`, et reçoit snapshots + aperçu composite ; la texture est téléversée côté app (`ViewportTextureCache`). Boucle egui non bloquante (`try_recv` par frame).
- Frontière moteur→UI : le worker répond `LayersChanged { layers, preview, can_undo, can_redo }` ; l'app convertit l'aperçu en `egui::ColorImage` et nourrit son `TextureHandle` (zéro régénération au zoom/pan — state-only).
- Modèle de rendu **« state-only »** : un réglage (opacité, position…) ne régénère jamais les pixels/textures ; il s'applique au draw GPU. Préserver ce modèle à tout prix.
- Rendu : aperçu composite CPU côté worker (plafonné, thread background) ; chemin natif wgpu zéro-copie (`register_native_texture`) prévu côté app.

## Historique & persistance
- Undo/redo : snapshots complets du document (`engines/photo-engine/src/history.rs`) quasi gratuits grâce aux `Arc<DynamicImage>` partagés. Les gestes continus (sliders, renommage, drag) passent par `push_coalesced` — toujours pousser le snapshot PRÉ-mutation, jamais après.
- Format projet `.cygp` (`engines/photo-engine/src/project.rs`) : JSON versionné, calques en PNG+base64. Toute évolution incompatible du modèle → incrémenter `FORMAT_VERSION` et gérer le refus proprement.

## Structure de l'app photo
Découpée par rôle (même schéma pour les futures apps) :
`main.rs` (boot eframe) · `app.rs` (PhotoApp + channels + `impl eframe::App`) · `layout.rs` (disposition propre à l'app) · `ui/` (widgets métier : `layers/`, `canvas.rs`, `toolbar.rs`, `properties.rs`, `engine_bridge.rs`). Ne pas regrossir vers un main.rs monolithique.

## Architecture de `packages/ui-kit` (en couches, voir lib.rs)
1. **`theme`** = SEULE source des couleurs/tailles/rayons (tokens `colors`, `spacing`, `radius`, `typography`).
2. **`components`** = composants génériques (`Button`, `Slider`, `Select`, `Tabs`, inputs, `Toggle`, `IconButton`, `icon_button`/`Icon` — seul contact avec `egui_material_icons` via `icons::glyph` — `ReorderableList`). Un composant n'écrit JAMAIS de couleur/taille en dur : il référence les tokens.
3. **Conteneurs** (`containers` : `Panel`, `Split` + `SplitState`, `Collapsible`, `Toolbar`, `Section`, `Card`, `Stack`) → 4. **Viewport générique** (affichage texture + zoom/pan, état canonique `ViewportState`) → 5. **`dialogs`** (modale, file picker, progression) → 6. **`utils`** (état drag générique).
- Les éléments spécifiques à une app restent dans `apps/<app>/src/ui/`. Promotion vers `packages/ui-kit` seulement quand une 2e app en a besoin (et jamais de types métier : ui-kit reste domain-agnostic, vérifié par `scripts/check_uikit_domain_agnostic.sh`).
- **Interdit de coder une couleur en dur hors `theme/`** — y compris dans les canvas (la sélection utilise `item_selected`, les indicateurs `drop_indicator`). Tailles de texte : passer par `typography`.

## Conventions
- `unwrap()`/`expect()` interdits hors tests ; pas d'emoji dans le code ni les commits.
- Commits courts et préfixés par l'app : `photo: fix blend-mode offset jump`.
- Chaque fichier `.rs` commence par l'en-tête GPL v3 (copier celui de `apps/photo/src/main.rs`).
- Thème : palette/tokens implémentés dans `packages/ui-kit/src/theme/` ; police Hanken Grotesk et icônes Material chargées depuis `assets/fonts/`.

<!-- graft:start -->
## Graft — repo context graph

This repo is indexed in `graft/`: small linked markdown nodes that explain each
system and carry exact file:line spans, kept in sync with the code through git.

For ANY task here — understanding how something works, finding where code lives,
or scoping a change — get context from the graph before grepping or opening
source files. Re-ask freely (it's cheap) and reuse literal identifiers you
already have (symbol, error string, file name) as the query. New to this repo?
Run `graft map` first — a token-budgeted orientation (dir clusters, hubs,
hotspots), no LLM, no key.

- Run `graft ask "<your question>" --source` → ranked nodes with the relevant
  code spans inlined (each hit's ≤8-line crux by default; `--full` for whole
  definitions when the crux isn't enough). Match the tool to the task shape:
  for understanding or editing, the top node IS the answer — cite its
  `covers:` file:line spans and edit straight from `--source`. For
  exhaustive tasks ("every occurrence / every caller of this pattern"), ranked
  results are top-N, not complete — run `graft grep "<literal>"` instead
  (exhaustive over indexed files, grouped by enclosing symbol), falling back
  to raw `grep -rn` only for unindexed files.
- `graft skeleton <file>` → every definition's signature + span, ~10× cheaper
  than reading the file; use it to skim an API surface.
- `graft callers <symbol>` gives precomputed, exact edges — who calls this.
  Add `--direction out` for what it calls, or `--depth N` to walk
  transitively for the full blast radius. For structural questions, skip
  ranking and use this directly.
- Or browse: `graft/INDEX.md` lists every node; follow the links.
- Monorepos and folders of multiple repos rank fairly across sub-projects —
  hits carry `[scope/]` labels naming which one they're from. Narrow with
  `graft ask "<task>" --in <scope>/` once you know where you're working.

If a returned span is truncated ("+N more lines"), open the file at that exact
range before finalizing. Only open source files when a node genuinely lacks a
needed detail, and then at the exact file:line the node points to — never
re-read whole files.

After big code changes, refresh the graph with `graft build` (deterministic,
no API key, $0). The graph is a derived index: it may lag the source tree, so
the code and `Cargo.toml` remain the source of truth.
<!-- graft:end -->
