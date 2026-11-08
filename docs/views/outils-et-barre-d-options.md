# Fiche de vue : outils, barre d'options, couleurs, curseur et règles (J5)

Proposition de la phase A de J5 (décisions 0035 à 0039), à valider avant toute mise en œuvre. Elle complète la maquette `ecran-principal.png`.

## Rail d'outils

De haut en bas, avec la lettre de raccourci :

1. Déplacer et transformer (V)
2. Recadrer (C)
3. Sélection rectangulaire (M)
4. Lasso (L)
5. Baguette magique (W)
6. Pinceau (B)
7. Tampon de clonage (S)
8. Gomme (E)
9. Remplissage et dégradé (G), avec un sous-menu au clic long
10. Pipette (I)
11. Panoramique (H)
12. Zoom (Z)

En bas, les pastilles de couleur, le bouton d'échange (`X`) et le retour aux couleurs par défaut (`D`).

## Barre d'options selon l'outil actif

| Outil | Réglages (mémorisés par outil) |
| --- | --- |
| Pinceau, gomme | Taille, dureté, opacité, flux, espacement, mode (pinceau seulement), stabilisateur, pression → taille, pression → flux |
| Tampon | Comme le pinceau, plus « aligné » et l'échantillon (calque courant ou image composée) |
| Sélections | Mode (remplacer, ajouter, soustraire, intersecter), adoucissement ; pour la baguette : tolérance, contiguë, échantillon |
| Remplissage | Tolérance, contiguë, mode, opacité |
| Dégradé | De la couleur de premier plan vers celle d'arrière-plan (grisé hors masque) |
| Recadrer | Rapport (libre, original, 1:1, 3:2, 4:3, 16:9), angle, supprimer les pixels hors cadre, grille des tiers, Valider ou Annuler |
| Transformer | X, Y, largeur, hauteur, angle, chaîne de proportions, Valider ou Annuler |

## Sélecteur de couleur

Boîte flottante ouverte par un clic sur une pastille :

- carré teinte et saturation, avec un curseur de valeur ;
- curseurs R, V, B (0 à 255) et saisie hexadécimale ;
- huit couleurs récentes ;
- Valider et Annuler.

Les couleurs sont saisies en sRGB direct.

## Curseur et pipette

- **Curseur** : cercle du diamètre au zoom courant (trait double clair et sombre, lisible sur tout fond), avec une croix centrale sous 4 px. Pendant Alt, une pipette remplace le cercle, avec un échantillon de la couleur visée.
- **Tampon** : un cercle de plus marque la source.

## Règles

En haut et à gauche du canevas, 18 px d'épaisseur, graduées en pixels du document selon le zoom (pas de 1, 2, 5 ou 10 × 10ⁿ). Un repère suit le pointeur. Elles s'affichent depuis Affichage > Règles (`Ctrl+R`).

## Sélection

Contour en tirets animés noir et blanc. La barre d'état donne la taille de la boîte englobante de la sélection.
