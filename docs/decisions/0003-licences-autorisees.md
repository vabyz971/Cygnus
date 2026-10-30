# 0003 — Licences autorisées pour les dépendances

- **Date** : 2026-10-30
- **Statut** : acceptée
- **Exigences concernées** : aucune (spec, section 5 : licence)

## Contexte

Toute dépendance doit être compatible avec la GPL v3 (`AGENTS.md`, règle 7), vérifiée par `cargo deny`. Il faut une liste explicite.

## Décision

`deny.toml` n'autorise que : GPL-3.0 (only et or-later), LGPL-3.0 (only et or-later), LGPL-2.1-or-later, MPL-2.0, Apache-2.0 (avec ou sans LLVM-exception), MIT, MIT-0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, BSL-1.0, Unicode-3.0, Unlicense, CC0-1.0. Les licences GPL-2.0-only, LGPL-2.1-only et toute licence non libre sont refusées. Seul le registre crates.io est accepté comme source ; aucune dépendance git.

## Conséquences

- La liste est prévue à l'avance : les licences autorisées mais pas encore rencontrées ne déclenchent pas d'avertissement.
- Ajouter une licence à la liste exige une nouvelle décision datée.
