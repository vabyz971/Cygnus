# Consigne : jalon J3, Calques et réglages

À placer dans `docs/tasks/J3-calques-reglages.md`. Avant de lancer l'agent, réexporter `docs/spec.md` (la spec a été mise à jour après J2).

Phrase à donner à l'agent : « Lis `AGENTS.md` et `docs/spec.md`, puis réalise la **phase A** de la consigne `docs/tasks/J3-calques-reglages.md`. Ne commence pas la phase B avant ma validation écrite. »

## Contexte

J0, J1 et J2 sont terminés. Le document ne contient pour l'instant que des calques pixel, et les panneaux Calques, Réglages, Histogramme et Historique affichent des données factices (module `placeholder`). J3 fait du document un vrai document de retouche : groupes, masques, calques de réglage, objets incorporés, modes de fusion, historique complet, et deux filtres de voisinage. Il branche l'interface sur le bus de commandes.

Sections de `docs/spec.md` concernées : 2.1, 3, 4.2 (CAL-1 à CAL-7), 4.3 (MAS-1), 4.4 (REG-1 à REG-4), 4.8 (HIS-1, HIS-3), 5, 7.1 à 7.3, 11, 10 (jalon J3). Reprendre aussi les décisions 0009 à 0019, en particulier 0012 (plan d'extension du rendu) et 0019 (mesures et cibles de fluidité).

**Ce jalon est une zone à supervision étroite** (`AGENTS.md`, section 6) : modèle du document, catalogue de commandes et passes de rendu. Il se déroule en deux phases ; la phase B ne commence qu'après validation écrite de la phase A.

## Phase A : proposition (aucun code de production)

Rédiger dans `docs/decisions/` des propositions numérotées, avec alternatives écartées et risques, sur :

1. **Groupes** : composition isolée, mode de fusion et opacité d'un groupe, groupe « passant » ou non, coût en tuiles intermédiaires.
2. **Masques** : modèle (un masque par calque ou par groupe, en niveaux de gris), stockage, rendu, densité, inversion. Le pinceau arrive en J5 : en J3, un masque se remplit et s'inverse par commandes.
3. **Calques de réglage** : modèle des paramètres de niveaux, courbes (RVB et par canal), teinte/saturation, luminosité/contraste, équilibre colorimétrique ; comment ils sont évalués sur GPU ; intensité (opacité) et masque ; détourage (CAL-5).
4. **Objets incorporés** : la source reste intacte et rééchantillonnable. Un décodage PNG et JPEG minimal (`photo/io`, lecture seule) est autorisé uniquement pour créer un objet incorporé depuis un fichier. L'interface d'import complète est en J6.
5. **Modes de fusion** : tous les modes de CAL-3 dans `blend.wgsl`, qui reste le seul exemplaire des formules. Les valeurs de test sont calculées d'après la spécification W3C *Compositing and Blending*, jamais d'après l'ancien dépôt.
6. **Historique** : annuler/refaire pour toute commande (HIS-1), entrées avec heure et source (HIS-3), mémoire bornée, interaction avec l'annulation par copies de tuiles de `pixels.write`, fusion d'entrées consécutives. Les traits de pinceau en une entrée (HIS-2) arrivent en J5, mais le mécanisme doit le permettre.
7. **Filtres de voisinage** (flou gaussien, netteté par masque flou) : passes avec marge, tuiles intermédiaires, comment le plan d'extension de 0012 s'applique, aperçu en direct pendant un glissement de paramètre.
8. **Ordonnancement du calcul** : budget de temps par image obligatoire pour le calcul des tuiles, affichage grossier d'abord, aperçu réduit en moins de 33 ms pendant un glissement, image finale en moins de 150 ms. Vérifier que le budget de tuiles est calculé d'après la fenêtre et la mémoire GPU, avec test en 4K (demandé à la fin de J2) ; si ce n'est pas fait, l'inclure.
9. **Interface** : branchement des panneaux Calques, Réglages, Histogramme (REG-4) et Historique sur le bus ; menu contextuel d'un calque (CAL-6, CAL-7, l'entrée « Afficher dans le graphe » reste grisée) ; message d'état quand l'affichage se replie sur un niveau grossier.
10. **Scénario photo sans fenêtre** : script JSON de commandes qui crée un objet incorporé depuis un fichier, ajoute des réglages avec masque, un groupe, un mode de fusion, un filtre, fait annuler/refaire, puis rend une image. Il sert de critère de sortie.
11. **Test de bandes f16** : mesurer la perte à travers des courbes et niveaux raides, et proposer s'il faut passer à f32 pour certaines passes.
12. **Découpage de la phase B** en petites étapes vérifiables, avec le filtre de voisinage en dernière étape : s'il menace le calendrier, il peut glisser vers J5 sur décision de l'utilisateur.

Terminer par un résumé de dix lignes au plus, avec les **trois décisions les plus risquées** et les questions à trancher, puis **s'arrêter**.

## Phase B : implémentation (après validation écrite de la phase A)

Selon la proposition validée :

- `photo/core` : groupes, masques, calques de réglage, objets incorporés, commandes correspondantes (créer, dupliquer, supprimer, renommer, regrouper, fusionner vers le bas, réordonner, détourer), historique complet.
- `photo/render` : composition de groupes isolés, passes de réglage, masques, tous les modes de fusion, filtres avec marge, ordonnancement par budget de temps.
- `apps/photo` : panneaux réels branchés sur le bus ; **suppression du module `placeholder`** ; message d'état du repli grossier.
- `apps/cli` : exécution du scénario photo JSON et rendu.
- Tests unitaires, tests de propriétés sur les commandes et l'historique (annuler puis refaire redonne le même document), images de référence sur les trois adaptateurs de la CI.
- Banc de fluidité relancé sur un document de 24 Mpx en 16 bits avec une dizaine de calques **dont des réglages et des masques**, rapport JSON dans `docs/bench/` sous un nom daté avec la machine.

## Interdit dans ce jalon

- Pinceau et autres outils (J5), sélections (J5), serveur MCP (J4), format `.cygnus` et dialogues d'import et d'export (J6).
- Un second chemin de rendu ou plusieurs copies des formules de fusion.
- Valeurs attendues tirées de l'ancien dépôt, ou copie de son code.
- Toute décision de la phase A prise sans validation.

## Critère de sortie

- Phase A validée par écrit.
- Le **scénario photo complet** réussit en mode sans fenêtre, et son rendu est conforme aux images de référence sur lavapipe, WARP et Metal.
- Annuler et refaire fonctionnent pour toutes les commandes du scénario, vérifiés par des tests de propriétés.
- Les panneaux affichent le vrai document ; `placeholder` n'existe plus.
- Les cibles de fluidité de la section 5 sont tenues sur le document de test avec réglages et masques, ou l'écart est consigné avec mesures et pistes.
- La décision sur le f16 est revue et consignée.
- `cargo fmt`, `clippy`, `test`, `build`, en-têtes et `cargo deny` passent ; la CI est verte sur Linux, Windows et macOS.

## En fin de tâche

Compte rendu court : ce qui est fait, les écarts avec la proposition validée, les mesures, les limites et les risques pour J4 et J5.
