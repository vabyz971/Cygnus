# Contributing to Cygnus

Thank you for your interest in improving Cygnus! This document covers everything you need to contribute productively: development setup, the architecture rules that keep the codebase healthy, code style, and the pull-request checklist.

## Table of contents

- [Development setup](#development-setup)
- [Project layout](#project-layout)
- [Architecture rules (strict)](#architecture-rules-strict)
- [Code style](#code-style)
- [Testing](#testing)
- [Commit messages](#commit-messages)
- [Pull-request checklist](#pull-request-checklist)
- [Good first contributions](#good-first-contributions)

---

## Development setup

**Toolchain**: Rust **1.85+** (the project uses edition 2024).

### Linux — NixOS / Nix (official environment)

```bash
nix develop          # provides cargo, clippy, rustfmt, Vulkan + Wayland libs
cargo build -p photo
```

### Linux — other distributions

Install the system dependencies:

```bash
pkg-config vulkan-loader libxkbcommon wayland    # + libx11 for X11 sessions
cargo run -p photo
```

### Windows / macOS

No extra configuration: wgpu selects DX12 or Metal automatically.

```bash
cargo run -p photo
```

---

## Project layout

```
packages/     ui-kit, math-utils, file-utils, preferences # reusable libraries
core/         datatypes                         # shared foundation
engines/      photo-engine, video-engine, audio-engine
apps/         photo, video, audio                 # final applications
```

Dependency direction is **strictly one-way**:

```
apps → engines, core, packages
engines → core and non-UI packages
packages ↛ engines or apps
```

Crate names sometimes differ from folder names — use `-p` with the crate name (`photo-engine`, `datatypes`, `ui-kit`…).

---

## Architecture rules (strict)

These rules exist because they are what keeps the suite fast and maintainable. A review will ask you to change anything that violates them.

1. **Business logic lives in `engines/*` and `core/*`.** An app = interface + orchestration only. No rendering logic in apps.
2. **Engines are PURE.** No UI framework dependencies. `photo-engine` knows nothing about egui/eframe; its buffers are plain data (`RgbaBuf`, `Arc<[u8]>`). Converting engine buffers into UI textures happens exclusively app-side.
3. **State-only rendering model.** A setting change (opacity, transform, blend mode) must NEVER regenerate pixels or textures — it applies at draw time on the GPU. This invariant is what makes sliders feel instant. Preserve it at all costs.
4. **Single UI-texture frontier.** Engine buffers are converted into `egui::TextureHandle`s in one place (app-side `ViewportTextureCache`) and synchronized once per frame. Do not create texture handles elsewhere.
5. **The theme is the only source of colors/sizes.** Never hardcode a color outside `packages/ui-kit/src/theme.rs` — including inside canvas shaders. Use the tokens implemented there.
6. **Canonical styles only.** Components reference `ui_kit::style::*`; a component never writes its own style closures.
7. **History contract.**
   - Snapshots for destructive/structural operations (paint, crop, add/remove/reorder).
   - Lightweight commands for micro-editions (opacity, transforms, filter params), coalesced over an 800 ms window.
   - Push the PRE-mutation state, never post.
8. **Project format `.cygp` is versioned.** Any incompatible model change requires bumping `FORMAT_VERSION` in `engines/photo-engine/src/project.rs` and handling older versions cleanly.

---

## Code style

- Every `.rs` file starts with the GPL v3 header (copy the one from `apps/photo/src/main.rs`).
- **No `unwrap()` / `expect()` outside tests.** Return `Result`/`Option` with descriptive French error messages (user-facing strings are French throughout).
- **No emojis** in code or commit messages.
- Naming follows standard Rust conventions (`snake_case` functions, `UpperCamelCase` types); comments are written in French to match the existing codebase.
- Prefer iterators over manual indexing; borrow instead of clone; keep hot paths allocation-free when possible.
- Run before every push:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs all three on every pull request — a red CI blocks the merge.

---

## Testing

```bash
cargo test -p photo-engine   # compositing, history, project round-trips
cargo test --workspace       # everything
```

- Unit tests live in `#[cfg(test)] mod tests` at the end of each module.
- **Golden pixel tests** in `engines/photo-engine/src/document/tests.rs` verify blend modes pixel-by-pixel with a ±1 tolerance. Do NOT weaken them to make a refactor pass — if your change alters expected values, it needs a very good justification and a discussion first.
- Project-format tests do real save/load round-trips through temp files; keep them working whenever you touch serialization.
- GPU-dependent paths must degrade gracefully: every GPU function returns `None`/falls back to CPU when no adapter exists, so tests pass on headless CI machines.

---

## Commit messages

Short and prefixed by the app or crate:

```
photo: fix blend-mode offset jump
photo-engine: renderer cache keyed by filter signature
docs: english readme + contributing guide
```

One logical change per commit. If your PR contains unrelated fixes, split them.

---

## Pull-request checklist

Before opening a PR, verify:

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace` has no new warnings
- [ ] `cargo test --workspace` passes
- [ ] Architecture rules above are respected (especially state-only rendering and pure engines)
- [ ] New public items are documented
- [ ] The `.cygp` format was not broken — or `FORMAT_VERSION` was bumped with clean handling of older versions
- [ ] Commit messages follow the convention

Open a draft PR early if you want feedback mid-work. For significant architecture changes (new engines, new core packages, format changes), open an issue first so we can discuss the design.

---

## Good first contributions

Ideas matched to the current roadmap:

- **Photo**: vector shapes, JPEG quality dialog for export
- **Nodal generator**: wire graph evaluation output into layer textures
- **Video / Audio**: move the foundations forward toward a functional timeline/mixer
- **Packaging**: Flatpak, AppImage, AUR, brew
- **Translations**: the UI strings are currently French — an i18n layer would be a welcome foundation
- Tests, benchmarks, documentation improvements

If something is unclear — architecture rules, where code belongs, how a subsystem works — open an issue or a discussion. Asking early saves everyone time.
