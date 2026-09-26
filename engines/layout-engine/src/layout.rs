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

//! Algorithme de placement : empilements ligne/colonne en deux passes.
//!
//! [`layout`] parcourt le document en profondeur : chaque conteneur mesure
//! d'abord ses enfants (passe 1 : tailles), puis les place selon l'axe, le
//! pas (`gap`), les marges et l'alignement transverse (passe 2 : positions).
//! Les pages imposent leur taille ; les feuilles texte sont mesurées via le
//! [`TextMeasurer`](crate::TextMeasurer). Coordonnées absolues accumulées
//! depuis l'origine du document.

use std::collections::HashMap;

use datatypes::Rect;

use crate::{Alignment, Content, Direction, FrameId, FrameKind, LayoutDoc, TextMeasurer};

/// Placement calculé : bornes absolues par cadre.
#[derive(Debug, Clone, Default)]
pub struct ComputedLayout {
    bounds: HashMap<FrameId, Rect>,
}

impl ComputedLayout {
    /// Bornes d'un cadre (`None` si non placé — id inconnu au layout).
    #[must_use]
    pub fn bounds_of(&self, id: FrameId) -> Option<Rect> {
        self.bounds.get(&id).copied()
    }

    /// Nombre de cadres placés.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bounds.len()
    }

    /// Vrai si aucun cadre placé.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }
}

/// Calcule le placement de tout le document (racines empilées en colonne
/// depuis l'origine, largeur disponible infinie).
#[must_use]
pub fn layout(doc: &LayoutDoc, measurer: &dyn TextMeasurer) -> ComputedLayout {
    let mut out = ComputedLayout::default();
    let mut y = 0.0;
    for &root in doc.roots() {
        let bounds = place_frame(doc, measurer, &mut out, root, 0.0, y, f32::INFINITY);
        y += bounds.height();
    }
    out
}

/// Place un cadre et sa descendance ; retourne ses bornes absolues.
fn place_frame(
    doc: &LayoutDoc,
    measurer: &dyn TextMeasurer,
    out: &mut ComputedLayout,
    id: FrameId,
    x: f32,
    y: f32,
    avail_w: f32,
) -> Rect {
    let Some(frame) = doc.find(id) else {
        return Rect::EMPTY;
    };
    let bounds = match &frame.kind {
        FrameKind::Leaf { content } => leaf_bounds(content, measurer, avail_w),
        FrameKind::Container {
            direction,
            gap,
            alignment,
        } => container_bounds(
            doc,
            measurer,
            out,
            frame,
            *direction,
            gap.max(0.0),
            *alignment,
            x,
            y,
            avail_w,
        ),
        FrameKind::Page {
            width,
            height,
            margins,
        } => {
            let w = width.max(0.0);
            let h = height.max(0.0);
            let mut cy = y + margins.top;
            for &child in &frame.children.clone() {
                let child_bounds = place_frame(
                    doc,
                    measurer,
                    out,
                    child,
                    x + margins.left,
                    cy,
                    (w - margins.horizontal()).max(0.0),
                );
                cy += child_bounds.height();
            }
            Rect::from_xywh(x, y, w, h)
        }
    };
    let (w, h) = frame
        .constraints
        .clamp_size(bounds.width(), bounds.height());
    let placed = Rect::from_xywh(x, y, w, h);
    out.bounds.insert(id, placed);
    placed
}

/// Bornes intrinsèques d'une feuille (relatives — l'appelant translate).
fn leaf_bounds(content: &Content, measurer: &dyn TextMeasurer, avail_w: f32) -> Rect {
    match content {
        Content::Empty => Rect::EMPTY,
        Content::Box { width, height } => {
            Rect::from_xywh(0.0, 0.0, width.max(0.0), height.max(0.0))
        }
        Content::Text { text, font_size } => {
            let (w, h) = measurer.measure(text, *font_size, avail_w.max(0.0));
            Rect::from_xywh(0.0, 0.0, w.max(0.0), h.max(0.0))
        }
    }
}

/// Passe 1 (tailles via `intrinsic_size`, source unique) puis passe 2
/// (positions) d'un conteneur. Retourne ses bornes absolues.
#[allow(clippy::too_many_arguments)]
fn container_bounds(
    doc: &LayoutDoc,
    measurer: &dyn TextMeasurer,
    out: &mut ComputedLayout,
    frame: &crate::Frame,
    direction: Direction,
    gap: f32,
    alignment: Alignment,
    x: f32,
    y: f32,
    avail_w: f32,
) -> Rect {
    let inner_w = (avail_w - frame.insets.horizontal()).max(0.0);
    let mut sizes = Vec::with_capacity(frame.children.len());
    for &child in &frame.children {
        sizes.push(intrinsic_size(doc, measurer, child, inner_w));
    }
    let cross_max: f32 = sizes
        .iter()
        .map(|(w, h)| match direction {
            Direction::Row => *h,
            Direction::Column => *w,
        })
        .fold(0.0, f32::max);
    // Passe 2 : positions (alignement sur l'axe transverse).
    let mut cursor = 0.0;
    for (index, &child) in frame.children.iter().enumerate() {
        let (cw, ch) = sizes[index];
        let cross_off = match alignment {
            Alignment::Start => 0.0,
            Alignment::Center => ((match direction {
                Direction::Row => cross_max - ch,
                Direction::Column => cross_max - cw,
            }) / 2.0)
                .max(0.0),
            Alignment::End => (match direction {
                Direction::Row => cross_max - ch,
                Direction::Column => cross_max - cw,
            })
            .max(0.0),
        };
        let (cx, cy) = match direction {
            Direction::Row => (
                x + frame.insets.left + cursor,
                y + frame.insets.top + cross_off,
            ),
            Direction::Column => (
                x + frame.insets.left + cross_off,
                y + frame.insets.top + cursor,
            ),
        };
        place_frame(doc, measurer, out, child, cx, cy, inner_w);
        cursor += match direction {
            Direction::Row => cw,
            Direction::Column => ch,
        } + gap;
    }
    // Taille depuis les tailles déjà mesurées (un seul parcours : pas
    // de second appel à `intrinsic_size`, qui serait exponentiel en
    // profondeur). Même formule, bornée par les contraintes du cadre.
    let main_total: f32 = sizes
        .iter()
        .map(|(w, h)| match direction {
            Direction::Row => *w,
            Direction::Column => *h,
        })
        .sum::<f32>()
        + gap * sizes.len().saturating_sub(1) as f32;
    let (w, h) = match direction {
        Direction::Row => (
            main_total + frame.insets.horizontal(),
            cross_max + frame.insets.vertical(),
        ),
        Direction::Column => (
            cross_max + frame.insets.horizontal(),
            main_total + frame.insets.vertical(),
        ),
    };
    let (w, h) = frame.constraints.clamp_size(w, h);
    Rect::from_xywh(x, y, w, h)
}

/// Taille intrinsèque d'un sous-arbre (sans placer — positions jetées).
fn intrinsic_size(
    doc: &LayoutDoc,
    measurer: &dyn TextMeasurer,
    id: FrameId,
    avail_w: f32,
) -> (f32, f32) {
    let Some(frame) = doc.find(id) else {
        return (0.0, 0.0);
    };
    let bounds = match &frame.kind {
        FrameKind::Leaf { content } => leaf_bounds(content, measurer, avail_w),
        FrameKind::Container { direction, gap, .. } => {
            let inner_w = (avail_w - frame.insets.horizontal()).max(0.0);
            let mut main = 0.0f32;
            let mut cross = 0.0f32;
            for (i, &child) in frame.children.iter().enumerate() {
                let (cw, ch) = intrinsic_size(doc, measurer, child, inner_w);
                match direction {
                    Direction::Row => {
                        main += cw + if i > 0 { gap.max(0.0) } else { 0.0 };
                        cross = cross.max(ch);
                    }
                    Direction::Column => {
                        main += ch + if i > 0 { gap.max(0.0) } else { 0.0 };
                        cross = cross.max(cw);
                    }
                }
            }
            match direction {
                Direction::Row => Rect::from_xywh(
                    0.0,
                    0.0,
                    main + frame.insets.horizontal(),
                    cross + frame.insets.vertical(),
                ),
                Direction::Column => Rect::from_xywh(
                    0.0,
                    0.0,
                    cross + frame.insets.horizontal(),
                    main + frame.insets.vertical(),
                ),
            }
        }
        FrameKind::Page { width, height, .. } => {
            Rect::from_xywh(0.0, 0.0, width.max(0.0), height.max(0.0))
        }
    };
    frame
        .constraints
        .clamp_size(bounds.width(), bounds.height())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Content, FrameKind, Insets, LayoutDoc, NullMeasurer};

    fn row_deux_boites() -> (LayoutDoc, FrameId, FrameId, FrameId) {
        let mut doc = LayoutDoc::new();
        let root = doc.create_root(
            "row",
            FrameKind::Container {
                direction: Direction::Row,
                gap: 10.0,
                alignment: Alignment::Start,
            },
        );
        let a = doc
            .create_child(
                root,
                "a",
                FrameKind::Leaf {
                    content: Content::Box {
                        width: 30.0,
                        height: 20.0,
                    },
                },
            )
            .expect("parent exists");
        let b = doc
            .create_child(
                root,
                "b",
                FrameKind::Leaf {
                    content: Content::Box {
                        width: 40.0,
                        height: 10.0,
                    },
                },
            )
            .expect("parent exists");
        (doc, root, a, b)
    }

    #[test]
    fn ligne_somme_pas_et_transverse_max() {
        let (doc, root, a, b) = row_deux_boites();
        let placed = layout(&doc, &NullMeasurer);
        assert_eq!(
            placed.bounds_of(a),
            Some(Rect::from_xywh(0.0, 0.0, 30.0, 20.0))
        );
        assert_eq!(
            placed.bounds_of(b),
            Some(Rect::from_xywh(40.0, 0.0, 40.0, 10.0))
        );
        assert_eq!(
            placed.bounds_of(root),
            Some(Rect::from_xywh(0.0, 0.0, 80.0, 20.0))
        );
    }

    #[test]
    fn colonne_empile_et_marges() {
        let mut doc = LayoutDoc::new();
        let root = doc.create_root(
            "col",
            FrameKind::Container {
                direction: Direction::Column,
                gap: 5.0,
                alignment: Alignment::Start,
            },
        );
        assert!(doc.set_insets(root, Insets::uniform(10.0)));
        let a = doc
            .create_child(
                root,
                "a",
                FrameKind::Leaf {
                    content: Content::Box {
                        width: 50.0,
                        height: 20.0,
                    },
                },
            )
            .expect("ok");
        let b = doc
            .create_child(
                root,
                "b",
                FrameKind::Leaf {
                    content: Content::Box {
                        width: 50.0,
                        height: 30.0,
                    },
                },
            )
            .expect("ok");
        let placed = layout(&doc, &NullMeasurer);
        assert_eq!(
            placed.bounds_of(a),
            Some(Rect::from_xywh(10.0, 10.0, 50.0, 20.0))
        );
        assert_eq!(
            placed.bounds_of(b),
            Some(Rect::from_xywh(10.0, 35.0, 50.0, 30.0))
        );
        assert_eq!(
            placed.bounds_of(root),
            Some(Rect::from_xywh(0.0, 0.0, 70.0, 75.0))
        );
    }

    #[test]
    fn alignement_centre_transverse() {
        let (mut doc, root, a, b) = row_deux_boites();
        assert!(doc.set_alignment(root, Alignment::Center));
        assert!(!doc.set_alignment(a, Alignment::End));
        let placed = layout(&doc, &NullMeasurer);
        // Hauteur max 20 : A (h=20) à y=0, B (h=10) centré à y=5.
        assert_eq!(placed.bounds_of(a).expect("a").y0, 0.0);
        assert_eq!(placed.bounds_of(b).expect("b").y0, 5.0);
    }

    #[test]
    fn texte_mesure_et_page_fixe() {
        let mut doc = LayoutDoc::new();
        let page = doc.create_root(
            "page",
            FrameKind::Page {
                width: 200.0,
                height: 300.0,
                margins: Insets::uniform(10.0),
            },
        );
        let t = doc
            .create_child(
                page,
                "titre",
                FrameKind::Leaf {
                    content: Content::Text {
                        text: String::from("ab"),
                        font_size: 10.0,
                    },
                },
            )
            .expect("ok");
        let placed = layout(&doc, &NullMeasurer);
        // NullMeasurer : "ab" à 10 pt = 12 × 12, placé sous les marges.
        assert_eq!(
            placed.bounds_of(t),
            Some(Rect::from_xywh(10.0, 10.0, 12.0, 12.0))
        );
        assert_eq!(
            placed.bounds_of(page),
            Some(Rect::from_xywh(0.0, 0.0, 200.0, 300.0))
        );
    }
}
