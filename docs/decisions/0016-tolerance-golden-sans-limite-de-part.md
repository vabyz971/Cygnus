# 0016 — Golden images : retrait de la limite de part de pixels

- **Date** : 2026-10-31
- **Statut** : acceptée (validée le 2026-10-31, complétée le 2026-11-01 : écart moyen borné) ; **tolérance exprimée en unités 16 bits depuis le 2026-11-01** (cache en entiers, 0026), section finale
- **Exigences concernées** : spec section 2.1 (critère d'acceptation) ; modifie 0013

## Contexte

0013 fixait la tolérance initiale entre adaptateurs à **1 pas de f16 par canal** sur la valeur linéaire, et **au plus 0,01 % des pixels** différents, à ne desserrer que sur preuve.

## Preuve (étape B4, 2026-10-31)

Golden image `normal-opacity` (640 × 400, 16 bits), référence produite sur lavapipe (Mesa 26.1.8), rendue sur une NVIDIA RTX 3080 Ti (Vulkan, pilote 615.78.08) :

- **une seule couleur** diffère, celle de l'aplat rouge à 50 % sur fond clair. Référence (58040, 45468, 46938, 65535), RTX (58040, 45468, 46937, 65535) : **1 unité sur 65 535 sur le canal bleu** ;
- cette couleur remplit **37 900 pixels (14,8 %)** de l'image ;
- aucun canal ne dépasse 1 pas de f16 (plus un pas de quantification de l'export) ;
- l'autre cas (`transparency-8bit`) est identique au bit près.

L'écart vient d'un arrondi différent d'une même valeur à la quantification 16 bits de l'export (fonctions `pow` du sRGB évaluées par des pilotes différents). Dans une image de test faite d'aplats, la part de pixels différents mesure la **surface** de l'aplat concerné, pas le nombre d'écarts. Le critère de part n'a donc pas de sens pour ces images.

## Décision (complétée le 2026-11-01 à la demande de l'utilisateur)

Trois critères, du plus strict au plus large :

1. **Même adaptateur : identité au bit près.** L'aperçu à 100 % et l'export 8 bits sont comparés sans aucune tolérance (`photo/render/tests/preview.rs`), de même que l'export de `cygnus render` et celui de la bibliothèque.
2. **Entre adaptateurs, écart maximal par pixel** : aucun canal d'aucun pixel ne dépasse **1 pas de f16** de la valeur linéaire, plus **un pas de quantification** de l'export 16 bits. C'est le rapport « écart / tolérance » du canal, qui doit rester ≤ 1.
3. **Entre adaptateurs, borne sur l'écart moyen** : la moyenne, sur tous les canaux de l'image, du rapport « écart / tolérance » reste **≤ 0,01** (`MEAN_RATIO_BOUND` dans `photo/render/tests/common/mod.rs`).

La **part de pixels différents** est rapportée, mais n'est plus limitée : c'est la limite retirée sur la preuve ci-dessus.

### Valeurs mesurées (2026-11-01)

| Comparaison | Pixels différents | Rapport maximal | Rapport moyen |
| --- | --- | --- | --- |
| `normal-opacity`, RTX 3080 Ti contre lavapipe | 37 900 sur 256 000 (14,8 %) | 0,084 | 0,0031 |
| `transparency-8bit`, RTX 3080 Ti contre lavapipe | 0 | 0 | 0 |
| les deux cas, lavapipe contre lui-même | 0 | 0 | 0 |

Le seul écart de la RTX vaut 8,4 % de la tolérance d'un canal, et la moyenne est 3 fois sous la borne. Metal (macos-15) et WARP (windows-2022) ont passé le critère 2 en CI le 2026-11-01. Le critère 3 y sera jugé au prochain passage.

## Conséquences

- Une erreur réelle de formule dépasse largement les critères 2 et 3. Une dérive diffuse d'un pilote (beaucoup de petits écarts) est arrêtée par le critère 3, même si chaque écart reste sous la tolérance.
- Tout desserrement d'un de ces critères exige une nouvelle preuve et une nouvelle décision.

## Tolérance en unités 16 bits (2026-11-01, cache en entiers de 0026)

Depuis 0021 (composition en valeurs encodées) et 0026 (cache `Rgba16Uint`), la chaîne de l'export ne contient plus de fonction transcendante ni de conversion flottante laissée au pilote : seules des opérations f32 (fusion, division du dépaquetage et de la dé-prémultiplication) et des arrondis écrits dans les shaders. Le pas de f16 n'a plus de sens comme unité. Les critères 2 et 3 deviennent :

2. **Écart maximal** : aucun canal d'aucun pixel ne diffère de plus de **`TOLERANCE_UNITS` = 1 unité** sur 65 535 de la référence. Une unité couvre une égalité d'arrondi tranchée autrement par la division f32 d'un autre pilote (WGSL tolère 2,5 ULP sur la division).
3. **Écart moyen** : la moyenne des écarts, sur tous les canaux de l'image, reste **≤ 0,01 unité** (`MEAN_UNITS_BOUND`).

Les valeurs W3C calculées à la main dans `photo/render/tests/render.rs` sont désormais exactes (32 767, 32 768, 16 384) et vérifiées avec la même tolérance.

### Valeurs mesurées (2026-11-01)

| Comparaison | Pixels différents | Écart maximal | Écart moyen |
| --- | --- | --- | --- |
| les deux cas, RTX 3080 Ti contre lavapipe | 0 | 0 | 0 |
| les deux cas, WARP (Microsoft Basic Render Driver, windows-2022) contre lavapipe | 0 | 0 | 0 |
| les deux cas, Metal (Apple Paravirtual device, macos-15) contre lavapipe | 0 | 0 | 0 |

Mesures de WARP et Metal relevées dans les annotations de la CI (exécution 41374718896, commit 9124d2b) ; la CI utilisait lavapipe de LLVM 20.1.2, la référence a été produite avec LLVM 21.1.8. Les quatre adaptateurs rendent les deux images **identiques au bit près** ; le test des 65 536 valeurs du format de calcul passe aussi sur les quatre. La tolérance de 1 unité reste une marge pour d'autres pilotes, jamais desserrée sans nouvelle mesure.

