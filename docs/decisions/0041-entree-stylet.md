# 0041 — Entrée stylet derrière une interface de `ui-kit`

- **Date** : 2026-11-19
- **Statut** : proposée (examen demandé par l'utilisateur pour Q1 de 0039 ; mise en œuvre à l'étape P8, après validation)
- **Exigences concernées** : PIN-2 ; décisions 0039 (proposition 8), 0035

## Contexte

Avec les versions figées (egui et egui-winit 0.36.2, winit 0.30.13), la pression n'arrive que sous Windows, et l'inclinaison nulle part (0039). L'utilisateur a demandé :

- d'isoler l'entrée stylet derrière une interface de `ui-kit`, destinée à être remplacée par winit 0.31 ;
- de traiter d'abord Wayland, puis Windows si le coût le permet ;
- de noter macOS et X11 comme limites ;
- et d'examiner d'abord comment les bibliothèques existantes contournent winit 0.30.

## Bibliothèques examinées (2026-11-19)

| Bibliothèque | Plateformes | Licence | Version publiée | Activité du dépôt | Remarques |
| --- | --- | --- | --- | --- | --- |
| [octotablet](https://github.com/Fuzzyzilla/octotablet) | Wayland (tablet-v2), Windows (Ink, RealTimeStylus) ; X11 prévu, macOS non | MIT | 0.1.0 (mars 2024), 6 785 téléchargements | derniers commits en octobre et novembre 2025 (« fix hanging issue on wayland »), non publiés ; 40 étoiles, 10 tickets ouverts | se branche par `raw-window-handle` 0.6, que fournissent eframe et winit 0.30 ; dépendances : wayland-client 0.31 (déjà dans l'arbre, par winit), `windows` 0.54 (une version de plus) |
| [wintab_lite](https://github.com/thehappycheese/wintab_lite) | Windows (Wintab) | MIT d'après crates.io, sans fichier de licence dans le dépôt | 1.0.1 (avril 2024) | dernier changement en août 2024 | liaison minimale ; écrite par son auteur après de mauvais résultats avec Windows Ink |
| [easytab](https://github.com/DexterHill0/easytab-rs) | Windows | MIT | 0.3.0 (juillet 2026) | 68 téléchargements | très jeune |

Point utile pour le poste de l'utilisateur : d'après un ticket de Wine, Sway, KDE et GNOME remettent la pression à zéro quand le stylet se lève, mais pas Hyprland ni niri, le compositeur de l'utilisateur. Il faudra forcer la pression à zéro à la levée.

## Proposition

1. **Interface dans `ui-kit`**, sans rien qui touche à l'image (AGENTS.md, règle 3) :
   - un trait `StylusSource`, qui donne les échantillons du stylet reçus depuis la dernière image : position dans la fenêtre, pression, inclinaison (`Option`), proximité, contact, horodatage ;
   - les outils de `apps/photo` le lisent, et retombent sur le pointeur d'egui (pression 1) quand il ne donne rien ;
   - le jour où egui suit winit 0.31, une seule implémentation fondée sur les événements de winit remplace les autres.
2. **Wayland d'abord**, par octotablet :
   - sous forme d'une dépendance git figée sur un commit, pour avoir le correctif de blocage d'octobre 2025, ou d'une version publiée si l'auteur en publie une ;
   - à vérifier : que `cargo deny` accepte la source git, et que wayland-client 0.31 ne duplique pas la version de winit.
3. **Windows ensuite**, par le même octotablet (Windows Ink), si l'essai sur matériel est concluant. Sinon, wintab_lite.
4. **macOS et X11** : pas de pression en bêta. La limite est notée au critère de sortie.
5. **Tests sans matériel** : l'interface reçoit des échantillons synthétiques, et l'implémentation Wayland est testée à la main par l'utilisateur.

## Risques

- **Maintenance d'octotablet** : un seul mainteneur, aucune version publiée depuis 2024. L'interface de `ui-kit` limite le coût d'un remplacement.
- **Deux sources d'événements**, egui et octotablet, à recaler dans le temps et dans l'espace : coordonnées de la fenêtre, facteur d'échelle.
