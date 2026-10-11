# app-shell

Generic application skeleton for Cygnus: action queue
[`ActionQueue`](src/actions.rs) + dock logic
[`DockTab`](src/dock.rs).

No UI dependency: the apps (photo, video, audio) implement
their `Behavior` and `DockTab`; `app-shell` only
generates, drains and reconciles.

## Usage

```rust
use app_shell::ActionQueue;

enum MyAction { Undo, Redo }

let mut queue = ActionQueue::<MyAction>::new();
queue.push(MyAction::Undo);
assert_eq!(queue.drain(), vec![MyAction::Undo]);
```

See `apps/photo/src/commands.rs` for a complete example.
