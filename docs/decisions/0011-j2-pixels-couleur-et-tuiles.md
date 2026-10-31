# 0011 — Format des pixels, couleur et tuiles (J2)

- **Date** : 2026-10-30
- **Statut** : acceptée avec conditions le 2026-10-30 (phase A du jalon J2) ; conditions intégrées ci-dessous
- **Exigences concernées** : spec section 5 (couleur, mémoire, fluidité), section 7.3

## Contexte

La spec demande un calcul interne en f16 linéaire sur GPU, une conversion vers l'écran à l'affichage, un canevas découpé en tuiles avec texture éparse, et un cache par tuile invalidé uniquement par les commandes qui touchent la zone.

## Proposition : format des pixels et couleur

- **Stockage de référence, côté processeur** : chaque calque pixel conserve ses pixels **dans la profondeur du document** (8 ou 16 bits entiers par canal, RGBA), **encodés dans le profil du document** (sRGB en J2), **alpha non prémultiplié**. C'est la représentation naturelle des images importées : une image 8 bits reste en 8 bits, sans conversion à l'import.
- **Calcul sur GPU** : la première passe décode chaque tuile dans le shader (entier vers flottant, sRGB vers linéaire, prémultiplication) en **`Rgba16Float` linéaire à alpha prémultiplié**. Tous les calculs se font en f32 dans les shaders ; seuls les résultats intermédiaires stockés sont en f16.
- **Pourquoi prémultiplié** : le filtrage et la réduction (zoom arrière, niveaux de détail) sont corrects sans halos, la fusion Normal est une seule opération, et les groupes isolés de J3 se composent naturellement. Les modes non séparables de CAL-3 (Teinte, Couleur…) dé-prémultiplient localement, dans le module de fusion lui-même (0012).
- **Pourquoi le stockage reste en entiers encodés** : 8 bits linéaires produiraient des aplats visibles dans les tons sombres. Le f16 pour tout stocker doublerait la mémoire des documents 8 bits.
- **Conversion à l'affichage** : linéaire prémultiplié, puis dé-prémultiplication sur le fond du canevas, puis encodage sRGB dans le shader. Le cadre d'egui est en `Rgba8Unorm` ou `Bgra8Unorm` **non sRGB** (vérifié dans `egui-wgpu` 0.36), donc l'encodage est fait par notre WGSL. Il est écrit **une seule fois**, dans `color.wgsl`, et partagé par l'affichage et l'export.
- **Profil de l'écran** : en J2, l'écran est supposé sRGB. La conversion vers le profil réel de l'écran (spec section 5) sera ajoutée plus tard sous forme de table 3D dans le même shader, sans changer l'architecture. **Décidé (Q3)** : sRGB seul pendant J2.
- **Images 8 bits importées (J6)** : elles sont stockées en 8 bits et décodées comme les autres. Rien à prévoir de plus en J2.

**Format de calcul isolé en un seul endroit.** Le format `Rgba16Float` n'est écrit qu'une fois, dans `photo/render/src/format.rs`, qui définit :

- le format de texture wgpu (`COMPUTE_FORMAT`) ;
- le jeton WGSL correspondant (`rgba16float`), injecté dans les shaders au moment de leur assemblage ;
- le type d'un texel côté processeur (`ComputeTexel`, `[half::f16; 4]`) et sa taille en octets.

Tout le reste du code (réservoirs, budgets, lectures, tests) passe par ces définitions. Un test parcourt les sources et les shaders, et échoue si `Rgba16Float` ou `rgba16float` apparaît ailleurs. Passer au f32 si la revue d'après J3 le décide ne toucherait que ce fichier.

**Limite de précision, acceptée (Q4) avec un test de bandes et une revue après J3.** Le f16 a 11 bits de mantisse. Un document 16 bits qui passe par le composite f16 linéaire perd un peu de précision par rapport à sa source entière : de l'ordre d'un millième en relatif, soit quelques dizaines d'unités sur 65 535 dans les hautes lumières. C'est invisible à l'œil, mais l'export 16 bits d'un calque intact n'est pas bit à bit identique à la source. L'alternative `Rgba32Float` supprime ce point, au prix d'un doublement de la mémoire GPU et de la bande passante. **Test de bandes** : un dégradé lisse 16 bits (et un dégradé sombre, zone la plus sensible) traverse la composition et l'export 16 bits. Le test vérifie que la sortie est monotone et qu'aucun saut entre pixels voisins ne dépasse un seuil fixé au premier passage, puis jamais desserré sans preuve. **La décision f16 est revue après J3**, avec les réglages (courbes, niveaux), qui amplifient les écarts.

## Proposition : tuiles

- **Taille : 256 × 256 px.** Une tuile f16 pèse 512 Kio. En 128 px, le nombre de dispatches et la taille des tables augmentent. En 512 px, l'invalidation est grossière : un petit trait salirait 2 Mio de calcul par calque.
- **Texture éparse émulée.** wgpu n'expose pas les textures éparses matérielles (*tiled resources*). On utilise donc un **réservoir de tuiles** : des `texture_2d_array` en `Rgba16Float`, chaque couche (*layer*) du tableau contenant 1 tuile. Une table des pages, sur le processeur et dans un tampon de stockage GPU, associe `(calque, niveau, tx, ty)` à un emplacement. Une tuile entièrement transparente n'a pas d'emplacement : c'est le côté creux.
- **Niveaux de détail** *(remplacé par 0018 : pyramide des tuiles composées et non par calque)*. Chaque calque a une pyramide de tuiles (niveau 0 = pleine résolution, puis divisions par 2), générée sur GPU quand une tuile change. Un zoom de 18 % compose au niveau 2. Sans cela, afficher en entier une photo de 24 Mpx à dix calques imposerait d'avoir 1,9 Gio de calques résidents sur GPU. **L'aperçu à 100 % utilise toujours le niveau 0, comme l'export**, ce qui garantit leur identité (spec 2.1). Aux autres zooms, l'aperçu est une réduction de cette image : c'est voulu et documenté.
- **Cache de composite par tuile** : la clé est `(niveau, tx, ty)` ; la valeur est l'emplacement de la tuile composée et l'empreinte de la pile de calques au moment du calcul. Le cache est invalidé uniquement par les `Changes` des commandes : une modification de pixels salit les tuiles couvertes (à tous les niveaux), une modification de propriété salit toutes les tuiles où le calque est présent.
- **Mémoire GPU** : budget configurable, par défaut 1 Gio sur GPU dédié et 512 Mio sur GPU intégré, dont le type est connu via `AdapterInfo::device_type`. Les tuiles non visibles sont évincées dans l'ordre LRU. La source de vérité reste côté processeur : une tuile évincée se recharge, sans aucune perte.
- **Mémoire GPU insuffisante** : chaque allocation se fait dans une portée d'erreur wgpu (`push_error_scope(OutOfMemory)`). En cas d'échec, on réduit le budget, on évince, et l'affichage passe à un niveau plus grossier pour la zone visible. Un message apparaît dans la barre d'état. L'**export** traite le document par bandes de tuiles et n'a donc jamais besoin de tout le document à la fois. Le cas extrême (même une bande ne tient pas) donne une erreur claire, jamais un plantage.
- **Perte du périphérique GPU** : on reconstruit tous les réservoirs depuis le stockage processeur, ce qui est possible parce que le processeur est la référence.

## Alternatives écartées

- **GPU comme source de vérité** : plus rapide pour le pinceau, mais une perte du périphérique fait perdre des données, et chaque enregistrement ou annulation exige des relectures.
- **Une texture par calque** : pas de stockage creux, mémoire proportionnelle à la taille du document et à dix calques, limite de 8192 ou 16384 px par côté.
- **Atlas 2D avec marges** : les tableaux de textures évitent les marges, car la composition lit texel à texel ; le filtrage à l'affichage est fait à la main (0012).

## Risques

- **Mémoire processeur** : 24 Mpx × 16 bits × RGBA × 10 calques = 1,9 Gio de RAM pour le scénario du banc d'essai. C'est acceptable sur un poste de photographe, mais un échange de tuiles sur disque pourrait devenir nécessaire après la bêta.
- **Niveaux de détail et fusion** : pour les modes non linéaires, fondre des calques réduits n'est pas exactement réduire la fusion. L'écart est visible seulement en vue réduite, jamais à 100 % ni à l'export.
