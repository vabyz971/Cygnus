# Consigne : jalon J2, Document et rendu

À placer dans `docs/tasks/J2-document-rendu.md`.

Phrase à donner à l'agent : « Lis `AGENTS.md` et `docs/spec.md`, puis réalise la **phase A** de la consigne `docs/tasks/J2-document-rendu.md`. Ne commence pas la phase B avant ma validation écrite. »

## Contexte

J0 (fondations) et J1 (interface) sont terminés. J2 construit le cœur de l'application : le **modèle du document**, le **bus de commandes** et le **rendu GPU par tuiles**, avec un seul type de calque, le calque pixel. C'est la zone la plus risquée du projet. Les règles de la section 2.1 de la spec s'appliquent strictement : un seul pipeline de rendu GPU, aucun chemin CPU de secours, des formules de fusion écrites une seule fois en WGSL.

Sections de `docs/spec.md` concernées : 2.1, 4.2 (CAL-1, CAL-2), 5 (fluidité, mémoire, couleur, cohérence), 7.1 à 7.3, 11 (risques), 10 (jalon J2).

**Ce jalon est une zone à supervision étroite** (`AGENTS.md`, section 6) : il se déroule en deux phases, et la phase B ne commence qu'après validation de la phase A par l'utilisateur.

## Phase A : proposition (aucun code de production)

Produire `docs/decisions/` un ou plusieurs fichiers numérotés contenant une **proposition argumentée** sur chacun des points ci-dessous, avec les alternatives écartées et les risques. Un petit prototype jetable est autorisé pour répondre à une question technique précise (par exemple tester un adaptateur GPU logiciel), à condition de ne pas le fusionner.

1. **Modèle du document** : structure de l'arbre de calques, identifiants stables des nœuds, propriétés (nom, visibilité, opacité, verrouillage), sérialisation (`serde`). Seul le calque pixel est implémenté en J2, mais le modèle doit déjà accepter groupes, masques, réglages et objets incorporés sans refonte (J3), et être projetable en graphe plus tard (section 7.1).
2. **Commandes et bus** : définition d'une commande (nom, paramètres typés, sérialisable), application, notification des vues, représentation de l'annulation. Le catalogue doit pouvoir produire automatiquement des outils MCP (J4). L'historique complet est prévu en J3 : dire seulement ce que J2 doit prévoir pour ne pas le bloquer.
3. **Format de pixel et couleur** : format interne (la spec propose f16 linéaire), convention d'alpha (prémultiplié ou non), conversion vers l'écran, cas des images 8 bits importées. Justifier.
4. **Tuiles** : taille des tuiles, stockage creux (texture éparse), cache par tuile et règle d'invalidation, gestion de la mémoire GPU, comportement quand la mémoire GPU est insuffisante.
5. **Pipeline de rendu** : compilation de l'arbre en passes GPU, organisation du code WGSL, **module unique des formules de fusion** (Normal obligatoire en J2 ; les autres modes de CAL-3 arrivent en J3), mécanisme pour qu'ajouter un mode ne touche qu'un endroit.
6. **Rendu sans fenêtre** : le même pipeline doit produire une image sans fenêtre (pour `cygnus render`, les tests et les agents). Proposer comment obtenir un adaptateur GPU **en intégration continue sur Linux, Windows et macOS** (pilote logiciel ou autre) et dire honnêtement ce qui ne sera pas possible sur chaque système.
7. **Tests d'images de référence** (golden images) : où sont stockées les images, comment elles sont comparées, **écart maximal toléré** proposé et justifié, gestion des différences entre pilotes.
8. **Intégration dans l'interface** : dessin du canevas par wgpu dans la fenêtre `egui` (callback), zoom et panoramique minimaux, conversion à l'affichage.
9. **Banc d'essai de fluidité** : scénario exact (photo de 24 Mpx en 16 bits, une dizaine de calques, panoramique et zoom), mesures relevées, comment le lancer. La spec propose 60 images/s : dire comment on le mesurera, sans promettre le résultat.
10. **GPU minimal supporté** : proposer comment le déterminer et ce qu'il faut mesurer.
11. **Dépendances nouvelles** (`wgpu`, `half`, `bytemuck`, `serde`, etc.) : versions, compatibilité entre elles et avec `egui`, licences.
12. **Découpage de la phase B** en petites étapes, chacune avec son critère de vérification.

Terminer la phase A par un résumé de dix lignes au plus, avec les **trois décisions les plus risquées** et les questions qu'il faut trancher. **S'arrêter là** et attendre la validation.

## Phase B : implémentation (après validation écrite de la phase A)

À livrer, d'après la proposition validée :

1. `photo/core` : modèle du document et commandes de base (créer un document, ajouter un calque pixel, modifier ses propriétés, supprimer, réordonner), toutes passant par le bus.
2. `photo/render` : rendu GPU par tuiles, module unique de fusion (Normal), rendu sans fenêtre.
3. `apps/cli` : commande `cygnus render <document> <sortie.png>` qui utilise exactement le pipeline de l'application.
4. `apps/photo` : le canevas factice de J1 est remplacé par le vrai canevas, qui affiche un document de démonstration, avec zoom et panoramique.
5. Tests unitaires du modèle et des commandes, tests d'images de référence en intégration continue sur les systèmes où un adaptateur GPU existe.
6. Banc d'essai de fluidité et rapport des mesures, consigné dans `docs/decisions/`.

## Interdit dans ce jalon

- Un second chemin de rendu (CPU ou autre) pour l'aperçu, même temporaire.
- Plusieurs copies des formules de fusion.
- Les autres types de calques (groupes, masques, réglages, objets incorporés) : J3.
- Pinceau et outils : J5. Import, export et format `.cygnus` : J6. Serveur MCP : J4.
- Copier du code de `cygnus-legacy` (ses formules de fusion servent uniquement de référence pour les tests).
- Toute décision de la phase A prise sans validation.

## Critère de sortie

- Phase A validée par écrit.
- Sur les documents de référence, l'export est **identique à l'aperçu** dans l'écart toléré, vérifié en intégration continue sur tous les systèmes où un adaptateur GPU est disponible, et la limite sur les autres est documentée.
- Les mesures de fluidité sont relevées sur du matériel réel, consignées avec la description de la machine, et les cibles de la section 5 sont confirmées ou corrigées par l'utilisateur.
- `cargo fmt`, `clippy`, `test`, `build`, en-têtes et `cargo deny` passent ; l'intégration continue est verte sur Linux, Windows et macOS.

## En fin de tâche

Compte rendu court : ce qui est fait, les écarts avec la proposition validée, les mesures obtenues, les limites constatées (en particulier côté CI et GPU) et les risques restants pour J3.
