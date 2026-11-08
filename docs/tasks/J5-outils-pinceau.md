# Consigne : jalon J5, Outils et pinceau

À placer dans `docs/tasks/J5-outils-pinceau.md`.

Phrase à donner à l'agent : « Lis `AGENTS.md` et `docs/spec.md`, puis réalise la **phase A** de la consigne `docs/tasks/J5-outils-pinceau.md`. Ne commence pas la phase B avant ma validation écrite. »

## Contexte

Le document, le rendu, les réglages, l'historique, les panneaux et le serveur MCP existent. J5 donne à l'utilisateur ses **outils** : peindre, sélectionner, transformer, recadrer. C'est le jalon où l'application devient une vraie application de retouche. Le critère de sortie de la spec est « retouche complète à la souris et à la tablette ».

Sections de `docs/spec.md` concernées : 3, 4.3 (MAS-1, SEL-1 à SEL-3), 4.5 (PIN-1 à PIN-5), 4.6 (TRA-1 à TRA-3), 4.7 (AFF-1), 4.8 (HIS-2), 5 (fluidité), 6 (barre d'options, rail d'outils), 7.2, 8 (outil `brush.stroke`), 10 (jalon J5).

**Préalable : vérifier l'état de J3 et J4.** Si B7 (budget de temps par image, tuiles manquantes en niveau grossier) n'est pas terminé, il passe **avant** tout le reste : un pinceau qui fige l'interface est inutilisable. Si B10 (filtres de voisinage : flou gaussien, netteté) n'est pas fait, il est replanifié en dernière étape de la phase B, et peut glisser vers J6 sur décision de l'utilisateur.

**Ce jalon touche des zones à supervision étroite** (moteur de trait sur GPU, modèle de sélection, catalogue de commandes) : deux phases, et la phase B ne commence qu'après validation écrite de la phase A.

## Phase A : proposition (aucun code de production)

Rédiger dans `docs/decisions/` des propositions numérotées, avec alternatives écartées et risques, sur :

1. **Moteur de trait** : empreintes (dabs) posées sur GPU, interpolation et espacement, profil de dureté, pression, inclinaison et vitesse (PIN-2), stabilisateur, différence entre **opacité** (par trait) et **flux** (par empreinte), mode de fusion du pinceau, gomme comme soustraction d'alpha. Où le trait est accumulé avant d'être fusionné dans le calque (tampon de trait en f32, puis écriture dans le cache entier de 16 bits), et quelle est la précision visée.
2. **Trait et commandes** : la commande `brush.stroke` (points, réglages, cible) est **rejouable** : même résultat par l'interface, par un script et par l'agent MCP. Aperçu pendant le geste avant l'envoi de la commande, une seule entrée d'historique par trait (HIS-2), annulation par copies de tuiles à mémoire bornée (même mécanisme que `pixels.write`). Dire quelle tolérance de reproductibilité entre adaptateurs est attendue pour un trait.
3. **Cibles** : calque pixel ou masque (MAS-1, PIN-3), selon la cible active ; comportement sur un calque verrouillé, caché, d'un autre type, ou sans tuile.
4. **Tampon de clonage** (PIN-4) : source d'échantillonnage (point d'alignement, calque courant ou image composée), traitement des bords, aperçu de la source.
5. **Sélections** : modèle (masque de couverture antialiasé), stockage, opérations (ajouter, soustraire, intersecter, inverser, adoucir) et leur coût, conversion en masque et inverse (SEL-3), effet d'une sélection sur le pinceau, le remplissage et les transformations, affichage (fourmis en marche), baguette magique (tolérance, contiguïté, algorithme et lieu d'exécution CPU ou GPU), lasso et rectangle (rastérisation antialiasée).
6. **Remplissage et dégradé** : outil de remplissage (seau) avec tolérance, outil de dégradé linéaire pour les masques (la commande existe déjà en J3).
7. **Transformations** (TRA-1 à TRA-3) : modèle (matrice par nœud), rééchantillonnage **en linéaire** (décision de 0021), qualité (filtre), cas de l'objet incorporé non destructif et du calque pixel (une seule rastérisation à la validation), recadrage du document avec ou sans suppression des pixels hors cadre, aperçu pendant le geste, annulation.
8. **Entrées** : souris, clavier et tablette sous Linux, Windows et macOS (pression, inclinaison) ; ce que `egui` et `winit` fournissent réellement, repli sans tablette, comment tester sans matériel (événements synthétiques).
9. **Interface** : rail d'outils branché, barre d'options liée à l'outil actif (réglages mémorisés par outil), pastilles de couleur avant-plan/arrière-plan avec un sélecteur de couleur, pipette, raccourcis `[` et `]`, Alt, Espace (PIN-5), curseur du pinceau, règles du canevas (AFF-1, reportées depuis J2). Fiches de design dans `docs/views/`.
10. **Performance** : latence du trait (entre l'entrée et l'affichage) sur un document de 24 Mpx en 16 bits avec une dizaine de calques ; cible à proposer et à mesurer, lien avec le budget de temps par image et la taille des réserves de tuiles.
11. **Ouverture** : les pinceaux sensibles à la lumière et les textures générées ne sont **pas** dans la bêta. Vérifier seulement que l'architecture du moteur de trait ne les empêche pas plus tard.
12. **Tests** : traits rejoués avec images de référence sur les adaptateurs de la CI, tests de propriétés sur les opérations de sélection (par exemple « inverser deux fois redonne la sélection »), transformations contre des valeurs calculées à la main, annuler/refaire de chaque outil.
13. **Découpage de la phase B** en petites étapes vérifiables, avec le moteur de trait en premier.

Terminer par un résumé de dix lignes au plus, avec les **trois décisions les plus risquées** et les questions à trancher, puis **s'arrêter**.

## Phase B : implémentation (après validation écrite de la phase A)

Selon la proposition validée :

- Moteur de trait, pinceau, gomme et tampon de clonage, avec `brush.stroke` dans le catalogue de commandes (donc automatiquement exposée au serveur MCP).
- Sélections et leurs opérations, baguette magique, lasso, rectangle, remplissage, dégradé.
- Transformations et recadrage.
- `apps/photo` : outils branchés, barre d'options, couleurs, pipette, raccourcis, règles ; entrées tablette.
- Banc de latence du trait, rapport JSON dans `docs/bench/` daté avec la machine.
- Si B10 reste à faire : flou gaussien et netteté en dernière étape.

## Interdit dans ce jalon

- Écrire dans un calque, un masque ou la sélection autrement que par une commande du bus.
- Plusieurs copies des formules de fusion, ou un second chemin de rendu.
- Outils texte, plume, formes, pinceaux à textures ou sensibles à la lumière (hors bêta).
- Toute décision de la phase A prise sans validation.

## Critère de sortie

- Phase A validée par écrit.
- Un trait rejoué par script donne le même résultat que par l'interface, vérifié par des images de référence sur les adaptateurs de la CI.
- Chaque outil s'annule et se refait, vérifié par des tests de propriétés ; un trait est une seule entrée d'historique.
- La **retouche complète à la souris** est validée à la main par l'utilisateur sur Linux, et à la **tablette** sur au moins un système (sinon la réserve est notée).
- La latence du trait est mesurée et la cible confirmée ou corrigée par l'utilisateur.
- `cargo fmt`, `clippy`, `test`, `build`, en-têtes et `cargo deny` passent ; la CI est verte sur Linux, Windows et macOS.

## En fin de tâche

Compte rendu court : ce qui est fait, les écarts avec la proposition validée, les mesures de latence, les limites constatées (tablette, plateformes) et les risques pour J6.
