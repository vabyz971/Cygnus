# 0015 — Dépendances nouvelles et découpage de la phase B (J2)

- **Date** : 2026-10-30
- **Statut** : acceptée avec conditions le 2026-10-30 (phase A du jalon J2) ; conditions intégrées ci-dessous
- **Exigences concernées** : `AGENTS.md`, règle 7

## Proposition : dépendances

| Crate | Version | Licence | Utilisée par | Raison |
| --- | --- | --- | --- | --- |
| `wgpu` | =30.0.1 | MIT OR Apache-2.0 | `photo/render` | Rendu. **Doit être exactement la version d'`egui-wgpu` 0.36.2** pour partager le `Device` d'eframe |
| `half` | 2.7 | MIT OR Apache-2.0 | `photo/render` | f16 côté processeur (téléversement, lecture, tests) |
| `bytemuck` | 1.25 | Zlib OR Apache-2.0 OR MIT | `photo/render` | Conversion sans copie des tampons GPU |
| `pollster` | 1.0 | Apache-2.0 OR MIT | `photo/render` | Attente bloquante des appels asynchrones de wgpu sans fenêtre |
| `png` | 0.18 | MIT OR Apache-2.0 | `photo/render` (tests), `apps/cli` | Écriture de `cygnus render` et des golden images (l'import/export complet reste en J6) |
| `schemars` | 1.2 | MIT | `crates/commands`, `photo/core` | Schéma JSON des paramètres de commandes, base des outils MCP (J4) |
| `serde`, `serde_json` | déjà présentes | MIT OR Apache-2.0 | `crates/commands`, `photo/core` | Sérialisation des commandes et du modèle |
| `thiserror` | 2.0 | MIT OR Apache-2.0 | `crates/commands`, `photo/core`, `photo/render` | Types d'erreur sans code répétitif (déjà présente dans l'arbre, via egui_dock) |

Toutes ces licences sont dans la liste de 0003. Les versions de `wgpu`, `half` et `bytemuck` sont **déjà celles du `Cargo.lock`** (tirées par eframe) : aucune nouvelle version n'entre pour elles. La règle de 0004 s'étend : `egui`, `eframe`, `egui_dock` et `wgpu` montent de version ensemble.

`photo/core` ne dépend pas de wgpu : le modèle et les commandes restent testables sans GPU.

## Proposition : entrée de `cygnus render` avant le format `.cygnus`

La consigne demande `cygnus render <document> <sortie.png>`, mais le format `.cygnus` n'arrive qu'en J6. En J2, le « document » est un **script de commandes** JSON : une liste de commandes du catalogue (0010), dans leur forme sérialisée, appliquées dans l'ordre à un document vide par le même bus que l'interface. Le même fichier sert aux golden images (0013), au banc d'essai (0014) et au document de démonstration. En J6, `cygnus render` acceptera aussi les fichiers `.cygnus`, et le script restera utile aux tests et aux agents. **Décidé (Q1)** : c'est un **script JSON de développement**, avec son propre champ de version (`{"version": 1, "commands": [...]}`), versionné dans le dépôt pour les cas de test. Il ne fait **pas partie du format de document** et n'engage aucune compatibilité de `.cygnus`.

## Proposition : découpage de la phase B

Chaque étape est un petit groupe de commits. Elle n'est terminée que si fmt, clippy, test, en-têtes et `cargo deny` passent.

| Étape | Contenu | Critère de vérification |
| --- | --- | --- |
| **B1** | `photo/render` : création d'instance, d'adaptateur et de périphérique (`device_descriptor`), sans rendu. Jobs CI GPU (lavapipe, WARP, Metal) | Un test GPU affiche l'adaptateur trouvé sur les trois runners, ou la CI documente l'absence. **Résultat rapporté à l'utilisateur avant la suite** |
| **B2** | `crates/commands` : `Catalog`, `CommandBus`, `Applied`, `Changes`, `Source`, journal, schéma | Tests unitaires : application, refus sans effet, journal, révision |
| **B3** | `photo/core` : modèle (0009) et commandes de J2 (0010), stockage processeur des tuiles | Tests : invariants, aller-retour serde, appliquer puis inverser = identité pour chaque commande, instantané du schéma |
| **B4** | `photo/render` : réservoir de tuiles, décodage, composition (Normal), export, `blend.wgsl` et `color.wgsl` uniques | Test de validation WGSL par naga ; tests de propriétés (0013) ; premières golden images sur lavapipe, vérifiées par l'utilisateur |
| **B5** | `apps/cli` : `cygnus render <script.json> <sortie.png>` | Test d'intégration : rendu d'un cas, comparaison à sa golden image |
| **B6** | `apps/photo` : vrai canevas (rappel wgpu), document de démonstration, zoom et panoramique ; suppression du canevas factice | Test aperçu = export (identité bit à bit à 100 %) ; capture d'écran fournie à l'utilisateur |
| **B7** | Pyramide de niveaux, budget, éviction, mémoire GPU insuffisante | Tests : éviction puis rechargement identique ; erreur simulée de mémoire sans plantage ; aperçu réduit cohérent |
| **B8** | Banc d'essai (sans fenêtre et dans l'application), rapport sur la machine de référence, puis sur les machines modestes disponibles | Rapport consigné dans `docs/decisions/` ; cible et GPU minimal confirmés ou corrigés par l'utilisateur |
