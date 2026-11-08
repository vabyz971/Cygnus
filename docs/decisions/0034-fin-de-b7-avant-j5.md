# 0034 — Fin de B7 avant J5 : budget de temps par image et affichage grossier d'abord

- **Date** : 2026-11-08
- **Statut** : proposée (préalable de la phase A de J5)
- **Exigences concernées** : spec 5 (fluidité), 7.3 ; décisions 0019, 0021, 0024, 0027

## Contexte : état de B7 au 2026-11-08

B7-lite (0027, point 5 ; 0019) a fait :

- les réserves de sources dimensionnées d'après le budget ;
- le préchargement au repos : un changement d'opacité ne renvoie aucune source ;
- un aperçu depuis des sources réduites pendant un glissement : 0,9 ms par image au lieu de 1 050 ms le même jour.

Il reste de 0024, et c'est ce qui manque à un pinceau utilisable :

1. **Aucun budget de temps par image** : la première vue, la première image d'un glissement et l'affinage après relâchement se calculent d'une traite. Mesuré le 2026-11-03 sur une machine lente : 840 à 1 040 ms, soit 170 à 200 ms à vitesse normale. Une image dépasse donc 100 ms.
2. **Pas d'affichage grossier d'abord** : la table des pages ne couvre que le niveau affiché ; une tuile manquante n'emprunte pas un niveau plus grossier.
3. **Sources réduites non conformes à 0024** :
   - elles sont construites sur GPU après envoi des tuiles pleine taille (l'alternative que 0024 avait écartée, d'où les 850 ms de la première image d'un glissement) ;
   - elles sont moyennées en valeurs encodées (0021 demande le linéaire pour les réductions) ;
   - elles ne sont pas bornées à 10 % de la mémoire des pixels ;
   - elles ne servent pas à la première vue.
4. **Préparation des envois sur le fil de l'interface** (point 5 de 0024).
5. **Mesures non faites** : scénario de 8 Gio et test 4K.

Un trait de pinceau ajoute des tuiles à recomposer à chaque image. Sans budget, un trait sur un grand pinceau en vue ajustée gèlerait l'interface. B7 passe donc **avant tout le reste de J5**, comme le demande la consigne.

## Proposition : B7 complet, en quatre étapes vérifiables (étapes **T1 à T4** de la phase B de J5)

1. **T1. File de travail par priorité et budget de temps** (0024, point 1) :
   - chaque image traite des lots de tuiles tant que le temps processeur passé reste sous 6 ms et que le coût GPU estimé reste sous 6 ms ;
   - le coût GPU d'une tuile est une moyenne glissante mesurée par `on_submitted_work_done` ;
   - ordre : tuiles visibles au niveau le plus grossier qui manque, puis tuiles visibles au niveau voulu, puis voisines ;
   - test : aucune image au-delà de 100 ms au banc, sur les phases première vue, relâchement et première image de glissement.
2. **T2. Affichage grossier d'abord** (0024, point 2) :
   - la table des pages couvre tous les niveaux du cache ;
   - le shader d'affichage prend le niveau voulu s'il est prêt, sinon le premier niveau plus grossier présent, sinon le damier ;
   - test : une vue dont seul un niveau grossier existe s'affiche agrandie, sans trou.
3. **T3. Sources réduites conformes à 0024** :
   - calculées **sur un fil de travail, sur le processeur**, depuis les tuiles du document, par moyenne **en linéaire** (décodage, moyenne prémultipliée, réencodage) ;
   - paresseuses (seulement le niveau de la vue entière et le suivant) ;
   - invalidées tuile par tuile par les commandes de pixels ;
   - mémoire bornée à 10 % des pixels du document, évincées en premier ;
   - elles servent à la première vue grossière et aux glissements. Le GPU ne reçoit plus que des tuiles déjà réduites ;
   - on garde la composition depuis les réserves réduites de B7-lite, on remplace leur construction ;
   - tests : sources réduites contre une moyenne linéaire calculée à la main ; même luminosité moyenne entre l'aperçu grossier et l'image affinée (condition de 0021) ; budget mémoire respecté.
4. **T4. Préparation des envois hors du fil de l'interface, et mesures** :
   - la lecture des `Arc` et la copie vers la mémoire d'envoi passent sur un fil de travail ;
   - banc relancé, avec ces objectifs :
     - aucune image au-delà de 100 ms ;
     - première vue grossière en moins de 100 ms ;
     - glissement en moins de 33 ms, première image comprise ;
     - image finale en moins de 150 ms ;
   - scénario de 8 Gio (40 calques de 24 Mpx en 16 bits) : mémoire processeur maximale et temps de la première image ; test 4K ;
   - rapports datés dans `docs/bench/`, et mesure de référence du code précédent le même jour (leçon du 2026-11-03).

## Alternatives écartées

- **Garder la construction GPU des sources réduites** (B7-lite) : elle exige d'envoyer d'abord toutes les tuiles pleine taille, d'où 850 ms sur la première image d'un glissement et l'impossibilité d'une première vue grossière rapide.
- **Commencer J5 sans B7** : interdit par la consigne, car un pinceau qui fige l'interface est inutilisable.

## Risques

- **Synchronisation du fil de travail** avec l'instantané du document : les tuiles sont des `Arc` immuables, donc en lecture seule, mais il faut annuler les travaux périmés à chaque révision.
- **Estimation du coût GPU** : une mesure trop optimiste fait dépasser le budget ; il faut une marge et une borne dure sur le nombre de tuiles par image.
