# 0032 — J4 : outils, ressources et contenu non fiable

- **Date** : 2026-11-03
- **Statut** : proposée (phase A du jalon J4)
- **Exigences concernées** : spec 2.2, 4.8 (HIS-1, HIS-3), 5, 7.2 ; décisions 0010, 0020, 0022, 0029

## Proposition 5 : outils

- **Génération** : un outil par commande du catalogue (`Catalog::commands()`), avec le nom, la description et le schéma JSON des paramètres déjà générés (0010). Aucun outil n'est écrit à la main ; un test compare la liste des outils au catalogue.
- **Noms** : ceux du catalogue (`layer.create`, `source.import`…), qui respectent les caractères permis par MCP. Ordre déterministe, celui du catalogue (recommandé par 2026-07-28).
- **Exclues par défaut** : les commandes `Development` (`pixels.fill`, `pixels.write`) et les commandes internes (absentes du catalogue par construction). Le mode sans fenêtre peut les inclure avec `--dev-tools`, pour les tests.
- **`history.undo` et `history.redo` (question Q2)** : je propose qu'un agent ne puisse annuler ou rétablir **que ses propres entrées**. Il est refusé si l'entrée en tête de pile vient de l'utilisateur (« la dernière action est celle de l'utilisateur »). L'utilisateur, lui, annule tout. Alternative : mêmes droits que l'utilisateur (règle « aucun pouvoir de plus », mais l'agent pourrait défaire le travail de l'utilisateur sans qu'il le voie).
- **Résultat d'un outil** : `structuredContent` avec la révision atteinte, les identifiants des nœuds créés et le résumé des changements (`PhotoChanges`). En cas de refus : erreur d'outil avec le message de `CoreError`, sans rien modifier.
- **Limites** : corps de requête à 1 Mio, chaînes à 4 096 caractères, noms de calques à 256 ; les autres bornes sont celles des commandes (validées par `photo/core`).
- **Exécution** : chaque appel passe par `resolve_and_execute` avec la source `Agent`. Il n'existe aucun autre chemin (0020).

## Proposition 6 : ressources

| URI | Contenu | Production |
| --- | --- | --- |
| `cygnus://document/tree` | arbre JSON : nœuds, genres, propriétés, réglages, sans pixels | lecture du document |
| `cygnus://document/preview?size=N` | aperçu PNG 8 bits, plus grand côté `N` ≤ 2048 (1024 par défaut) | rendu GPU, voir ci-dessous |
| `cygnus://document/histogram` | histogramme R, V, B, moyenne et écart type | passe d'histogramme (0025) |
| `cygnus://history` | entrées : nom, **forme publique** de la commande, source, heure, annulée ou non | bus, voir ci-dessous |
| `cygnus://schema/commands` | schéma du catalogue (`catalog_json`) | généré |

Chaque résultat porte `ttlMs` (0 pour ce qui change à chaque révision) et `cacheScope`, exigés par 2026-07-28.

**Aperçu sans bloquer l'interface** : la requête passe au fil de l'interface (0033), qui la confie au moteur de rendu de la fenêtre au début d'une image. Le rendu est réduit : seules les tuiles du niveau 0 sont composées, puis réduites par la pyramide du canevas, jamais une exportation pleine taille d'un document de 24 Mpx. Le résultat est relu de façon asynchrone, comme l'histogramme. Le fil du serveur attend le PNG au plus 10 s, puis renvoie une erreur. Sans fenêtre, le même rendu se fait sur le moteur de la session.

**Forme publique des actions annulées puis rétablies (demandé par l'utilisateur, 0029, point 7)** :

- aujourd'hui, une entrée rétablie garde son nom, mais sa commande devient l'inverse de l'inverse, une commande interne non sérialisable : le chemin d'un `source.import` disparaît ;
- proposition : `Entry` du bus garde un champ **`public`**, la commande telle qu'elle a été émise (forme sérialisable, résultat résolu exclu), fixé une fois à l'application et **jamais remplacé** par l'annulation ni le rétablissement. Le bus continue de rejouer `command` et `inverse` comme aujourd'hui ;
- quand des commandes fusionnent dans un geste, `public` prend la dernière commande, comme `command` ;
- la mémoire de `public` est comptée dans le budget de l'historique. Les commandes à grosse charge (`pixels.write`) sont des outils de développement, exclus par défaut ;
- la ressource `cygnus://history` et le panneau Historique lisent `public`. Un test vérifie qu'après annuler puis rétablir, le chemin d'import est toujours lisible.

## Proposition 7 : contenu non fiable

Les noms de calques, les noms de fichiers sources, les métadonnées (J6) et tout texte du document sont des **données saisies par quelqu'un d'autre que l'agent**, jamais des instructions :

- ils ne figurent que dans des **champs JSON typés** des ressources (`"name": "…"`), jamais dans les descriptions d'outils ni dans les messages d'erreur sous forme de texte libre. Une erreur cite un nœud par son identifiant ;
- les caractères de contrôle sont retirés et la longueur est bornée (256 caractères) à la création et à la lecture ;
- chaque ressource qui contient du texte du document porte un champ `"untrusted": ["name", …]` qui liste les champs non fiables. Les instructions du serveur (texte de `server/discover`) rappellent que ces champs sont des données ;
- les actions à conséquence hors du document (disque) restent soumises aux confirmations (0031), quoi que dise un nom de calque.

Limite connue : rien n'empêche un modèle de suivre une instruction cachée dans un nom de calque. Le serveur ne peut que l'étiqueter, et la protection réelle reste la confirmation des accès disque et l'annulation de tout le reste.

## Risques

- **Q2** : droits d'annulation de l'agent.
- **Aperçu** : un rendu demandé pendant un geste de l'utilisateur ralentit l'image en cours (borné par la taille maximale).
- **Modification du bus** (`public`) : touche `crates/commands`, utilisé par toutes les entrées. Le changement est additif, et les tests de l'historique (0022) doivent rester verts.
