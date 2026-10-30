# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

Les règles de `AGENTS.md` (importé ci-dessus) s'appliquent intégralement et ne sont pas répétées ici. Ce fichier ne contient que ce qui les complète.

## État actuel du dépôt

- Jalon J0 (`docs/tasks/J0-fondations.md`) : workspace Cargo de crates vides, lints, CI, vérification des en-têtes et des licences. Aucune logique métier, aucune dépendance `egui`/`egui_dock`/`wgpu` avant J1/J2.
- Les paquets portent le préfixe `cygnus-` (`cygnus-photo-core`, `cygnus-cli`...) ; les dépendances internes sont déclarées dans `[workspace.dependencies]` du `Cargo.toml` racine (décision 0001).
- Maquette de l'écran principal : `docs/views/Cygnus Photo — model.png`.
- Ancien code (lecture seule) : `../Cygnus-legacy` (avec majuscule sur ce poste).
- Les flakes ne voient que les fichiers suivis par git : un nouveau fichier doit être `git add` avant `nix build`.

## Commandes

Environnement : `nix develop` (chargé automatiquement par direnv via `.envrc`) fournit Rust, `cargo-deny`, les bibliothèques wgpu et Node.js. Node sert au serveur MCP `graft` déclaré dans `.mcp.json` (lancé par `npx`, version figée), qui indexe le code pour la navigation.

```sh
cargo build --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p <paquet> <nom_du_test>    # un seul test, ex. cargo test -p cygnus-cli binary_exits
scripts/check-headers.sh                # en-têtes GPL v3 dans chaque .rs
cargo deny check                        # licences compatibles GPL v3
```

La CI (GitHub Actions) exécute fmt, clippy, test et build sur Linux, Windows et macOS ; le script d'en-têtes et `cargo deny` sur Linux seulement. L'environnement principal de l'utilisateur est NixOS.

## Architecture (vue d'ensemble)

- **Flux des modifications** : l'interface (`apps/photo`), le serveur MCP (`crates/mcp-server`) et la ligne de commande (`apps/cli`) envoient des commandes typées et sérialisables au bus (`crates/commands`). Le bus applique, inscrit dans l'historique (source utilisateur ou agent) et notifie les vues. Les outils MCP et le schéma des commandes sont **générés** depuis ce catalogue, jamais écrits à la main.
- **Modèle** : un arbre de calques unique (`photo/core`), conçu pour être affiché plus tard aussi comme graphe de nœuds (hors bêta).
- **Rendu** : `photo/render` compile l'arbre en passes GPU wgpu (WGSL), canevas en tuiles avec cache par tuile invalidé par les commandes, calcul en f16 linéaire. Le canevas est dessiné par wgpu directement dans la fenêtre egui, hors de la boucle de widgets. Aperçu et export passent par le même chemin, vérifié par des tests d'images de référence.
- **Graphe des dépendances internes** (spec, section 7.4) :
  - `ui-kit`, `commands`, `file-format` : aucun crate du projet
  - `mcp-server` → `commands`
  - `photo/core` → `commands`, `file-format`
  - `photo/render`, `photo/io` → `photo/core`
  - `apps/photo` → `crates/*`, `photo/*`
  - `apps/cli` → `commands`, `photo/*`
- **Mode sans fenêtre** (`apps/cli`) : ouvrir un document, appliquer des commandes, rendre une image. C'est l'outil pour vérifier son travail sans interface.

## Langue

Échanges avec l'utilisateur et documents du dépôt en français ; code, commentaires et documentation du code en anglais.
