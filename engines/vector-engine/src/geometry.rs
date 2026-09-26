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

//! Géométrie vectorielle : tracés Bézier et bornes.
//!
//! [`Path`] est une suite de verbes (move/line/quad/cubic/close) en
//! `datatypes::Vec2`. [`Path::bounds`] retourne l'englobant des points de
//! contrôle : CONSERVATIF et exact en pratique (une courbe de Bézier tient
//! dans l'enveloppe convexe de ses contrôles — le backend affine au
//! tessellage).

use datatypes::{Rect, Vec2};

/// Un verbe de tracé (coordonnées espace dessin).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathVerb {
    /// Déplace le point courant (débute un sous-tracé).
    MoveTo(Vec2),
    /// Segment droit vers le point.
    LineTo(Vec2),
    /// Bézier quadratique (contrôle, arrivée).
    QuadTo(Vec2, Vec2),
    /// Bézier cubique (contrôle 1, contrôle 2, arrivée).
    CubicTo(Vec2, Vec2, Vec2),
    /// Ferme le sous-tracé courant.
    Close,
}

impl PathVerb {
    /// Points de contrôle du verbe (pour bornes et aplats futurs).
    #[must_use]
    pub fn points(self) -> Vec<Vec2> {
        match self {
            PathVerb::MoveTo(p) | PathVerb::LineTo(p) => vec![p],
            PathVerb::QuadTo(c, p) => vec![c, p],
            PathVerb::CubicTo(c1, c2, p) => vec![c1, c2, p],
            PathVerb::Close => Vec::new(),
        }
    }
}

/// Tracé vectoriel : suite ordonnée de verbes.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Path {
    verbs: Vec<PathVerb>,
}

impl Path {
    /// Tracé vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nombre de verbes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.verbs.len()
    }

    /// Vrai si aucun verbe.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }

    /// Verbes dans l'ordre.
    #[must_use]
    pub fn verbs(&self) -> &[PathVerb] {
        &self.verbs
    }

    /// Ajoute un verbe (constructeur chaînable).
    pub fn push(&mut self, verb: PathVerb) {
        self.verbs.push(verb);
    }

    /// Englobant des points de contrôle (un point seul donne un rectangle
    /// dégénéré — vide — positionné sur le point ; vide si aucun point).
    #[must_use]
    pub fn bounds(&self) -> Rect {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for verb in &self.verbs {
            for p in verb.points() {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }
        }
        if min_x.is_infinite() {
            return Rect::EMPTY;
        }
        Rect::new(min_x, min_y, max_x, max_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32) -> Vec2 {
        Vec2::new(x, y)
    }

    #[test]
    fn bornes_segments_exactes() {
        let mut path = Path::new();
        assert!(path.is_empty());
        path.push(PathVerb::MoveTo(v(0.0, 0.0)));
        path.push(PathVerb::LineTo(v(10.0, 5.0)));
        assert_eq!(path.len(), 2);
        assert_eq!(path.bounds(), Rect::new(0.0, 0.0, 10.0, 5.0));
    }

    #[test]
    fn bornes_courbes_conservatives() {
        // Cubique montant à y=7.5 max mais contrôles à y=30 : l'englobant
        // des contrôles couvre la courbe (propriété d'enveloppe convexe).
        let mut path = Path::new();
        path.push(PathVerb::MoveTo(v(0.0, 0.0)));
        path.push(PathVerb::CubicTo(v(0.0, 30.0), v(10.0, 30.0), v(10.0, 0.0)));
        let bounds = path.bounds();
        assert_eq!((bounds.x0, bounds.y1), (0.0, 30.0));
        // La courbe réelle (échantillonnée) tient dedans.
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let mt = 1.0 - t;
            let y = 3.0 * mt * mt * t * 30.0 + 3.0 * mt * t * t * 30.0;
            assert!(y <= bounds.y1 + 0.001, "y={y}");
        }
    }

    #[test]
    fn trace_vide_bornes_vides() {
        assert!(Path::new().bounds().is_empty());
    }
}
