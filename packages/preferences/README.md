# preferences

Persistent preference management: serialized model
(versioned JSON), hardware detection, keyboard
shortcuts.

Modules:
- [`model`]: `Prefs` struct + version migration
- [`hardware`]: CPU/RAM/GPU detection (sync, once)
- [`keybindings`]: combo parser (Ctrl+Shift+S → `Vec<String>`)

The model is versioned: any incompatible change
requires bumping `PREFS_FORMAT_VERSION` in
`src/model.rs` and handling backward compatibility.

The app loads preferences at startup (`PhotoApp::new`)
and saves them after each mutation.
