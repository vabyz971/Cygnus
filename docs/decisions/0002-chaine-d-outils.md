# 0002 — Version de la chaîne d'outils Rust

- **Date** : 2026-10-30
- **Statut** : acceptée
- **Exigences concernées** : aucune (spec, section 5 : qualité du code, plateformes)

## Contexte

`AGENTS.md` demande une chaîne stable fixée dans `rust-toolchain.toml`. Le développement principal se fait sous NixOS, où le shell `nix develop` fournit `rustc` depuis nixpkgs et ignore `rust-toolchain.toml` (pas de rustup). La CI, elle, utilise rustup et lit ce fichier.

## Décision

La chaîne est fixée à la version exacte fournie par le nixpkgs verrouillé dans `flake.lock` : `1.99.0`, avec les composants `rustfmt` et `clippy`. `rust-version` du workspace vaut `1.99`.

## Conséquences

- Le poste local (Nix) et la CI (rustup) compilent avec la même version, donc les mêmes lints clippy.
- Monter de version = mettre à jour `flake.lock`, puis `rust-toolchain.toml`, `rust-version` et la version passée à `cargo-deny-action` dans la CI, dans le même commit.
