# 0025 — Interface branchée sur le bus, suppression de `placeholder` (J3)

- **Date** : 2026-11-01
- **Statut** : acceptée avec conditions le 2026-11-01 (phase A du jalon J3) ; conditions intégrées ci-dessous
- **Exigences concernées** : CAL-4, CAL-6, CAL-7, REG-2, REG-3, REG-4, HIS-3, AFF-2 ; spec 7.1 et 7.2

## Proposition

Chaque panneau est un **consommateur** des changements (curseur propre, 0010). Toute action passe par une commande. La sélection, l'état déplié des groupes et le zoom sont de l'état de vue, pas des commandes.

- **Calques** (CAL-4, CAL-6) :
  - arbre du document, groupes dépliables ;
  - chaque ligne montre la vignette, le nom, le genre, l'œil, le verrou et la vignette du masque ;
  - les vignettes sont produites depuis la pyramide réduite des sources (0024), sans rendu GPU dédié. Un réglage ou un filtre montre son icône ;
  - glisser-déposer : `layer.move` ; double-clic : renommer (`layer.update`) ;
  - le mode et l'opacité du calque sélectionné sont modifiables. Un glissement d'opacité est un geste (0022).
- **Menu contextuel** (CAL-7) : Dupliquer, Supprimer, Renommer, Regrouper, Dégrouper, Fusionner vers le bas, Détourer ou Ne plus détourer, Ajouter, Inverser ou Supprimer le masque, et **Afficher dans le graphe, grisé**.
- **Boutons du bas du panneau** : nouveau calque, nouveau groupe, nouveau réglage (menu des cinq réglages), ajouter un masque, nouveau filtre, supprimer.
- **Réglages** (REG-2, REG-3) :
  - les paramètres du réglage sélectionné, avec intensité et densité du masque ;
  - éditeur de courbes RVB et par canal, aux points déplaçables, dessiné avec la fonction de courbe de `photo/core` ;
  - curseurs pour les autres réglages ;
  - chaque glissement est un geste.
- **Histogramme** (REG-4) :
  - calculé sur GPU par une passe de comptage (compteurs atomiques, dans le cœur de WebGPU) sur le résultat composé, à un niveau réduit d'environ 1 Mpx ;
  - relu sans bloquer (lecture asynchrone, résultat à l'image suivante), recalculé à chaque révision mais au plus 10 fois par seconde ;
  - montre R, V, B, la moyenne et l'écart type. C'est une approximation documentée, sur le niveau réduit.
- **Historique** (HIS-3) : voir 0022 ; il est branché sur les piles du bus.
- **Menu Édition** : Annuler et Rétablir deviennent actifs (Ctrl+Z et Ctrl+Maj+Z), tout comme les commandes de calque du menu Calque.
- **Message d'état** (repli grossier, report de J2) : quand le canevas affiche un niveau plus grossier que voulu ou calcule encore, la barre d'état affiche « Affichage simplifié, calcul en cours » ; s'il s'agit d'un repli imposé par la mémoire, « Affichage simplifié : mémoire GPU limitée ». Les deux textes passent par les traductions.
- **Suppression du module `placeholder`** :
  - calques, courbe, histogramme et historique sont réels ;
  - les **options du pinceau** deviennent l'état réel des réglages d'outil, sans effet avant J5, avec des valeurs par défaut et non des valeurs de maquette ;
  - la **ligne EXIF** (AFF-3) est masquée tant que le document n'a pas de métadonnées : leur lecture arrive avec l'import complet en J6 (question Q7) ;
  - le texte d'action de la barre d'état devient l'aide de l'outil actif ;
  - « Masque : … » montre la vraie cible quand un masque est sélectionné.

## Risques

- Le glisser-déposer dans un arbre, avec egui, demande un widget maison : à prévoir dans `crates/ui-kit` (arbre générique, sans notion de calque).
