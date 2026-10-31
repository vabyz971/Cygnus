# 0004 — Versions d'egui, eframe et egui_dock

- **Date** : 2026-10-30
- **Statut** : acceptée
- **Exigences concernées** : aucune (spec, sections 6 et 11.2 ; jalon J1)

## Contexte

J1 introduit l'interface. `egui_dock` suit `egui` avec un temps de retard : au 2026-10-30, la dernière version d'`egui` et d'`eframe` est la 0.37.0, mais la dernière version d'`egui_dock` (0.21.1, août 2026) dépend de `egui ^0.36`. Mélanger deux versions d'`egui` est impossible (types incompatibles).

## Décision

Versions figées (`=`) dans `[workspace.dependencies]` :

| Crate | Version | Licence |
| --- | --- | --- |
| `egui` | 0.36.2 | MIT OR Apache-2.0 |
| `eframe` | 0.36.2 (wgpu 30) | MIT OR Apache-2.0 |
| `egui_dock` | 0.21.1, fonctionnalité `serde` | MIT |
| `egui-phosphor` | 0.14.0 (voir 0005) | MIT OR Apache-2.0 |

`eframe` est compilé sans ses fonctionnalités par défaut, avec : `wgpu` (moteur de rendu choisi dès maintenant pour J2), `accesskit` (lecteurs d'écran, spec section 5), `wayland`, `x11`. Les polices intégrées d'egui sont exclues (voir 0006). La persistance d'`eframe` n'est pas utilisée : la disposition a son propre fichier versionné (voir 0007).

`cargo deny` signale quatre dépendances indirectes « non maintenues », sans vulnérabilité connue. Elles sont ignorées nommément dans `deny.toml`, avec leur raison :

- `paste` (RUSTSEC-2024-0436), macro de compilation tirée par `egui_dock` ;
- `zvariant`, `zbus_names`, `zvariant_derive` (RUSTSEC-2026-0370, 0371, 0372), tirées par l'accessibilité d'`eframe` sous Linux ; ces crates ont été fusionnées dans `zbus`.

## Conséquences

- Passer à `egui` 0.37 attend une version d'`egui_dock` compatible. La montée de version se fait pour les quatre crates d'un coup, dans un seul commit, après vérification de leurs dépendances sur crates.io.
- À chaque montée de version, revoir les avis ignorés : les retirer dès qu'ils ne s'appliquent plus.
