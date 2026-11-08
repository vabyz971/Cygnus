# 0034 — Fin de B7 avant J5 : budget de temps par image et affichage grossier d'abord

- **Date** : 2026-11-08
- **Statut** : acceptée le 2026-11-08 (« tu peux poursuivre B7 et B10 ») ; mise en œuvre le 2026-11-08, écarts ci-dessous
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

## Mise en œuvre (2026-11-08)

Commits 120d075 (T1, T2), db2dfe1 (T3) et 70ddc3c (T4). Mesures dans 0019, section « Mesure après B7 ».

**Ce qui est fait comme proposé :**

- **T1** :
  - le canevas planifie son travail comme avant, puis n'en exécute qu'un préfixe par image : un plan est en ordre de dépendance, donc tout préfixe est cohérent ;
  - le budget par défaut est de 6 ms de processeur, 6 ms de GPU estimé et 64 tuiles. Le coût GPU d'une tuile est une moyenne glissante mesurée par `on_submitted_work_done`, et le coût processeur, une moyenne mesurée lot par lot ;
  - un lot compte au plus 8 tuiles, et une image fait toujours au moins un lot ;
  - l'application demande une nouvelle image tant que du travail reste.
- **T2** :
  - la table des pages couvre tous les niveaux ;
  - le shader d'affichage prend la tuile du niveau voulu, sinon celle du premier niveau plus grossier présent, sinon le damier ;
  - une tuile invalidée reste affichée jusqu'à son remplacement : après un changement, l'ancienne image reste visible au lieu d'un trou.
- **T3** :
  - les sources réduites sont calculées sur 4 fils de travail, depuis les tuiles du document, en linéaire et prémultipliées, puis réencodées en 16 bits. Les tests les comparent à un calcul à la main en f64 ;
  - le GPU ne reçoit que des tuiles réduites ;
  - une entrée garde des références faibles vers les tuiles d'origine : elle est périmée dès que l'une change, sans risque de réemploi d'une adresse ;
  - la mémoire est bornée à 10 % des pixels, avec au moins 32 Mio pour les petits documents. Les entrées les plus anciennes sont évincées, jamais celles de l'image en cours ;
  - les réductions devenues inutiles sont annulées ;
  - l'aperçu d'un glissement s'écarte maintenant de l'image exacte d'au plus un niveau 8 bits, pour un écart moyen de 0,001 (test `drag_preview`).
- **T4** : un scénario de 8 Gio et une fenêtre 4K ont été mesurés, ainsi qu'une référence du code précédent le même jour.

**Écarts :**

1. **Niveau des tuiles approximatives** : ce n'est pas toujours le niveau de la vue entière. C'est le plus fin, à partir du niveau affiché, dont les sources réduites des tuiles visibles tiennent dans leur budget mémoire, avec 4 au plus. En vue ajustée sur 2126 × 1086, c'est le niveau 2 pour un affichage au niveau 1 : le niveau 1 demanderait plus de 10 % de la mémoire des pixels. L'aperçu est donc plus flou, mais tenu dans le budget.
2. **Première vue grossière en moins de 100 ms : non tenue** sur la machine de référence (187 à 215 ms, avec l'image finale). Voir 0019 : il faut lire les 1,4 Gio de pixels, et le GPU le fait aussi vite par le chemin exact. Les images restent sous 34 ms : l'interface ne se fige plus.
3. **Image finale en moins de 150 ms : manquée de peu** (152 à 157 ms au relâchement), bornée par l'envoi des sources.
4. **Préparation des envois hors du fil de l'interface (T4) : faite en partie seulement** :
   - la copie de conversion des tuiles 16 bits est supprimée : elles sont envoyées depuis leur propre mémoire ;
   - la copie restante, faite par `write_texture`, reste sur le fil de l'interface, dans le budget de 6 ms. La déplacer sur un fil de travail obligerait à réserver des emplacements de la réserve d'une image à l'autre, pour qu'un envoi ne remplace pas une tuile encore lue par un travail non soumis ;
   - en régime, les images restent sous 9 ms : le gain ne justifie pas cette complexité pour l'instant. À revoir si une machine modeste le demande.
5. **Les canevas de test sans budget (`with_capacity`)** finissent chaque image et attendent les sources réduites. Seul le canevas de l'application, et le banc, travaillent par images. Les tests du budget l'activent explicitement.
