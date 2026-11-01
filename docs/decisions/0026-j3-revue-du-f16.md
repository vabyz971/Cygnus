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

## Alternatives écartées

- **Tout passer en f32** : double la mémoire du cache et la bande passante de l'affichage, pour un gain nul sur le résultat final.
- **Garder la passe de décodage f16** : perte mesurée ci-dessus.
