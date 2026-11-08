# 0039 — J5 : entrées, interface, tests et découpage

- **Date** : 2026-11-08
- **Statut** : proposée (phase A du jalon J5)
- **Exigences concernées** : PIN-2, PIN-5, AFF-1 ; spec 6, 10 (J5) ; décisions 0025, 0034 à 0038

Cette décision traite les points 8 (entrées), 9 (interface), 12 (tests) et 13 (découpage) de la consigne.

## Proposition 8 : entrées

**Ce que fournissent réellement les bibliothèques** (versions figées : `egui` et `egui-winit` 0.36.2, `winit` 0.30.13).

À vérifier sur matériel en phase B : les seules sources trouvées sont des projets tiers, et la documentation de `winit` ne détaille pas le cas des tablettes.

| Système | Souris et clavier | Pression | Inclinaison |
| --- | --- | --- | --- |
| Windows | oui | **oui** : le stylet arrive comme un toucher avec force, qu'egui transmet en `Event::Touch { force }` | non (perdue par winit) |
| Linux Wayland | oui | **non** : winit 0.30 ne lie pas le protocole tablet-v2, le stylet devient un pointeur | non |
| Linux X11 | oui | **non** : winit ne lit pas ces axes | non |
| macOS | oui | **non** pour une tablette (seuls les pavés Force Touch) | non |

**Conséquence** : avec les versions figées, PIN-2 n'est tenu que **sous Windows, et pour la pression seulement**. Le poste principal de l'utilisateur (NixOS, Wayland) n'a pas de pression. Options (**question Q1**) :

- **A. Accepter** la limite en bêta : pression sous Windows, aucune ailleurs, et la réserve notée au critère de sortie (la consigne le permet : « sinon la réserve est notée ») ;
- **B. Attendre** une version d'egui fondée sur winit 0.31, qui transmet les événements de stylet (pression, inclinaison). Cela dépend du calendrier d'egui et de la vérification de compatibilité egui, egui_dock et wgpu (AGENTS.md, règle 7) ;
- **C. Lire la tablette à côté de winit** (libinput sous Linux, API native sous macOS) : du code par plateforme, des droits d'accès aux périphériques sous Linux. **Écartée** par défaut.

Recommandation : **A maintenant**, avec une couche `StrokeInput { position, pressure: Option, tilt: Option, time }` qui isole l'outil de la source des événements, puis **B** dès qu'une version compatible existe.

**Repli sans tablette** : la pression vaut 1, et la vitesse peut moduler le diamètre (option).

**Tests sans matériel** : l'outil reçoit des `StrokeInput` synthétiques (tests unitaires du contrôleur d'outil). Un test d'intégration envoie à egui des `Event::Touch` avec force et des événements de pointeur simulés, puis vérifie la commande produite.

## Proposition 9 : interface

Fiche de design : `docs/views/outils-et-barre-d-options.md`.

- **Rail d'outils branché** : outils du pinceau, de la gomme, du tampon, de la sélection (rectangle, lasso, baguette), du remplissage, du dégradé, du recadrage, de la transformation, de la pipette, du panoramique et du zoom. Les outils hors bêta restent absents.
- **Barre d'options liée à l'outil actif** : les réglages sont **mémorisés par outil** et persistés dans les préférences (`~/.config/cygnus/photo/tools.json`).
- **Couleurs** : pastilles avant-plan et arrière-plan, échange (`X`), retour au noir et blanc (`D`), et sélecteur de couleur en boîte flottante (carré TSV, curseurs, saisie hexadécimale, couleurs récentes). Les couleurs sont en sRGB direct.
- **Pipette** : outil, ou Alt maintenu avec un outil de peinture (PIN-5). Elle lit l'image composée (option « calque courant ») ; la lecture d'un pixel est asynchrone et arrive à l'image suivante.
- **Raccourcis** (PIN-5) :
  - `[` et `]` diminuent ou augmentent la taille par pas proportionnels (10 %) ;
  - Maj + `[` ou `]` règlent la dureté ;
  - Alt donne la pipette, Espace le panoramique (déjà fait), et les lettres des outils sont celles de Photoshop (B, E, S, M, L, W, G, C, V).
- **Curseur du pinceau** : contour du diamètre au zoom courant, avec une croix au centre quand le diamètre est sous 4 px. Il est dessiné par egui, et le curseur système est masqué au-dessus du canevas.
- **Règles** (AFF-1, reportées de J2) : en haut et à gauche du canevas, graduées en pixels du document selon le zoom, avec un repère de la position du pointeur. Elles se montrent et se cachent depuis le menu Affichage.

## Proposition 12 : tests

- **Traits rejoués** : golden images sur lavapipe, comparées sur WARP et Metal (0016). Un même trait est envoyé par le contrôleur d'outil simulé, puis rejoué par script : égalité au bit près sur un adaptateur.
- **Moteur** : positions d'empreintes contre un calcul à la main (droite, espacement), couverture d'une empreinte contre le profil en f64, plafond de l'opacité, gomme (alpha réduit, couleur inchangée), flux contre opacité.
- **Sélections**, en tests de propriétés :
  - inverser deux fois redonne la sélection ;
  - ajouter A puis soustraire A redonne la sélection d'origine sur la zone hors de A ;
  - l'intersection est commutative ;
  - un rectangle aligné sur les pixels est exact ;
  - le lasso est comparé à un polygone calculé à la main ;
  - la baguette sur des aplats donne une zone connue.
- **Transformations** :
  - translation entière exacte, et rotation de 90° exacte ;
  - mise à l'échelle × 2 d'un motif connu, contre des valeurs calculées en linéaire ;
  - objet incorporé : la source n'est jamais modifiée ;
  - recadrage avec et sans suppression.
- **Annuler et refaire** pour chaque outil, en propriété : tout annuler puis tout refaire redonne le même document. Un trait forme une seule entrée.
- **Banc de latence du trait** (0035).

## Proposition 13 : découpage de la phase B

| Étape | Contenu | Vérification |
| --- | --- | --- |
| **T1 à T4** | Fin de B7 (0034) : budget de temps, affichage grossier d'abord, sources réduites sur le processeur en linéaire, envois hors du fil de l'interface | Aucune image au-delà de 100 ms ; glissement sous 33 ms dès la première image |
| **P1** | Moteur de trait : positions d'empreintes (cœur), shader d'empreinte et de fusion, `brush.stroke` résolue, golden images | Valeurs à la main ; rejeu au bit près |
| **P2** | Outil Pinceau dans l'interface : aperçu, stabilisateur, curseur, `[` `]`, couleurs, pipette ; banc de latence | Latence mesurée, cible confirmée par l'utilisateur |
| **P3** | Gomme, peinture du masque, refus des cibles, tampon de clonage | Tests de cibles ; golden image du clonage |
| **P4** | Modèle de sélection, rectangle, lasso, opérations, affichage | Tests de propriétés |
| **P5** | Baguette, seau, conversions sélection ↔ masque, dégradé | Zones connues ; annuler et refaire |
| **P6** | **B10** : flou gaussien et netteté (0023), puis adoucissement de la sélection, qui en dépend | Golden images ; mesure du temps |
| **P7** | Transformations : matrice des objets incorporés, `layer.transform`, recadrage, aperçu | Valeurs à la main ; source intacte |
| **P8** | Entrées tablette (selon Q1), barre d'options par outil, règles, fiche finale ; essai à la main par l'utilisateur | Retouche complète à la souris ; tablette notée |

B10 est placé en **P6**, avant l'adoucissement qui en dépend, et non en toute dernière étape. **Mise à jour du 2026-11-08** : B10 est fait avant J5, à la demande de l'utilisateur (0040) ; P6 se réduit à l'adoucissement de la sélection. B7 (T1 à T4) est fait aussi (0034). Il peut glisser vers J6 sur décision, mais l'adoucissement de la sélection (SEL-2) glisserait avec lui.

**Dépendance avec J4** : la consigne J5 dit que le serveur MCP existe. **Sa phase B n'a pas commencé** : la phase A (0030 à 0033) attend ta validation, avec les questions Q1 et Q2 de J4. `brush.stroke` sera exposée à l'agent automatiquement dès que le serveur existera (outils générés depuis le catalogue). L'ordre entre la phase B de J4 et celle de J5 est la **question Q2**.

## Résumé (phase A de J5)

1. B7 n'est pas fini (pas de budget de temps ni d'affichage grossier d'abord, et des sources réduites non conformes à 0024) : il passe en premier (0034).
2. Le trait est une commande résolue, `brush.stroke`. Les positions d'empreintes sont calculées dans le cœur en f64, le rendu se fait sur GPU par un seul shader, l'aperçu partage les deux, et chaque trait forme une seule entrée d'historique (0035, 0036).
3. Les sélections sont des couvertures en tuiles hors de l'arbre. Leur rastérisation et la baguette se font sur le processeur, l'adoucissement attend B10 (0037).
4. Les transformations passent par une matrice affine pour les objets incorporés, et une rastérisation unique pour les calques pixel. Le rééchantillonnage se fait en linéaire, avec Catmull-Rom et un préfiltre (0038).
5. **Décisions les plus risquées** :
   - le trait comme commande résolue, avec un aperçu identique au bit près et une tolérance entre adaptateurs ;
   - la sélection, nouveau genre de nœud du modèle, qui dépend de B10 pour l'adoucissement ;
   - la tablette, limitée à la pression sous Windows avec winit 0.30.
6. **Questions** :
   - **Q1** : stratégie pour la tablette (A, B ou C) ;
   - **Q2** : ordre entre la phase B de J4 et celle de J5 ;
   - **Q3** : rotation du recadrage dès J5, ou plus tard ;
   - **Q4** : échantillon par défaut du clonage (calque courant ou image composée) ;
   - **Q5** : la latence visée (33 ms au p95) ;
   - **Q6** : la sélection est-elle enregistrée dans le fichier `.cygnus` (J6) ?
