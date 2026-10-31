# 0001 — Noms des crates du workspace

- **Date** : 2026-10-30
- **Statut** : acceptée
- **Exigences concernées** : aucune (spec, section 7.4)

## Contexte

La section 7.4 de la spec fixe l'arborescence (`crates/ui-kit`, `photo/core`, `apps/cli`, etc.) mais précise que les noms sont indicatifs. Les noms de dossiers seuls (`core`, `io`, `photo`) sont trop génériques pour des noms de paquets Cargo et entreraient en collision entre applications.

## Décision

Chaque crate porte le préfixe `cygnus-`, suivi du domaine pour les crates propres à une application :

| Dossier | Paquet |
| --- | --- |
| `crates/ui-kit` | `cygnus-ui-kit` |
| `crates/commands` | `cygnus-commands` |
| `crates/mcp-server` | `cygnus-mcp-server` |
| `crates/file-format` | `cygnus-file-format` |
| `photo/core` | `cygnus-photo-core` |
| `photo/render` | `cygnus-photo-render` |
| `photo/io` | `cygnus-photo-io` |
| `apps/photo` | `cygnus-photo` (binaire `cygnus-photo`) |
| `apps/cli` | `cygnus-cli` (binaire `cygnus-cli`) |

Les dépendances internes sont déclarées une seule fois dans `[workspace.dependencies]` du `Cargo.toml` racine, puis référencées avec `workspace = true`.

## Conséquences

- Une future application suit le même schéma (`cygnus-<app>-<domaine>`).
- Le flake Nix construit le paquet `cygnus-photo`.
- Le binaire sans fenêtre s'appelle `cygnus` depuis la décision 0017 (le paquet reste `cygnus-cli`).
