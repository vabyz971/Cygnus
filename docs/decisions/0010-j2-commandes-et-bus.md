# 0010 — Commandes et bus de commandes (J2)

- **Date** : 2026-10-30
- **Statut** : acceptée avec conditions le 2026-10-30 (phase A du jalon J2) ; conditions intégrées ci-dessous
- **Exigences concernées** : HIS-1, HIS-3 (préparation) ; spec sections 7.2 et 8

## Contexte

Toute modification du document passe par une commande typée et sérialisable (`AGENTS.md`, règle 2). Le catalogue des commandes doit produire automatiquement les outils MCP (J4) et servir à l'interface, à la ligne de commande et aux tests. L'historique complet arrive en J3 ; J2 ne doit pas le bloquer.

## Proposition

**Partie générique (`crates/commands`, sans connaître l'image).**

- Trait `Catalog` implémenté par l'énumération des commandes d'une application. Il fournit le nom stable de chaque commande, son application au document cible et le schéma JSON de ses paramètres.
- `apply(self, &mut Target) -> Result<Applied, CommandError>`, où `Applied` contient :
  - **l'inverse**, sous forme de commande du même catalogue, ou d'enregistrement de restauration pour les pixels (anciennes tuiles) ;
  - **les changements** (`Changes`) : nœuds touchés, propriétés modifiées, rectangles de pixels salis.
- `CommandBus<C: Catalog>` : applique la commande, ajoute à un journal une entrée `{ commande, inverse, source, horodatage, groupe }`, incrémente une révision et renvoie les `Changes`.
- **Source** : `User | Agent | Cli | Test`, renseignée par l'appelant, pour HIS-3.
- **Groupe** : identifiant de transaction facultatif, pour qu'un trait de pinceau compte pour une seule entrée (HIS-2, J5).
- **Notification des vues : plusieurs consommateurs indépendants.** Le bus tient un **journal des changements** horodaté par révision croissante. Chaque consommateur garde **son propre curseur** et lit `changes_since(curseur)`, qui renvoie l'union des `Changes` depuis cette révision et la nouvelle révision. Les consommateurs sont par exemple le cache de rendu, le panneau Calques, l'enregistrement automatique, ou les notifications MCP de J4. Lire ne consomme rien : un consommateur n'efface jamais ce qu'un autre n'a pas encore lu, et l'ordre de lecture est indifférent. Le journal est borné en nombre d'entrées. Un consommateur dont le curseur est sorti de la fenêtre reçoit `Changes::Everything` (tout relire), jamais un résultat incomplet. Il n'y a ni abonnés ni rappels : egui redessine chaque image, et chaque consommateur lit quand il en a besoin. Un test vérifie que deux consommateurs lisant à des rythmes différents voient les mêmes changements.
- **Erreurs** : `CommandError` typé (nœud inconnu, calque verrouillé, paramètre hors bornes, mode non pris en charge). Une commande refusée ne modifie rien.

**Partie Photo (`photo/core`).** Énumération `PhotoCommand`, sérialisée en `{"command": "layer.create", "params": {...}}`. Les noms suivent la table de la spec, section 8. Commandes de J2 :

| Commande | Effet |
| --- | --- |
| `document.create` | Dimensions, résolution, profondeur 8/16 bits, profil (sRGB seulement en J2) |
| `layer.create` | Nouveau calque pixel vide, à une position donnée |
| `layer.update` | Nom, visibilité, opacité, mode, verrouillage (paramètres facultatifs) |
| `layer.delete` | Suppression ; l'inverse restaure le nœud et ses tuiles |
| `layer.move` | Nouveau parent et position |
| `pixels.fill` | Remplit un rectangle d'une couleur (base de l'outil Remplissage) |
| `pixels.write` | Écrit un bloc de pixels (base du pinceau en J5 et de l'import en J6) |

Les deux commandes `pixels.*` sont nécessaires en J2 : sans import ni pinceau, c'est le seul moyen de mettre du contenu dans un calque pixel, pour la démonstration, les golden images et `cygnus render`. **Validé (Q6), avec deux conditions** :

- elles sont **marquées « développement »** dans le catalogue (`CommandInfo::stability = Development`). Le marquage figure dans le schéma généré, et J4 décidera de leur exposition (exclues par défaut). Elles seront remplacées ou stabilisées avec le pinceau (J5) et l'import (J6) ;
- leur **annulation se fait par copies des tuiles écrasées**, partagées par `Arc`, **dans un budget mémoire borné** (512 Mio par défaut, réglable). Au-delà du budget, les inverses les plus anciens sont abandonnés : ces entrées deviennent non annulables, et le journal le signale. La mémoire ne grandit jamais sans limite. Un test vérifie le respect du budget.

**Schéma pour le serveur MCP.** Les structures de paramètres dérivent `schemars::JsonSchema`. Le catalogue expose `schema()`, qui produit la liste `{nom, description, schéma des paramètres}`. J4 n'aura qu'à la publier. Un test fige ce schéma (instantané) pour détecter tout changement involontaire de l'interface publique.

**Ce que J2 prépare pour l'historique (J3), sans l'implémenter** : chaque commande produit son inverse, testée systématiquement (appliquer puis inverser donne un document identique). Le journal conserve source, horodatage et groupe. Annuler et refaire, ainsi que le panneau Historique, arrivent en J3.

## Alternatives écartées

- **Trait objet par commande** (`Box<dyn Command>`) : la sérialisation et le schéma deviennent difficiles, et le catalogue fermé que demande J4 disparaît.
- **Annulation par instantanés complets du document** : simple, mais beaucoup trop coûteuse en mémoire avec les pixels. Les inverses ciblés et le partage des tuiles par `Arc` sont bien plus économes.
- **Observateurs ou rappels** : ils compliquent la propriété des données en Rust et ne servent à rien en mode immédiat.

## Risques

- Fournir un inverse exact pour chaque commande demande de la discipline. Le test « appliquer puis inverser » sur chaque commande du catalogue est la parade.
