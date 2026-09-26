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

//! Décodage : le trait que les lecteurs réels implémenteront.
//!
//! Le moteur ne décode RIEN lui-même (pas de lecteur complet ici) : il
//! adresse des images ([`FrameDescriptor`]) et laisse le backend les
//! produire. [`NullDecoder`] répond synthétiquement (tests, maquettes).

use crate::{Clip, FrameDescriptor};

/// Décodeur d'images : adresse temporelle → descripteur prêt à produire.
/// L'implémentation réelle vit hors de ce crate (ffmpeg/GStreamer…).
pub trait FrameDecoder {
    /// Descripteur de l'image au temps `pts` (relatif au clip),
    /// `None` si hors clip. Ne décode aucun pixel : le backend demande
    /// les pixels ensuite, par descripteur.
    fn frame_at(&self, clip: &Clip, pts: f32) -> Option<FrameDescriptor>;
}

/// Décodeur nul : descripteurs synthétiques calés sur le clip
/// (dimensions recopiées, index calculé — aucun pixel).
#[derive(Debug, Clone, Copy, Default)]
pub struct NullDecoder;

impl FrameDecoder for NullDecoder {
    fn frame_at(&self, clip: &Clip, pts: f32) -> Option<FrameDescriptor> {
        let index = clip.frame_at(pts)?;
        Some(FrameDescriptor {
            clip: clip.id,
            index,
            pts,
            width: clip.width,
            height: clip.height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodeur_nul_cale_et_borne() {
        let clip = Clip::new("c", 4.0, 25.0, 1280, 720);
        let frame = NullDecoder.frame_at(&clip, 1.0).expect("dedans");
        assert_eq!((frame.index, frame.pts), (25, 1.0));
        assert_eq!((frame.width, frame.height), (1280, 720));
        assert_eq!(frame.clip, clip.id);
        assert!(NullDecoder.frame_at(&clip, 4.0).is_none());
        assert!(NullDecoder.frame_at(&clip, -1.0).is_none());
    }
}
