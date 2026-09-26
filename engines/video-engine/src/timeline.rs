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

//! Timeline éditoriale : pistes, régions, décision d'images.
//!
//! [`Timeline::active_at`] répond à la seule question éditoriale qui compte
//! ici : « quels clips couvrent le temps `t`, et à quel temps clip ? »
//! (vitesse incluse). Ni décodage ni compositing — le backend demandera les
//! images aux décodeurs puis les pliera selon [`CompositeDesc`](crate::CompositeDesc).

use crate::ClipId;

/// Région de clip sur une piste : placement + vitesse de lecture.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TimelineClip {
    /// Clip source.
    pub clip: ClipId,
    /// Début sur la timeline (secondes).
    pub start: f32,
    /// Durée prélevée (secondes de timeline).
    pub duration: f32,
    /// Vitesse (1 = normal, 0.5 = ralenti… ; ≤ 0 = figé sur la première image).
    pub speed: f32,
}

impl TimelineClip {
    /// Temps clip pour un temps timeline (`None` si hors région).
    #[must_use]
    pub fn clip_pts_at(&self, t: f32) -> Option<f32> {
        if !t.is_finite() || t < self.start || t >= self.start + self.duration {
            return None;
        }
        if self.speed <= 0.0 {
            return Some(0.0);
        }
        Some((t - self.start) * self.speed)
    }
}

/// Piste : régions ordonnées (index 0 = dessous de pile).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Track {
    /// Nom d'affichage.
    pub name: String,
    /// Régions.
    pub clips: Vec<TimelineClip>,
}

/// Clip actif à un temps : piste + temps clip.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveClip {
    /// Index de piste (0 = dessous).
    pub track: usize,
    /// Clip source.
    pub clip: ClipId,
    /// Temps dans le clip (secondes).
    pub clip_pts: f32,
}

/// Timeline : pistes empilées (index élevé = dessus).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Timeline {
    tracks: Vec<Track>,
}

impl Timeline {
    /// Timeline vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nombre de pistes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Vrai si vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Ajoute une piste (dessus de pile) et retourne son index.
    pub fn add_track(&mut self, name: impl Into<String>) -> usize {
        self.tracks.push(Track {
            name: name.into(),
            clips: Vec::new(),
        });
        self.tracks.len() - 1
    }

    /// Place une région sur une piste (faux si piste inconnue).
    pub fn place(&mut self, track: usize, clip: TimelineClip) -> bool {
        match self.tracks.get_mut(track) {
            Some(slot) => {
                slot.clips.push(clip);
                true
            }
            None => false,
        }
    }

    /// Clips couvrant `t`, pistes dans l'ordre (dessous → dessus),
    /// régions dans l'ordre de placement.
    #[must_use]
    pub fn active_at(&self, t: f32) -> Vec<ActiveClip> {
        let mut active = Vec::new();
        for (track, slot) in self.tracks.iter().enumerate() {
            for region in &slot.clips {
                if let Some(clip_pts) = region.clip_pts_at(t) {
                    active.push(ActiveClip {
                        track,
                        clip: region.clip,
                        clip_pts,
                    });
                }
            }
        }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip_at(start: f32, duration: f32, speed: f32) -> TimelineClip {
        TimelineClip {
            clip: ClipId::new(),
            start,
            duration,
            speed,
        }
    }

    #[test]
    fn actifs_chevauchements_et_trous() {
        let mut timeline = Timeline::new();
        assert!(timeline.is_empty());
        let v1 = timeline.add_track("V1");
        let v2 = timeline.add_track("V2");
        assert!(!timeline.place(9, clip_at(0.0, 5.0, 1.0)));
        timeline.place(v1, clip_at(0.0, 5.0, 1.0));
        timeline.place(v2, clip_at(3.0, 5.0, 1.0));

        assert_eq!(timeline.active_at(1.0).len(), 1);
        let both = timeline.active_at(4.0);
        assert_eq!(both.len(), 2);
        assert_eq!((both[0].track, both[1].track), (0, 1));
        assert!(timeline.active_at(20.0).is_empty());
        // Bornes : début inclus, fin exclusive.
        assert_eq!(timeline.active_at(0.0).len(), 1);
        assert_eq!(timeline.active_at(8.0).len(), 0);
    }

    #[test]
    fn vitesse_et_fige() {
        let mut timeline = Timeline::new();
        let v = timeline.add_track("V");
        let id = ClipId::new();
        timeline.place(
            v,
            TimelineClip {
                clip: id,
                start: 10.0,
                duration: 4.0,
                speed: 0.5,
            },
        );
        let active = timeline.active_at(12.0);
        assert_eq!(active.len(), 1);
        assert!((active[0].clip_pts - 1.0).abs() < 0.001);

        let mut frozen = Timeline::new();
        let f = frozen.add_track("F");
        frozen.place(
            f,
            TimelineClip {
                clip: id,
                start: 0.0,
                duration: 2.0,
                speed: 0.0,
            },
        );
        assert_eq!(frozen.active_at(1.0)[0].clip_pts, 0.0);
    }
}
