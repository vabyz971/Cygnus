# 0028 — Formules des réglages et évaluation des courbes (J3, B5)

- **Date** : 2026-11-02
- **Statut** : acceptée avec conditions le 2026-11-02 (Q1 tranchée ci-dessous ; force de l'équilibre colorimétrique en attente des planches, Q2)
- **Exigences concernées** : REG-1, REG-2, REG-3, CAL-3, CAL-5, MAS-1 ; spec section 5 (couleur)

## Contexte

0021 fixe le cadre des réglages : un seul module `adjust.wgsl`, couleur directe encodée en sRGB (Q5), mélange par intensité × masque, forme des courbes définie une seule fois dans `photo/core` (`Curve::evaluate`). Les paramètres et leurs bornes sont dans `photo/core/src/adjust.rs` (0020). Il manque la formule exacte de chaque réglage, la façon dont le résultat rejoint l'accumulateur (mode, alpha, détourage) et la précision des courbes.

Notations : `C` couleur directe encodée d'un canal, dans 0..1 ; `clamp01` borne à 0..1 ; `Lum(C) = 0,3 R + 0,59 G + 0,11 B` et `SetLum` sont ceux du W3C, déjà dans `blend.wgsl` (B4).

## Proposition : application d'un réglage dans la passe de composition

Opération `Adjust { kind, params, opacity, mode, mask }` de l'interpréteur (0021), sur l'accumulateur prémultiplié `acc` :

1. `a = acc.a`. Si `a = 0`, rien ne change (aucune couleur à régler).
2. `C = unpremultiply(acc).rgb` (déjà borné à 0..1).
3. `C' = f(C)`, la fonction du réglage, en f32, bornée à 0..1.
4. `w = opacité × couverture du masque` (même couverture que les calques : inversion, densité, valeur vide).
5. Mélange par le mode du nœud, avec la formule générale de B4 sur un fond opaque : `M = blend(mode, (C, 1), (C' × w, w)).rgb`, soit `M = (1 − w) × C + w × B(C, C')`. En mode Normal : `M = C + w × (C' − C)`.
6. `acc = (M × a, a)` : **un réglage ne change jamais l'alpha**.

Aucune formule de fusion nouvelle : l'étape 5 appelle `blend()`. Un réglage en mode Luminosité (cas courant d'une courbe de contraste) fonctionne donc sans cas particulier.

Portée, par construction de l'interpréteur :

- dans un groupe isolé, le réglage n'agit que sur le contenu du groupe déjà composé sous lui ; dans un groupe passant, aussi sur le fond copié (comme Photoshop) ;
- **détouré** (CAL-5) : il agit dans l'accumulateur isolé de l'ensemble détouré, donc seulement sur la base et les calques détourés sous lui ; l'alpha ne changeant pas, source-atop n'a rien à ajouter ;
- plusieurs réglages empilés s'enchaînent en f32 dans la même passe, **sans arrondi 16 bits entre eux** : le seul arrondi est celui du cache.

**Q1 (décidée le 2026-11-02) : rien ne se détoure sur un réglage ou un filtre.** Un réglage n'a pas de pixels : composé seul dans l'accumulateur isolé d'un ensemble détouré, il agirait sur du vide et annulerait son effet. Les commandes refusent donc toute modification qui ferait d'un réglage ou d'un filtre la base d'un nœud détouré (`CoreError::ClipOntoEffect`, message « layer N cannot be clipped onto an adjustment or filter layer »), le document restant inchangé. Toutes les commandes de structure sont vérifiées après coup, comme la limite d'imbrication : `layer.update` (détourage), `layer.create` (réglage inséré sous un calque détouré), `layer.delete` et `layer.move` (base retirée ou déplacée), `layer.duplicate`, `layer.group` et `layer.ungroup`. Tests : `photo/core/tests/clip_effects.rs`.

Un réglage ou un filtre **détouré sur un calque pixel** reste permis : il n'agit que sur la base, et seulement là où elle a de l'opacité (test `a_clipped_adjustment_acts_only_on_its_base_where_it_is_opaque`). Un document venu d'ailleurs qui contiendrait quand même un nœud détouré sur un réglage s'affiche sans erreur : le réglage s'applique normalement, le nœud détouré n'apparaît nulle part (`Document::clipped_onto_effects` le signale).

## Proposition : formule de chaque réglage

Tous les paramètres viennent de `adjust.rs`, avec les bornes déjà validées par les commandes. Les valeurs transcendantes qui ne dépendent que des paramètres (tangente du contraste) sont calculées côté processeur en f64 et passées au shader.

### Niveaux

Pour un canal de paramètres `(ib, iw, g, ob, ow)` : `ib, iw, ob, ow` dans 0..1 avec `ib < iw` ; `g` dans 0,1..10 ; `ob > ow` est permis (inversion) :

```text
t  = clamp01((C − ib) / (iw − ib))
t' = t^(1 / g)          (g > 1 éclaircit les tons moyens, convention Photoshop ;
                         t' = t exactement si g = 1, t' = 0 si t = 0)
L(C) = ob + t' × (ow − ob)
```

Ordre (confirmé le 2026-11-02) : les niveaux du canal (rouge, vert ou bleu), **puis** les niveaux maîtres sur les trois. Même ordre pour les courbes. Documenté sur `Levels` et `Curves` dans `photo/core/src/adjust.rs`, donc dans le schéma des commandes, et vérifié par un test (« levels, channel then master »).

**Zone morte au point noir** (ajoutée le 2026-11-02 après la CI sur Metal) : `t^(1/g)` a une pente infinie en 0. L'arrondi f32 d'un pilote, environ 1,5 × 10⁻⁸ après la dé-prémultiplication d'un pixel semi-transparent sur Metal, y devenait 10 unités 16 bits avec g = 2, et deviendrait 0,16 avec g = 10. Une entrée à moins de 2⁻²⁰ (1/16 d'unité 16 bits) au-dessus du point noir donne `t = 0`. Ce seuil vaut environ 60 fois le bruit mesuré et reste 16 fois plus petit que l'écart entre deux niveaux 16 bits : une entrée exactement sur un niveau n'est jamais touchée. Test : `levels_gamma_is_stable_at_input_black`.

Exemple calculé à la main : maître `ib = 0,2, iw = 0,8, g = 2`, A = (0,2 ; 0,6 ; 0,8) : `t = (0 ; 0,6667 ; 1)`, `t' = √t = (0 ; 0,8165 ; 1)`.

### Courbes

`Curve::evaluate` (Hermite cubique monotone de Fritsch et Carlson, déjà dans `photo/core`), canal puis maître, comme les niveaux. Évaluation dans le shader : voir la section suivante.

Exemple : maître (0, 0), (0,5 ; 0,25), (1, 1), pentes (0,5 ; 1 ; 1,5). En x = 0,2, sur le segment [0 ; 0,5] avec `t = 0,4` : `0,144 × 0,5 × 0,5 + 0,352 × 0,25 − 0,096 × 0,5 × 1 = 0,076`. A donne (0,076 ; 0,368 ; 0,676).

### Teinte/saturation

Par TSL (HSL), comme prévu par 0021. `h` en degrés dans −180..180, `s` et `l` dans −100..100 :

```text
(H, S, L) = HSL(C)         H dans 0..1, S = 0 pour un gris
H' = fract(H + h / 360)
S' = clamp01(S × (1 + s / 100))           (−100 : gris ; +100 : saturation doublée)
L' = L × (1 + l / 100)          si l ≤ 0  (−100 : noir)
     L + (1 − L) × l / 100      si l > 0  (+100 : blanc)
C' = RGB(H', S', L')
```

Un gris reste gris quelle que soit la teinte ou la saturation (S = 0).

**Approximation de la TSL.** La clarté TSL `(max + min) / 2` n'est pas une luminosité perçue : à clarté TSL égale, un jaune pur (luminance Rec. 601 de 0,89) paraît bien plus clair qu'un bleu pur (0,11). En conséquence :

- tourner la teinte d'une couleur saturée change sa luminosité perçue (un jaune tourné vers le bleu s'assombrit fortement) ;
- la saturation TSL vaut 1 pour toute couleur dont un canal est à 0 ou à 1, même très sombre ou très claire : une forte saturation positive agit peu sur ces couleurs, et beaucoup sur les tons moyens ;
- la clarté ±100 va jusqu'au noir et au blanc purs, en passant par des couleurs plus vives (TSL) et non par des gris.

Photoshop ne publie pas sa formule : celle-ci en est une approximation documentée, pas une copie. Pour changer la teinte sans changer la luminosité perçue, on met le réglage en mode Couleur ou Teinte (modes W3C, fondés sur `Lum`).

Exemple : A est en HSL (200° ; 0,6 ; 0,5). Avec `h = 120, s = −50, l = 20` : (320° ; 0,3 ; 0,6), soit C' = (0,72 ; 0,48 ; 0,64).

### Luminosité/contraste

`b = luminosité / 100`, `c = contraste / 100`, dans −1..1 :

```text
C1 = C × (1 + b)            si b ≤ 0
     C + (1 − C) × b        si b > 0
k  = tan(π/4 × (1 + c))     calculé en f64 côté processeur ;
                            c = −1 : k = 0 (gris uniforme à 0,5) ;
                            c = +1 : k borné à 131 072 (seuil à 0,5)
C' = clamp01((C1 − 0,5) × k + 0,5)
```

Le pivot du contraste est 0,5 encodé. La formule ne dépend pas du contenu de l'image (pas de moyenne), donc une tuile se calcule seule.

Exemple : `b = 20, c = 50`, A : C1 = (0,36 ; 0,68 ; 0,84), k = tan(67,5°) = 2,41421, C' = (0,16201 ; 0,93456 ; 1).

### Équilibre colorimétrique

Trois plages pondérées par la clarté TSL `L = (max + min) / 2` de la couleur d'entrée. Les poids sont des rampes de largeur 0,25 centrées sur 1/3 et 2/3 :

```text
w_ombres = clamp01(0,5 − (L − 1/3) / 0,25)
w_hautes = clamp01(0,5 + (L − 2/3) / 0,25)
w_moyens = clamp01(0,5 + (L − 1/3) / 0,25) × clamp01(0,5 − (L − 2/3) / 0,25)
Δ = 0,7 × (ombres × w_ombres + moyens × w_moyens + hautes × w_hautes) / 100
    (par axe : cyan-rouge sur R, magenta-vert sur G, jaune-bleu sur B)
C1 = clamp01(C + Δ)
C' = SetLum(C1, Lum(C))   si « conserver la luminosité », sinon C1
```

Le facteur 0,7 limite l'effet maximal à ±0,7 sur un canal. **Q2 (en attente)** : la force n'est pas fixée avant que l'utilisateur ait jugé à l'œil les planches à ±25, ±50 et ±100 (équilibre colorimétrique et teinte/saturation, sur un dégradé et une image de test générée). La valeur 0,7 est provisoire, en une seule constante (`COLOR_BALANCE_STRENGTH`, `photo/render/src/adjust.rs`). `SetLum` est celui de `blend.wgsl` (W3C), réutilisé et non recopié.

Poids des trois plages selon la clarté TSL (ombres, tons moyens, hautes lumières). Ils somment à 1 partout, et chaque plage est pleine sur un tiers de l'axe :

| Clarté L | 0 | 1/6 | 1/3 | 1/2 | 2/3 | 5/6 | 1 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Ombres | 1 | 1 | 0,5 | 0 | 0 | 0 | 0 |
| Tons moyens | 0 | 0 | 0,5 | 1 | 0,5 | 0 | 0 |
| Hautes lumières | 0 | 0 | 0 | 0 | 0,5 | 1 | 1 |

Les ombres sont pleines jusqu'à L = 0,208 et nulles à partir de 0,458 ; les hautes lumières sont nulles jusqu'à 0,542 et pleines à partir de 0,792.

Exemple : tons moyens cyan-rouge +50, A : L = 0,5, w = (0 ; 1 ; 0), Δ_R = 0,35, C1 = (0,55 ; 0,6 ; 0,8). Lum(A) = 0,502 et Lum(C1) = 0,607, d'où C' = (0,445 ; 0,495 ; 0,695).

## Proposition : évaluation des courbes dans le shader

Pour garder un seul exemplaire de la formule, le shader lit une **table précalculée par `Curve::evaluate` et interpole linéairement** entre deux entrées (jamais de lecture au plus proche). Erreur maximale de l'interpolation par rapport à `evaluate`, mesurée sur 200 000 entrées tirées au hasard (unités de 16 bits sur 65 535) :

| Courbe | 256 | 1 024 | 4 096 | 16 384 | 65 536 |
| --- | --- | --- | --- | --- | --- |
| S douce (0,25 → 0,18 ; 0,75 → 0,82) | 0,55 | 0,035 | 0,002 | — | — |
| Raide (0,45 → 0,05 ; 0,55 → 0,95) | 62 | 4,0 | 0,25 | 0,016 | 0,001 |
| Hautes lumières ×10 (0,9 → 0 ; 1 → 1) | 47 | 3,1 | 0,19 | 0,012 | 0,001 |
| Très raide (points écartés de 0,01) | 4 170 | 365 | 27 | 1,7 | 0,11 |

La table de 1 024 entrées prévue par 0021 ne suffit pas pour les courbes raides. Proposition :

- **taille adaptative** (décidée) : `Curve::lookup_table` part de 4 096 entrées et double la taille, jusqu'à **65 536 au plus**, tant que la borne de l'erreur d'interpolation dépasse **0,25 unité de 16 bits**. La borne est analytique : sur un pas `h`, l'erreur d'une interpolation linéaire vaut au plus `h² / 8 × max |f''|`, et `f''` est linéaire sur chaque segment cubique, donc maximal à une de ses extrémités ;
- **domaine de la table** (ajouté à l'implémentation) : la table couvre seulement l'intervalle du premier au dernier point, où la courbe est cubique ; le shader borne son entrée à cet intervalle, la courbe étant plate au-delà. Une table sur 0..1 aurait un coude au premier ou au dernier point intérieur, que l'interpolation linéaire ne suit qu'à `h × pente / 4` près : jusqu'à 1 unité même à 65 536 entrées ;
- **précision mesurée** (test `lookup_tables_stay_within_a_quarter_of_a_16_bit_unit`) : au plus 0,25 unité d'écart d'interpolation, plus l'arrondi f32 de `evaluate` lui-même, qui atteint 0,1 unité sur la courbe très raide (pente d'environ 140) : la table ne peut pas être plus proche que sa référence. Borne du test : 0,35 unité ;
- à 65 536 entrées sur le domaine 0..1, chaque niveau 16 bits tombe exactement sur une entrée (test `a_full_table_hits_every_16_bit_level_exactly`) ;
- les tables (canaux et maître, en f32) sont rangées dans une **réserve de tables** sur le GPU, indexée par le contenu de la courbe : une table n'est calculée et envoyée que si la courbe change. Les courbes identiques (par exemple les canaux laissés à l'identité) partagent une table. Si les tables d'un lot ne tiennent plus, le tampon est remplacé par un plus grand qui ne garde que celles du lot. Courbe douce : 16 Kio ; pire cas : 256 Kio par courbe ;
- **glissement d'un point mesuré au banc** (0019, 2026-11-02) : 0,1 ms de calcul des tables sur le processeur et 16 Kio envoyés par pas ; une image complète coûte 0,6 ms à 300 %, sous la cible de 33 ms. À la vue ajustée, l'image coûte 177 ms, à cause de la recomposition de tout le niveau 0, pas des tables : B7.

Alternative écartée : évaluer la spline dans le shader. Ce serait un second exemplaire de la formule, déjà écarté par 0021.

## Tests (écrits avant le code)

- **Valeurs calculées à la main** pour chaque réglage (les exemples ci-dessus et les cas limites : identité à 1 unité près, gris inchangé par teinte et saturation, contraste −100 et +100, niveaux inversés, conserver la luminosité), en 16 bits avec des couleurs exactes (multiples de 0,2, comme en B4).
- **Cinq réglages empilés** (niveaux, courbes, teinte/saturation, luminosité/contraste, équilibre colorimétrique), comparés au calcul analytique en f64 enchaîné sans arrondi intermédiaire : écart d'au plus 1 unité. Le même calcul avec un arrondi 16 bits entre chaque réglage doit s'en écarter (le test prouve qu'il n'y a pas d'arrondi intermédiaire).
- **Courbe raide sur un dégradé 16 bits** (test de bandes étendu, 0026) : sortie monotone, aucun pas au-delà de `pente × pas d'entrée + 1`. Les cas d'étirement de 0026 (ombres ×10, hautes lumières ×10, tons moyens ×8) sont repris avec des niveaux et des courbes ; les seuils sont fixés au premier passage et consignés dans 0026.
- **Chaque réglage avec masque** (moitié noire, densité 0,5, inversé) et **détouré** sur une base partielle : le réglage n'agit que sous la couverture du masque et de la base, et l'alpha est inchangé.
- **Mode et intensité** : un réglage en mode Luminosité est comparé à la valeur calculée à la main, et une intensité de 0 donne l'identité.
- **Précision des tables** : test unitaire dans `photo/core`, avec erreur au plus 0,25 unité sur les quatre courbes du tableau.
- **Golden images** : une par réglage, sur lavapipe, avec des couleurs qui évitent les égalités d'arrondi (leçon de B4).
- **Ajoutés à la validation** (2026-11-02) : alpha 0 et alpha d'une unité sans NaN ; niveaux avec point noir égal au point blanc (refusés par les commandes, seuil sans NaN dans le rendu) ; teinte/saturation sur un pixel gris ; contraste ±100 ; réglage dans un groupe passant avec masque ; refus du détourage sur un réglage ou un filtre.

## Banc

Le document du banc passe à **10 calques dont 3 réglages** (courbes, niveaux, teinte/saturation) **et 2 masques** (sur un calque pixel et sur un réglage), comme le demande 0027. Mesure officielle du panoramique à cache limité à 128 tuiles (0027). Les rapports de B3 restent la référence sans réglages ; l'écart de coût des réglages est rapporté à part.

## Conséquences

- `adjust.wgsl` contient toutes les formules de réglage ; il appelle `blend()`, `lum()` et `set_lum()` de `blend.wgsl` sans les recopier. Le test d'unicité des formules est étendu.
- Les opérations reçoivent un décalage dans un tampon de paramètres par lot ; les tables de courbes vont dans leur propre réserve résidente.
- 0021 (table de 1 024 entrées) est révisée sur ce point.
- La teinte/saturation et l'équilibre colorimétrique ne reproduisent pas exactement Photoshop, dont les formules ne sont pas publiées : ce sont des formules documentées ici, testées à la main.
