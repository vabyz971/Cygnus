# AGENTS.md

Ce fichier est lu par chaque agent IA au début de chaque session. Il fixe les règles du dépôt `cygnus`. En cas de doute entre ce fichier et le reste, ce fichier et `docs/spec.md` priment.

## 1. Le projet

Cygnus est une suite créative open source (GPL v3) écrite en Rust. La première application est **Cygnus Photo**, un éditeur de retouche photo non destructif dont l'aperçu est strictement identique à l'export.

- Source de vérité des exigences : `docs/spec.md` (cahier des charges). Chaque exigence a un identifiant (DOC-1, CAL-2, PIN-1, etc.).
- Décisions d'architecture : `docs/decisions/`, un fichier par décision, daté.
- Fiches de design des vues d'interface : `docs/views/`.
- Ancien code (lecture seule) : dépôt `cygnus-legacy`, cloné dans un dossier voisin (`../cygnus-legacy`), jamais à l'intérieur de ce dépôt.

## 2. Règles non négociables

1. **Un seul pipeline de rendu GPU** (wgpu). Aucun chemin CPU de secours pour l'aperçu. Les formules de fusion des calques n'existent qu'en **un seul exemplaire**, en WGSL. Toute exception exige une décision datée dans `docs/decisions/`.
2. **Un modèle, deux vues.** Le document est un arbre de calques unique. Toute modification passe par une **commande** typée et sérialisable envoyée au bus de commandes ; l'interface, le serveur MCP et la ligne de commande n'ont aucun autre moyen de modifier le document.
3. **Sens des dépendances** : `apps` dépend de `photo`, et tout dépend de `crates`, jamais l'inverse. `crates/ui-kit` ne connaît ni les calques ni les pixels.
4. **Aucun `unwrap()` ni `expect()` hors des tests.** Les erreurs se propagent avec `Result` et s'affichent clairement.
5. **Périmètre.** Ne pas ajouter de fonctionnalité absente de la colonne « Dans la bêta » de la section 3 de `docs/spec.md`. Une idée hors périmètre se note dans `docs/decisions/` comme proposition, elle ne se code pas.
6. **Code de l'ancien dépôt** : consultable pour comprendre, jamais copié tel quel. On réécrit. Les anciennes formules de fusion servent uniquement de référence pour les tests.
7. **Dépendances** : toute nouvelle dépendance doit avoir une licence compatible avec la GPL v3 (vérifiée par `cargo deny`) et être justifiée dans le message de commit. Les versions d'`egui`, d'`egui_dock` et de `wgpu` doivent être vérifiées compatibles entre elles avant d'être figées.

## 3. Conventions de code

- Édition Rust 2024, chaîne d'outils stable fixée dans `rust-toolchain.toml`.
- **Commentaires et documentation du code en anglais.** Documents du dépôt (`docs/`, README) en français. Échanges avec l'utilisateur en français.
- **En-tête de licence GPL v3 dans chaque fichier source** (voir modèle ci-dessous).
- Pas d'emoji, ni dans le code, ni dans les commentaires, ni dans les messages de commit.
- Commits courts, un sujet par commit, préfixés par domaine : `core:`, `render:`, `io:`, `ui:`, `mcp:`, `cli:`, `ci:`, `docs:`. Le message mentionne les identifiants d'exigences traités (par exemple `core: add layer opacity (CAL-2)`).
- Les textes visibles par l'utilisateur ne sont jamais écrits en dur dans le code : ils passent par le système de traductions (le français est la première langue).

Modèle d'en-tête (remplacer l'année et le titulaire) :

```rust
// This file is part of Cygnus.
// Copyright (C) <year> <copyright holder>
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.
```

## 4. Structure du dépôt

```text
cygnus/
  crates/            code partagé, indépendant de l'image
    ui-kit/          thème, panneaux ancrables, dialogues, préférences
    commands/        bus de commandes, historique, génération des outils MCP
    mcp-server/      serveur MCP générique
    file-format/     conteneur .cygnus (archive, manifeste, versions)
  photo/             tout ce qui est propre à la retouche photo
    core/  render/  io/
  apps/
    photo/           application Photo
    cli/             mode sans fenêtre
  docs/              spec.md, decisions/, views/
```

**Règle d'extraction.** Pendant la bêta, seul le code déjà indépendant de l'image va dans `crates/`. Quand une deuxième application a besoin d'un composant, on l'extrait vers `crates/`, avec un test et une décision datée.

## 5. Boucle de travail pour chaque tâche

1. Lire la consigne de la tâche et les exigences citées dans `docs/spec.md`.
2. Si la consigne est ambiguë ou contredit la spec, **poser la question avant de coder**.
3. Écrire les tests d'abord, puis le code.
4. Avant tout commit, ces trois commandes doivent réussir :
   - `cargo fmt --all --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`
5. Si le rendu change, les tests d'images de référence (golden images) doivent aussi passer.
6. Livrer en petits commits et terminer par un court compte rendu : ce qui est fait, ce qui reste, ce qui était ambigu et comment tu as tranché.

## 6. Zones à supervision étroite

Pour ces zones, **proposer d'abord** (dans la conversation ou dans un fichier de `docs/decisions/`) et attendre la validation de l'utilisateur avant d'implémenter :

- Passes de rendu GPU et formats de pixels.
- Format de fichier `.cygnus` et compatibilité entre versions.
- Sécurité du serveur MCP (activation, jeton, confirmations).
- Modèle du document et catalogue de commandes.

Les dialogues, panneaux, préférences et raccourcis, à partir des fiches de `docs/views/`, peuvent être traités plus librement.

## 7. Décisions d'architecture

Une décision = un fichier `docs/decisions/NNNN-titre-court.md`, numéroté et daté, avec : contexte, décision, conséquences. Copier `docs/decisions/0000-modele.md`.
