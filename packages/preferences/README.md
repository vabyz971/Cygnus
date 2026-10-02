# preferences

Gestion des préférences persistantes : modèle sérialisé
(JSON, versionné), détection du matériel, raccourcis
clavier.

Modules :
- [`model`] : structure `Prefs` + migration de version
- [`hardware`] : détection CPU/RAM/GPU (sync, une seule fois)
- [`keybindings`] : parseur de combinaisons (Ctrl+Shift+S → `Vec<String>`)

Le modèle est versionné : toute évolution incompatible
nécessite d'incrémenter `PREFS_FORMAT_VERSION` dans
`src/model.rs` et de gérer la rétro-compatibilité.

L'app charge les préférences au démarrage (`PhotoApp::new`)
et les sauvegarde après chaque mutation.