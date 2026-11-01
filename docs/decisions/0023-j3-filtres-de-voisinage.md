# 0023 — Filtres de voisinage : flou gaussien, netteté par masque flou (J3)

- **Date** : 2026-11-01
- **Statut** : acceptée avec conditions le 2026-11-01 (phase A du jalon J3) ; conditions intégrées ci-dessous
- **Exigences concernées** : spec section 3 (filtres) ; applique 0012, point 3

## Proposition

- **Étapes** : le compilateur coupe le programme à chaque filtre (0012).
  1. L'étape qui précède le filtre est matérialisée en tuiles intermédiaires au **format d'étape f32** (`Rgba32Float`). C'est le seul endroit où un résultat intermédiaire est stocké avant d'être encore modifié (voir 0026). La clé de cache est l'empreinte du sous-programme.
  2. La passe de filtre lit ses tuiles d'entrée avec une **marge** égale au rayon du noyau, par la table des pages : il n'y a pas de bordure recopiée. Une tuile voisine manquante est calculée d'abord.
  3. L'étape suivante reprend par `Base { stage }`.
- **Flou gaussien** : séparable, deux passes (horizontale puis verticale) avec une tuile intermédiaire f32. Le rayon est l'écart type σ, en pixels du document. La marge vaut ⌈3σ⌉. Au-delà d'un pas de σ, le noyau est évalué sur un niveau de la pyramide des sources, puis suréchantillonné : σ jusqu'à 32 px est exact, au-delà c'est une approximation documentée. **Plafond décidé : 100 px en bêta (Q6).**

  **Pourquoi c'est exact jusqu'à 32 px, et approché de 32 à 100 px :**
  - un noyau exact a 6σ + 1 coefficients par passe : 193 jusqu'à σ = 32, mais 601 à σ = 100, par pixel et par passe. Le coût croît avec σ, et la marge (3σ = 300 px) touche jusqu'à 5 × 5 tuiles autour de chaque tuile ;
  - au-delà de 32 px, le flou est calculé sur le niveau L de la pyramide des sources tel que σ / 2^L soit compris entre 16 et 32 (L = 1 ou 2 pour σ ≤ 100), puis agrandi par interpolation bilinéaire en linéaire. Le coût par pixel reste celui de σ ≤ 32 ;
  - l'erreur est faible : un gaussien de σ ≥ 32 px n'a presque plus d'énergie au-dessus de la fréquence de Nyquist du niveau réduit (moins de e^(−π²) ≈ 5 × 10⁻⁵ de l'amplitude au bord de bande pour L = 1 et σ = 32). La perte est donc invisible ; elle est mesurée en phase B contre un noyau exact (test de référence, hors temps réel).
- **Espace du flou, par choix** : le flou est calculé en valeurs encodées sRGB, comme le reste de la composition (tableau de 0021). Moyenner des valeurs encodées **assombrit les détails clairs et fins** : un point lumineux sur fond sombre perd plus de luminance qu'avec un flou en linéaire, et les bords clairs s'étalent en sombre. C'est le **comportement par défaut de Photoshop** (« fusion RVB avec gamma 1,0 » désactivée), retenu par cohérence avec ce logiciel et avec les modes de fusion. Un flou en linéaire, physiquement plus juste, est possible plus tard comme option, par une nouvelle décision.
- **Netteté par masque flou** : `sortie = entrée + quantité × (entrée − flou)` là où `|entrée − flou|` dépasse le seuil, en valeurs encodées sRGB comme les réglages (0021). Elle réutilise la passe de flou.
- **Invalidation** : une zone salie est dilatée de la marge de chaque filtre traversé. Seules les tuiles concernées sont recalculées.
- **Niveaux réduits** : sur un niveau L, le rayon est divisé par 2^L. L'aperçu réduit reste juste en apparence, et l'export et le niveau 0 restent exacts.
- **Aperçu en direct pendant un glissement** : le filtre est calculé sur le niveau affiché, ou un niveau plus grossier si le budget l'exige (0024), puis le niveau final est recalculé à la fin du geste.
- **Export** : le même programme, les mêmes étapes et les mêmes passes ; l'export procède par lots (0011), marges comprises.

## Risques

- Un grand rayon à 100 % demande beaucoup de tuiles voisines. Le temps de l'image finale (150 ms) peut ne pas être tenu pour les grands rayons sur une vue plein écran : l'écart sera mesuré et consigné.
- **C'est la dernière étape de la phase B. Si elle menace le calendrier, elle peut glisser vers J5 sur décision de l'utilisateur** (consigne).
