# Cygnus Photo — Cahier des charges (bêta)

Oct 30, 2026 · @vabyz971

## 1. Vision et positionnement

Cygnus Photo est un éditeur de retouche photo open source (GPL v3), écrit en Rust, dont l'aperçu à l'écran est strictement identique à l'export et dont chaque réglage reste modifiable.

**Public cible.** Photographes passionnés et semi-professionnels qui retouchent leurs images en 16 bits et veulent un flux non destructif, sans abonnement, sur Linux, Windows et macOS.

**Promesse.**

- Ce que tu vois est ce que tu exportes : un seul chemin de rendu GPU, pas de chemin de secours.
- Rien n'est jamais perdu : calques, réglages et masques restent éditables, l'image source reste intacte.
- L'application peut être pilotée par un agent IA (option désactivée par défaut), avec les mêmes actions que l'utilisateur.

**Références d'usage.** L'organisation des calques et l'interface modulaire d'Affinity Photo servent de modèle : un utilisateur qui connaît ce type de logiciel ne doit pas être perdu. Les différences visées sont la transparence (code et format de fichier ouverts), la cohérence aperçu/export et l'ouverture à l'automatisation.

**Plateformes.** Linux (développement principal sous NixOS), Windows, macOS.

**Nom de travail.** Cygnus Photo, format de fichier natif `.cygnus` (déjà utilisés dans la maquette).

## 2. Fonctionnalités phares

Deux fonctionnalités définissent la bêta : une édition entièrement non destructive dont l'aperçu égale l'export, et un agent IA intégré, activable à la demande.

### 2.1 Édition 100 % non destructive, rendu GPU identique à l'export

- Aucune opération ne modifie les pixels source : l'image importée est conservée telle quelle, sous forme d'objet incorporé.
- Tout calque, réglage, filtre et masque reste paramétrable à tout moment, y compris après fermeture et réouverture du fichier.
- Un seul pipeline de rendu GPU (wgpu) produit l'aperçu et l'export. Il n'existe pas de chemin CPU de secours.
- Les formules de fusion (modes de calque) existent en une seule source WGSL, jamais en plusieurs copies.

**Critère d'acceptation.** Pour chaque document de référence, l'export plein format est identique au rendu de l'aperçu à 100 %. L'écart maximal toléré est fixé dans le premier test d'images de référence (golden images) et vérifié en intégration continue.

### 2.2 Agent MCP intégré

- Un serveur MCP local est inclus dans l'application. Il est désactivé par défaut et s'active dans *Préférences > Agent*.
- Les outils exposés sont les mêmes commandes que celles de l'interface : l'agent ne dispose d'aucun pouvoir que l'utilisateur n'a pas.
- Chaque action de l'agent apparaît dans l'historique, marquée comme telle, et reste annulable.
- L'agent peut lire l'arbre du document, l'aperçu et l'histogramme pour vérifier le résultat de ses actions.

Le détail (outils, ressources, sécurité) est en section 8.

### 2.3 Pistes ultérieures (hors bêta)

- Sélections intelligentes locales (ciel, sujet).
- Génération et modification d'image par IA, ajoutées comme calques non destructifs avec leurs métadonnées de provenance.
- Fenêtre graphe de nœuds (voir section 7).

## 3. Périmètre de la bêta

La bêta est « utilisable » quand un photographe peut ouvrir un JPEG, le retoucher avec des calques de réglage et des masques peints, enregistrer en `.cygnus`, rouvrir le fichier et exporter en JPEG ou en TIFF 16 bits.

| Domaine | Dans la bêta | Hors bêta (après) |
| --- | --- | --- |
| Document | Calques pixel, groupes, masques, objets incorporés, calques de réglage | Calques de forme, texte, vectoriel |
| Réglages | Niveaux, courbes, teinte/saturation, luminosité/contraste, équilibre colorimétrique | Mixage de canaux, LUT, autres réglages |
| Outils | Déplacer, recadrer, sélection (rectangle, lasso, baguette magique), pinceau, gomme, remplissage, pipette, tampon de clonage, zoom, panoramique | Correcteur, densité +/−, netteté et flou localisés, sélections intelligentes |
| Filtres | Flou gaussien, netteté (masque flou) | Réduction du bruit, autres filtres |
| Fichiers | Import/export PNG, JPEG, WebP, TIFF en 8 et 16 bits ; format natif `.cygnus` | RAW (personas Développement), PSD, HEIF |
| Couleur | 8 et 16 bits, profils ICC d'entrée, export en sRGB, Display P3 ou Adobe RGB | Épreuvage écran, CMYK |
| Interface | Écran principal, panneaux ancrables, workspaces sauvegardables, thème sombre, interface en français | Thème clair, autres langues, personas Fluidité et Tone Mapping |
| Historique | Annuler/refaire complet, panneau Historique | Snapshots nommés |
| Agent | Serveur MCP local, désactivé par défaut | Génération et modification d'image par IA |
| Graphe | Modèle de document prêt à l'accueillir | Fenêtre graphe (lecture seule, puis édition) |
| Plateformes | Linux, Windows, macOS | Version mobile |

**Règle de périmètre.** Toute fonctionnalité absente de la colonne « Dans la bêta » est refusée tant que les critères de sortie de la section 10 ne sont pas atteints.

## 4. Exigences fonctionnelles

Chaque exigence porte un identifiant (DOC-1, CAL-2, etc.) pour que les agents et les tests puissent s'y référer. Chaque ligne doit pouvoir devenir un test d'acceptation.

### 4.1 Document et fichiers

- **DOC-1** Nouveau document : dimensions, unité, résolution, profil colorimétrique, profondeur (8 ou 16 bits), arrière-plan, avec préréglages (écran, photo, impression).
- **DOC-2** Ouvrir un PNG, JPEG, WebP ou TIFF : l'image devient un calque source de type objet incorporé, jamais modifié.
- **DOC-3** Enregistrer au format natif `.cygnus`, ouvert et documenté, qui conserve l'arbre de calques et tous les paramètres.
- **DOC-4** Enregistrement automatique et récupération après une fermeture inattendue.
- **DOC-5** Plusieurs documents ouverts en onglets.
- **DOC-6** Exporter en PNG, JPEG, WebP ou TIFF avec choix de la qualité, de la taille, de la profondeur et du profil.
- **DOC-7** Modifier à tout moment les paramètres du document (dimensions, résolution, profil, profondeur).

### 4.2 Calques

- **CAL-1** Types de calques : pixel, groupe, objet incorporé, réglage. Un masque peut être attaché à tout type.
- **CAL-2** Propriétés : nom, visibilité, opacité, mode de fusion, verrouillage.
- **CAL-3** Modes de fusion initiaux : Normal, Produit, Écran, Superposition, Lumière douce, Obscurcir, Éclaircir, Différence, Teinte, Saturation, Couleur, Luminosité.
- **CAL-4** Actions : créer, dupliquer, supprimer, renommer, regrouper, fusionner vers le bas, réordonner par glisser-déposer.
- **CAL-5** Détourage (clipping) d'un calque sur le calque situé dessous.
- **CAL-6** Le panneau affiche pour chaque calque : vignette, nom, type, œil, verrou et vignette du masque, comme dans la maquette.
- **CAL-7** Menu contextuel d'un calque : actions standard, plus « Afficher dans le graphe » (grisé tant que la fenêtre graphe n'existe pas).

### 4.3 Masques et sélections

- **MAS-1** Masque en niveaux de gris, peint au pinceau, rempli au dégradé, inversé, avec réglage de densité.
- **SEL-1** Outils de sélection : rectangle, lasso, baguette magique (tolérance, contiguïté).
- **SEL-2** Opérations : ajouter, soustraire, intersecter, inverser, adoucir les bords.
- **SEL-3** Convertir une sélection en masque, et un masque en sélection.

### 4.4 Réglages

- **REG-1** Chaque réglage est un calque de réglage non destructif : niveaux, courbes, teinte/saturation, luminosité/contraste, équilibre colorimétrique.
- **REG-2** Le panneau Réglages affiche les paramètres du réglage sélectionné, avec intensité et densité du masque.
- **REG-3** Les courbes s'éditent en RVB et par canal, avec points déplaçables.
- **REG-4** L'histogramme (RVB, moyenne, écart type) se met à jour à chaque modification.

### 4.5 Pinceau et retouche

- **PIN-1** Pinceau rendu sur GPU : taille, dureté, opacité, flux, mode de fusion, espacement, stabilisateur.
- **PIN-2** Pression, inclinaison et vitesse d'une tablette graphique influencent le trait.
- **PIN-3** Peinture sur un calque pixel ou sur un masque, selon la cible active.
- **PIN-4** Gomme et tampon de clonage utilisent le même moteur de trait.
- **PIN-5** Raccourcis de la maquette : `[` et `]` pour la taille, Alt pour la pipette, Espace pour déplacer.

### 4.6 Transformation et recadrage

- **TRA-1** Recadrer avec rapport d'aspect, rotation d'angle et aperçu de la zone conservée.
- **TRA-2** Transformer un calque : déplacer, mettre à l'échelle, pivoter, avec rééchantillonnage de qualité.
- **TRA-3** Les transformations restent non destructives sur un objet incorporé.

### 4.7 Affichage

- **AFF-1** Zoom (affichage « Adapter à l'écran », 100 %), panoramique, règles.
- **AFF-2** Barre d'état : zoom, dimensions du document, profil colorimétrique, état de l'enregistrement, état du GPU.
- **AFF-3** Métadonnées EXIF affichées sous le canevas (boîtier, focale, ouverture, vitesse, ISO).

### 4.8 Historique

- **HIS-1** Annuler et refaire pour toute action, y compris celles de l'agent.
- **HIS-2** Un trait de pinceau continu compte pour une seule entrée.
- **HIS-3** Le panneau Historique liste les actions avec leur heure et leur source (utilisateur ou agent).

## 5. Exigences non fonctionnelles

Les cibles chiffrées ci-dessous sont des propositions : chacune est confirmée ou corrigée par une mesure sur du matériel réel avant la fin du jalon J2.

| Critère | Exigence | Vérification |
| --- | --- | --- |
| Fluidité | Pinceau, zoom et panoramique restent fluides sur une photo de 24 Mpx en 16 bits avec une dizaine de calques. Cible proposée : 60 images/s. | Banc d'essai automatisé |
| Mémoire | Canevas découpé en tuiles, texture GPU éparse : un document n'est jamais chargé en entier si ce n'est pas nécessaire. | Test sur très grande image |
| Couleur | Calcul interne en virgule flottante 16 bits (f16) linéaire sur GPU, conversion vers le profil de l'écran à l'affichage. | Images de référence |
| Cohérence | L'aperçu à 100 % et l'export sont identiques (section 2.1). | Tests golden en intégration continue |
| Plateformes | Linux, Windows, macOS ; rendu via wgpu (Vulkan, Metal, DirectX 12). Le GPU minimal supporté reste à définir. | Intégration continue sur les trois systèmes |
| Fiabilité | Aucun `unwrap()` ni `expect()` hors tests ; erreurs affichées clairement ; fichier corrompu = message, jamais un plantage. | Revue automatique, tests de fichiers invalides |
| Sécurité | Décodeurs de fichiers soumis à du fuzzing ; serveur MCP local uniquement, désactivé par défaut, protégé par un jeton ; aucune télémétrie. | Tests de sécurité, revue manuelle |
| Accessibilité | Contrastes suffisants, navigation au clavier des panneaux et dialogues, échelle d'interface réglable. | Liste de contrôle par vue |
| Internationalisation | Tous les textes sortent du code dès le premier jour ; français en premier, autres langues ensuite. | Revue de code |
| Qualité du code | Conventions d'`AGENTS.md` : commentaires en anglais, en-têtes GPL v3, `clippy` sans avertissement, tests pour toute logique de rendu. | Intégration continue |
| Licence | GPL v3 ; chaque dépendance doit y être compatible. | Vérification automatique des licences |

## 6. Interface et vues à concevoir

L'écran principal est déjà dessiné ; il reste onze vues à concevoir, dont cinq indispensables pour la bêta.

**Écran principal (maquette existante).** Barre de menus et personas en haut (Photo, Développement, Fluidité, Tone Mapping, Export), barre d'options contextuelle de l'outil, rail d'outils à gauche, onglets de documents, canevas avec règles, panneaux ancrables à droite (Histogramme, Réglages, Calques, Historique) et barre d'état. Pour la bêta, seuls les personas Photo et Export sont actifs.

**Technique.** Interface en `egui`, panneaux ancrables et réorganisables avec `egui_dock`. La barre d'outils, la barre d'options et la barre d'onglets de documents sont codées séparément. La disposition se sauvegarde sous forme de workspaces.

| Vue | Contenu | Priorité | État |
| --- | --- | --- | --- |
| Nouveau document | Préréglages, dimensions, unité, résolution, profil, profondeur 8/16 bits, arrière-plan | Indispensable | À concevoir |
| Ouvrir / importer | Dialogue de fichier natif, options d'import (profil, objet incorporé) | Indispensable | À concevoir |
| Exporter | Format, qualité, taille, profil, poids estimé, aperçu | Indispensable | À concevoir |
| Préférences | Général, Interface, Performance, Outils et tablette, Raccourcis, Couleurs, Agent | Indispensable | À concevoir |
| Paramètres du document | Dimensions, résolution, profil, profondeur, métadonnées | Indispensable | À concevoir |
| Taille de l'image et du canevas | Dimensions, rééchantillonnage, ancrage | Importante | À concevoir |
| Écran d'accueil | Documents récents, nouveau, ouvrir | Importante | À concevoir |
| Dialogues de filtres | Paramètres avec aperçu en direct (flou, netteté) | Importante | À concevoir |
| Panneau Agent | État du serveur MCP, journal des actions de l'agent | Importante | À concevoir |
| Fenêtre graphe | Nœuds du document, ouverte depuis le menu contextuel d'un calque | Après la bêta | À concevoir |
| Palette de commandes | Recherche de toutes les commandes | Après la bêta | À concevoir |

**Règle.** Chaque vue est livrée avec une fiche de design : contenu, états (vide, chargement, erreur, succès), raccourcis clavier et comportement au redimensionnement. La fiche sert de consigne à l'agent qui l'implémente.

## 7. Architecture cible

Le document est un arbre de calques unique ; l'interface, le serveur MCP et la ligne de commande n'agissent sur lui que par des commandes.

&#91;embedded content: architecture cible · 3 entrées, 1 bus, 1 modèle, 3 services\]

Les vues lisent le modèle ; l'application, le serveur MCP et la ligne de commande le modifient uniquement par le bus de commandes.

**Version texte du schéma.** Trois entrées (application `egui`, serveur MCP, ligne de commande sans fenêtre) envoient des commandes au bus de commandes. Le bus les applique au modèle du document. Le modèle alimente trois services : le rendu GPU (il compile l'arbre), les fichiers (lecture et écriture) et les vues du modèle (panneau Calques, graphe après la bêta).

### 7.1 Un modèle, deux vues

- Le modèle du document est la source de vérité : l'arbre de calques.
- **Vue 1, par défaut :** le panneau Calques, à la manière d'Affinity.
- **Vue 2, optionnelle :** la fenêtre graphe, ouverte par le menu contextuel d'un calque (« Afficher dans le graphe ») ou par *Fenêtre > Graphe*. Elle montre le même arbre : masquer un calque dans l'une le masque dans l'autre.
- Quand le graphe permet une structure que la liste ne montre pas bien (un masque partagé par deux réglages), le panneau Calques affiche quand même le calque, avec un badge de lien. Un clic ouvre le graphe sur ce nœud.
- Bêta : panneau Calques seul, modèle déjà compatible avec le graphe. Jalon suivant : graphe en lecture seule, puis édition.

### 7.2 Commandes

- Toute modification est une commande typée et sérialisable (un nom et des paramètres).
- Le bus applique la commande, l'inscrit dans l'historique et prévient les vues.
- Le catalogue de commandes est unique : il alimente l'interface, le serveur MCP (section 8), la ligne de commande et les tests.

### 7.3 Rendu

- L'arbre est compilé en graphe de rendu GPU (wgpu). Les passes sont écrites en WGSL et les formules de fusion n'existent qu'en un seul exemplaire.
- Le canevas est découpé en tuiles (texture éparse). Le calcul se fait en f16 linéaire, converti à l'affichage.
- Un cache par tuile est invalidé uniquement par les commandes qui touchent la zone.
- Le canevas est dessiné directement par wgpu dans la fenêtre `egui`, hors de la boucle de widgets.

### 7.4 Organisation du dépôt

Un seul dépôt, nommé `cygnus`, contient toutes les applications. Photo est la première ; les suivantes réutilisent l'interface et les mécanismes communs sans les recoder.

```text
cygnus/
  crates/            code partagé, indépendant de l'image
    ui-kit/          thème, panneaux ancrables, dialogues, préférences
    commands/        bus de commandes, historique, génération des outils MCP
    mcp-server/      serveur MCP générique
    file-format/     conteneur .cygnus (archive, manifeste, versions)
  photo/             tout ce qui est propre à la retouche photo
    core/  render/  io/
  apps/
    photo/           application Photo
    cli/             mode sans fenêtre
  docs/
```

Règles de dépendance : `apps` dépend de `photo`, et tout dépend de `crates`, jamais l'inverse. `ui-kit` ne connaît ni les calques ni les pixels. Les noms sont indicatifs.

| Crate | Rôle | Dépend de |
| --- | --- | --- |
| `crates/ui-kit` | Thème, panneaux ancrables, dialogues, préférences, raccourcis (`egui`, `egui_dock`) | Aucun crate du projet |
| `crates/commands` | Bus de commandes, catalogue typé, historique, génération des outils MCP | Aucun crate du projet |
| `crates/mcp-server` | Serveur MCP générique alimenté par un catalogue de commandes | `crates/commands` |
| `crates/file-format` | Conteneur `.cygnus` générique : archive, manifeste, versions | Aucun crate du projet |
| `photo/core` | Modèle du document pixel et commandes de Photo | `crates/commands`, `crates/file-format` |
| `photo/render` | Compilation de l'arbre en passes GPU, WGSL, tuiles | `photo/core` |
| `photo/io` | Décodage et encodage d'images, profils ICC | `photo/core` |
| `apps/photo` | Binaire Photo : assemble l'interface, `photo/*` et le serveur MCP ; contient les panneaux propres à Photo (Calques, Réglages) | `crates/*`, `photo/*` |
| `apps/cli` | Mode sans fenêtre pour tests et agents | `crates/commands`, `photo/*` |

**Règle d'extraction.** Pendant la bêta, seul le code déjà indépendant de l'image (interface générale, bus de commandes, conteneur de fichier) va dans `crates/` ; le reste reste dans `photo/` ou `apps/photo`. Quand une deuxième application a besoin d'un composant, on l'extrait vers `crates/`, avec un test et une décision datée dans `docs/decisions/`.

Chaque application est un exécutable distinct au départ. Les réunir plus tard en un seul programme à modes « studio » reste possible sans changer cette structure.

Les noms de la table sont ceux des dossiers. Les paquets Cargo portent le préfixe `cygnus-` (par exemple `cygnus-photo-core`), comme décidé dans `docs/decisions/0001`.

## 8. Serveur MCP intégré

Le serveur MCP donne à un agent IA les mêmes commandes que l'interface, sans pouvoir supplémentaire, et reste désactivé tant que l'utilisateur ne l'active pas.

**Activation et accès**

- Désactivé par défaut ; s'active dans *Préférences > Agent*. L'état (actif ou non) est visible dans la barre d'état.
- Accessible uniquement depuis la machine locale. Un jeton d'accès est généré à l'activation, affiché une fois et révocable.
- Les actions qui touchent le disque (enregistrer, exporter) demandent une confirmation de l'utilisateur par défaut ; un réglage permet de les autoriser pour un dossier donné.
- Chaque requête de l'agent est inscrite dans le panneau Agent avec son résultat.

**Outils exposés (noms indicatifs)**

| Famille | Outils | Effet |
| --- | --- | --- |
| Document | `document.create`, `document.open`, `document.save`, `document.export` | Cycle de vie du fichier |
| Calques | `layer.create`, `layer.update`, `layer.delete`, `layer.move`, `layer.group` | Structure et propriétés de l'arbre |
| Réglages | `adjustment.add`, `adjustment.update` | Calques de réglage et leurs paramètres |
| Masques et sélections | `mask.create`, `selection.set`, `selection.to_mask` | Zones d'application des effets |
| Peinture | `brush.stroke` | Un trait défini par ses points, ses réglages de pinceau et sa cible |
| Historique | `history.undo`, `history.redo` | Retour en arrière, y compris sur les actions de l'agent |

**Ressources lisibles**

- L'arbre du document (calques, propriétés, réglages) en JSON.
- L'aperçu rendu, en PNG à une taille demandée, pour que l'agent vérifie son résultat.
- L'histogramme et la liste de l'historique.
- Le schéma de toutes les commandes, généré depuis le code pour rester exact.

**Règle d'architecture.** Les outils MCP ne sont pas écrits à la main : ils sont générés à partir du catalogue de commandes de l'application (section 7). Ajouter une commande à l'interface l'ajoute automatiquement à l'agent.

## 9. Workflow de développement avec agents IA

Le code est écrit par des agents IA, encadrés par une spec, des tâches courtes et des vérifications automatiques à chaque étape ; l'humain décide, relit et valide.

**Dépôt et documents de pilotage**

- Un seul dépôt, `cygnus`, pour toutes les applications : Photo est la première, les suivantes s'y ajoutent et partagent l'interface (section 7.4).
- L'ancien code est archivé sous un tag. Il est consulté en lecture seule dans un dossier voisin, hors du nouveau dépôt, sans copie automatique.
- `AGENTS.md` : conventions de code (commentaires en anglais, en-têtes GPL v3, aucun `unwrap()` ni `expect()` hors tests, pas d'emoji, commits courts préfixés).
- `docs/spec.md` : ce cahier des charges, source de vérité des exigences et de leurs identifiants.
- `docs/decisions/` : une décision d'architecture par fichier, avec sa date et sa raison. Toute extraction de code vers `crates/` y est consignée.
- `docs/views/` : une fiche de design par vue d'interface (section 6).

**Boucle d'une tâche**

1. L'humain choisit une ou quelques exigences (par exemple CAL-2 et CAL-3) et rédige la consigne : contexte, identifiants, fichiers concernés, critères d'acceptation, interdits.
2. L'agent écrit d'abord les tests, puis le code.
3. Contrôles automatiques obligatoires avant tout commit : `cargo fmt`, `cargo clippy`, `cargo test`, tests d'images de référence si le rendu change.
4. Le commit mentionne les identifiants d'exigences traités.
5. L'humain relit le diff et, pour l'interface, une capture d'écran du résultat avant de fusionner.

**Niveau de supervision**

- Supervision étroite : passes de rendu GPU, formats de fichier, sécurité du serveur MCP, modèle de document.
- Délégation large : dialogues et vues à partir des fiches de design, panneaux, préférences, raccourcis.
- Chaque nouvelle zone démarre par un crate squelette qui compile, avant toute implémentation.

**Outillage pour les agents**

- Mode sans fenêtre (`headless`) : ouvrir un document, appliquer des commandes, rendre une image, sans interface graphique. C'est ce qui permet à un agent de vérifier son travail seul.
- Serveur MCP de l'application utilisable en développement pour piloter l'application réelle.
- Intégration continue sur Linux, Windows et macOS.
- Un jeu de documents de référence versionné, avec leurs images attendues.

## 10. Jalons de la bêta

La bêta s'atteint en huit jalons ordonnés, chacun fermé par un critère que les tests ou une démonstration peuvent vérifier.

&#91;embedded content: feuille de route · 8 jalons, chacun avec son critère de sortie\]

Les jalons sont ordonnés mais pas datés : leur durée sera estimée à la fin de J0, avec la vitesse réelle du travail avec les agents.

**Version texte de la feuille de route.**

| Jalon | Contenu | Critère de sortie |
| --- | --- | --- |
| J0 Fondations | Dépôt neuf, `AGENTS.md`, crates squelettes, intégration continue, mode sans fenêtre | Intégration continue verte sur Linux, Windows et macOS |
| J1 Interface | Écran principal de la maquette, panneaux ancrables, workspaces | Prototype `egui_dock` validé sur les trois systèmes |
| J2 Document, rendu | Modèle, bus de commandes, rendu GPU par tuiles, calque pixel | Aperçu identique à l'export, cibles de fluidité mesurées |
| J3 Calques, réglages | Groupes, masques, réglages, historique, objets incorporés | Scénario photo complet réussi en mode sans fenêtre |
| J4 Agent MCP | Serveur MCP généré depuis le catalogue, panneau Agent | Un agent exécute le scénario photo |
| J5 Outils, pinceau | Sélections, pinceau, gomme, tampon, recadrage, transformation | Retouche complète à la souris et à la tablette |
| J6 Fichiers, vues | Nouveau document, import, export, préférences, format `.cygnus` | Ouvrir, retoucher, enregistrer, rouvrir, exporter |
| J7 Stabilisation | Correction de bogues, performances, documentation du format | Critères de sortie de la bêta atteints (ci-dessous) |

**Critères de sortie de la bêta (J7)**

- Chaque exigence de la section 4 a un test d'acceptation qui passe.
- L'aperçu et l'export sont identiques sur les documents de référence, sur Linux, Windows et macOS.
- Le scénario photo complet (ouvrir un JPEG, retoucher avec réglages et masques, enregistrer, rouvrir, exporter) réussit à la main et en mode sans fenêtre.
- Un agent MCP réalise ce même scénario.
- Aucun plantage sur la série de fichiers invalides.
- Le format `.cygnus` est documenté publiquement.

## 11. Hors périmètre, risques et décisions ouvertes

Huit décisions restent à trancher avant ou pendant les premiers jalons, et six risques sont identifiés avec leur parade.

**Hors périmètre de ce projet.** Les studios Vector, Layout, Audio et Vidéo, les versions mobiles, la génération d'image par IA, le RAW et les extensions tierces. Ils pourront faire l'objet de cahiers des charges séparés une fois la bêta Photo publiée.

### 11.1 Décisions ouvertes

| Décision | Options | Proposition |
| --- | --- | --- |
| Conteneur du format `.cygnus` | Archive ZIP (JSON + tuiles) ou format binaire propre | Archive ZIP : lisible, documentable, simple à tester |
| Historique dans le fichier | Conservé ou non | Non en bêta |
| Tampon de clonage | Dans la bêta ou après | Dans la bêta (retouche photo de base) |
| Langues de l'interface | Français seul ou français + anglais | Français + anglais dès la bêta si les textes sont externalisés |
| Gestion des profils ICC | Complète ou limitée à sRGB, Display P3, Adobe RGB | Limitée en bêta |
| GPU minimal supporté | À mesurer | À fixer au jalon J2 |
| Transport du serveur MCP | HTTP local, socket local ou stdio | HTTP local avec jeton |
| Préfixes de commit | Par domaine (`rendu:`, `ui:`, `fichier:`) ou unique | Par domaine |
| Sort de l'ancien dépôt Cygnus | Le renommer en cygnus-legacy et l'archiver, ou garder le nom avec une branche orpheline | Réglé : ancien dépôt renommé en legacy, nouveau dépôt Cygnus créé |

### 11.2 Risques

| Risque | Parade |
| --- | --- |
| Rendu GPU légèrement différent selon le pilote ou le système | Tolérance définie par test, images de référence par plateforme, intégration continue sur les trois systèmes |
| Retour d'une seconde voie de rendu (dette de l'ancien projet) | Règle écrite dans `AGENTS.md` : un seul pipeline, toute exception exige une décision datée |
| Code de rendu incorrect produit par un agent | Tests d'abord, images de référence, supervision étroite sur ces zones |
| Le périmètre grossit | Règle de périmètre de la section 3, tout ajout passe par une décision |
| Limites d'`egui_dock` (fenêtres flottantes, workspaces) | Prototype des panneaux dès le jalon J1 |
| `egui` lent pour un très grand canevas | Canevas dessiné directement par wgpu, hors de la boucle de widgets `egui` |
