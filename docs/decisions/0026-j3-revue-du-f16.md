# 0026 — Revue du format de calcul f16 (J3, point 11)

- **Date** : 2026-11-01
- **Statut** : acceptée avec conditions le 2026-11-01 (phase A du jalon J3) ; conditions intégrées ci-dessous ; révise 0011 ; **format du cache composé changé le 2026-11-01** : entiers 16 bits `Rgba16Uint` empaquetés à la main au lieu du f16 (section « Format du cache composé » ci-dessous)
- **Exigences concernées** : spec section 5 (couleur)

## Mesure (prototype jetable, hors dépôt, 2026-11-01)

Simulation numérique exacte (arrondi f16 de la norme IEEE 754 par `struct` de Python) d'un dégradé 16 bits sRGB, à travers des niveaux raides appliqués en f32 sur des valeurs encodées, puis exporté en 16 bits. Trois variantes de stockage intermédiaire :

| Cas (étirement) | Valeurs d'entrée | Source stockée en f16 avant le réglage | Tout en f32 | Source entière lue en f32, résultat seul en f16 |
| --- | --- | --- | --- | --- |
| Ombres ×10 (niveaux 0..0,1) | 6 554 | 3 992 distinctes, pas max 53 | 6 554, pas 10 | 5 541, pas 21 |
| Ombres profondes ×40 | 1 639 | 1 639, pas max 83 | 1 639, pas 40 | 1 639, pas 58 |
| Hautes lumières ×10 (0,9..1) | 6 555 | **436** distinctes, pas max **163** | 6 555, pas 10 | 5 540, pas 21 |
| Tons moyens ×8 (0,45..0,575) | 8 193 | **815** distinctes, pas max **142** | 8 193, pas 8 | 6 247, pas 21 |

Pas maximaux en unités de 16 bits sur 65 535. En niveaux 8 bits, le pire cas de la colonne f16 vaut 163 / 257 = 0,63 niveau : il n'est pas visible à l'écran (8 bits), mais **il dégrade un export 16 bits** à une précision effective de 9 à 10 bits après un réglage raide. Dans la dernière colonne, le pire pas vaut 0,08 niveau 8 bits.

## Conclusion et proposition

- **La perte ne vient pas du f16 en général**, mais du **stockage en f16 d'une donnée qu'un réglage va ensuite étirer**. Un résultat final stocké en f16 (cache du canevas, export) ne perd que ce que donne un arrondi uniforme.
- **Proposition** :
  1. **Sources lues en entiers dans la passe de composition**, en f32 (0021) : la passe de décodage f16 de J2 disparaît.
  2. **Résultats intermédiaires qu'une étape suivante modifie** (étapes de filtres, 0023) : **format d'étape f32** (`Rgba32Float`, dans le cœur de WebGPU pour l'écriture depuis un shader).
  3. **Résultat composé final** (cache, pyramide, affichage, export) : ~~f16, inchangé~~ **entiers 16 bits `Rgba16Uint`, empaquetés à la main** (choix du 2026-11-01, section suivante).
  4. Les deux formats (cache et étape f32) sont nommés uniquement dans `photo/render/src/format.rs`, et le test qui l'impose est étendu.
- **Vérification en phase B** : le test de bandes de J2 est étendu sur GPU, avec des niveaux et des courbes raides (les cas ci-dessus). Les seuils sont fixés au premier passage, puis jamais desserrés sans preuve. La décision est consignée à nouveau à la fin de J3 avec ces mesures GPU.

## Format du cache composé : prototype comparatif (2026-11-01)

Prototype jetable hors dépôt (`/tmp/cygnus-proto-cache`, wgpu 30.0.1), dans l'espace de 0021 (valeurs sRGB encodées, alpha prémultiplié). Trois formats de 8 octets par pixel : `Rgba16Float` (f16), `Rgba16Uint` avec empaquetage manuel dans le shader (`round(clamp(c) × 65535)`), `Rgba16Unorm` (fonctionnalité optionnelle `TEXTURE_FORMAT_16BIT_NORM`). Mesuré sur une NVIDIA RTX 3080 Ti (Vulkan, pilote 615.78.08) et sur lavapipe (Mesa 26.1.8).

**Arrondi du f16 par le pilote, mesuré.** Les 65 536 valeurs `v / 65535` écrites dans une texture f16 depuis un shader, puis relues :

| Adaptateur | Conformes à l'arrondi vers zéro | Conformes à l'arrondi au plus proche | 63 569 devient |
| --- | --- | --- | --- |
| RTX 3080 Ti | 65 536 / 65 536 | 33 794 (cas où les deux coïncident) | f16 `0x3bc2`, export 63 551 |
| lavapipe | 65 536 / 65 536 | 33 794 | f16 `0x3bc2`, export 63 551 |

Les deux adaptateurs **tronquent** (arrondi vers zéro) ; l'arrondi au plus proche aurait donné 63 583. L'hypothèse d'un écart « d'un pas selon le pilote » avancée auparavant était **supposée** d'après une seule valeur ; elle est remplacée par cette mesure. WARP et Metal n'ont pas été mesurés pour le f16, devenu sans objet.

**Comparaison** (résultats identiques sur les deux adaptateurs, sauf les durées) :

| Critère | f16 encodé | uint16 empaqueté | unorm16 |
| --- | --- | --- | --- |
| Export 16 bits d'un calque opaque intact | 6 153 / 65 536 valeurs exactes, erreur max **32** unités | **65 536 / 65 536, erreur 0** | 65 536 / 65 536, erreur 0 |
| Hautes lumières ×10 : valeurs distinctes, pas max | 3 686, 32 | **6 553, 10** (minimum théorique) | 6 553, 10 |
| Tons moyens ×8 | 4 096, 32 | **8 192, 8** | 8 192, 8 |
| Ombres ×10 | 3 687, 32 | **6 554, 10** | 6 554, 10 |
| Mémoire | 8 o/px (512 Kio par tuile de 256²) | 8 o/px | 8 o/px |
| Affichage 2560 × 1440, au plus proche (RTX / lavapipe) | 0,030 / 3,0 ms | 0,031 / 3,2 ms | 0,056 / 3,1 ms |
| Affichage, bilinéaire en linéaire décodé à la main (RTX / lavapipe) | 0,035 / 6,6 à 7,1 ms | 0,037 / 6,6 à 6,8 ms | 0,059 / 6,1 à 6,3 ms |
| Disponibilité | cœur | cœur | fonctionnalité optionnelle |

- Après des niveaux raides, quelques sorties diffèrent d'un pas entre la RTX et lavapipe (123 à 201 valeurs sur 65 536, aucune dans les ombres), **dans tous les formats** : l'écart vient du calcul f32 du shader (division des niveaux), pas du stockage.
- Le décodage manuel du filtrage linéaire coûte environ 3,5 ms de plus que l'échantillonnage au plus proche sur lavapipe, et rien de mesurable sur la RTX, **quel que soit le format**. L'échantillonneur matériel de unorm16 n'apporte donc rien, puisque le filtrage se fait de toute façon à la main, en linéaire (0021).

**Décision (validée par l'utilisateur le 2026-11-01)** : cache composé, pyramide et étapes stockées entre passes en **`Rgba16Uint`**, empaquetés et dépaquetés par deux fonctions WGSL (`pack_compute`, `unpack_compute`) définies uniquement dans `photo/render/src/format.rs`. L'arrondi (au plus proche, égalités au pair, `round` de WGSL) est écrit dans notre code au lieu d'être laissé à la conversion du pilote. Les calculs des passes restent en f32.

## Précision de l'export 16 bits (corrigée le 2026-11-01)

Le tableau précédent supposait un arrondi au plus proche du f16, avec ± 16 unités au plus dans les hautes lumières. **La mesure ci-dessus le contredit** : les pilotes tronquent, l'erreur atteint 32 unités (un pas entier) et seulement 9,4 % des valeurs étaient exportées exactement. Le cache en entiers 16 bits supprime cette perte. Mesures dans les tests (`photo/render/tests/precision.rs`, `cache_format.rs`), identiques sur la RTX et lavapipe :

- **Format du cache seul** : les 65 536 valeurs 16 bits écrites par `pack_compute`, relues par copie et par `unpack_compute`, reviennent exactement. Ce test tourne sur chaque job GPU de la CI (lavapipe, WARP, Metal).
- **Calque opaque 16 bits intact** : export 16 bits identique à la source pour les 65 536 valeurs ; export 8 bits égal à `(v16 + 128) / 257`.
- **8 bits** : les 256 valeurs font l'aller-retour exact 8 → 16 → 8 (`v × 257`, puis `(v16 + 128) / 257`), en couleur opaque comme en alpha.
- **Pixels semi-transparents (limite documentée)** : la couleur est stockée prémultipliée, arrondie à une demi-unité ; la dé-prémultiplication de l'export divise cette erreur par l'alpha. Borne : `0,5 × 65535 / alpha + 0,5` unités (alpha en unités 16 bits), plus une unité pour l'arithmétique f32. Mesures sur un dégradé d'alpha de 0 à 65 535 sous une couleur fixe :

| Alpha (16 bits) | Erreur max de la couleur, export 16 bits | Export 8 bits |
| --- | --- | --- |
| 32 768 à 65 535 (≥ 50 %) | 1 unité | 1 niveau |
| 2 571 à 32 767 (4 à 50 %) | 12 unités | 1 niveau |
| 257 à 2 570 (0,4 à 4 %) | 123 unités | 1 niveau |
| 1 à 256 (< 0,4 %, invisible en alpha 8 bits) | jusqu'à la couleur entière | 77 niveaux |

L'alpha est toujours exporté exactement. En 8 bits, la couleur reste à un niveau près dès que le pixel a un alpha 8 bits non nul (alpha ≥ 129). Le f16 était plus fin pour les très petites valeurs prémultipliées ; cette perte, limitée aux pixels presque transparents, est acceptée en échange de l'exactitude des pixels opaques.

## Alternatives écartées

- **Tout passer en f32** : double la mémoire du cache et la bande passante de l'affichage, pour un gain nul sur le résultat final.
- **Cache en f16** (proposition initiale) : arrondi laissé au pilote (troncature mesurée), export 16 bits inexact, bandes deux à quatre fois plus larges après un réglage raide, pour la même mémoire.
- **Cache en `Rgba16Unorm`** : mêmes résultats que `Rgba16Uint`, mais fonctionnalité optionnelle de wgpu, et le filtrage matériel qu'elle permet ne sert pas (filtrage manuel en linéaire).
- **Garder la passe de décodage f16** : perte mesurée ci-dessus.
