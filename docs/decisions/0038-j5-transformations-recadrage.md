# 0038 — J5 : transformations et recadrage

- **Date** : 2026-11-08
- **Statut** : proposée (phase A du jalon J5)
- **Exigences concernées** : TRA-1, TRA-2, TRA-3, DOC-2 ; spec 7.2, 7.3 ; décisions 0020, 0021, 0029

Cette décision traite le point 7 de la consigne. **Elle touche le modèle (`Placement`), le catalogue et une passe de rendu (zones supervisées).**

## Modèle

- **Objet incorporé** (TRA-3, non destructif) : `Placement` devient une **matrice affine 2 × 3** (translation, rotation, échelle, cisaillement non proposé dans l'interface) avec l'identité par défaut. Le format sérialisé reste compatible : `{x, y}` est lu comme une translation (0020 l'avait prévu). La source n'est jamais modifiée. Une transformation ne change que la matrice, et son inverse est la matrice précédente.
- **Calque pixel** (TRA-2) : la transformation est **rastérisée une seule fois, à la validation**, par une commande résolue `layer.transform {id, matrix}`. Le rendu rééchantillonne les tuiles sur le GPU. L'annulation se fait par copies de tuiles. Pendant le geste, rien n'est écrit (aperçu ci-dessous).
- **Masque** d'un calque : il suit le calque, avec option « lié » (par défaut, comme dans Photoshop). Le masque d'un objet incorporé est rastérisé à la validation, puisqu'un masque est en pixels.

## Rééchantillonnage

- **En linéaire** (0021) : décodage sRGB → linéaire, valeurs prémultipliées, filtre, puis réencodage.
- **Filtre** :
  - **agrandissement et petites réductions** (échelle ≥ 0,5) : Catmull-Rom, un filtre bicubique net, aux poids négatifs, avec un résultat borné à 0..1 ;
  - **fortes réductions** : **préfiltrage par moyenne de surface** jusqu'au facteur 2 près, puis Catmull-Rom, pour éviter le crénelage ;
  - **rotation de 90°, 180° ou 270° et translation entière** : copie exacte, sans filtre (cas testé au bit près).
- **Lieu** : dans `photo/render`, une passe de rééchantillonnage par tuile de sortie, qui lit les tuiles sources sous la matrice inverse. Pour un objet incorporé, la composition lit la source **à travers la matrice** : c'est la **table des pages par source** prévue par 0021. **Cela modifie le shader de composition** (lecture sur plusieurs tuiles sources, filtrage). Pour garder une seule passe, l'objet incorporé transformé est d'abord rééchantillonné dans une **tuile d'étape** (f32, 0026), puis lu par le programme comme une source ordinaire. Le résultat est mis en cache par tuile tant que la matrice et la source ne changent pas.

## Recadrage (TRA-1)

- Commande `document.crop {rect, angle, delete_outside}` :
  - `rect` dans le repère du document, sans limite de rapport d'aspect (la contrainte vit dans l'outil) ;
  - `angle` en degrés, autour du centre du cadre ;
  - `delete_outside` vaut faux par défaut.
- **Sans suppression** : les calques pixel gardent leurs pixels hors du nouveau cadre (`bounds` peut dépasser le document) ; seuls l'origine et la taille du document changent. Les objets incorporés voient leur matrice composée par la translation (et la rotation).
- **Avec rotation** : chaque calque pixel et chaque masque est rééchantillonné une fois (comme `layer.transform`), et chaque objet incorporé reçoit la rotation dans sa matrice, de façon non destructive.
- **Avec suppression** : les tuiles hors cadre sont écartées.
- L'annulation restaure le document : snapshot des nœuds et des tuiles touchées, comme les commandes de structure. Il faut en mesurer la mémoire sur 24 Mpx en 16 bits avec 10 calques.

## Aperçu pendant le geste

- Le canevas dessine le calque en cours de transformation **à part** : ses tuiles composées seules (sans les autres calques) sont affichées à travers la matrice par le shader d'affichage, au niveau grossier si besoin, au-dessus d'une composition sans ce calque. C'est approximatif pour les modes non Normal, mais **rapide**.
- À la validation, la commande recalcule exactement.
- Pour le recadrage : un voile assombrit la zone écartée, avec les poignées, l'angle, le rapport d'aspect imposé (libre, original, 1:1, 3:2, 4:3, 16:9) et une grille des tiers.

## Alternatives écartées

- **Transformation non destructive des calques pixel** (matrice gardée, rastérisée au rendu) : chaque coup de pinceau ultérieur devrait passer par l'inverse de la matrice. C'est complexe, et réservé aux objets incorporés (TRA-3).
- **Lanczos 3 par défaut** : plus net, mais plus coûteux, avec des halos plus forts. Catmull-Rom suffit en bêta, et le filtre pourra devenir une option.

## Risques

- **Modification du shader de composition** pour lire des sources transformées (zone supervisée), ou recours aux tuiles d'étape (mémoire).
- **Mémoire de l'annulation d'un recadrage avec rotation** : toutes les tuiles de tous les calques. Elle est bornée par le budget de l'historique, qui écarte les plus anciennes entrées.
- **Saut visuel** entre l'aperçu approximatif (calque seul, puis composé) et le résultat exact, pour les modes non Normal.
