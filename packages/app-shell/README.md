# app-shell

Squelette d'application générique pour Cygnus : file d'actions
[`ActionQueue`](src/actions.rs) + logique de docks
[`DockTab`](src/dock.rs).

Aucune dépendance UI : les apps (photo, video, audio) implémentent
leur `Behavior` et leur `DockTab` ; `app-shell` ne fait que
générer, drainer et réconcilier.

## Usage

```rust
use app_shell::ActionQueue;

enum MyAction { Undo, Redo }

let mut queue = ActionQueue::<MyAction>::new();
queue.push(MyAction::Undo);
assert_eq!(queue.drain(), vec![MyAction::Undo]);
```

Voir `apps/photo/src/commands.rs` pour un exemple complet.