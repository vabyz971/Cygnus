# 0029 — Mise en œuvre de B6 : import, objets incorporés, fusion vers le bas, scénario

- **Date** : 2026-11-02
- **Statut** : consignée (mise en œuvre de 0020, 0021 et 0027, plan accéléré de l'utilisateur) ; les choix marqués « à valider » attendent l'avis de l'utilisateur
- **Exigences concernées** : DOC-2, CAL-4, REG-1, HIS-1, HIS-3 ; spec section 5 (sécurité des entrées)

## Contexte

0020 décide que `source.import` (lire un fichier) et `layer.merge_down` (rendre deux calques) passent par **un seul chemin**, `resolve_and_execute`, au-dessus du bus, et que `photo/core` ne dépend ni du disque ni du GPU. 0027 fixe les décodeurs, le scénario photo et ses images générées. Cette note dit comment c'est fait et où la mise en œuvre s'écarte des textes.

## Ce qui est fait

- **Chemin unique** : `photo/core/src/resolve.rs` définit `resolve_and_execute` et deux traits, `Import` (mis en œuvre par `photo/io`, `FileImporter`) et `Merge` (mis en œuvre par `photo/render`, sur `Renderer`). L'appelant fournit ceux dont il dispose (`Resolvers`). Les commandes publiques portent leur résultat résolu dans un champ **non sérialisé et absent du schéma** ; appliquées sans lui, elles sont refusées par le bus (`CoreError::Unresolved`, message « send it through resolve_and_execute »). Aucun autre chemin ne peut donc les exécuter. `Script::build_with`, `cygnus render` et la session de l'interface passent par cette fonction.
- **`source.import`** : décodage dans `photo/io` (PNG 8 et 16 bits, tous types de couleur ; JPEG par `zune-jpeg` en chemin scalaire, identique sur toutes les plateformes). L'identifiant est le SHA-256 du contenu décodé (`sha2`). La source est stockée une fois, et l'annulation la conserve. Refaire ne relit pas le fichier (vérifié par un test). Sont refusés sans plantage, avec un message : fichier vide, non image, tronqué, corrompu, plus de 65 535 px de côté, plus de 256 Mpx.
- **Objets incorporés** : rendus comme des calques pixel, leurs tuiles lues dans leur source. Un test vérifie qu'un objet incorporé donne exactement la même image qu'un calque pixel de mêmes valeurs, pour les quatre couples de profondeurs.
- **`layer.merge_down`** : les deux calques sont rendus seuls par le chemin d'export, et le résultat remplace le calque du dessous (même identifiant, opacité 1, mode Normal, sans masque).
- **Scénario photo** : variante sans filtre de 0027, filtres reportés (B10), dans `apps/cli/tests/scenario/`. Il est rendu par le binaire `cygnus render` et comparé à sa golden image selon 0016. Un test vérifie que tout annuler puis tout refaire redonne le même document.

## Écarts et choix à valider

1. **Fusion vers le bas limitée aux cas exacts** (à valider). 0020 exige « exactement ce que l'on voyait ». Ce n'est possible que si la composition des deux calques seuls, puis sur le fond, égale leur composition sur le fond (associativité de source-over et de source-atop). La fusion est donc **refusée**, avec un message, quand :
   - un des deux calques n'est ni un calque pixel ni un objet incorporé (réglage, filtre, groupe) ;
   - un des deux est masqué (invisible) ou verrouillé ;
   - un des deux n'est pas en mode Normal ;
   - le calque du dessous est détouré et celui du dessus ne l'est pas ;
   - le calque du dessus sert de base à des calques détourés au-dessus de lui.
   
   Photoshop fusionne quand même ces cas, avec un résultat approché. On pourra assouplir plus tard, sur décision.
2. **Précision de la fusion** : le résultat est arrondi une fois de plus, à la profondeur du document. Mesuré : au plus 1 unité 16 bits dans un document 16 bits, et au plus 1 niveau dans un document 8 bits.
3. **Objets incorporés placés à l'origine** : aucune commande ne les déplace en J3. Un décalage non nul (document venu d'ailleurs) est refusé au rendu avec un message, en attendant la table des pages de 0021 et les transformations de J5.
4. **Emplacement du scénario** : `apps/cli/tests/scenario/` au lieu de `photo/render/tests/golden/`. Le banc des golden images de `photo/render` ne peut pas lire de fichiers (`photo/render` ne dépend pas de `photo/io`), alors que `apps/cli` dépend des deux. Les jobs GPU de la CI exécutent ces tests sur lavapipe, WARP et Metal.
5. **Images du scénario** : générées par `cargo run -p cygnus-photo-io --example fixtures -- apps/cli/tests/fixtures`. Elles sont déterministes et versionnées avec leur programme. `jpeg-encoder` n'est qu'une dépendance de développement.
6. **Fuzzing des décodeurs** : pas de cible `cargo-fuzz` en B6 (outil nocturne, plan accéléré). Les fichiers invalides, tronqués, corrompus et trop grands sont couverts par des tests ordinaires. Le fuzzing rejoint J6, comme le prévoit déjà la fin de 0027.
7. **Historique après refaire** (pour J4) : une entrée annulée puis rétablie garde son nom (`source.import`), mais sa commande devient l'inverse de l'inverse, une commande interne non sérialisable. Le chemin public n'est donc conservé que pour les entrées jamais annulées. La ressource « historique » de J4 devra garder la forme publique de chaque entrée.

## Dépendances ajoutées

| Crate | Licence | Rôle |
| --- | --- | --- |
| `zune-jpeg` 0.5, `zune-core` 0.5 | MIT OR Apache-2.0 OR Zlib | décodage JPEG (0027, Q8) |
| `sha2` 0.11 | MIT OR Apache-2.0 | identifiant des sources (0020) |
| `jpeg-encoder` 0.7 (développement) | MIT OR Apache-2.0 | génération des images de test |

`cargo deny check` passe.
