# ui-kit

Design system egui pour Cygnus — architecture en couches (bas vers haut) :

## Couches

1. **`theme`** — SEULE source des couleurs, tailles, rayons, typographie.
   - Voir `packages/ui-kit/src/theme/` (tokens `colors`, `spacing`,
     `radius`, `typography`). Jamais de valeurs codées en dur ailleurs.

2. **`primitives`** — briques génériques (`Text`, `Icon`, `divider`,
   `Surface`), indépendantes du domaine.

3. **`icons`** — enum `CygnusIcon` + registre (`IconRegistry`,
   `ALL_ICONS`). Seule source d'import `egui_material_icons`.

4. **`components`** — composants génériques (`Button`, `Slider`,
   `Select`, `Tabs`, `input`, `Toggle`, `IconButton`, `reorderable_list`,
   `menu`). Style uniquement via les tokens du thème.

5. **`containers`** — conteneurs agnostiques (`Section`, `Card`,
   `Stack`, `Panel`, `Split` + `SplitState`, `Collapsible`,
   `Toolbar`). Pas de dépendance UI.

6. **`layout`** — état workspace (`PanelId`, `WorkspaceState`,
   persistence JSON via `preferences`). Pas de dépendance UI.

7. **`i18n`** — clés de traduction stables + catalogue (`Catalog`,
   `Language`, `TextKey`, `PhotoCatalog`). Zéro allocation au
   rendu.

8. **`viewport`** — état zoom/pan (`ViewportState`), overlays.

9. **`dialogs`** — modales, file picker, progression.

10. **`utils`** — état de drag & drop générique.

11. **`context`** — dépendances communes (`UiContext` : egui,
    thème, icônes, traduction). Pas de dépendance moteurs ni apps.

## Règles d'or

- Aucun composant n'écrit de couleur/taille/enum en dur : il référence
  les tokens du thème.
- `unwrap()` / `expect()` interdits hors tests.
- Pas d'emoji dans le code ni les commits.
- Les widgets métier vivent dans `apps/*/src/ui/` — ui-kit reste
  domain-agnostic (vérifié par `scripts/check_uikit_domain_agnostic.sh`).

## Usage

```rust
use ui_kit::theme::CygnusTheme;
use ui_kit::components::Button;
use ui_kit::i18n::Catalog;
```