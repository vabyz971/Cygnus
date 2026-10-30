# Consigne : jalon J0, Fondations

À placer dans `docs/tasks/J0-fondations.md` du dépôt `cygnus`, puis à donner à l'agent avec cette phrase : « Lis `AGENTS.md` et `docs/spec.md`, puis réalise la consigne `docs/tasks/J0-fondations.md`. »

## Contexte

Le dépôt `cygnus` est neuf. Il contient déjà `docs/spec.md` (cahier des charges). Ce jalon pose uniquement les fondations : l'arborescence, les contrôles de qualité et l'intégration continue. **Aucune logique métier.**

Sections de `docs/spec.md` concernées : 5 (qualité du code, licence, plateformes), 7.4 (organisation du dépôt), 9 (workflow), 10 (jalon J0).

## À livrer

1. **Workspace Cargo** à la racine, avec ces membres et rien d'autre :
   `crates/ui-kit`, `crates/commands`, `crates/mcp-server`, `crates/file-format`, `photo/core`, `photo/render`, `photo/io`, `apps/photo`, `apps/cli`.
   - Chaque crate compile et ne contient que l'en-tête de licence et un commentaire de documentation (en anglais) décrivant son rôle, d'après la table de la section 7.4.
   - Les dépendances internes (par chemin) suivent exactement la colonne « Dépend de » de la section 7.4.
   - Aucune dépendance externe dans le code des crates.
2. **Binaires** :
   - `apps/cli` : un `main` qui se termine avec succès sans rien faire, plus un test d'intégration qui vérifie que le binaire s'exécute et retourne le code 0.
   - `apps/photo` : un `main` qui se termine avec succès sans rien faire. La fenêtre arrive au jalon J1.
3. **Lints et qualité** :
   - `[workspace.lints]` dans le `Cargo.toml` racine, hérités par chaque crate, avec `clippy::unwrap_used` et `clippy::expect_used` en `deny`.
   - `clippy.toml` autorisant `unwrap` et `expect` dans les tests uniquement.
   - `rust-toolchain.toml` (édition 2024, chaîne stable) et `rustfmt.toml` minimal.
4. **Vérification des en-têtes** : un script `scripts/check-headers.sh` qui échoue si un fichier `.rs` n'a pas l'en-tête de licence de `AGENTS.md`.
5. **Vérification des licences** : `deny.toml` pour `cargo deny`, n'autorisant que des licences compatibles avec la GPL v3.
6. **Intégration continue** (GitHub Actions) sur **Linux, Windows et macOS** : `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --workspace`. Sur Linux seulement : le script d'en-têtes et `cargo deny check`.
7. **Fichiers du dépôt** : `README.md` court (but du projet, état, lien vers `docs/spec.md`), `LICENSE` (texte de la GPL v3), `.gitignore` (Rust), `.editorconfig`, `docs/decisions/0000-modele.md` (modèle de décision : contexte, décision, conséquences), `docs/views/.gitkeep`.
8. **Optionnel** : un `flake.nix` avec un shell de développement (NixOS est l'environnement principal de l'utilisateur). Ne pas le faire si cela retarde le reste.

## Interdit dans ce jalon

- Toute logique métier, toute structure de données du document, tout rendu.
- Les dépendances `egui`, `egui_dock` et `wgpu` : elles arrivent en J1 et J2.
- Copier du code de `cygnus-legacy`.
- Créer des crates qui ne figurent pas dans la liste ci-dessus.

## Critère de sortie

- L'intégration continue est **verte sur Linux, Windows et macOS**.
- En local, ces commandes réussissent : `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --workspace`.
- `scripts/check-headers.sh` et `cargo deny check` réussissent.

## En fin de tâche

Terminer par un compte rendu court : ce qui a été fait, les choix que tu as dû faire seul (à consigner dans `docs/decisions/`), et tout point ambigu de la spec rencontré en route.
