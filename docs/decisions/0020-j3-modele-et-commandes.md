# 0020 — Modèle et commandes de J3 : groupes, masques, réglages, objets incorporés, filtres

- **Date** : 2026-11-01
- **Statut** : acceptée avec conditions le 2026-11-01 (phase A du jalon J3) ; conditions intégrées ci-dessous
- **Exigences concernées** : CAL-1 à CAL-7, MAS-1, REG-1, REG-3, DOC-2 ; spec 7.1 et 7.2 ; prolonge 0009 et 0010

## Contexte

J2 a posé l'arène de nœuds (0009) avec `Group` (racine seule) et `Pixel`, un champ `mask` réservé et l'énumération complète des modes de fusion. J3 remplit ces cases sans refonte et étend le catalogue de commandes (0010).

## Proposition : types de nœuds

```text
NodeKind
  Group      { children, isolation: Isolated | PassThrough }
  Pixel      { bounds }
  Adjustment { adjustment: Adjustment }
  Filter     { filter: Filter }
  Embedded   { source: SourceId, placement: Placement }
  Mask       { bounds, density: f32, inverted: bool, enabled: bool }
Node (champs communs, inchangés) + clip: bool (CAL-5)
```

- **Groupes** : `Isolated` compose ses enfants à part, puis fusionne le résultat avec son mode et son opacité. `PassThrough` (« passant ») laisse ses enfants agir directement sur ce qui est dessous ; seuls son opacité et son masque s'appliquent, par mélange avec le fond d'avant le groupe (sémantique de Photoshop). **Valeur par défaut décidée : `PassThrough`** (Q2, comme Photoshop ; l'utilisateur vérifie le comportement d'Affinity).
- **Masques** : un masque est un nœud `Mask` en niveaux de gris, un canal à la profondeur du document, stocké en tuiles dans la même `PixelStore` (tuiles à 1 canal). Il est désigné par `mask: Some(id)` sur un calque, un groupe, un réglage, un filtre ou un objet incorporé. Son parent est le nœud qui le porte, et il n'apparaît pas dans `children`. Une seule référence par masque en J3 ; le masque partagé du graphe (7.1) reste possible plus tard, sans changer le format.
  - **Densité** (MAS-1, REG-2) : couverture effective = `1 − densité × (1 − m)`. À densité 0, le masque est sans effet.
  - **Inversion** : `m' = 1 − m`.
- **Réglages** (REG-1, paramètres validés par les commandes) :
  - `Levels` : pour le canal maître et R, V, B, `input_black`, `input_white`, `gamma`, `output_black` et `output_white` ;
  - `Curves` : courbes maître, R, V et B, de 2 à 16 points chacune, abscisses strictement croissantes dans `0..=1`, interpolation cubique monotone (REG-3) ;
  - `HueSaturation` : teinte de −180 à 180, saturation et luminosité de −100 à 100 ;
  - `BrightnessContrast` : luminosité et contraste de −100 à 100 ;
  - `ColorBalance` : ombres, tons moyens et hautes lumières, chacun avec trois axes cyan-rouge, magenta-vert et jaune-bleu de −100 à 100, plus « préserver la luminosité ».

  L'**intensité** (REG-2) est l'opacité du nœud ; la densité est celle de son masque.
- **Filtres** (non destructifs, comme des calques de réglage) : `GaussianBlur { radius }` et `UnsharpMask { radius, amount, threshold }` agissent sur ce qui est dessous dans leur groupe. Pour filtrer un seul calque, on le met dans un groupe avec le filtre. Leur rendu est décrit dans 0023.
- **Objets incorporés** (DOC-2) : la source est stockée une fois dans `Document.sources: Map<SourceId, Source>`, avec largeur, hauteur, profondeur, tuiles RGBA, nom d'origine et empreinte du contenu. **Elle n'est jamais modifiée.** `Placement` contient un décalage entier en J3 ; le champ prévoit la transformation affine de J5 (TRA-2, TRA-3) sans changement de format.
- **Détourage** (CAL-5) : `clip: true` sur un nœud le détoure sur le premier nœud non détouré situé dessous dans le même groupe.

## Proposition : catalogue de commandes de J3

| Commande | Effet | Inverse |
| --- | --- | --- |
| `layer.create` | Ajoute un calque pixel, un groupe, un réglage ou un filtre (le genre et ses paramètres sont dans les paramètres) | `layer.delete` |
| `layer.duplicate` | Copie un nœud et son sous-arbre ; les tuiles sont partagées par `Arc` | `layer.delete` |
| `layer.delete` | Tout genre de nœud, sous-arbre et masque compris | restauration du sous-arbre (interne) |
| `layer.update` | Nom, visibilité, opacité, mode, verrouillage, détourage, isolation d'un groupe | `layer.update` |
| `layer.move` | Réordonne et change de parent (glisser-déposer, CAL-4) | `layer.move` |
| `layer.group` / `layer.ungroup` | Regroupe des nœuds voisins, défait un groupe | l'opération opposée |
| `layer.merge_down` | Fusionne un calque avec celui du dessous en un calque pixel | restauration (interne) |
| `adjustment.update` | Paramètres d'un réglage | `adjustment.update` |
| `filter.update` | Paramètres d'un filtre | `filter.update` |
| `mask.create` | Masque blanc ou noir sur un nœud | `mask.delete` |
| `mask.delete` | Supprime un masque | restauration (interne) |
| `mask.update` | Densité, inversion, activation | `mask.update` |
| `mask.fill` | Remplit un rectangle d'une valeur de gris | restauration de tuiles (interne) |
| `mask.gradient` | Dégradé linéaire entre deux points (MAS-1), décidé en J3 (Q4) | restauration de tuiles (interne) |
| `source.import` | Crée un objet incorporé depuis un fichier PNG ou JPEG | `layer.delete` et retrait de la source |
| `history.undo` / `history.redo` | Voir 0022 | — |

**Commandes résolues** (risque n° 3, condition de validation : **un seul chemin**). Deux commandes ont besoin de plus que le modèle pour s'appliquer : `layer.merge_down` doit **rendre** les deux calques sur GPU, et `source.import` doit **lire un fichier**. `photo/core` ne peut dépendre ni du rendu ni du disque (7.4). Proposition :

- ces commandes restent publiques dans le catalogue, donc dans le schéma MCP de J4 ;
- un **résolveur** placé au-dessus du bus (dans `photo/render` pour la fusion, dans `photo/io` pour l'import) les transforme, avant leur application, en une commande interne qui porte déjà les pixels obtenus ;
- le journal enregistre la commande publique (avec sa source et son heure) et l'inverse de la commande interne ;
- **un seul chemin de résolution** : une seule fonction, `resolve_and_execute`, placée dans un module de session commun aux trois entrées. L'interface, la ligne de commande et le serveur MCP n'ont aucun autre moyen d'exécuter ces commandes ; un test vérifie que les trois entrées appellent cette fonction ;
- **image importée stockée dans le document, empreinte seule dans l'historique** :
  - `source.import` décode le fichier puis insère la source dans `Document.sources`, indexée par son empreinte (SHA-256 du contenu décodé) ;
  - le journal ne garde que la commande publique (chemin), l'empreinte et l'identifiant du calque créé, jamais les pixels ;
  - annuler retire le calque mais laisse la source dans le document. Refaire recrée le calque en désignant la source par son empreinte, sans relire le fichier ;
  - une source qui n'est plus désignée par aucun calque est écartée à l'enregistrement (J6).

**`source.import` et le disque.** Le chemin est relatif au script ou absolu. **Décidé (Q3)** : pour l'agent, la lecture d'un fichier demande une confirmation, ou se limite à un dossier autorisé, comme l'écriture (spec section 8). Sa mise en œuvre arrive avec le serveur MCP (J4).

## Alternatives écartées

- **Masque comme canal alpha supplémentaire de chaque calque** : il ne s'applique ni aux groupes ni aux réglages sans cas particulier, et ne se partage pas plus tard dans le graphe.
- **Filtres destructifs** (appliqués aux pixels) : contraires à la spec 2.1 (« tout filtre reste paramétrable »).
- **Faire lire les fichiers ou rendre directement par `photo/core`** : inverse le sens des dépendances.
- **Objet incorporé transformé dès J3** : la transformation et son rééchantillonnage de qualité relèvent de TRA-2 et TRA-3 (J5).

## Risques

- La fusion vers le bas d'un calque qui a un masque, un détourage ou un mode non Normal doit donner exactement ce que l'on voyait. Elle est rendue par le même pipeline que l'aperçu, puis vérifiée par une golden image.
