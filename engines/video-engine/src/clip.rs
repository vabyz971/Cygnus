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

//! Clips et frames : descripteurs temporels, sans pixels.
//!
//! Un [`Clip`] décrit un média (durée, cadence, dimensions) ; une
//! [`FrameDescriptor`] désigne une image à décoder (clip + index + temps).
//! Les pixels restent chez les décodeurs (voir [`FrameDecoder`](crate::FrameDecoder)).

use ids::EntityId;

/// Identifiant stable d'un [`Clip`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct ClipId(EntityId);

impl ClipId {
    /// Nouvel identifiant aléatoire.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Identifiant nul (sentinelle, jamais attribué).
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// L'entité sous-jacente.
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Enveloppe une entité existante.
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }
}

impl Default for ClipId {
    /// Identifiant nul — même valeur que [`ClipId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for ClipId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vc{}", self.0.as_uuid())
    }
}

/// Clip source : média + cadence + dimensions (pas de pixels ici).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Clip {
    /// Identifiant stable.
    pub id: ClipId,
    /// Nom d'affichage.
    pub name: String,
    /// Durée en secondes (> 0 attendu).
    pub duration_secs: f32,
    /// Images par seconde (> 0 attendu).
    pub fps: f32,
    /// Largeur en pixels.
    pub width: u32,
    /// Hauteur en pixels.
    pub height: u32,
}

impl Clip {
    /// Nouveau clip (durée/cadence non finies ou nulles → 0, taille gardée).
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        duration_secs: f32,
        fps: f32,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            id: ClipId::new(),
            name: name.into(),
            duration_secs: sanitize_pos(duration_secs),
            fps: sanitize_pos(fps),
            width,
            height,
        }
    }

    /// Nombre d'images (0 si durée ou cadence nulle).
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        if self.duration_secs <= 0.0 || self.fps <= 0.0 {
            return 0;
        }
        (f64::from(self.duration_secs) * f64::from(self.fps)).floor() as u64
    }

    /// Le temps (secondes, relatif au clip) est-il couvert ?
    /// (`0 <= pts < duration` — la borne de fin appartient à l'image
    /// suivante / à rien).
    #[must_use]
    pub fn contains_pts(&self, pts: f32) -> bool {
        pts.is_finite() && pts >= 0.0 && pts < self.duration_secs
    }

    /// Index d'image pour un temps (`None` si hors clip).
    #[must_use]
    pub fn frame_at(&self, pts: f32) -> Option<u64> {
        if !self.contains_pts(pts) {
            return None;
        }
        Some((f64::from(pts) * f64::from(self.fps)).floor() as u64)
    }
}

fn sanitize_pos(v: f32) -> f32 {
    if v.is_finite() { v.max(0.0) } else { 0.0 }
}

/// Image à décoder : adresse temporelle + dimensions attendues.
/// Les pixels sont produits par le décodeur, jamais stockés ici.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrameDescriptor {
    /// Clip source.
    pub clip: ClipId,
    /// Index d'image.
    pub index: u64,
    /// Temps en secondes (relatif au clip).
    pub pts: f32,
    /// Largeur attendue.
    pub width: u32,
    /// Hauteur attendue.
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_images_et_bornes() {
        let clip = Clip::new("rushes", 10.0, 25.0, 1920, 1080);
        assert_eq!(clip.frame_count(), 250);
        assert!(clip.contains_pts(0.0));
        assert!(clip.contains_pts(9.999));
        assert!(!clip.contains_pts(10.0));
        assert!(!clip.contains_pts(-0.1));
        assert_eq!(clip.frame_at(0.0), Some(0));
        assert_eq!(clip.frame_at(9.999), Some(249));
        assert_eq!(clip.frame_at(10.0), None);
        assert_eq!(clip.frame_at(f32::NAN), None);
    }

    #[test]
    fn clip_degenere_sans_images() {
        let clip = Clip::new("vide", 0.0, 25.0, 640, 480);
        assert_eq!(clip.frame_count(), 0);
        assert!(!clip.contains_pts(0.0));
    }
}
