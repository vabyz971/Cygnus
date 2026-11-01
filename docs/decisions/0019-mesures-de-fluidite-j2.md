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
