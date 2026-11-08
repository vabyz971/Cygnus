# 0035 — J5 : moteur de trait sur GPU

- **Date** : 2026-11-08
- **Statut** : proposée (phase A du jalon J5)
- **Exigences concernées** : PIN-1, PIN-2, PIN-4, MAS-1 ; spec 5 (fluidité), 7.3 ; décisions 0021, 0026, 0028, 0034

Cette décision traite les points 1 (moteur), 10 (performance) et 11 (ouverture) de la consigne.

## Proposition 1 : moteur de trait

**Empreintes et interpolation**

- **Un trait est une suite de points d'entrée** : position en pixels du document, pression 0 à 1, inclinaison en degrés (si disponible), temps en millisecondes depuis le début du trait.
- **Stabilisateur** : il est appliqué **avant** la commande, dans l'outil de l'interface (moyenne mobile exponentielle sur la position, avec un rattrapage en fin de trait). La commande `brush.stroke` contient les points **déjà stabilisés**, donc le rejeu ne dépend pas du stabilisateur.
- **Interpolation** : la courbe passe par les points (spline de Catmull-Rom centripète), échantillonnée par longueur d'arc. Une empreinte est posée tous les `espacement × diamètre` (espacement de 1 % à 200 %, 10 % par défaut). La pression, l'inclinaison et la vitesse sont interpolées linéairement entre les points. Le calcul des positions d'empreintes se fait **sur le processeur, en f64, dans `photo/core`** : c'est une fonction pure, testée sans GPU, et identique sur toutes les plateformes.
- **Profil de dureté** : la couverture d'une empreinte à la distance normalisée `r` (0 au centre, 1 au bord) vaut 1 jusqu'à `dureté`, puis décroît en lissage d'Hermite (`smoothstep`) jusqu'à 0 au bord. La couverture est intégrée sur le pixel par un suréchantillonnage 4 × 4 pour les petites empreintes, sous 4 px de diamètre, afin qu'un trait fin ne scintille pas.
- **Dynamique** (PIN-2) : le diamètre, le flux et l'opacité peuvent chacun suivre la pression, par une courbe en deux points réglée par outil. Proposition par défaut : la pression agit sur le diamètre (de 20 % à 100 %) et sur le flux. L'inclinaison aplatit l'empreinte en ellipse orientée, quand elle est disponible. La vitesse peut réduire le diamètre (désactivé par défaut).

**Opacité, flux et accumulation**

- **Flux** (par empreinte) : chaque empreinte dépose `couverture × flux` dans un **tampon de trait**, couche de couverture en f32 (format d'étape `Rgba32Float` de 0026, un seul canal utilisé) de la taille des tuiles touchées. L'accumulation se fait par **source-over sur la couverture** : `t = t + c × (1 − t)`. Repasser sur une zone l'épaissit donc jusqu'à 1.
- **Opacité** (par trait) : à la fusion, la couverture du trait est multipliée par l'opacité. Un trait ne dépasse jamais son opacité, quel que soit le nombre d'empreintes, comme dans Photoshop.
- **Fusion dans la cible** : le trait est une source de couleur unie (la couleur du pinceau) d'alpha `couverture × opacité`. Elle est fusionnée dans le calque **par `blend()` de `blend.wgsl`**, avec le mode du pinceau, en sRGB encodé (Q1 de 0021). Il n'y a pas de seconde copie des formules.
- **Gomme** (PIN-4) : même moteur, avec la fusion « destination-out » : `α' = α × (1 − couverture × opacité)`, la couleur directe étant inchangée.
- **Où vit le trait pendant le geste** : le tampon de trait reste sur le GPU. L'aperçu (0036) compose la cible **avec** le trait au moment du rendu des tuiles touchées, sans écrire dans la cible. À la validation, la commande `brush.stroke` recalcule le trait par le **même shader** et écrit les tuiles du calque.
- **Précision visée** : la couverture est accumulée en f32 et la fusion se fait en f32. Le résultat est arrondi **une fois** à la profondeur du document. On vise au plus une unité 16 bits d'écart avec un calcul de référence en f64 fait dans le test, comme pour les golden images.

**Mode de fusion du pinceau** : les douze modes de CAL-3, choisis dans la barre d'options. La valeur par défaut est Normal.

## Proposition 10 : performance

- **Latence visée** : entre un événement d'entrée et l'image qui le montre, **au plus deux images à 60 Hz (33 ms) au p95**, et aucune image au-delà de 16,7 ms pendant un trait. Le cas mesuré : 24 Mpx en 16 bits, 10 calques dont des réglages, à 100 % et en vue ajustée, pinceau de 300 px.
- **Pendant un trait** :
  - seules les tuiles touchées par les nouvelles empreintes sont recomposées ;
  - en vue ajustée, l'aperçu passe par le niveau grossier de 0034 ;
  - la file à budget de temps de 0034 empêche un grand pinceau de geler l'interface : les tuiles en retard s'affichent d'abord au niveau grossier.
- **Sources** : la cible du trait reste résidente (préchargement de B7-lite). Le tampon de trait occupe au plus les tuiles visibles touchées.
- **Banc** : nouvelle phase « trait », avec des événements synthétiques à 120 Hz sur une trajectoire fixe. Elle mesure le temps par image, l'écart entre l'horodatage de l'événement et la fin du travail GPU de l'image qui le montre, et les tuiles recomposées. Rapport JSON daté dans `docs/bench/`.

## Proposition 11 : ouverture (hors bêta)

Les pinceaux sensibles à la lumière et les textures générées ne sont pas dans la bêta. L'architecture les permet sans changement de format :

- le shader d'empreinte reçoit des paramètres par empreinte, auxquels une texture d'empreinte ou un bruit peuvent s'ajouter ;
- la commande garde des champs optionnels avec valeur par défaut (`#[serde(default)]`), ajoutables plus tard ;
- un pinceau qui lit l'image sous lui lirait les tuiles composées, déjà accessibles au tampon de clonage (0036).

## Alternatives écartées

- **Empreintes calculées sur le processeur** : contraire à PIN-1 (« pinceau rendu sur GPU ») et trop lent pour de grands pinceaux en 16 bits.
- **Accumuler directement dans le calque**, empreinte par empreinte : l'opacité ne pourrait plus plafonner le trait, et l'annulation devrait copier des tuiles à chaque empreinte.
- **Tampon de trait en entiers 16 bits** : les empreintes à faible flux s'accumulent mal (bandes), et 0026 réserve le f32 aux résultats intermédiaires qu'une étape modifie.

## Risques

- Le recalcul du trait à la validation doit donner exactement l'aperçu. Le même shader, sur le même adaptateur, avec les mêmes positions d'empreintes calculées en f64, le garantit à l'arrondi f32 près.
- Les empreintes très nombreuses (petit espacement, long trait) coûtent cher. Une empreinte touche peu de tuiles, et les empreintes sont regroupées par tuile dans un même lot.
