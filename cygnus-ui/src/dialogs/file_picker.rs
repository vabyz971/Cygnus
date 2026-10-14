// Cygnus — Suite créative professionnelle open source
// Copyright (C) 2026 vabyz971
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! NON-BLOCKING file pickers (via `rfd` on a dedicated thread).
//!
//! Rule §3.4: the egui loop never blocks. These helpers
//! spawn a thread that opens the native dialog (blocking) then
//! sends the path back through a channel; the app polls the `Receiver`
//! every frame with `try_recv`.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// Opens an image-open dialog (PNG/JPEG/TIFF/WebP/BMP).
/// Returns the `Receiver` to poll (`Ok(path)` when chosen).
pub fn pick_image_to_open() -> Receiver<Option<PathBuf>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let picked = rfd::FileDialog::new()
            .add_filter(
                "Image",
                &["png", "jpg", "jpeg", "tif", "tiff", "webp", "bmp"],
            )
            .pick_file();
        let _ = tx.send(picked);
    });
    rx
}

/// Opens an image-save dialog (PNG/JPEG).
/// Returns the `Receiver` to poll (`Ok(path)` when chosen).
pub fn pick_image_to_save() -> Receiver<Option<PathBuf>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let picked = rfd::FileDialog::new()
            .add_filter("Image PNG", &["png"])
            .add_filter("Image JPEG", &["jpg", "jpeg"])
            .save_file();
        let _ = tx.send(picked);
    });
    rx
}

/// Opens a project-open dialog (`.cygp`, legacy
/// `.csophoto`/`.csphoto` accepted for reading).
/// Returns the `Receiver` to poll (`Ok(path)` when chosen).
pub fn pick_project_to_open() -> Receiver<Option<PathBuf>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let picked = rfd::FileDialog::new()
            .add_filter("Projet Cygnus", &["cygp", "csophoto", "csphoto"])
            .pick_file();
        let _ = tx.send(picked);
    });
    rx
}

/// Opens a project "Save as" dialog (`.cygp`).
/// Returns the `Receiver` to poll (`Ok(path)` when chosen).
pub fn pick_project_to_save() -> Receiver<Option<PathBuf>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let picked = rfd::FileDialog::new()
            .add_filter("Projet Cygnus", &["cygp"])
            .save_file();
        let _ = tx.send(picked);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pickers_return_pending_receiver() {
        // Without a graphical environment, the rfd thread fails fast;
        // the point: the call does not block and the channel exists.
        // (Cannot click inside a native dialog when headless.)
        let rx = pick_image_to_open();
        // Immediate non-blocking poll: either Pending, or already
        // Disconnected (fast failure off-GUI) — never blocking.
        let _ = rx.try_recv();
        let rx = pick_image_to_save();
        let _ = rx.try_recv();
        let rx = pick_project_to_open();
        let _ = rx.try_recv();
        let rx = pick_project_to_save();
        let _ = rx.try_recv();
    }
}
