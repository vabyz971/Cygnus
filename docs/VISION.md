# Vision — Cygnus Photo

> 1 page. Lit ce fichier avant toute décision de périmètre.
> Carte du code : `graft map` / `graft ask "<question>" --source`.

## Positionnement

Cygnus Photo vise le modèle **Affinity (by Canva)**, pas un clone de Photoshop :

- **Personas** : des espaces adaptés à chaque étape (Photo / Develop / Liquify / Tone Mapping / Export), pas tous les outils dans un seul écran.
- **Non-destructif par défaut** : Live Filters, calque RAW ré-éditable, masques composables.
- **StudioLink-like** : photo + vectoriel + texte + mise en page dans **un seul document `.cygp`** (modes `Pixel` / `Vector` / `Layout` de l'app, voir `apps/photo/src/ui/modebar.rs`).

## Principes

1. Moteurs purs, apps = interface + orchestration (`ARCHITECTURE.md`).
2. Rendu **state-only** : un réglage n régénère jamais les pixels.
3. Tout réglage destructif a un équivalent non-destructif quand ça existe côté Affinity.
4. Qualité > quantité : 10 filtres live finis > 70 aperçus.

## Non-objectifs (décidé, voir `docs/DECISIONS.md`)

- Pas de parité PSD byte-exact façon Photocraft : import/export correct suffit.
- Pas de plugins `.8bf` / ExtendScript / cloud Adobe.
- Pas d'IA générative avant la v1.0.
- Pas de catalogue RAW type Lightroom (éditeur, pas DAM).
