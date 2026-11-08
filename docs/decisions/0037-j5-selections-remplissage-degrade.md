# 0037 — J5 : sélections, remplissage et dégradé

- **Date** : 2026-11-08
- **Statut** : proposée (phase A du jalon J5)
- **Exigences concernées** : SEL-1, SEL-2, SEL-3, MAS-1 ; spec 7.2, 8 ; décisions 0020, 0022, 0023, 0036

Cette décision traite les points 5 (sélections) et 6 (remplissage et dégradé) de la consigne. **Elle touche le modèle du document (zone supervisée).**

## Proposition 5 : sélections

**Modèle**

- La sélection est un **masque de couverture antialiasé**, en niveaux de gris à la profondeur du document, stocké en tuiles grises dans la `PixelStore` comme les masques (0020). Elle est **hors de l'arbre des calques** : `Document.selection: Option<NodeId>`, qui désigne un nœud `Selection` invisible au rendu.
- **Aucune sélection = tout est sélectionné** pour les outils.
- Elle fait partie du document, donc de l'historique, et la décision devra dire si elle est enregistrée dans le fichier (`.cygnus`, J6 ; zone supervisée). Proposition : enregistrée, comme dans Photoshop.

**Commandes** (toutes dans le catalogue, donc exposées à l'agent) :

| Commande | Effet | Inverse |
| --- | --- | --- |
| `selection.rect` | rectangle `{rect, mode}` | restauration de tuiles |
| `selection.lasso` | polygone `{points, mode}` | restauration de tuiles |
| `selection.wand` | baguette `{x, y, tolerance, contiguous, sample, mode}` (commande résolue) | restauration de tuiles |
| `selection.invert` | `1 − couverture` | `selection.invert` |
| `selection.feather` | adoucissement `{radius}` | restauration de tuiles |
| `selection.clear` | désélectionne | restauration |
| `selection.to_mask` | ajoute ou remplace le masque du calque | restauration |
| `mask.to_selection` | la sélection devient le masque | restauration |

`mode` vaut `replace`, `add`, `subtract` ou `intersect` (SEL-2). Opérations sur les couvertures `a` (existante) et `b` (nouvelle) :

- ajouter : `a + b − a·b` ;
- soustraire : `a·(1 − b)` ;
- intersecter : `a·b`.

**Rastérisation** (rectangle, lasso) : sur le processeur, dans `photo/core`, fonction pure et déterministe. Couverture par suréchantillonnage 4 × 4 en balayage de lignes (règle « pair-impair » pour le lasso), limitée aux tuiles touchées. C'est rapide : proportionnel à la surface touchée, et sans GPU.

**Coût des opérations** : invert, add, subtract et intersect parcourent les tuiles existantes (une sélection vide ne stocke rien). Elles se font sur le processeur, en O(pixels touchés) ; on mesurera en phase B sur 24 Mpx entier sélectionné.

**Adoucissement** : flou gaussien de la couverture, de rayon 0,1 à 250 px. C'est un filtre de voisinage, qui partage le calcul de **B10** (0023). Il se fera sur le GPU par le même code que le filtre de flou, qui doit donc arriver avant l'adoucissement (**dépendance**, voir le découpage dans 0039).

**Baguette magique** (SEL-1) :

- commande **résolue** : le résolveur rend l'échantillon sur le GPU (calque courant ou image composée), le relit, puis le cœur fait le **remplissage par diffusion sur le processeur**, par tuiles et sur une pile de segments de lignes ;
- tolérance de 0 à 255 en niveaux 8 bits, sur la plus grande différence de canal (comme Photoshop) ; contiguïté à option ;
- bord antialiasé : une couverture partielle pour les pixels dont l'écart est proche de la tolérance (rampe d'un niveau) ;
- pas de baguette sur GPU : un remplissage par diffusion parallèle est complexe pour un gain faible, sur une opération ponctuelle.

**Effet de la sélection** : le pinceau, la gomme, le clonage, le remplissage et les transformations multiplient leur effet par la couverture de la sélection. Une sélection vide (après soustraction) bloque l'outil, avec un message.

**Affichage** (« fourmis en marche ») :

- au niveau affiché du canevas, le contour de la couverture à 50 % est extrait par la méthode des carrés marchants, sur le processeur, sur les tuiles visibles ;
- il est dessiné par egui en traits tiretés animés (8 images par seconde), et recalculé seulement quand la sélection ou la vue changent ;
- la couverture partielle (adoucissement) n'est pas montrée, comme dans Photoshop. Un mode « masque rapide » (voile rouge) est **hors bêta**.

**Conversion** (SEL-3) :

- `selection.to_mask` copie la couverture dans le masque du calque cible ;
- `mask.to_selection` copie le masque, en tenant compte de son inversion et de sa densité, dans la sélection.

## Proposition 6 : remplissage et dégradé

- **Seau** : il réutilise la diffusion de la baguette (même code et mêmes options) pour trouver la zone, puis la remplit de la couleur de premier plan, par le chemin de fusion du pinceau. Le mode et l'opacité viennent de la barre d'options. Commande `pixels.bucket`, **résolue**, avec une seule entrée d'historique.
- **Dégradé** : outil qui pilote `mask.gradient` (existe depuis J3) sur la cible masque, de la couleur de premier plan vers celle d'arrière-plan converties en gris (luminance). Un dégradé sur un calque pixel n'est **pas** demandé par la spec en bêta : l'outil est grisé sur un calque pixel. Il serait facile d'ajouter `pixels.gradient` plus tard.
- La sélection limite les deux outils.

## Alternatives écartées

- **Sélection vectorielle** (chemins) : pas de couverture partielle naturelle, et un adoucissement complexe ; les chemins sont hors bêta.
- **Sélection comme calque masque dans l'arbre** : elle polluerait l'arbre et le rendu, et compliquerait les commandes sur les calques.

## Risques

- **Modèle** (zone supervisée) : un nouveau genre de nœud hors de l'arbre, et la question de l'enregistrement dans le fichier.
- **Dépendance de l'adoucissement à B10**, reporté jusqu'ici.
- **Coût de la baguette** sur un document entier en image composée : il faut relire la composition à 100 %. On la borne à 4 096 tuiles et on la mesure en phase B.
