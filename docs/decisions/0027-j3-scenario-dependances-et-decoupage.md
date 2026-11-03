# 0027 — Scénario photo, dépendances et découpage de la phase B (J3)

- **Date** : 2026-11-01
- **Statut** : acceptée avec conditions le 2026-11-01 (phase A du jalon J3) ; conditions intégrées ci-dessous

## Proposition : scénario photo sans fenêtre (critère de sortie)

Script de développement (format 1, 0015) `photo/render/tests/golden/scenario-photo.json`, avec ses images sources dans `photo/render/tests/fixtures/` :

- un PNG 16 bits et un JPEG 8 bits **générés par nous**, procéduraux, sous la licence du projet : aucune photo de tiers dans le dépôt ;
- les fichiers générés sont versionnés, et le programme qui les produit l'est aussi.

Étapes :

1. `document.create`, puis `source.import` du PNG (objet incorporé) ;
2. `layer.create` d'un réglage Courbes, `mask.create`, puis `mask.gradient` ;
3. un groupe isolé contenant un calque pixel en mode Produit et un réglage Teinte/saturation détouré dessus ;
4. `source.import` du JPEG en mode Écran avec une opacité de 60 % ;
5. un filtre de netteté par masque flou au-dessus du groupe, et des Niveaux au sommet ;
6. `history.undo` ×3, `history.redo` ×2, puis une modification qui vide la pile de refaire ;
7. rendu 16 bits.

**Vérifications :**

- le rendu est conforme à la golden image sur lavapipe, WARP et Metal, selon les critères de 0016 ;
- `cygnus render` l'exécute en entier ;
- les tests de propriétés vérifient que tout annuler, puis tout refaire, redonne le même document pour toutes les commandes du scénario ;
- une variante sans filtre sert de repli si les filtres glissent vers J5.

## Proposition : dépendances nouvelles

| Crate | Version | Licence | Utilisée par | Raison |
| --- | --- | --- | --- | --- |
| `zune-jpeg` | 0.5 | MIT OR Apache-2.0 OR Zlib | `photo/io` | Décodage JPEG minimal (lecture seule) pour `source.import` |
| `png` | déjà présente | MIT OR Apache-2.0 | `photo/io` | Décodage PNG 8 et 16 bits |

**Choix du décodeur JPEG** (Q8, décidé sous réserve de `cargo deny`) :

| Crate | Licence | Points forts | Points faibles |
| --- | --- | --- | --- |
| **`zune-jpeg` 0.5** (retenue) | MIT OR Apache-2.0 OR Zlib | rapide (SIMD), maintenue (version de mars 2026), sans `unsafe` hors SIMD, déjà employée par `image` | API plus bas niveau |
| `jpeg-decoder` 0.3 | MIT OR Apache-2.0 | ancienne référence, simple | en maintenance réduite (dernière version juin 2025), plus lente |
| `image` | MIT OR Apache-2.0 | tous formats | très grosse dépendance, inutile : J3 ne lit que PNG et JPEG |

**Plan de fuzzing des décodeurs** :
- B6 : des cibles `cargo-fuzz` dans `photo/io/fuzz/` (`decode_png`, `decode_jpeg`), avec pour seule règle : jamais de plantage ni de boucle infinie, une erreur ou une image valide ;
- un corpus de départ de fichiers valides générés et de fichiers tronqués ;
- en CI Linux, une courte exécution (60 s par cible) à chaque push sur `main`, et une plus longue chaque nuit ;
- J6 : toutes les entrées de fichiers (TIFF, WebP, `.cygnus`) rejoignent ce dispositif.

Les profils ICC sont ignorés en J3 (source supposée sRGB) et seront pris en charge en J6. Les décodeurs refusent :

- les images de plus de 65 535 pixels de côté ou de plus de 256 Mpx (bombes de décompression) ;
- tout fichier invalide, par une erreur claire et jamais un plantage.

Le fuzzing des décodeurs (spec section 5) est prévu en J6.

## Proposition : découpage de la phase B

| Étape | Contenu | Vérification |
| --- | --- | --- |
| **B1** | `photo/core` : nouveaux genres de nœuds, masques, détourage, isolation, sources et catalogue de 0020, hors commandes résolues | Invariants ; appliquer puis inverser pour chaque commande ; instantané du catalogue |
| **B2** | Historique (0022) : piles, gestes, fusion, `history.undo` et `history.redo` | Tests de propriétés : tout annuler et tout refaire ; budget mémoire |
| **B3** | Rendu : réserve de sources entières sur GPU, interpréteur (groupes isolés et passants, masques, détourage), suppression de la passe de décodage f16 | Golden images et propriétés (groupe à opacité 1 = calques à plat…) ; identité aperçu-export maintenue |
| **B4** | Les 12 modes de fusion dans `blend.wgsl` | Valeurs calculées à la main d'après le W3C ; une golden image par mode |
| **B5** | Réglages (`adjust.wgsl`, table des courbes) ; test de bandes GPU ; revue du f16 consignée | Valeurs calculées à la main ; seuils de bandes mesurés |
| **B6** | `photo/io` (PNG, JPEG) et commandes résolues `source.import` et `layer.merge_down` | Fichiers invalides refusés sans plantage ; golden image de fusion vers le bas |
| **B7** | Ordonnancement par budget de temps, pyramide des sources, affichage grossier, aperçu de glissement, message d'état | Banc : aucune image au-delà de 100 ms, cibles de 0019 ; test 4K |
| **B8** | Interface : panneaux branchés, menu contextuel, histogramme GPU ; suppression de `placeholder` | Captures fournies ; tests de traduction ; aucun module `placeholder` |
| **B9** | Scénario photo, `cygnus render`, golden images sur les trois adaptateurs en CI | CI verte avec `CYGNUS_REQUIRE_GPU=1` |
| **B10** | Filtres de voisinage (0023), **en dernier** | Golden images flou et netteté ; mesure du temps final. Peut glisser vers J5 sur décision |

Le banc de fluidité est relancé en B7, puis à la fin de J3, sur 24 Mpx en 16 bits et 10 calques dont des réglages et des masques. Le rapport est rangé dans `docs/bench/` sous un nom daté avec la machine.

## Points suivis (décidés par l'utilisateur, 2026-11-02)

1. **Réserve de sources à dimensionner en B7.** La réserve fixe de 256 tuiles de calques (et 256 de masques) est plus petite qu'une vue à 100 % (450 à 600 tuiles de calques à 2126 × 1086 avec 10 calques) : elle sature, et un changement de propriété renvoie presque toutes les sources de la vue (184 tuiles sur 200 mesurées après B3, 0019), contrairement à la condition de 0021. En B7, sa taille est calculée d'après le budget mémoire du GPU, comme le cache des composites (0018), et doit au moins contenir les sources d'une vue à 100 %. Vérification : le banc rapporte zéro source renvoyée lors d'un changement d'opacité.
2. **Mesure officielle du panoramique** : le scénario à cache limité à 128 tuiles (`--cache-slots 128`), qui compose réellement des tuiles nouvelles. Le cache calculé garde toute la pyramide du document de 24 Mpx, donc le panoramique n'y compose rien et ne mesure que l'affichage.
3. **Première vue** : la mesure comprend l'envoi initial de toutes les sources lues (`write_texture`), la composition et l'attente de la fin du travail GPU ; elle exclut la construction du document, la création des pipelines et l'allocation des réserves.
4. **Plan accéléré (décidé par l'utilisateur, 2026-11-02)** : B5, puis B6, puis B8 s'enchaînent sans validation intermédiaire, CI verte à chaque envoi. **B7 (ordonnancement par budget de temps, pyramide des sources, aperçu grossier) et B10 (filtres de voisinage) sont reportés après J4.** Le scénario photo prend donc sa variante sans filtre (section du scénario ci-dessus), et le point 1 (réserve de sources) attend B7.
