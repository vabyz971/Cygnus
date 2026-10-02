# file-utils

Utilitaires fichiers : erreurs (`FileError`), drag & drop
(accepte plusieurs fichiers, retour `Vec`), dialogues natifs
via `rfd` (pas d'egui).

Fonctions clés :
- [`FileError`] (IO, format, etc.)
- [`load_image`] (PNG/JPEG/etc. → `RgbaBuf`)
- [`open_file_picker`] / [`save_file_picker`] (rfd)

Les apps importent ces helpers dans `ui/dialogs.rs` et
dans la logique de chargement/export.