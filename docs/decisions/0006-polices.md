# 0006 — Polices de l'interface

- **Date** : 2026-10-30
- **Statut** : acceptée (validée par l'utilisateur le 2026-10-30)
- **Exigences concernées** : aucune (spec, sections 5 et 6 ; jalon J1)

## Contexte

Les polices intégrées à egui (crate `epaint_default_fonts`) comprennent Ubuntu-Light, sous licence Ubuntu Font Licence, que `deny.toml` refuse car elle n'est pas compatible avec la GPL v3. Il faut fournir nos propres polices, proches de la maquette (une sans-serif moderne et une police à chasse fixe pour les valeurs numériques).

Les candidates proches de la maquette sont sous SIL Open Font License 1.1 (OFL). L'OFL est une licence libre ; la FSF la juge incompatible avec la GPL pour du code, mais acceptable pour des polices livrées avec un logiciel GPL, l'OFL autorisant explicitement d'intégrer la police à un logiciel. Les alternatives sans ambiguïté (polices Apache-2.0, GNU FreeFont) ont un rendu plus éloigné de la maquette.

## Décision

- **Inter** 4.1 (Regular, SemiBold) pour le texte et les titres.
- **JetBrains Mono** 2.304 (Regular) pour les nombres et les valeurs.

Les deux sont sous OFL-1.1. Les fichiers TTF officiels et leurs licences sont versionnés dans `crates/ui-kit/assets/fonts/` (`*-LICENSE.txt`) et intégrés au binaire. Elles ne passent pas par une crate et ne sont donc pas vues par `cargo deny` : cette décision tient lieu de vérification. Les polices intégrées d'egui sont désactivées (`default-features = false` sur `egui` et `eframe`).

## Conséquences

- Toute distribution de Cygnus doit inclure les deux fichiers de licence OFL.
- Les noms réservés de l'OFL interdisent de distribuer une version modifiée des polices sous leur nom d'origine : les fichiers restent tels quels.
- Ajouter une police à l'interface exige une nouvelle décision.
