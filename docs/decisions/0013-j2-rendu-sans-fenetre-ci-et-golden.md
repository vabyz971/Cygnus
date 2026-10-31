# 0013 — Rendu sans fenêtre, GPU en intégration continue, images de référence (J2)

- **Date** : 2026-10-30
- **Statut** : acceptée avec conditions le 2026-10-30 (phase A du jalon J2) ; conditions intégrées ci-dessous
- **Exigences concernées** : spec sections 2.1 (critère d'acceptation), 5 (cohérence, plateformes), 9 (outillage)

## Contexte

Le même pipeline doit produire une image sans fenêtre, pour `cygnus render`, les tests et les agents. Il faut un adaptateur GPU en intégration continue sur Linux, Windows et macOS, sans chemin CPU de secours dans notre code, et une tolérance justifiée pour les golden images.

## Mesure du prototype jetable (2026-10-30)

Prototype hors dépôt (`/tmp/cygnus-proto`, non fusionné), wgpu 30.0.1 : composition Normal de deux calques f16 linéaires prémultipliés de 512 × 512 avec opacité 0,6, lecture du résultat, comparaison entre adaptateurs et avec un calcul f32 sur processeur fait **pour cette mesure seulement**.

| Adaptateur | `Rgba16Float` en écriture depuis un shader / fusion | Écart avec le calcul f32 |
| --- | --- | --- |
| NVIDIA RTX 3080 Ti, Vulkan, pilote 615.78.08 | oui / oui | 0,000488 linéaire ; 1 niveau en 8 bits sRGB |
| lavapipe (Mesa 26.1.8, llvmpipe LLVM 21), Vulkan, CPU | oui / oui | 0,000488 linéaire ; 1 niveau en 8 bits sRGB |

Entre les deux adaptateurs : **1 048 552 valeurs identiques au bit près sur 1 048 576**, et un écart maximal de 0,000244, soit un pas de f16 autour de 0,5.

## Proposition : obtenir un adaptateur

- **Code unique** : `photo/render` crée l'instance avec `Backends::PRIMARY`, choisit l'adaptateur par `power_preference` ou par la variable `CYGNUS_ADAPTER` (sous-chaîne du nom, pour les tests), et demande le périphérique avec `device_descriptor(adapter)` (0012). Aucun chemin CPU dans notre code. Un adaptateur logiciel n'est qu'un pilote parmi d'autres, vu par wgpu comme un GPU.
- **Linux (ubuntu-24.04)** : **lavapipe** (Vulkan logiciel de Mesa), via le paquet `mesa-vulkan-drivers`. Si sa version est trop ancienne, on prend la construction binaire de Mesa utilisée par la CI de wgpu. Mesuré localement : fonctionne.
- **Windows (windows-2022)** : **WARP**, le rendu logiciel DirectX 12 de Microsoft, présent dans le système. La CI de wgpu installe une version plus récente de la DLL ; on fera de même si la version système pose problème.
- **macOS (macos-14, Apple Silicon)** : **Metal** sur le GPU paravirtualisé du runner. La CI de wgpu lance ses tests GPU sur ce runner.

**Ce qui ne sera pas possible, honnêtement :**

- aucune mesure de **performance** n'a de sens en CI : lavapipe et WARP sont des processeurs, et le GPU macOS est virtualisé. La CI vérifie l'exactitude, pas la fluidité ;
- les **vrais pilotes** (NVIDIA, AMD, Intel, Apple natif) ne sont pas testés en CI. Ils le sont seulement sur les machines des développeurs, d'où la tolérance ci-dessous ;
- sur macOS, le GPU paravirtualisé peut exposer des limites réduites. Si une exigence de `device_descriptor` n'y est pas satisfaite, les tests GPU macOS sont marqués « non disponibles » dans le rapport de CI (jamais silencieusement verts), et la limite est documentée ;
- **le tout est à confirmer dans la première étape de la phase B (B1)**, sur les vrais runners. Les points Windows et macOS reposent sur la CI de wgpu, pas sur un essai de notre part.

## Proposition : images de référence

- **Emplacement** : `photo/render/tests/golden/<cas>.png`, en PNG 16 bits sRGB (sortie de l'export), avec à côté le script de commandes du cas (`<cas>.json`, voir 0015). Chaque cas reste petit (≤ 1024 × 1024) pour des tests rapides sur lavapipe.
- **Création** : générées par notre pipeline sur **lavapipe**, adaptateur de référence reproductible, avec `CYGNUS_UPDATE_GOLDEN=1`, puis **vérifiées à l'œil par l'utilisateur** avant d'être commitées.
- **Pas d'oracle CPU des formules.** Comparer à un calcul processeur de la fusion créerait une seconde copie des formules (règle 1). Les tests vérifient plutôt des **propriétés** qui ne recopient pas la formule :
  - un calcul opaque recouvre ce qui est dessous ;
  - une opacité 0 est sans effet ;
  - ajouter un calque transparent ne change rien ;
  - un aplat 50 % donne la valeur connue de la spec Normal.

  **Décidé (Q2) : aucun appui sur l'ancien code.** Les valeurs attendues ponctuelles sont **calculées à la main** d'après la spécification W3C *Compositing and Blending Level 1*. Chaque valeur est écrite en dur dans le test, avec le calcul détaillé en commentaire et la référence de la section W3C. Elles complètent les golden images.
- **Comparaison**, dans l'espace de l'export 16 bits :
  - **même adaptateur, aperçu contre export : identité bit à bit (écart 0)**. C'est le critère de la spec 2.1, testé en rendant la vue à 100 % hors écran avec le shader d'affichage et en la comparant à l'export 8 bits ;
  - **adaptateur différent de la référence : tolérance initiale décidée de 1 pas de f16** par canal sur la valeur linéaire du composite, et **au plus 0,01 % des pixels** différents. Le prototype mesure 1 pas sur 0,002 % des valeurs. **La tolérance ne se desserre que sur preuve** : un écart reproductible, expliqué et consigné dans une nouvelle décision ;
  - en échec : écriture de `<cas>-actual.png` et `<cas>-difference.png`, publiés comme artefacts de CI.
- **Différences entre pilotes** : une seule série de références (lavapipe). Une série propre à un système n'est ajoutée que si un écart dépasse la tolérance de façon stable et expliquée, avec une nouvelle décision.

## Risques

- WARP et le Metal paravirtualisé ne sont pas encore essayés par nous : c'est l'objet de B1.
- Le comportement de lavapipe peut changer avec la version de Mesa. On fige la version utilisée en CI.
