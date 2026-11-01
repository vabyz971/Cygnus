# 0021 — Rendu de J3 : groupes, masques, réglages, objets incorporés, modes de fusion

- **Date** : 2026-11-01
- **Statut** : acceptée avec conditions le 2026-11-01 (phase A du jalon J3) ; conditions intégrées ci-dessous
- **Exigences concernées** : CAL-3, CAL-5, MAS-1, REG-1 ; spec 2.1 et 7.3 ; applique le plan d'extension de 0012

## Proposition : programme de composition

Le programme par tuile de 0012 reçoit ses opérations définitives. Elles sont toutes évaluées dans **une seule passe**, en f32, avec une pile de registres de profondeur fixe (8) :

| Opération | Effet |
| --- | --- |
| `Layer { source, opacity, mode, mask }` | Lit une tuile source (calque pixel ou objet incorporé), l'atténue par l'opacité et le masque, puis la fusionne avec `blend(mode)` |
| `PushGroup { isolation }` | Groupe isolé : nouvel accumulateur transparent. Groupe passant : copie du fond sur la pile |
| `PopGroup { opacity, mode, mask, isolation }` | Groupe isolé : fusionne l'accumulateur du groupe dans le parent. Groupe passant : `mix(fond sauvegardé, accumulateur, opacité × masque)` |
| `Adjust { kind, params, opacity, mask }` | Voir plus bas |
| `ClipBegin` / `ClipEnd { opacity, mode }` | Détourage : la base et ses calques détourés forment un groupe isolé. Les calques détourés sont composés en « source-atop » (`co = cs·αb + cb·(1 − αs)`, `αo = αb`), puis le groupe est fusionné avec le mode et l'opacité de la base, comme dans Photoshop |
| `Base { stage }` | Reprise après un filtre (0023) |

Une imbrication plus profonde que la pile coupe le programme en étapes (0012, point 3) : elle n'est jamais refusée.

## Proposition : sources entières lues dans la passe de composition (changement mesuré)

En J2, une passe de décodage écrivait les tuiles de calques en f16 avant la composition. **Le prototype du point 11 de la consigne (0026) montre que ce stockage en f16 avant un réglage raide produit des bandes** : 436 valeurs distinctes au lieu de 6 555 sur des hautes lumières étirées ×10. Proposition :

- les tuiles sources (calques pixel, objets incorporés) sont envoyées telles quelles dans une **réserve de sources entières** `Rgba16Uint`, par `write_texture`, sans conversion ni passe de décodage. Les tuiles 8 bits sont étendues en multipliant par 257, ce qui est exact ;
- les masques vont dans une réserve `R16Uint` ;
- la passe de composition convertit les entiers en f32 et prémultiplie, sans fonction de transfert : la composition se fait en valeurs encodées sRGB (tableau plus bas, Q1) ;
- seul le résultat composé est stocké, au format de calcul (f16) ;
- **ces réserves restent sur le GPU d'une image à l'autre** (condition de validation : **résidence par tuile**) :
  - chaque tuile source (calque, objet incorporé ou masque, coordonnées de tuile) a son emplacement, attribué à la demande ;
  - le **budget mémoire GPU** de ces réserves est calculé comme celui du cache (0018) : couches de tableau de l'adaptateur et part de `gpu::cache_memory_budget` ;
  - quand le budget est atteint, les tuiles sources non utilisées par le travail en cours sont évincées, de la moins récemment utilisée à la plus récente. Elles sont renvoyées depuis la `PixelStore` si besoin ;
  - un changement de propriété ne renvoie aucune donnée résidente : il ne fait que recomposer, ce qui est la condition des 150 ms de 0024 ;
  - **test à budget réduit** : avec une réserve de quelques tuiles, un document plus grand se rend à l'identique de l'export, avec des évictions comptées.

Les formats `Rgba16Uint` et `R16Uint` sont dans le cœur de WebGPU : le GPU minimal ne change pas.

## Proposition : réglages

- **Un seul module**, `adjust.wgsl`, qui contient tous les réglages, comme `blend.wgsl` pour les fusions.
- **Espace de calcul** (décidé, Q5) : un réglage agit sur la couleur **directe encodée en sRGB**, comme dans Photoshop et Affinity. Séquence : dé-prémultiplier, appliquer, prémultiplier ; l'accumulateur étant déjà en valeurs encodées (tableau plus bas), il n'y a plus de conversion. Le résultat est mélangé à l'accumulateur par `intensité × masque`.
- **Paramètres** : un tampon de stockage par lot ; chaque opération a un décalage dans ce tampon.
- **Courbes** : la forme de la courbe (interpolation cubique monotone) est calculée **une fois, côté processeur**, dans `photo/core`, et précalculée en une table de 1 024 valeurs par canal pour le shader. La même fonction sert à dessiner la courbe dans le panneau Réglages : un seul exemplaire de la formule pour l'aperçu et l'éditeur.
- **Niveaux, teinte/saturation, luminosité/contraste, équilibre colorimétrique** : formules dans `adjust.wgsl`. Teinte/saturation passe par HSL, et l'équilibre colorimétrique pondère ombres, tons moyens et hautes lumières selon la luminance.

## Proposition : modes de fusion

- Les **12 modes de CAL-3** sont dans `blend.wgsl` et nulle part ailleurs : séparables (Produit, Écran, Superposition, Lumière douce, Obscurcir, Éclaircir, Différence) et non séparables (Teinte, Saturation, Couleur, Luminosité, avec les fonctions `Lum`, `Sat`, `ClipColor` et `SetLum` du W3C).
- **Espace de calcul** (décidé, Q1) : **la fonction de fusion `B(Cb, Cs)` et le mélange d'opacité et d'alpha** (formule W3C générale `Cs' = (1 − αb)·Cs + αb·B(Cb, Cs)`, puis source-over) se font **dans le même espace, en valeurs encodées sRGB** (prémultipliées pour le mélange), sauf démonstration contraire. Le mode Normal de J2, composé en linéaire, change donc d'espace : les golden images de J2 et les valeurs calculées à la main seront refaites en B3 et soumises à l'utilisateur.
- **Valeurs de test** : calculées à la main d'après le W3C *Compositing and Blending Level 1*, avec le détail du calcul en commentaire (comme en J2), plus une golden image par mode.

## Espace de calcul de chaque étape (demandé avec Q1)

| Étape | Espace | Raison |
| --- | --- | --- |
| Stockage des sources (CPU, réserve GPU) | entiers, sRGB encodé, alpha droit | tel que décodé, sans perte |
| Lecture dans la passe de composition | f32, sRGB encodé, prémultiplié | aucune conversion de transfert : on reste dans l'espace de mélange |
| Masques | couverture 0..1, sans fonction de transfert | ce n'est pas une couleur |
| Fonction de fusion `B` (12 modes) | sRGB encodé, direct | Q1, comme Photoshop et Affinity |
| Mélange d'opacité, alpha, source-over, source-atop | sRGB encodé, prémultiplié | Q1 : même espace que `B` |
| Groupes isolés et passants | sRGB encodé, prémultiplié | même espace que leurs enfants |
| Réglages | sRGB encodé, direct | Q5 |
| Filtres (flou, netteté) | sRGB encodé, prémultiplié | même espace que le reste de la composition, comme Photoshop par défaut |
| Résultats intermédiaires d'étape | f32, sRGB encodé, prémultiplié | 0026 |
| Cache composé, pyramide stockée | f16, sRGB encodé, prémultiplié | 0026 ; le f16 est plus régulier en valeurs encodées qu'en linéaire |
| **Réduction de la pyramide, filtrage bilinéaire de l'affichage, futurs rééchantillonnages (J5)** | **linéaire** (décodage, moyenne, réencodage) | **démonstration contraire proposée** : moyenner des valeurs encodées assombrit les détails fins contrastés (un damier noir et blanc réduit donne 0,5 encodé, soit 21 % de luminance au lieu de 50 %). Ces étapes ne servent qu'à l'affichage réduit et aux transformations ; elles ne touchent pas la composition à 100 % ni l'export |
| Affichage et export | sRGB encodé, quantifié en 8 ou 16 bits | plus de conversion de transfert à la sortie |

**Validé par l'utilisateur le 2026-11-02, avec ces conditions :**

- **même réduction linéaire pour l'aperçu grossier et l'image affinée** : à un zoom inférieur à 100 %, les tuiles approximatives de l'aperçu (sources réduites, 0024) et les tuiles exactes (pyramide des résultats, 0018) sont réduites par la même fonction, en linéaire. Un test compare leur **luminosité moyenne** sur une même vue, pour que l'affinage ne fasse pas « sauter » la luminosité (B7) ;
- **coût mesuré** : le décodage et le réencodage du filtrage bilinéaire en linéaire dans le shader d'affichage (quatre décodages et un encodage par pixel d'écran) sont mesurés au banc, en comparant avec et sans (B3 et B7) ;
- **précision de l'export 16 bits** : documentée dans 0026.

**Conséquence pour la spec** : la section 5 indique « calcul interne en f16 linéaire ». Si ce tableau est confirmé, elle devient « composition en valeurs encodées sRGB (f32 dans les passes, f16 en cache), rééchantillonnages en linéaire ». C'est à l'utilisateur de réexporter la spec.

## Proposition : objets incorporés

Le programme les traite comme des calques pixel. Pour une tuile de sortie, la source décalée d'un nombre entier de pixels touche jusqu'à quatre tuiles sources. L'opération `Layer` les trouve par une table des pages propre à la source, comme le fait l'affichage. Le rééchantillonnage filtré arrive avec les transformations (J5).

## Alternatives écartées

- **Garder la passe de décodage en f16** : précision insuffisante devant les réglages raides (mesuré).
- **Décoder en f32 dans une réserve intermédiaire** : double la mémoire et garde une passe inutile.
- **Une passe GPU par calque** (fusion matérielle) : impossible pour les modes non Normal sans copie intermédiaire (0012).
- **Courbes évaluées par spline dans le shader** : il faudrait un second exemplaire de la formule pour l'éditeur de courbes.

## Risques

- La pile de 8 registres de `vec4<f32>` et l'interpréteur alourdissent le shader ; les GPU intégrés y sont sensibles. Le banc (0024) le mesure.
- Le choix de l'espace de `B` (Q1) change toutes les images de référence des modes : il doit être tranché avant B4.
