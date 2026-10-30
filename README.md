# Cygnus

Cygnus est une suite créative open source (GPL v3) écrite en Rust. Sa première application, **Cygnus Photo**, est un éditeur de retouche photo non destructif dont l'aperçu est strictement identique à l'export, utilisable sur Linux, Windows et macOS.

## État

Jalon J0 (fondations) : workspace Cargo, contrôles de qualité et intégration continue. Aucune fonctionnalité n'est encore disponible.

## Documents

- Cahier des charges : [`docs/spec.md`](docs/spec.md)
- Décisions d'architecture : [`docs/decisions/`](docs/decisions/)
- Règles pour les contributeurs et les agents : [`AGENTS.md`](AGENTS.md)

## Développement

```sh
nix develop            # sous NixOS ; sinon rustup lit rust-toolchain.toml
cargo build --workspace
cargo test --workspace
```

## Licence

GNU General Public License v3 ou ultérieure, voir [`LICENSE`](LICENSE).
