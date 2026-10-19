# Cygnus


Cygnus is a creative suite (Photo, Video, Audio) developed in **Rust** using **egui** (via eframe) and **wgpu**.

## Project status

| App | Version |
|-----|--------|
| **Photo** | `0.8.0` |
| **Video** | `0.1.0` |
| **Audio** | `0.1.0` |


## Project structure

```
Cygnus/
├── apps/                     # User-facing applications
│   ├── photo/                # Photo editor (layers, tools, canvas)
│   ├── video/                # Video editor (foundations)
│   └── audio/                # Audio station (foundations)
├── engines/                  # PURE business engines (zero UI dependencies)
│   ├── photo-engine/         # Photo engine: layer tree, compositing, live filters,                             # hybrid history, GPU compute, project format, export
│   ├── video-engine/         # Video engine (upcoming)
│   └── audio-engine/         # Audio engine (upcoming)
├── core/                     # Shared foundation reused across apps
│   ├── datatypes/            # Shared types: nodes, sockets, parameters, Vec2
├── cygnus-ui/                # egui design system: theme/ (SOLE source of tokens),
│                             #   widgets/ (buttons, icons via CygnusIcon, reorderable
│                             #   list), panels/, viewport/ (pan/zoom), dialogs/
├── packages/                 # Reusable libraries (never depend on engines/apps)
│   ├── math-utils/           # Shared math: canonical affine `Transform2D`
│   │                         #   (canonical `Vec2` = datatypes)
│   ├── file-utils/           # I/O: drag & drop, file dialogs
│   └── preferences/          # Persistent preferences, hardware, keybindings
├── assets/fonts/             # Hanken Grotesk, Material Icons
├── flake.nix                 # NixOS dev environment (Vulkan, Wayland)
└── Cargo.toml                # Rust workspace
```

### Architecture philosophy
- **Strict modularity**: business logic lives in `engines/*` and `core/*`, never in the apps. An app = interface + orchestration.
- **Pure engines**: `photo-engine` (layer tree, compositing, history) has no UI dependency and can later serve the video module (titles, image compositing).
- **Shared style, per-app layouts**: all widgets come from `cygnus-ui` (theme, `CygnusIcon`, generic panels/viewport/dialogs); each app owns its layout and its domain widgets in `apps/*/src/ui/`. cygnus-ui never references engine or app types.
- **Non-blocking UI**: the egui loop never blocks — heavy work (decode, export, compositing) runs on background threads and reports back over `mpsc` channels polled each frame.
- **Layered dependencies**: `apps/*` may depend on `engines/*`, `core/*`, and `packages/*`; `engines/*` may depend on `core/*` and non-UI `packages/*`. Packages never depend on engines or apps.
- **State-only rendering**: settings (opacity, transform, blend) never regenerate pixels — they apply at draw time. This invariant is what makes the UI feel instant.
- **Rust + egui + wgpu**: one codebase, native GPU rendering on all three platforms.

---

## Building

### Prerequisites
- Rust 1.89+ (2024 edition)
- System dependencies (Linux): `pkg-config`, `vulkan-loader`, `libxkbcommon`, `wayland` (+ `libx11` for X11)

### Linux (NixOS / Nix)
```bash
nix develop        # ready-to-use dev shell
cargo build --release -p photo
```

### Linux (classic distros)
```bash
cargo build --release -p photo
./target/release/photo
```

### Windows / macOS
```bash
cargo build --release -p photo
```
No special configuration: wgpu automatically selects DX12 (Windows) or Metal (macOS).

### Running the other apps
```bash
cargo run -p video
cargo run -p audio
```

---

## Contributing

Contributions are **very welcome** — this is a young project and every contribution counts, from bug fixes to the rendering engine.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide: development setup, architecture rules, code style and the pull-request checklist.

Quick summary:
1. **Open an issue first** for significant changes (architecture, new engines).
2. **Respect modularity**: business logic → `engines/*` / `core/*`, widgets → `cygnus-ui`, orchestration → `apps/*`.
3. **Preserve the state-only rendering model** — profile before optimizing.
4. **Quality gate**: `cargo clippy --workspace` clean, `cargo fmt` before every commit.
5. Short, descriptive commit messages prefixed by the app (`photo: fix blend-mode offset jump`).

### AI-generated contributions are accepted but must be validated by a human.


## License

Cygnus is free software licensed under the **GNU GPL v3** — see the [LICENSE](LICENSE) file.

- You are free to use, study, modify and redistribute this software
- Any derivative version must remain open source under the same license (copyleft)
- Every source file starts with the standard GPL header

```
Cygnus — Suite créative professionnelle open source
Copyright (C) 2026 vabyz971

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.
```

---

*Built with Rust, egui and wgpu — for creators on Linux.*
