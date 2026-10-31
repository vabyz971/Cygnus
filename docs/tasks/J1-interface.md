# Consigne : jalon J1, Interface

À placer dans `docs/tasks/J1-interface.md`. Avant de lancer l'agent, enregistrer la maquette de l'écran principal dans `docs/views/ecran-principal.png` (c'est la référence visuelle de cette tâche).

Phrase à donner à l'agent : « Lis `AGENTS.md` et `docs/spec.md`, puis réalise la consigne `docs/tasks/J1-interface.md`. »

## Contexte

J0 est terminé : le workspace, les contrôles de qualité et l'intégration continue sont en place. J1 construit **l'enveloppe de l'application** : la fenêtre, l'écran principal de la maquette, les panneaux ancrables et la sauvegarde de la disposition. **Il n'y a toujours ni document, ni rendu d'image.**

Sections de `docs/spec.md` concernées : 6 (interface), 7.4 (crates), 5 (accessibilité, internationalisation), 11 (risque `egui_dock`), 10 (jalon J1).

## Objectif de risque

La section 11 identifie un risque : les limites d'`egui_dock` (fenêtres flottantes, workspaces). Ce jalon sert d'abord à **valider ou invalider** ce choix tôt. Si `egui_dock` ne permet pas ce qui est demandé ci-dessous, **s'arrêter et le dire**, avec les limites constatées et les alternatives, plutôt que de contourner.

## À livrer

1. **Dépendances** (à ajouter seulement maintenant) : `egui`, `eframe` avec le backend `wgpu`, `egui_dock`. Vérifier que leurs versions actuelles sont compatibles entre elles, les figer, et consigner le choix dans `docs/decisions/`. Le dessin personnalisé du canevas par wgpu arrive en J2 ; le backend wgpu d'`eframe` est choisi dès maintenant pour ne pas changer de base ensuite.
2. **`crates/ui-kit`** (indépendant de l'image, il ne connaît ni calques ni pixels) :
   - Thème sombre reproduisant la maquette : couleurs, rayons des angles, espacements, tailles de police, centralisés en un seul endroit.
   - Système de traductions minimal : toutes les chaînes affichées passent par des clés ; le français est la seule langue fournie, avec un test qui échoue si une clé est utilisée sans traduction.
   - Composants réutilisables nécessaires à l'écran principal : barre de menus, onglets de personas, barre d'options, rail d'outils vertical, barre d'onglets de documents, barre d'état, conteneur de panneaux ancrables.
3. **`apps/photo`** : une fenêtre qui affiche l'écran principal de `docs/views/ecran-principal.png`, avec :
   - Barre de menus : Fichier, Édition, Document, Calque, Sélection, Filtres, Affichage, Fenêtre, Aide (entrées internes inertes).
   - Personas : Photo et Export actifs ; Développement, Fluidité et Tone Mapping visibles mais désactivés (hors bêta).
   - Barre d'options de l'outil Pinceau (taille, dureté, opacité, flux, mode, stabilisateur), valeurs factices.
   - Rail d'outils à gauche, limité aux outils de la bêta (section 3 de la spec) : déplacer, recadrer, rectangle de sélection, lasso, baguette magique, pinceau, tampon de clonage, gomme, pipette, panoramique, zoom. Les outils texte, plume et formes sont **omis**.
   - Barre d'onglets de documents avec un seul onglet factice.
   - Zone centrale du canevas : un fond neutre avec un texte indiquant que le rendu arrive en J2.
   - Panneaux ancrables à droite : Histogramme, Réglages, Calques, Historique. Les onglets Canaux et Tracés de la maquette sont **omis** (hors bêta).
   - Barre d'état en bas.
4. **Panneaux ancrables** avec `egui_dock` : glisser un panneau pour le réancrer, le détacher en fenêtre flottante, le fermer et le rouvrir depuis le menu *Fenêtre*.
5. **Workspaces** : la disposition (panneaux, positions, tailles, panneaux ouverts) est sauvegardée à la fermeture, restaurée au démarrage, et une commande *Fenêtre > Réinitialiser la disposition* revient à la disposition par défaut. Le format de sauvegarde est versionné.
6. **Données factices** : tout ce qui est factice (calques d'exemple, courbe, histogramme) est isolé dans un module nommé `placeholder` pour être supprimé au jalon J3.
7. **Tests** : aller-retour de sérialisation de la disposition, restauration d'une disposition invalide ou d'une ancienne version (retour à la disposition par défaut sans plantage), couverture des traductions. Les tests ne doivent pas exiger d'écran (la CI n'en a pas).

## Interdit dans ce jalon

- Document, calques réels, commandes, historique réel.
- Rendu du canevas par wgpu (J2).
- Serveur MCP.
- Copier du code de `cygnus-legacy`.
- Ajouter une fonctionnalité d'interface absente de la maquette ou de la colonne « Dans la bêta » de la spec.

## Décisions à consigner dans `docs/decisions/`

- Versions figées d'`egui`, `eframe`, `egui_dock` et leur compatibilité.
- Jeu d'icônes retenu : licence compatible avec la GPL v3 (vérifiée par `cargo deny` ou par une note dans la décision si ce ne sont pas des crates).
- Police retenue et sa licence.
- Emplacement du fichier de configuration selon le système et crate utilisée.
- Toute limite d'`egui_dock` rencontrée.

## Critère de sortie

- L'intégration continue reste **verte sur Linux, Windows et macOS** (compilation et tests, sans fenêtre).
- L'application se lance et affiche l'écran principal conforme à la maquette ; les panneaux se réancrent, se détachent, se ferment et se rouvrent ; la disposition est restaurée après redémarrage.
- Une capture d'écran de l'application sur chaque système disponible est fournie à l'utilisateur, qui valide manuellement le comportement (la validation « sur les trois systèmes » du jalon exige de lancer l'application sur chacun).

## En fin de tâche

Compte rendu court : ce qui est fait, les limites d'`egui_dock` éventuellement rencontrées, les décisions consignées, les écarts avec la maquette et leur raison.
