# 0024 — Ordonnancement du calcul des tuiles par budget de temps (J3)

- **Date** : 2026-11-01
- **Statut** : acceptée avec conditions le 2026-11-01 (phase A du jalon J3) ; conditions intégrées ci-dessous
- **Exigences concernées** : spec section 5 (fluidité) et 7.3 ; cibles de 0019 ; complète 0018

## Contexte

En J2, `Canvas::prepare` calcule d'une traite toutes les tuiles manquantes. La première vue d'un document de 24 Mpx et 10 calques bloque l'interface 2,3 s ; un changement de propriété bloque une image de 119 ms. Cibles de la spec : jamais plus de 100 ms de blocage, aperçu réduit en moins de 33 ms pendant un glissement, image finale en moins de 150 ms, et un budget de temps par image obligatoire.

**Budget de mémoire** (vérification demandée) : il est fait. Il est calculé d'après la fenêtre et la mémoire GPU, et testé en 4K (0018, commit `0b8d578`).

## Proposition

1. **File de travail par priorité.** Chaque image, le canevas fait la liste des tuiles à calculer, puis en traite le plus possible **dans un budget de temps** (proposé : 6 ms de travail processeur et un coût GPU estimé de 6 ms). Le reste attend l'image suivante. Ordre de priorité :
   - tuiles visibles au niveau le plus grossier qui manque ;
   - tuiles visibles au niveau voulu ;
   - tuiles voisines (anticipation du panoramique).

   Le coût GPU d'une tuile est une moyenne glissante, mesurée par la fin de travail de la file (`on_submitted_work_done`).
2. **Affichage grossier d'abord.** La table des pages couvre **tous les niveaux**. Le shader d'affichage prend le niveau voulu s'il est prêt, sinon le premier niveau plus grossier disponible (agrandi), sinon le damier. L'image s'affine sans attendre la fin du calcul.
3. **Aperçu grossier sans calcul plein format** (risque n° 1). La pyramide de 0018 est faite de résultats composés : un niveau grossier n'existe qu'après le calcul complet du niveau 0, ce qui interdit l'aperçu rapide d'un document neuf.

   **Pourquoi des pyramides par calque en plus de 0018** (condition de validation) : 0018 a écarté les pyramides par calque **comme cache d'affichage**, parce qu'elles multiplient les tuiles à garder et donnent un résultat approché pour les modes non linéaires. Elles reviennent ici dans un **rôle différent et limité** : servir de source rapide à un **aperçu provisoire**, jamais de résultat. La pyramide de 0018 reste la seule vérité affichée dès qu'elle est calculée. Sans sources réduites, il faut composer tout le niveau 0 (2,3 s mesurées pour 24 Mpx et 10 calques) avant la première image grossière : la cible de 100 ms est impossible. Les règles imposées :

   - **construction paresseuse, en arrière-plan** : une tuile réduite n'est calculée que quand un aperçu en a besoin, sur un fil de travail, jamais sur le fil de l'interface ;
   - **niveaux limités à ceux de l'aperçu** : seul le niveau qui correspond à la vue entière dans la fenêtre (et le suivant) est construit, pas toute la chaîne de 1/2 à 1/16 ;
   - **éviction prioritaire** : ce sont les premières données évincées, avant tout cache exact, et elles ont leur propre budget (au plus 10 % de la mémoire des pixels du document) ;
   - **mesure sur un scénario de 8 Gio** de pixels (par exemple 40 calques de 24 Mpx en 16 bits) : mémoire processeur maximale, temps de la première image grossière et respect du budget, en phase B (B7).

   Fonctionnement :
   - les tuiles réduites des sources (calques, masques, objets incorporés) sont calculées par moyenne en linéaire (tableau de 0021), invalidées tuile par tuile par les commandes de pixels, et recalculées paresseusement ;
   - un niveau grossier peut alors être **composé directement depuis les sources réduites**. C'est approximatif pour les modes non linéaires et les filtres (rayons réduits), mais **rapide** ;
   - la tuile approximative est **remplacée** par la tuile exacte, réduite depuis le niveau 0, dès que celle-ci est calculée ;
   - **l'aperçu à 100 % et l'export n'utilisent jamais les tuiles approximatives** : l'identité au bit près est intacte.
4. **Pendant un glissement** (geste ouvert, 0022), seules les tuiles visibles sont recalculées, à un niveau choisi pour tenir dans le budget d'une image de 33 ms. À la fin du geste, le niveau voulu est recalculé dans la file. Les sources restant sur le GPU (0021), recalculer n'envoie aucune donnée.
5. **Travail processeur hors du fil de l'interface** : la préparation des envois de tuiles (lecture des `Arc` de l'instantané, copie vers la mémoire d'envoi) passe sur un fil de travail. Le fil de l'interface ne fait qu'enregistrer et soumettre les commandes GPU.
6. **Message d'état** : le canevas expose son état : niveau affiché, nombre de tuiles en attente, repli imposé par le budget. Le contenu du message est dans 0025.

## Mesures attendues, à vérifier au banc

Banc relancé sur 24 Mpx, 10 calques **dont des réglages et des masques** :

- aucune image au-delà de 100 ms ;
- première vue grossière en moins de 100 ms ;
- aperçu réduit en moins de 33 ms pendant un glissement d'opacité ;
- image finale en moins de 150 ms.

Tout écart est consigné avec mesures et pistes.

## Alternatives écartées

- **Calculer le niveau 0 par morceaux sans aperçu grossier** : l'interface ne bloque plus, mais le canevas reste vide plusieurs secondes.
- **Pyramide par calque calculée sur GPU** : elle demande de tout envoyer avant d'afficher quoi que ce soit, donc le même coût de départ.
- **Fil de rendu séparé avec son propre périphérique** : complexité de synchronisation avec egui, sans gain sur le coût GPU.

## Risques

- **Mémoire processeur** : bornée par le budget des sources réduites (au plus 10 % de la mémoire des pixels), au lieu des 33 % d'une pyramide complète.
- Les tuiles approximatives d'un niveau grossier peuvent différer visiblement des tuiles exactes pour les modes non linéaires : un léger « saut » à l'affinage. C'est accepté pour la fluidité, et documenté.
