# 0007 — Emplacement des fichiers de configuration

- **Date** : 2026-10-30
- **Statut** : acceptée
- **Exigences concernées** : aucune (spec, section 6 : workspaces ; jalon J1)

## Contexte

La disposition des panneaux (workspace) est sauvegardée à la fermeture et restaurée au démarrage. Il faut un emplacement conforme aux conventions de chaque système, et commun aux futures applications de la suite.

## Décision

La crate `directories` 6 (MIT OR Apache-2.0) fournit le dossier, via `ProjectDirs::from("org", "Cygnus", "Cygnus")`, avec un sous-dossier par application (`cygnus_ui_kit::config::app_config_dir`) :

| Système | Dossier de Cygnus Photo |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/cygnus/photo/` (en général `~/.config/cygnus/photo/`) |
| macOS | `~/Library/Application Support/org.Cygnus.Cygnus/photo/` |
| Windows | `%APPDATA%\Cygnus\Cygnus\config\photo\` |

La disposition y est écrite dans `workspace.json` : `{ "version": N, "layout": ... }`. L'écriture passe par un fichier temporaire renommé ensuite, pour ne jamais laisser un fichier tronqué. Au démarrage, un fichier absent, illisible, d'une autre version ou incohérent (panneau inconnu ou en double, largeur invalide) donne la disposition par défaut, avec un message sur la sortie d'erreur et sans plantage.

## Conséquences

- Une modification du format de la disposition (par exemple un panneau renommé) exige d'incrémenter `LAYOUT_VERSION` dans `apps/photo/src/panels.rs`. Il n'y a pas de migration : un ancien fichier donne la disposition par défaut.
- Les futures préférences iront dans le même dossier, dans leurs propres fichiers versionnés.
