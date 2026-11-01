# 0026 — Revue du format de calcul f16 (J3, point 11)

- **Date** : 2026-11-01
- **Statut** : acceptée avec conditions le 2026-11-01 (phase A du jalon J3) ; conditions intégrées ci-dessous ; révise 0011
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
  3. **Résultat composé final** (cache, pyramide, affichage, export) : **f16, inchangé**.
  4. Les deux formats (calcul f16 et étape f32) sont nommés uniquement dans `photo/render/src/format.rs`, et le test qui l'impose est étendu.
- **Vérification en phase B** : le test de bandes de J2 est étendu sur GPU, avec des niveaux et des courbes raides (les cas ci-dessus). Les seuils sont fixés au premier passage, puis jamais desserrés sans preuve. La décision est consignée à nouveau à la fin de J3 avec ces mesures GPU.

## Précision de l'export 16 bits (demandée le 2026-11-02)

Le résultat composé est stocké en f16 avant l'export ; avec le tableau de 0021, il contient des **valeurs encodées sRGB** dans 0..1. L'écart entre deux valeurs f16 voisines dépend de la plage, et l'arrondi en coûte au plus la moitié :

| Plage encodée | Écart f16 (en unités 16 bits) | Erreur maximale d'arrondi | Précision effective | En niveaux 8 bits |
| --- | --- | --- | --- | --- |
| 0,5 à 1 (hautes lumières) | 2⁻¹¹ ≈ 32 unités | ± 16 unités (0,024 %) | environ 11 bits | 0,06 niveau |
| 0,25 à 0,5 | 16 unités | ± 8 | environ 12 bits | 0,03 |
| 0,125 à 0,25 | 8 unités | ± 4 | environ 13 bits | 0,016 |
| en dessous de 0,125 (ombres) | 4 unités ou moins | ± 2 ou moins | 14 à 16 bits | — |

- **Conséquence** : un export 16 bits ne conserve pas toute la précision d'une source 16 bits. Il garde environ **11 bits dans les hautes lumières**, et davantage vers les ombres. Pour un export 8 bits, la perte est nulle en pratique (au plus 0,06 niveau, arrondi compris).
- **Ce que cela ne touche pas** : les calculs de la passe de composition (f32), les réglages (sources entières lues en f32), et l'identité aperçu-export (même quantification).
- **Ce qui la supprimerait** : exporter depuis un résultat composé en format d'étape f32 au lieu du cache f16. L'export ne passant pas par le cache du canevas (il procède par lots, 0011), c'est possible sans toucher à l'aperçu, mais l'identité au bit près de l'aperçu à 100 % (8 bits) et de l'export devrait alors être revérifiée. **Non fait en J3** ; à rouvrir si un utilisateur 16 bits le demande.

## Alternatives écartées

- **Tout passer en f32** : double la mémoire du cache et la bande passante de l'affichage, pour un gain nul sur le résultat final.
- **Garder la passe de décodage f16** : perte mesurée ci-dessus.
