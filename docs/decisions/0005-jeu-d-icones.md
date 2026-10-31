# 0005 — Jeu d'icônes

- **Date** : 2026-10-30
- **Statut** : acceptée
- **Exigences concernées** : aucune (spec, section 6 ; jalon J1)

## Contexte

La maquette utilise des icônes au trait fin (rail d'outils, personas, panneaux). Il faut un jeu complet, sous licence compatible avec la GPL v3, utilisable dans `egui` sans rendu SVG.

## Décision

Icônes **Phosphor**, graisse « regular », via la crate `egui-phosphor` 0.14.0 (MIT OR Apache-2.0 ; la police Phosphor elle-même est sous licence MIT). La licence est vérifiée par `cargo deny`. Les glyphes sont réexportés par `cygnus_ui_kit::icons`.

Les icônes ont leur propre famille de police (`theme::icon_family()`), où Phosphor passe en premier, et s'affichent par `widgets::icon` ou `widgets::icon_label`. Raison : Inter possède des glyphes sur certains points de code de la zone privée utilisés par Phosphor, et Phosphor possède des glyphes pour certaines lettres. Aucune des deux polices ne peut donc passer en premier à la fois pour le texte et pour les icônes. Dans les familles de texte, Phosphor reste en dernier recours.

## Conséquences

- Une icône ne s'écrit jamais dans la même chaîne qu'un texte : elle passe par `widgets::icon`.
- Une icône absente de Phosphor demanderait une autre source, à consigner dans une nouvelle décision.
