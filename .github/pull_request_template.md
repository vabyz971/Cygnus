## Description

What does this PR change, and why? Reference the issue it closes if any (e.g. `Fixes #123`).

## Scope

Affected application/engine/package and relevant subsystem (for example: `photo`, `photo-engine`, `cygnus-ui/image_canvas`).

Do not include unrelated refactors: one logical change per PR.

## Type de changement

- [ ] Bug fix
- [ ] New feature
- [ ] Documentation
- [ ] Refactor (no behavior change)
- [ ] Other (describe)

## Tests et validation

Commands run and results:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Performance impact, if relevant: none measured / describe the measured result.

UI changes: attach screenshots or state that none are included.

Documentation: updated where needed, or state that no update was needed.

## Checklist

Vérifié avant ouverture (voir `CONTRIBUTING.md`) :

- [ ] `cargo fmt --all -- --check` passe
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` : aucune alerte
- [ ] `cargo test --workspace` passe (y compris les golden tests de `document.rs`, non affaiblis)
- [ ] Les règles d'architecture sont respectées (moteurs purs, rendu « state-only », aucune couleur en dur hors `theme.rs`)
- [ ] Le format `.cygp` n'est pas cassé — ou `FORMAT_VERSION` incrémenté avec gestion propre des anciennes versions
- [ ] Nouveaux items publics documentés
- [ ] Message de commit conforme (préfixe app/crate, une logique par commit, pas d'emoji)

## Notes pour le relecteur

Tout ce qui mérite une attention particulière : chemins GPU/CPU, historique, format projet, dépendances.