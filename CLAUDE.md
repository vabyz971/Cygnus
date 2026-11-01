# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

Les règles de `AGENTS.md` (importé ci-dessus) s'appliquent intégralement et ne sont pas répétées ici. Ce fichier ne contient que ce qui les complète.

## État actuel du dépôt

- Jalons livrés : J0 (fondations) et J1 (interface). J2 en cours (`docs/tasks/J2-document-rendu.md`, décisions 0009 à 0017) : bus de commandes, modèle et commandes Photo, rendu GPU par tuiles, `cygnus render` et vrai canevas (document de démonstration, `apps/photo/src/canvas.rs`) pyramide des tuiles composées avec cache au budget calculé d'après la fenêtre et la mémoire GPU (décision 0018) et banc d'essai de fluidité (décision 0019) faits. Pour J3, obligatoire : un budget de temps par image pour le calcul des tuiles (cibles de 0019 : interface jamais bloquée plus de 100 ms, aperçu réduit en moins de 33 ms pendant un glissement).
- Interface : `egui`/`eframe` (moteur wgpu) et `egui_dock`, versions figées ensemble (décision 0004 : `egui_dock` impose `egui` 0.36). Tout le thème (couleurs, tailles, polices) est dans `crates/ui-kit/src/theme.rs` ; les icônes Phosphor passent par `widgets::icon` (famille de police dédiée, décision 0005).
- Traductions : chaque texte affiché passe par une clé déclarée dans `apps/photo/src/keys.rs` (`define_keys!`) et traduite dans `apps/photo/i18n/fr.txt` ; un test échoue sur toute clé sans traduction ou traduction sans clé.
- Tout ce qui est factice (calques, courbe, histogramme, valeurs du pinceau) est dans `apps/photo/src/placeholder.rs`, à supprimer en J3.
- Disposition sauvegardée dans `~/.config/cygnus/photo/workspace.json` (décision 0007) ; incrémenter `LAYOUT_VERSION` si `Panel` change.
- Les paquets portent le préfixe `cygnus-` (`cygnus-photo-core`, `cygnus-cli`...) ; les dépendances internes sont déclarées dans `[workspace.dependencies]` du `Cargo.toml` racine (décision 0001).
- Maquette de l'écran principal : `docs/views/ecran-principal.png`.
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
cargo run --release -p cygnus-photo-render --example fluidity -- --out r.json   # banc sans fenêtre
CYGNUS_BENCH=fluidity cargo run --release -p cygnus-photo                        # banc dans l'application
cargo deny check                        # licences compatibles GPL v3
cargo run -p cygnus-cli -- render photo/render/tests/golden/normal-opacity.json /tmp/out.png
```

Tests GPU : sans adaptateur, ils sont sautés avec un message, sauf avec `CYGNUS_REQUIRE_GPU=1` (CI). `CYGNUS_ADAPTER=llvmpipe` choisit lavapipe ; sous NixOS, ajouter `VK_DRIVER_FILES=<mesa>/share/vulkan/icd.d/lvp_icd.x86_64.json`. Les golden images (`photo/render/tests/golden/`) se régénèrent uniquement sur lavapipe avec `CYGNUS_UPDATE_GOLDEN=1`, puis se relisent à l'œil ; l'instantané du catalogue de commandes avec `CYGNUS_UPDATE_SNAPSHOT=1`.

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
