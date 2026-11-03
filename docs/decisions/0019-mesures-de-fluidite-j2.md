# 0019 — Mesures de fluidité de J2 et proposition de cibles

- **Date** : 2026-10-31
- **Statut** : acceptée avec corrections par l'utilisateur le 2026-11-01 (voir « Cibles décidées »)
- **Exigences concernées** : spec section 5 (fluidité, mémoire) ; banc d'essai de 0014 ; étape B8 de 0015

## Machine de référence

| Élément | Valeur |
| --- | --- |
| GPU | NVIDIA GeForce RTX 3080 Ti, Vulkan, pilote 615.78.08 |
| Processeur | AMD Ryzen 7 5800X, 8 cœurs, 16 fils |
| Mémoire | 32 Gio |
| Système | NixOS 26.05, Wayland (compositeur niri) |
| Programme | commit de B8, profil `release` |

Aucune machine modeste n'a encore été mesurée.

## Scénario

Il suit 0014 et est défini une seule fois dans `photo/render/src/bench.rs` :

- **document** : 6000 × 4000 px (24 Mpx), 16 bits, 10 calques pixel couverts entièrement, mode Normal, opacités de 30 à 100 % ; 1,88 Gio de pixels en mémoire processeur ;
- **cache du canevas** : 256 tuiles, soit 128 Mio de mémoire GPU ;
- **parcours** : « adapter à l'écran » (première image, puis 120 images), panoramique à 100 % sur toute la largeur (240 images), zoom continu de 5 à 800 % (240), panoramique en diagonale à 300 % (120), changement d'opacité du calque 5 suivi de 30 images.

**Lancement :**

- sans fenêtre : `cargo run --release -p cygnus-photo-render --example fluidity -- --out rapport.json` ;
- dans l'application : `CYGNUS_BENCH=fluidity CYGNUS_BENCH_OUT=rapport.json cargo run --release -p cygnus-photo`.

Les rapports bruts sont dans `docs/bench/` (JSON, un par exécution).

## Résultats (temps d'image en millisecondes)

### Dans l'application (mesure de référence)

Fenêtre réelle, canevas de 2126 × 1086 pixels (taille imposée par le compositeur en mosaïque). Le temps d'une image est l'intervalle entre deux images ; il comprend l'interface, la mise à jour du canevas, le GPU et la présentation. La synchronisation verticale a été demandée désactivée, mais le compositeur l'impose : la médiane est plafonnée à 6,06 ms (165 Hz, fréquence de l'écran), et les durées plus courtes ne sont pas mesurables ici.

| Phase | Images | Médiane | p95 | p99 | Max | Tuiles composées |
| --- | --- | --- | --- | --- | --- | --- |
| Adapter, première image | 1 | 2432 | — | — | 2432 | 384 |
| Adapter, cache chaud | 120 | 6,06 | 6,10 | 7,73 | 332 | 0 |
| Panoramique 100 % | 240 | 6,06 | **24,49** | 30,54 | 229 | 155 |
| Zoom 5 → 800 % | 240 | 6,06 | 6,13 | 7,82 | 697 | 121 |
| Panoramique 300 % | 120 | 6,06 | 6,15 | 6,24 | 7,69 | 0 |
| Changement d'opacité | 30 | 6,06 | 7,71 | 106,6 | 107 | 20 |

### Sans fenêtre (2560 × 1440, sans présentation, attente de fin du GPU)

| Phase | Images | Médiane | p95 | p99 | Max | Tuiles composées |
| --- | --- | --- | --- | --- | --- | --- |
| Adapter, première image | 1 | 2342 | — | — | 2342 | 384 |
| Adapter, cache chaud | 120 | 0,15 | 0,17 | 0,35 | 0,86 | 0 |
| Panoramique 100 % | 240 | 0,12 | **17,48** | 27,87 | 259 | 140 |
| Zoom 5 → 800 % | 240 | 0,12 | 0,16 | 0,76 | 750 | 128 |
| Panoramique 300 % | 120 | 0,11 | 0,11 | 0,12 | 22,9 | 6 |
| Changement d'opacité | 30 | 0,12 | 0,14 | 125 | 125 | 35 |

### Corrections faites pendant B8, sur mesure

La première mesure sans fenêtre (rapport `…-avant.json`) montrait, pendant le zoom, des pics de **2,6 s** (99e centile de 2599 ms) et 3256 tuiles recomposées. Deux causes ont été trouvées et corrigées :

1. Le repli vers un niveau plus grossier, quand les tuiles visibles ne tiennent pas dans le cache, vidait tout le cache, alors que le dépassement était détecté avant toute modification. Chaque image du zoom entre 50 et 55 % recomposait tout le niveau 0.
2. L'éviction ne distinguait pas les niveaux. Elle évince désormais le niveau 0 d'abord (0018).

Après correction : 128 tuiles composées pendant le zoom, et un 99e centile de 0,76 ms. L'envoi des tuiles au GPU a aussi été simplifié (copie directe des octets au lieu d'une conversion valeur par valeur) : gain d'environ 10 % sur la première image.

## Analyse

- **Vue stable, zoom, panoramique dans une zone déjà calculée** : très en dessous de 16,7 ms. L'affichage seul coûte environ 0,15 ms.
- **Coût d'une tuile nouvelle** : environ **6 ms par tuile de 10 calques**. Il est dominé par le processeur : préparation et envoi de 5 Mio de données de calques par tuile, que le GPU décode à chaque fois. C'est lui qui fait :
  - la **première image** d'un document de 24 Mpx en vue entière (384 tuiles) : environ 2,4 s ;
  - les **pics du panoramique à 100 %**, quand une colonne de tuiles entre dans la vue : p95 de 17,5 ms sans fenêtre et de 24,5 ms dans l'application ;
  - le **changement d'opacité** d'un calque : 107 à 125 ms pour 20 à 35 tuiles visibles. C'est une image isolée, mais perceptible.

## Mesure après le budget calculé de 0018 (2026-11-01, sans fenêtre, 2560 × 1440)

Cache de 513 tuiles (256,5 Mio) : toute la pyramide reste en mémoire.

| Phase | Images | Médiane | p95 | p99 | Max | Tuiles composées | Évictions |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Adapter, première image | 1 | 2313 | — | — | 2313 | 384 | 0 |
| Adapter, cache chaud | 120 | 0,15 | 0,17 | 0,32 | 0,36 | 0 | 0 |
| Panoramique 100 % | 240 | 0,13 | **0,15** | 0,30 | 0,34 | 0 | 0 |
| Zoom 5 → 800 % | 240 | 0,12 | 0,20 | 0,33 | 1,66 | 0 | 0 |
| Panoramique 300 % | 120 | 0,12 | 0,14 | 0,18 | 0,30 | 0 | 0 |
| Changement d'opacité | 30 | 0,11 | 0,14 | 119 | 119 | 35 | 0 |

Ce qui reste au-dessus des cibles : la première vue (2,3 s d'une traite) et l'image qui suit un changement de propriété (119 ms). Les deux viennent du calcul des tuiles fait en une seule fois.

## Mesure après B3 de J3 (2026-11-01, sans fenêtre)

Code de B3 : réserves de sources entières résidentes (256 tuiles de calques, 256 de masques), plus de passe de décodage f16, interpréteur à registres. Même document (24 Mpx, 16 bits, 10 calques), même parcours, RTX 3080 Ti. Le banc rapporte désormais les envois et évictions de la réserve de sources. Rapports : `docs/bench/2026-11-01-rtx3080ti-sans-fenetre-b3-*.json`.

**Fenêtre de 2126 × 1086** (taille de la mesure dans l'application), cache calculé de 513 tuiles :

| Phase | Médiane | p95 | p99 | Max | Tuiles composées | Sources envoyées | Sources évincées |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Adapter, première image | 229 | — | — | 229 | 384 | 3 840 | 3 584 |
| Adapter, cache chaud | 0,12 | 0,13 | 0,14 | 0,2 | 0 | 0 | 0 |
| Panoramique 100 % | 0,10 | **0,12** | 0,25 | 0,7 | 0 | 0 | 0 |
| Zoom 5 → 800 % | 0,10 | 0,16 | 0,18 | 1,6 | 0 | 0 | 0 |
| Panoramique 300 % | 0,10 | 0,12 | 0,23 | 0,6 | 0 | 0 | 0 |
| Changement d'opacité | 0,09 | 0,11 | 15,3 | 15,3 | 20 | 184 | 184 |

**2560 × 1440**, à comparer à la mesure après 0018 ci-dessus : première image **229 ms** (contre 2313), changement d'opacité **51 ms** (contre 119), les autres phases inchangées (p95 ≤ 0,3 ms).

**Panoramique sur des tuiles non calculées** : le cache calculé garde toute la pyramide, donc le panoramique à 100 % ci-dessus ne compose rien. Avec un cache fixé à 128 tuiles (`--cache-slots 128`), à 2126 × 1086 :

| Phase | Médiane | p95 | p99 | Max | Tuiles composées | Sources envoyées |
| --- | --- | --- | --- | --- | --- | --- |
| Panoramique 100 % | 0,12 | **4,9** | 5,7 | 143 | 166 | 1 660 |
| Zoom 5 → 800 % | 0,10 | 0,15 | 1,6 | 169 | 430 | 4 300 |
| Panoramique 300 % | 0,09 | 0,95 | 4,3 | 8,4 | 36 | 344 |

Le p95 du panoramique sur contenu non calculé passe de 17,5 ms (J2, sans fenêtre) à 4,9 ms : une tuile nouvelle de 10 calques coûte environ 0,6 ms au lieu de 6. Le maximum (143 ms) est la première image du panoramique, qui compose toute la vue d'un coup : c'est le budget de temps par image (0024, B7) qui le traitera. La cause exacte du gain (envoi direct par `write_texture` au lieu du tampon de décodage et de la passe f16) n'a pas été isolée.

**Saturation de la réserve de sources (256 tuiles de calques)** : elle sature, comme prévu.

- La première vue lit toutes les tuiles du niveau 0 (384 × 10 = 3 840 tuiles de calques, 1,9 Gio) : chaque tuile est envoyée une fois, puis évincée. Seules les pyramides de sources de 0024 éviteront ce volume.
- Une vue à 100 % de 2126 × 1086 touche 45 à 60 tuiles, soit 450 à 600 tuiles de calques : plus que la réserve. Pendant le panoramique, chaque colonne nouvelle renvoie ses 10 calques (1 660 envois pour 166 tuiles, autant d'évictions).
- Conséquence : après la première vue, un **changement d'opacité renvoie 184 tuiles sur 200** au lieu de 0, contrairement à la condition de 0021 (« un changement de propriété ne renvoie aucune donnée résidente »). Il reste sous la cible de 150 ms (15 à 51 ms), mais la réserve doit au moins contenir une vue à 100 % : à dimensionner d'après le budget mémoire du GPU en B7 (proposition à venir avec 0024).

## Mesure après B5 de J3 (2026-11-02, sans fenêtre, 2126 × 1086)

Document du banc changé (0027, 0028) : 24 Mpx, 16 bits, **10 calques dont 7 calques pixel et 3 réglages** (courbes, niveaux masqués, teinte/saturation) **et 2 masques en dégradé** (sur les niveaux et sur un calque pixel). Deux phases ajoutées : un point de la courbe glissé, une commande `adjustment.update` par image, pendant 60 images à 300 % puis à la vue ajustée. Les chiffres ne se comparent pas directement à ceux de B3 (3 calques pixel de moins, 3 réglages et 2 masques de plus). Rapports : `docs/bench/2026-11-02-rtx3080ti-sans-fenetre-b5-*.json`.

| Phase | Cache calculé (513 tuiles) : médiane / p95 / max | Cache de 128 tuiles (mesure officielle du panoramique) : médiane / p95 / max | Tuiles composées | Sources envoyées (cache 128) |
| --- | --- | --- | --- | --- |
| Adapter, première image | 196 | 169 | 384 | 3 456 |
| Adapter, cache chaud | 0,12 / 0,15 / 0,8 | 0,12 / 0,12 / 0,1 | 0 | 0 |
| Panoramique 100 % | 0,11 / 0,14 / 0,3 | 0,10 / **3,9** / 157 | 168 | 1 512 |
| Zoom 5 → 800 % | 0,10 / 0,15 / 1,6 | 0,09 / 0,24 / 134 | 430 | 3 834 |
| Panoramique 300 % | 0,09 / 0,10 / 0,1 | 0,09 / 0,71 / 6,8 | 36 | 287 |
| Changement d'opacité | 0,09 / 0,13 / 6,6 | 0,09 / 0,10 / 3,4 | 20 | 42 |
| **Glissement d'un point de courbe, 300 %** | 0,58 / 0,63 / 2,1 | 0,60 / 0,66 / 2,4 | 20 par image | 0 |
| **Glissement d'un point de courbe, vue ajustée** | **177 / 185 / 200** | **175 / 182 / 220** | 384 par image | 3 456 par image |

- **Courbes pendant le glissement** : le calcul des quatre tables d'une courbe prend **0,1 ms** sur le processeur (médiane, 0,14 ms au plus). Seule la courbe maître change (les trois courbes de canal identiques restent résidentes) : 63 tables envoyées pour 60 pas, **16 Kio par pas** (4 096 entrées). À 300 %, l'image complète d'un pas coûte 0,6 ms, sous la cible de 33 ms.
- **À la vue ajustée, la cible de 33 ms n'est pas tenue (177 ms)** : un changement de propriété invalide tout le cache, et l'image réduite se recalcule depuis les 384 tuiles du niveau 0 ; la réserve de 256 tuiles sources sature et renvoie 3 456 tuiles par image. Ce coût est celui de la première vue : il revient à B7 (aperçu grossier depuis les pyramides de sources, budget de temps par image, réserve dimensionnée ; points suivis de 0027).
- Panoramique officiel (cache de 128 tuiles) : p95 de 3,9 ms, sous la cible de 16,7 ms ; le maximum (157 ms) reste la première image du panoramique, pour B7.

## Mesure après B7-lite (2026-11-03, sans fenêtre, 2126 × 1086)

B7-lite (0027, plan accéléré ; l'ordonnanceur complet reste reporté après J4) :

- **réserves de sources dimensionnées d'après le budget** : 512 Mio sur GPU dédié, 256 Mio sinon, soit 819 tuiles de calques et 819 de masques ici, contre 256 ;
- **préchargement au repos** : quand une image n'a rien à composer à pleine taille, les sources des tuiles visibles reviennent dans la réserve, 16 envois par image au plus ;
- **aperçu réduit pendant un glissement** : le niveau affiché, jusqu'au niveau 2, est composé depuis des sources réduites construites sur GPU une fois, puis gardées tant que les pixels ne changent pas ; les tuiles d'aperçu sont recalculées exactement au relâchement.

**La machine était nettement plus lente ce jour-là** pour les phases limitées par le processeur (préparation et envoi des tuiles). Le code de B5 (commit 40b3e6e), mesuré à 196 ms de première vue le 2026-11-02, en donnait 1 005 à 1 034 ms le 2026-11-03. Les comparaisons ci-dessous opposent donc les deux codes **le même jour**, en mesures alternées. Rapports : `docs/bench/2026-11-03-*`.

| Phase (cache calculé) | Code de B5, même jour | B7-lite |
| --- | --- | --- |
| Glissement d'un point de courbe, vue ajustée : médiane / p95 | 1 050 / 1 075 ms | **0,9 / 1,0 ms** |
| idem, première image du glissement | 1 082 ms | 842 à 870 ms (construction des sources réduites, une fois) |
| Relâchement (affinage exact, une image) | — | 973 ms |
| Changement d'opacité : sources renvoyées | 88 | **0** |
| Panoramique 100 % : p95 | 0,13 ms | 6,5 ms (préchargement) |
| Première vue | 1 005 à 1 034 ms | 1 024 à 1 041 ms |

Avec le cache limité à 128 tuiles (mesure officielle du panoramique), le p95 du panoramique à 100 % vaut 19,0 ms, contre 19,6 ms pour le code de B5 le même jour (3,9 ms la veille, sur une machine plus rapide). Le préchargement ne le dégrade donc pas.

Restent pour B7 :

- la première image d'un glissement (construction des sources réduites) ;
- l'image d'affinage qui suit le relâchement ;
- la première vue.

Ces trois coûts sont faits d'une traite, en une image : c'est le rôle du budget de temps par image et des pyramides de sources construites à l'avance.

## Cibles décidées (utilisateur, 2026-11-01)

1. **60 images/s** (p95 ≤ 16,7 ms) pour la vue stable, le zoom et le panoramique : confirmé.
2. **Première vue** : l'interface n'est jamais bloquée plus de **100 ms**. Un affichage grossier apparaît d'abord, puis s'affine.
3. **Changement de propriété** : image finale en moins de **150 ms**, et **aperçu réduit en moins de 33 ms** pendant un glissement (par exemple d'un curseur d'opacité).
4. **Obligatoire en J3** : un **budget de temps par image** pour le calcul des tuiles. Le canevas répartit le travail sur plusieurs images, affiche les niveaux disponibles en attendant, et ne dépasse jamais le budget dans une image.

État à la fin de J2 : la cible 1 est tenue. Les cibles 2 et 3 ne le sont pas encore, et le budget de temps par image (4) est la première tâche de rendu de J3.

## Proposition initiale (historique)

1. **Cible de 60 images/s (p95 ≤ 16,7 ms)** : **confirmée** pour la vue stable, le zoom et le panoramique dans une zone déjà calculée. **Non atteinte** pour le panoramique à 100 % qui découvre de nouvelles tuiles (p95 de 24,5 ms dans l'application).
2. **Corrections de cible proposées pour la bêta** :
   - première vue d'un document de 24 Mpx et 10 calques : **moins de 3 s**, avec une vue réduite affichée dès que possible ;
   - modification d'une propriété de calque : **moins de 150 ms** pour une vue plein écran ;
   - panoramique à 100 % sur du contenu non calculé : **p95 ≤ 33 ms** (30 images/s) en bêta, 60 images/s restant l'objectif.
3. **Pistes d'amélioration**, à décider plus tard et non codées en J2 :
   - garder sur le GPU, dans un budget, les tuiles de calques déjà décodées, pour ne plus les renvoyer ni les redécoder ;
   - étaler la composition des tuiles nouvelles sur plusieurs images, en affichant le niveau grossier en attendant ;
   - préparer les données des tuiles sur un fil séparé.

## GPU minimal

Il est défini par des fonctions et des limites wgpu vérifiées au démarrage (0014) ; c'est fait en B6. Pour la performance, il faut au moins une machine modeste (GPU intégré récent ou Apple M1) qui passe le même banc. **C'est à faire par l'utilisateur ou des testeurs** : la mesure ci-dessus vient d'une machine haut de gamme.
