# 0022 — Historique complet (J3)

- **Date** : 2026-11-01
- **Statut** : acceptée avec conditions le 2026-11-01 (phase A du jalon J3) ; conditions intégrées ci-dessous
- **Exigences concernées** : HIS-1, HIS-2 (mécanisme), HIS-3 ; prolonge 0010

## Contexte

Depuis J2, chaque commande produit son inverse, et le bus garde un journal (commande, inverse, source, heure, groupe) borné en mémoire. Il manque annuler, refaire, et le regroupement des gestes.

## Proposition

- **Piles** dans `CommandBus` (`crates/commands`, générique) :
  - **annuler** applique l'inverse de la dernière entrée annulable ;
  - l'application d'un inverse rend à son tour l'inverse de l'inverse, c'est-à-dire la commande qui refait. Il est placé sur la **pile de refaire** ;
  - **refaire** l'applique de la même façon ;
  - une nouvelle commande **vide la pile de refaire**.

  Les identifiants de nœuds restent stables, comme testé en J2.
- **Annuler et refaire produisent des changements** dans le journal, comme toute commande : les vues et le cache des tuiles se mettent à jour par le même chemin (0010).
- **Gestes regroupés** (HIS-2 et glissements) : le `GroupId` existant devient un **geste** que l'interface ouvre et ferme : glissement d'un curseur d'opacité, déplacement d'un point de courbe, et plus tard un trait de pinceau (J5). Un geste forme **une seule entrée** de l'historique ; l'annuler l'annule en entier.
- **Fusion d'entrées consécutives** dans un geste : quand deux commandes du même geste visent la même propriété du même nœud (par exemple 60 `layer.update` d'opacité pendant un glissement), seule la dernière est gardée, avec l'inverse de la première. La mémoire ne grandit pas avec la durée du glissement.
- **Saturation de l'historique** (condition de validation, remplace le comportement de J2 où les entrées perdaient leurs données mais restaient listées) :
  - au-delà du budget mémoire (512 Mio par défaut) ou du nombre maximal d'entrées, **les entrées les plus anciennes sont supprimées en entier** de l'historique, jamais celles d'un geste en cours ;
  - le bus compte les suppressions et expose un **avertissement** (`HistoryTruncated { removed }`), lisible par chaque consommateur ;
  - l'interface le montre de façon visible, dans la barre d'état (« Historique saturé : les N plus anciennes étapes ont été retirées ») et en tête du panneau Historique ;
  - il n'y a plus d'entrée non annulable dans la liste : tout ce qui est listé s'annule.
- **Copies de tuiles** : les inverses de `pixels.*`, `mask.fill`, `mask.gradient` et `layer.merge_down` gardent les tuiles écrasées (partagées par `Arc`), comptées dans le budget. Rien ne change par rapport à J2.
- **Commandes `history.undo` et `history.redo`** : elles sont dans le catalogue, parce que la spec (section 8) les expose à l'agent. Le bus les exécute lui-même. Elles sont enregistrées avec leur source (utilisateur ou agent, HIS-1 et HIS-3), sans créer d'entrée annulable.
- **Panneau Historique** (HIS-3) : une ligne par entrée, avec le nom traduit de la commande ou du geste, l'heure et la source. Un clic annule ou refait jusqu'à ce point.

## Tests (propriétés)

- Pour toute suite de commandes valides : tout annuler redonne le document de départ (identifiants compris, compteur mis à part comme en J2), puis tout refaire redonne le document final.
- Annuler un geste annule toutes ses commandes ; une commande après une annulation vide la pile de refaire.
- Le budget mémoire est respecté pendant un long glissement.

## Alternatives écartées

- **Annuler par instantanés du document** : mémoire excessive avec les pixels.
- **Historique propre à l'interface** : l'agent et la ligne de commande doivent pouvoir annuler (spec 8, HIS-1).
