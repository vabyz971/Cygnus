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

//! Sélecteurs de fichiers NON BLOQUANTS (via `rfd` sur thread dédié).
//!
//! Règle §3.4 : la boucle egui ne bloque jamais. Ces helpers
//! spawn un thread qui ouvre le dialogue natif (bloquant) puis
//! renvoie le chemin via un channel ; l'app poll le `Receiver` à
//! chaque frame avec `try_recv`.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// Ouvre un dialogue d'ouverture d'image (PNG/JPEG/TIFF/WebP/BMP).
/// Retourne le `Receiver` à poller (`Ok(path)` si choisi).
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

/// Ouvre un dialogue d'enregistrement d'image (PNG/JPEG).
/// Retourne le `Receiver` à poller (`Ok(path)` si choisi).
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

/// Ouvre un dialogue d'ouverture de projet (`.cygp`, hérités
/// `.csophoto`/`.csphoto` acceptés en lecture).
/// Retourne le `Receiver` à poller (`Ok(path)` si choisi).
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

/// Ouvre un dialogue « Enregistrer sous » de projet (`.cygp`).
/// Retourne le `Receiver` à poller (`Ok(path)` si choisi).
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
        // Sans environnement graphique, le thread rfd échoue vite ;
        // l'essentiel : l'appel ne bloque pas et le channel existe.
        // (On ne peut pas cliquer dans un dialogue natif en headless.)
        let rx = pick_image_to_open();
        // Poll non bloquant immédiat : soit En attente, soit déjà
        // Disconnected (échec rapide hors GUI) — jamais de blocage.
        let _ = rx.try_recv();
        let rx = pick_image_to_save();
        let _ = rx.try_recv();
        let rx = pick_project_to_open();
        let _ = rx.try_recv();
        let rx = pick_project_to_save();
        let _ = rx.try_recv();
    }
}
