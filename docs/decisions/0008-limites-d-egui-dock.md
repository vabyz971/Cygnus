# 0008 — Limites d'egui_dock constatées au jalon J1

- **Date** : 2026-10-30
- **Statut** : acceptée
- **Exigences concernées** : aucune (spec, section 11.2 : risque `egui_dock` ; jalon J1)

## Contexte

Le jalon J1 devait valider ou invalider `egui_dock` (version 0.21.1) pour les panneaux ancrables et les workspaces : réancrer, détacher en fenêtre flottante, fermer et rouvrir, sauvegarder et restaurer la disposition.

## Constats

Tout ce que demande J1 est possible, avec ces limites :

1. **Fenêtres flottantes internes.** Un panneau détaché devient une fenêtre egui dessinée dans la fenêtre principale, et non une fenêtre du système. Elle ne peut pas sortir de la fenêtre de l'application ni aller sur un autre écran. `egui_dock` ne gère pas les fenêtres natives multiples (viewports egui).
2. **Rectangles non sérialisables en JSON.** `egui_dock` enregistre dans son état les rectangles calculés à chaque image. Avant le premier affichage ils sont infinis, ce que JSON ne sait pas représenter (écrits `null`, puis illisibles). Parade : `DockLayout` remet ces rectangles à zéro au moment de sérialiser (`crates/ui-kit/src/dock.rs`). Un test couvre l'aller-retour.
3. **Panique interne possible.** Le code d'`egui_dock` contient un `todo!()` atteint si l'on dépose autre chose qu'un onglet (un nœud ou une fenêtre entière). Ce cas n'a pas été atteint pendant les essais ; il est hors de notre contrôle.
4. **Pas de rappel de visibilité par onglet.** Pour fermer et rouvrir un panneau depuis le menu *Fenêtre*, on retire l'onglet de l'état puis on le réinsère. Un panneau rouvert revient en bas de la colonne, pas à son ancienne place.

## Décision

On garde `egui_dock`. Les panneaux sont placés dans une colonne ancrable à droite (redimensionnable), séparée du canevas et de sa barre d'onglets de documents, que la spec demande de coder à part. À l'intérieur de la colonne, les panneaux s'empilent, se partagent l'espace et se regroupent en onglets ; ils peuvent flotter n'importe où au-dessus de la fenêtre.

## Conséquences

- Si les fenêtres natives multiples deviennent nécessaires, il faudra soit contribuer cette fonction à `egui_dock`, soit écrire notre propre gestion des fenêtres flottantes avec les viewports egui. Ce serait une nouvelle décision.
- Le point 3 est à surveiller à chaque montée de version d'`egui_dock`.
- Ancrer des panneaux à gauche du canevas n'est pas possible dans cette organisation ; ce serait un changement de disposition, pas une limite de la bibliothèque.
