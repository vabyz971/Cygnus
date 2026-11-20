# 0036 — J5 : commande de trait, cibles et tampon de clonage

- **Date** : 2026-11-08
- **Statut** : proposition 2 acceptée pour l'étape P1 le 2026-11-19 (`brush.stroke` résolue, une entrée par trait, annulation par copies de tuiles ; mise en œuvre dans 0035) ; propositions 3 et 4 (cibles masque, tampon de clonage) à valider avec l'étape P3
- **Exigences concernées** : PIN-3, PIN-4, MAS-1, HIS-2 ; spec 7.2, 8 (`brush.stroke`) ; décisions 0010, 0020, 0022, 0029, 0035

Cette décision traite les points 2 (trait et commandes), 3 (cibles) et 4 (tampon de clonage) de la consigne.

## Proposition 2 : `brush.stroke`, rejouable

**Paramètres** (catalogue, donc schéma MCP généré) :

| Champ | Contenu |
| --- | --- |
| `target` | identifiant d'un calque pixel ou d'un masque |
| `tool` | `brush`, `eraser` ou `clone` |
| `points` | liste de `{x, y, pressure, tilt_x, tilt_y, time_ms}` (les trois derniers facultatifs) |
| `brush` | diamètre, dureté, espacement, flux, opacité, mode, couleur (RVBA directe, 0 à 1), courbes de dynamique |
| `clone` | pour `clone` : décalage de la source et échantillon (0036, proposition 4) |

**Résolution** : comme `layer.merge_down`, c'est une **commande résolue** (0020, 0029). `photo/core` calcule les positions d'empreintes (fonction pure, f64), puis le résolveur de `photo/render` (trait `Paint`, à côté de `Merge`) rend le trait sur GPU par le shader de 0035. Le cœur reçoit les tuiles résultantes de la cible, plus l'inverse par copies de tuiles (`PixelsRestore`, mémoire comptée dans le budget de l'historique, même mécanisme que `pixels.write`). Le chemin est unique : `resolve_and_execute`, pour l'interface, les scripts, la ligne de commande et l'agent MCP.

**Aperçu pendant le geste, sans commande** : l'outil de l'interface accumule les points et rend le trait **en aperçu** dans le tampon de trait (0035). Le canevas compose la cible avec ce tampon, par une opération `Stroke` du programme par tuile, placée juste au-dessus de la cible. Au relâchement, une seule commande `brush.stroke` est envoyée. Elle forme **une seule entrée d'historique** (HIS-2), sans geste à grouper, et l'aperçu disparaît dans la même image où la commande est appliquée.

**Reproductibilité** :

- **sur un même adaptateur**, le rejeu d'une commande donne le même résultat **au bit près**. Positions en f64, même shader, même ordre d'empreintes. Un test le vérifie : commande envoyée par l'interface simulée, puis rejouée par un script ;
- **entre adaptateurs**, l'écart toléré est celui de 0016 : au plus 1 unité 16 bits par canal, et un écart moyen d'au plus 0,01 unité. Les empreintes antialiasées font intervenir des arrondis f32, mais aucune fonction transcendante hors `sqrt` et `smoothstep` ;
- chaque golden image de trait est faite sur lavapipe, puis comparée sur WARP et Metal en CI.

**Limites de taille** : 100 000 points par trait au plus, et diamètre de 1 à 5 000 px. Au-delà, la commande est refusée avec un message. L'agent ne peut donc pas saturer la mémoire par un seul appel.

## Proposition 3 : cibles

- **Cible active** (PIN-3) : le calque sélectionné, ou son masque quand le masque est la cible (0025, déjà fait en B8).
- **Calque pixel** : peinture en couleur.
- **Masque** : le trait dépose la **luminance de la couleur** (`Lum` de 0028) comme valeur de gris. La gomme ramène vers le noir ; le noir et le blanc des pastilles servent de raccourci (touche `X` pour échanger les couleurs, comme dans Photoshop).
- **Refus**, avec un message clair et sans effet :
  - calque **verrouillé** ;
  - **réglage, filtre, groupe ou objet incorporé**. L'objet incorporé n'est jamais modifié (DOC-2) : l'interface propose de le pixelliser (`layer.rasterize`, nouvelle commande résolue qui le convertit en calque pixel, annulable) ;
  - **calque caché** : refusé, avec le message « le calque est masqué », comme le comportement par défaut de Photoshop.
- **Calque sans tuile** à l'endroit peint : la tuile est créée. Un calque pixel s'étend à toute la surface du document (`bounds`).
- **Sélection active** (0037) : la couverture du trait est multipliée par la couverture de la sélection.

## Proposition 4 : tampon de clonage

- **Point d'alignement** : Alt + clic fixe la source. Au premier point du trait, le décalage source − destination est fixé, puis reste constant pendant le trait. Option « aligné » : le décalage est gardé d'un trait à l'autre ; sinon chaque trait repart de la source d'origine.
- **Échantillon** (**question Q4**) : le **calque courant** par défaut, ou l'**image composée** (tous les calques visibles). Dans les deux cas, l'échantillon est l'état **d'avant le trait**, figé au premier point, ce qui évite qu'un trait se recopie lui-même.
- **Rendu** : le shader d'empreinte du clonage lit l'échantillon au lieu d'une couleur unie, puis suit le même chemin de fusion que le pinceau. Pour l'image composée, les tuiles composées de la zone source sont lues dans le cache du canevas, ou rendues comme pour l'export si elles manquent.
- **Bords** : hors du document, la source est transparente et rien n'est déposé (pas de répétition ni d'étirement des bords).
- **Aperçu de la source** : pendant le survol, un cercle montre la position de la source ; pendant le trait, une croix la suit. Le contenu sous le pinceau est montré en transparence (50 %), coupé au cercle.
- La commande `brush.stroke` (outil `clone`) contient le décalage et le choix de l'échantillon. Rejouée, elle relit la source dans l'état du document au moment du rejeu, qui est le même que lors de l'application si l'historique est rejoué dans l'ordre.

## Alternatives écartées

- **Trait comme suite de commandes (une par segment) groupées en geste** : beaucoup d'entrées internes, une annulation coûteuse, et un rejeu par l'agent très bavard.
- **Commande de trait non résolue, calculée sur le processeur dans le cœur** : contraire à PIN-1, et beaucoup plus lente.

## Risques

- **Identité aperçu/rejeu** : l'aperçu et la commande partagent les positions d'empreintes calculées par le cœur et le même shader. Un test d'égalité au bit près sur un adaptateur la vérifie.
- **Mémoire de l'annulation** pour un très grand trait, qui touche beaucoup de tuiles : elle est comptée dans le budget de l'historique de 0022, qui écarte les plus anciennes entrées.

## Mise en œuvre de l'étape P2 (2026-11-20)

**Aperçu pendant le geste : écart à la proposition 2.** L'opération `Stroke` du programme par tuile, avec un tampon de trait composé par le canevas, n'est pas faite. Elle aurait agrandi le shader de composition, dont WARP ne supporte pas la croissance (0040). À la place, `photo/render/src/stroke.rs` (`StrokePreview`) fonctionne ainsi :

- l'aperçu garde le document du début du trait, et une copie où la cible montre le trait en cours. C'est cette copie que le canevas affiche ;
- à chaque nouvel événement, les empreintes de tout le trait sont recalculées (la fin de la courbe dépend du point suivant) ;
- seules les tuiles dont la liste d'empreintes change sont repeintes, depuis leur contenu d'origine, par la passe même de `brush.stroke` ;
- au relâchement, la commande part avec ces tuiles déjà résolues : ce qui était affiché est exactement ce qui est écrit, sans second calcul.

Ce que les tests vérifient (`stroke_preview.rs`) :

- **identité** : un trait nourri par morceaux donne exactement le même document que la commande calculée d'un coup, et que la commande rejouée par un script ;
- **une seule entrée** d'historique par trait ;
- **repeinte minimale** : seules les tuiles touchées par les nouvelles empreintes sont repeintes ;
- **trait vide** : un trait qui ne peint rien n'envoie aucune commande.

**Outil Pinceau dans l'interface** :

- **geste** : tous les déplacements du pointeur d'une image sont pris, pas seulement le dernier, ainsi que le point d'appui d'avant le seuil de glissement. Un simple clic pose une empreinte ;
- **stabilisateur** : moyenne exponentielle, où chaque point avance de 35 % vers la position ; elle rattrape le pointeur au relâchement ;
- **curseur** : cercle du diamètre au zoom courant, en double trait sombre et clair, avec une croix sous 4 px ; le curseur système est masqué au-dessus du canevas ;
- **pipette** : Alt et clic, ou Alt et glissement, prennent la couleur composée sous le pointeur, lue par le chemin de l'export (`Renderer::sample`). Elle donne la couleur de premier plan, opaque ;
- **raccourcis** : `[` et `]` changent la taille par pas de 10 % (un pixel au moins), et Maj avec ces touches la dureté par pas de 10 %. Sur un clavier français, egui donne ces touches comme du texte, que l'outil lit aussi ;
- **cible** : le calque sélectionné. Sans calque pixel sélectionné, ou sur un calque verrouillé, la barre d'état donne la raison du refus.

**Latence du trait** (banc sans fenêtre, RTX 3080 Ti, vue de 2126 × 1086, document de 24 Mpx en 16 bits avec 10 calques, pinceau de 300 px) :

- les entrées arrivent en temps réel à 120 Hz, et les images sont cadencées à 60 Hz ;
- la latence va de l'arrivée d'une entrée à la fin du travail GPU de l'image qui la montre ;
- rapports : `docs/bench/2026-11-20-*`.

| Phase | Latence médiane | p95 | Max | Image la plus longue |
| --- | --- | --- | --- | --- |
| Trait à 100 % | 10,3 ms | 11,8 à 12,9 ms | 18,0 ms (une fois 72 ms) | 15,2 ms (une fois 72 ms) |
| Trait en vue ajustée | 12,3 à 12,7 ms | 16,9 à 17,4 ms | 18,3 à 19,2 ms | 14,5 à 15,8 ms |

- **La cible de 33 ms au p95 est tenue** : de 12 à 17 ms.
- **Une image de 72 ms** s'est produite une fois sur deux passages, et ne s'est pas reproduite. C'est sans doute une perturbation de la machine, à surveiller.
- **Cadence de 16,7 ms** : aucune autre image du trait ne la dépasse.
- La mesure dans l'application, avec présentation à l'écran, reste à faire à la main.

**Pas encore fait** (étapes suivantes de 0039) :

- sélecteur de couleur en boîte flottante ;
- réglages mémorisés par outil dans les préférences ;
- pression et inclinaison (P8, 0041) ;
- peinture de masque et gomme (P3).
