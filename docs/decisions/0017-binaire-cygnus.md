# 0017 — Le binaire sans fenêtre s'appelle `cygnus`

- **Date** : 2026-10-31
- **Statut** : acceptée
- **Exigences concernées** : aucune (consigne J2, phase B, point 3 ; spec section 9)

## Contexte

La consigne de J2 demande la commande `cygnus render <document> <sortie.png>`. Le binaire du paquet `apps/cli` s'appelait `cygnus-cli` (décision 0001), qui prévoyait de le raccourcir par une décision ultérieure.

## Décision

- Le binaire s'appelle **`cygnus`** ; le paquet Cargo reste `cygnus-cli`.
- Interface de J2 : `cygnus render <script.json> <sortie.png> [--bits 8|16]`. Le « document » est un script de développement (0015) jusqu'à l'arrivée du format `.cygnus` (J6). La profondeur de l'export est par défaut celle du document.
- Codes de sortie : 0 succès, 1 échec (fichier, script, GPU, rendu), 2 erreur d'usage. Sans argument, ou avec `--help`, l'aide est affichée et le code est 0.
- Les messages du mode sans fenêtre sont en anglais. Ce sont des diagnostics pour les développeurs, les tests et les agents, au même titre que les messages d'erreur sur la sortie d'erreur de l'application. La traduction passerait par le système de `crates/ui-kit`, dont `apps/cli` ne doit pas dépendre (spec 7.4) ; si elle devient nécessaire, il faudra d'abord extraire les traductions dans un crate indépendant de l'interface, par une décision.

## Conséquences

- Les scripts, la documentation et les agents utilisent `cygnus render`.
- Les tests d'intégration appellent `CARGO_BIN_EXE_cygnus`.
