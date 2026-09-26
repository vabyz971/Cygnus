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

//! Transitions et compositing DÉCRITS (pas exécutés) : le backend raster
//! commun pliera les images plus tard — avec les MÊMES [`BlendMode`] que
//! la photo (vocabulaire partagé de `datatypes`, aucune duplication).

use datatypes::BlendMode;

use crate::ClipId;

/// Transition entre deux images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// Coupe franche.
    Cut,
    /// Fondu enchaîné (smoothstep).
    CrossDissolve,
}

impl TransitionKind {
    /// Coupe franche.
    pub const DEFAULT: Self = Self::Cut;
}

/// Transition : nature + durée (secondes).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Transition {
    /// Nature.
    pub kind: TransitionKind,
    /// Durée en secondes (≥ 0).
    pub duration: f32,
}

impl Transition {
    /// Nouvelle transition (durée négative ramenée à 0).
    #[must_use]
    pub fn new(kind: TransitionKind, duration: f32) -> Self {
        Self {
            kind,
            duration: duration.max(0.0),
        }
    }

    /// Facteur de mélange 0..=1 pour une progression 0..=1 (bornée).
    /// `Cut` = tout-ou-rien à 0.5 ; `CrossDissolve` = smoothstep.
    #[must_use]
    pub fn sample(self, progress: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        match self.kind {
            TransitionKind::Cut => {
                if t < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
            TransitionKind::CrossDissolve => t * t * (3.0 - 2.0 * t),
        }
    }
}

/// Description d'une fusion : fond + premier plan + pondération.
/// Le backend l'exécute avec le pipeline raster partagé.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompositeDesc {
    /// Image de fond (`None` = transparent).
    pub bottom: Option<ClipId>,
    /// Image de premier plan.
    pub top: ClipId,
    /// Opacité 0..=100 (même unité que la photo).
    pub opacity: f32,
    /// Mode de fusion PARTAGÉ avec la photo.
    pub mode: BlendMode,
}

impl CompositeDesc {
    /// Nouvelle description (opacité bornée).
    #[must_use]
    pub fn new(bottom: Option<ClipId>, top: ClipId, opacity: f32, mode: BlendMode) -> Self {
        Self {
            bottom,
            top,
            opacity: opacity.clamp(0.0, 100.0),
            mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_bornees_et_monotones() {
        let cut = Transition::new(TransitionKind::Cut, 1.0);
        assert_eq!(
            (cut.sample(0.0), cut.sample(0.49), cut.sample(0.5)),
            (0.0, 0.0, 1.0)
        );
        let dissolve = Transition::new(TransitionKind::CrossDissolve, 2.0);
        assert_eq!(dissolve.sample(0.0), 0.0);
        assert_eq!(dissolve.sample(1.0), 1.0);
        assert!((dissolve.sample(0.5) - 0.5).abs() < 0.001);
        assert!(dissolve.sample(0.25) < dissolve.sample(0.75));
        assert_eq!(Transition::new(TransitionKind::Cut, -3.0).duration, 0.0);
    }

    #[test]
    fn composite_partage_blend_mode_photo() {
        let desc = CompositeDesc::new(None, ClipId::new(), 150.0, BlendMode::Multiply);
        assert_eq!(desc.opacity, 100.0);
        // Mêmes ids numériques que les shaders photo (pas de divergence).
        assert_eq!(desc.mode.id(), 1);
        assert_eq!(format!("{}", desc.mode), "Multiply");
    }
}
