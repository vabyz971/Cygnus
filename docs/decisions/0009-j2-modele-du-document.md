# 0009 — Modèle du document (J2)

- **Date** : 2026-10-30
- **Statut** : acceptée avec conditions le 2026-10-30 (phase A du jalon J2) ; conditions intégrées ci-dessous
- **Exigences concernées** : CAL-1, CAL-2, DOC-1 (partiel) ; spec sections 7.1 et 7.2

## Contexte

J2 n'implémente que le calque pixel, mais le modèle doit accueillir sans refonte les groupes, masques, réglages et objets incorporés (J3). Il doit aussi pouvoir être affiché plus tard comme graphe, avec par exemple un masque partagé par deux réglages (spec 7.1). Il doit enfin se sérialiser (`serde`) pour le futur format `.cygnus` (J6), le serveur MCP (J4) et les tests.

## Proposition

**Structure en arène, pas en arbre de `Box`.** Le document possède une table de nœuds indexée par identifiant, et chaque nœud connaît son parent et la liste ordonnée de ses enfants :

```text
Document
  id, size (largeur, hauteur en px), resolution (ppp), profile (sRGB en J2),
  depth (U8 | U16), root: NodeId, nodes: Map<NodeId, Node>, next_id: u64
Node
  id: NodeId, parent: Option<NodeId>, name: String,
  visible: bool, opacity: f32 (0..=1), blend: BlendMode, locked: bool,
  mask: Option<NodeId>,            // réservé, toujours None en J2
  kind: NodeKind
NodeKind (non exhaustif)
  Group { children: Vec<NodeId> }  // seule la racine en J2
  Pixel { bounds: Rect }           // pixels stockés hors de l'arbre, voir 0011
  // J3 : Adjustment, Embedded, Mask
```

- **Identifiants stables** : `NodeId(u64)`, attribué par un compteur du document, jamais réutilisé et sérialisé avec le document. Un identifiant survit à l'enregistrement, à l'annulation et au réordonnancement. Les commandes, l'historique, le serveur MCP et la future vue graphe désignent les nœuds uniquement par leur identifiant, jamais par leur position.
- **Ordre** : l'ordre de dessin est celui de `children`, de bas en haut. Réordonner modifie seulement cette liste.
- **Pixels hors de l'arbre** : le modèle ne contient que la structure et les propriétés. Les pixels d'un calque sont dans une réserve de tuiles indexée par `NodeId` (0011). Cloner ou sérialiser l'arbre reste donc très peu coûteux, et l'annulation des modifications de pixels travaille tuile par tuile.
- **Projection en graphe** : l'arène, avec ses arêtes explicites (parent, enfants, `mask`), se projette directement en graphe. Un masque partagé sera un nœud `Mask` désigné par plusieurs `mask: Some(id)`. Le panneau Calques montrera alors un badge de lien.
- **Invariants vérifiés par le modèle** (et testés) : une seule racine, pas de cycle, un identifiant présent au plus une fois dans les enfants, `opacity` dans `0..=1`, un calque verrouillé refuse les modifications de propriétés et de pixels.
- **Sérialisation** : `serde` sur toutes les structures, avec un champ de version du modèle. Le format de fichier lui-même reste l'affaire de J6.
- **`BlendMode`** : énumération des douze modes de CAL-3, déclarée dès J2 pour stabiliser la sérialisation. Seul `Normal` est accepté par les commandes en J2 ; les autres renvoient une erreur « non pris en charge » jusqu'à J3.

## Alternatives écartées

- **Arbre récursif** (`Vec<Box<Node>>`) : les identifiants ne sont pas naturels, un masque partagé est impossible, et retrouver un nœud oblige à parcourir l'arbre. Il se projette mal en graphe.
- **Identifiants UUID** : inutilement longs à l'intérieur d'un document. Un `u64` local suffit, car les identifiants ne doivent être uniques que dans un document.
- **Pixels dans les nœuds** : chaque clonage de l'arbre (aperçu, annulation) copierait des centaines de mégaoctets.

## Risques

- Le passage de `mask` à de vrais nœuds masques partagés (J3) pourra demander d'affiner la règle de visibilité dans la liste. Le champ est réservé dès maintenant pour éviter une rupture de format.
