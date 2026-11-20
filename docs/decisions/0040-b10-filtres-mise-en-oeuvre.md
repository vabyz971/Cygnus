# 0040 — B10 : mise en œuvre des filtres de voisinage, écarts à 0023

- **Date** : 2026-11-08
- **Statut** : acceptée le 2026-11-19 : la tolérance de 4 unités est réservée aux filtres ; origine de l'écart de Metal et mesure du flou de 100 px ci-dessous
- **Exigences concernées** : spec section 3 (filtres) ; décisions 0016, 0023, 0026, 0034, 0039

## Contexte

B10 était reporté après J4 (0027), puis placé en étape P6 de J5 (0039). L'utilisateur a demandé le 2026-11-08 de « poursuivre B7 et B10 ». Le flou gaussien et la netteté par masque flou sont donc rendus, selon 0023, avec les écarts ci-dessous. Commits a22eca4 (rendu) et d98743d (interface).

## Ce qui est fait comme 0023 le prévoit

- **Coupure du programme à chaque filtre**. Pour les filtres f1 à fn d'un programme :
  - les tuiles de sortie ont besoin du résultat de fn ;
  - fn lit son étape (le programme coupé avant fn) sur les tuiles voisines, dans sa marge de ⌈3σ⌉ pixels ;
  - cette étape lit à son tour les résultats de f1 à fn−1, et ainsi de suite.

  Les zones grandissent du dernier filtre au premier. Les passes s'exécutent du premier au dernier, dans le même encodeur que la composition.
- **Flou séparable** en deux passes : horizontale, puis verticale. Elles lisent leurs voisines par une table des pages, sans bordure recopiée.
- **Netteté** : `sortie = entrée + quantité × (entrée − flou)` là où l'écart dépasse le seuil.
- Tout est calculé **en valeurs encodées sRGB**, prémultipliées (0021), comme le reste de la composition.
- **Niveaux réduits** : le rayon est divisé par 2^L. Cela vaut pour les tuiles approximatives de B7, composées depuis les sources réduites.
- **Invalidation** : une modification de pixels invalide les tuiles du canevas jusqu'à la portée cumulée des filtres visibles.
- **Export et canevas** passent par le même chemin. Un test vérifie que le canevas à 100 % est identique à l'export 8 bits, avant et après une modification qui traverse la marge.
- **Interface** :
  - bouton « Nouveau filtre » du panneau Calques et menu Filtres : ils ajoutent un calque de filtre au-dessus de la sélection ;
  - ses paramètres se règlent dans le panneau Réglages : rayon, quantité, seuil, intensité, densité du masque. Chaque glissement forme une entrée d'historique.

## Écarts et choix non prévus par 0023 (à valider)

1. **Pas d'approximation au-delà de σ = 32 px** : le noyau est exact jusqu'au plafond de 100 px (601 coefficients par passe). L'évaluation sur un niveau réduit puis l'agrandissement (0023) n'est pas faite. Mesure sur le banc (24 Mpx, 10 calques plus un flou, RTX 3080 Ti, 2126 × 1086) :
   - aucune image au-delà de 38 ms : le budget de B7 tient ;
   - image finale de la première vue : 0,31 s à σ = 4, 0,35 s à σ = 30 et 0,69 s à σ = 100, contre 0,19 à 0,21 s sans filtre ;
   - rapports : `docs/bench/2026-11-08-*-b10-*`.

   L'approximation reste possible si une machine modeste le demande. Le noyau exact est plus juste, et plus simple.
2. **Format des étapes : `Rgba16Uint`** (décision finale de 0026), et non le f32 évoqué dans sa proposition. Les deux passes du flou sont chacune arrondies à 16 bits. Conséquences mesurées :
   - l'écart avec la référence f64 des tests est d'au plus 3 unités 16 bits ;
   - **entre adaptateurs, l'écart dépasse la tolérance de 0016** (1 unité) : 2 unités avec le flou seul, 3 avec une netteté à 150 % (RTX 3080 Ti contre lavapipe, écart moyen 0,0036). Une égalité d'arrondi bascule différemment d'un pilote à l'autre, et la netteté amplifie cet écart d'un facteur 1 + quantité.

   **Proposition** : les golden images nommées `filter-*` ont leur propre tolérance, de 4 unités au plus, avec la même borne moyenne de 0,01. Le reste de 0016 est inchangé. WARP et Metal seront mesurés par la CI.

   **Alternative** : stocker l'étape intermédiaire du flou en f32. Le résultat de la passe horizontale ne serait plus arrondi, au prix de deux fois plus de mémoire pour cette étape.
3. **Bord du document** : le flou lit le pixel du document le plus proche (bord étendu), comme Photoshop. 0023 ne le précisait pas.
4. **Mélange d'un calque de filtre**, non défini par 0023 :
   - en mode Normal, le résultat filtré remplace ce qui est dessous, au prorata de l'opacité et du masque : `A + w × (F − A)` ;
   - dans un autre mode, le résultat filtré est d'abord fusionné sur ce qui est dessous par `blend()`, puis mêlé de la même façon. Par exemple, un flou en mode Écran donne une lueur.
5. **Pas de stockage des registres de la pile** :
   - la suite du programme après un filtre recalcule les opérations qui le précèdent, à chaque pixel ;
   - à l'endroit du filtre, elle prend le résultat filtré au lieu de l'accumulateur ;
   - le préfixe est donc composé deux fois, mais aucune pile de registres n'est stockée par tuile d'étape.
6. **Netteté** :
   - le seuil s'applique canal par canal, sur les valeurs prémultipliées, l'alpha compris ;
   - la couleur est ensuite bornée par l'alpha.
7. **Limites** :
   - les zones d'un lot doivent tenir dans des réserves de 256 tuiles (64 pour les tuiles filtrées, voir le correctif ci-dessous), et leurs sources dans les réserves de sources. Les lots sont découpés en conséquence ;
   - si une seule tuile dépasse, le rendu échoue avec un message. C'est le cas pour plus de trois flous de 100 px empilés, ou pour un très grand nombre de calques sous un grand flou avec un GPU intégré (réserves de 409 tuiles) ;
   - la limite de 8 niveaux d'imbrication n'est pas levée.
8. **Calque de filtre détouré sur une base vide** : là où la base n'a pas de pixels, l'étape est transparente.

## Correctif après la CI du 2026-11-08 : WARP

**Constat.** Sous WARP (Windows), le premier envoi de B10 faisait perdre le périphérique (`DXGI_ERROR_DEVICE_REMOVED`) dès le premier export, même sans filtre dans le document. Une bissection sur une branche de diagnostic l'a localisé. Le shader de composition d'avant B10 passe ; tout ajout le fait tomber :
- un cas de plus dans le `switch` ;
- ou une texture de plus.

Le shader inlinait déjà `blend()` quatre fois et `blend_atop()` deux fois. Les autres pistes ont été écartées par l'essai :
- les passes de filtre compilées au démarrage ;
- les réserves d'une seule tuile ;
- l'initialisation des textures.

**Correctif** (même résultat au bit près sur lavapipe et la RTX ; toute la suite GPU passe sous WARP, avec la golden image des filtres identique à celle de lavapipe) :

1. **Composition** : les opérations qui fusionnent préparent leurs arguments dans le `switch`. `blend()` et `blend_atop()` sont appelées une seule fois, après lui. Le shader inline ces fonctions une fois chacune au lieu de six.
2. **Tuiles filtrées** :
   - elles sont recopiées dans 64 emplacements réservés à la fin des réserves de calques (pleine taille et réduite), que l'opération `Filter` lit par la liaison existante. La composition garde ses liaisons d'avant B10 ;
   - une tuile de sortie peut donc dépendre de 64 tuiles filtrées au plus, toutes étapes comprises. Par exemple, deux flous de 100 px empilés passent (1 + 25 tuiles), mais pas trois ;
   - avec la limite par défaut de WebGPU (256 couches de tableau), la réserve de calques passe de 256 à 192 tuiles.
3. **Passes et réserves de filtres** : elles sont créées au premier filtre rencontré. Un document sans filtre ne les alloue pas.

## Tests

- `photo/render/tests/filters.rs` :
  - flou de σ 0,5, 3, 40 et 90 contre une référence f64 sur 3 × 2 tuiles, dont une marge de plus d'une tuile ;
  - netteté et son seuil ;
  - opacité de 50 % ;
  - filtre dans un groupe isolé ;
  - canevas égal à l'export, et suivi d'une modification à travers la marge.
- Golden image `filter-blur-sharpen` :
  - un flou masqué par un dégradé, une netteté à 150 % et une lueur en mode Écran ;
  - faite sur lavapipe, relue à l'œil.
- Tests unitaires du noyau (somme à 1, rapport des coefficients), de la table des pages et de la dilatation des zones.

## Conséquences

- L'étape P6 de 0039 est faite en partie : il reste l'adoucissement de la sélection (SEL-2), qui réutilisera ces passes en J5.
- Questions à l'utilisateur :
  - la tolérance des golden images de filtres (point 2) ;
  - le noyau exact au lieu de l'approximation au-delà de 32 px (point 1) ;
  - la règle de mélange en mode autre que Normal (point 4).

## Origine de l'écart de Metal (2026-11-19)

Écarts mesurés par la CI sur la golden image `filter-blur-sharpen` :

| Adaptateur | Pixels différents | Au-delà de 1 unité | Écart max | Écart moyen |
| --- | --- | --- | --- | --- |
| lavapipe (référence) | 0 | 0 | 0 | 0 |
| WARP | 0 | 0 | 0 | 0 |
| RTX 3080 Ti (local) | 764 | 73 | 3 | 0,0036 |
| Metal (Apple Paravirtual) | 768 | 75 | 3 | 0,0036 |

**Ce que les mesures établissent :**

- l'écart ne vient pas de Metal en particulier : la RTX donne le même, sur un nombre de pixels presque identique ;
- les deux rastériseurs logiciels (lavapipe et WARP) sont identiques au bit près ;
- le flou seul, sans netteté, donne au plus 2 unités entre la RTX et lavapipe ;
- avec la netteté à 150 %, l'écart monte à 3 unités.

**Mécanisme** : chaque passe du flou somme jusqu'à 601 produits en f32, puis arrondit à 16 bits. Une somme qui tombe à moins d'un ulp f32 d'un demi-niveau bascule d'une unité selon l'ordre ou la fusion des opérations. La passe verticale hérite de cet écart et peut ajouter le sien, d'où 2 unités au plus. La netteté amplifie ensuite l'écart d'un facteur 1 + quantité (2,5 ici), d'où 3 unités.

**Cause probable, non isolée** : les compilateurs des GPU matériels (NVIDIA et Apple) fusionnent les multiplications-additions (FMA, un seul arrondi), ce que les rastériseurs logiciels ne font pas. C'est une hypothèse cohérente avec les mesures ; je ne l'ai pas isolée par un test. La tolérance de 4 unités, réservée aux images `filter-*`, couvre ce mécanisme jusqu'à une quantité de netteté de 150 %.

## Flou de 100 px en pleine résolution (2026-11-19)

Banc sans fenêtre : RTX 3080 Ti, vue de 2126 × 1086, document de 24 Mpx en 16 bits avec 10 calques, plus un flou gaussien au-dessus. Options `--blur 100 --drag-blur --export` : le rayon du flou est glissé de 100 à 70 px, une valeur par image. Rapports : `docs/bench/2026-11-19-*`.

| Mesure | Sans flou | σ = 30 | σ = 100 |
| --- | --- | --- | --- |
| Export 16 bits du document entier (pleine résolution) | 603 ms | 642 ms | 704 ms |
| Glissement du rayon à 300 %, en pleine résolution : image la plus longue | — | 7,1 ms | 7,1 ms |
| Glissement du rayon en vue ajustée, aperçu réduit : image la plus longue | — | 12,0 ms | 7,0 ms |
| Première vue : vue sans trou, puis image finale | 189 ms | 360 ms | 411 ms, puis 704 ms |
| Relâchement : image finale | 151 ms | 251 ms | 294 ms |

- **L'aperçu réduit reste sous 33 ms**, avec 7 ms au plus à σ = 100. Les tuiles approximatives sont composées depuis les sources réduites, avec un rayon divisé par 2^L.
- **En pleine résolution**, aucune image ne dépasse 7,1 ms pendant le glissement. Le budget étale le travail : 544 tuiles composées en 60 images à σ = 100, contre 1 200 sans flou. La vue n'est donc entièrement à jour qu'après 2 à 3 images environ, et les tuiles en attente montrent l'état précédent.
- **Le flou de 100 px coûte 100 ms de plus à l'export** de 24 Mpx.
