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

//! Pont vers le Scene Graph : le layout calcule, la scène place.
//!
//! [`apply_to_scene`] écrit l'origine de chaque cadre calculé dans le
//! `offset` du transform local de son nœud de scène mappé (le reste du
//! transform — échelle, rotation — est préservé : le layout ne place que
//! des positions). Cadres non mappés ou nœuds disparus : ignorés sans
//! erreur. C'est l'unique sortie du moteur vers la scène — aucun renderer
//! n'est touché ici.

use scene::Scene;

use crate::{ComputedLayout, LayoutDoc};

/// Applique les placements aux nœuds de scène mappés.
/// Retourne le nombre de nœuds effectivement touchés.
#[must_use = "le compte dit combien de nœuds ont bougé"]
pub fn apply_to_scene(computed: &ComputedLayout, doc: &LayoutDoc, scene: &mut Scene) -> usize {
    let mut touched = 0;
    for &root in doc.roots() {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if let Some(frame) = doc.find(id) {
                stack.extend(frame.children.iter().copied());
                let (Some(target), Some(bounds)) = (frame.scene, computed.bounds_of(id)) else {
                    continue;
                };
                if let Some(mut local) = scene.local_transform(target) {
                    local.offset_x = bounds.x0;
                    local.offset_y = bounds.y0;
                    if scene.set_local_transform(target, local).is_some() {
                        touched += 1;
                    }
                }
            }
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Alignment, layout};
    use crate::{Content, Direction, FrameKind, LayoutDoc, NullMeasurer};

    #[test]
    fn placements_nourrissent_la_scene() {
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
            .expect("ok");
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
            .expect("ok");

        let mut scene = Scene::new();
        let na = scene.create_node(scene::NodeKind::Image);
        let nb = scene.create_node(scene::NodeKind::Image);
        // Cadre non mappé + nœud disparu : ignorés.
        assert!(doc.map_to_scene(a, na));
        assert!(doc.map_to_scene(b, nb));

        let computed = layout(&doc, &NullMeasurer);
        assert_eq!(apply_to_scene(&computed, &doc, &mut scene), 2);
        let ta = scene.local_transform(na).expect("mapped");
        let tb = scene.local_transform(nb).expect("mapped");
        assert_eq!((ta.offset_x, ta.offset_y), (0.0, 0.0));
        assert_eq!((tb.offset_x, tb.offset_y), (40.0, 0.0));
        // Idempotent : second passage, mêmes valeurs, même compte.
        assert_eq!(apply_to_scene(&computed, &doc, &mut scene), 2);
    }

    #[test]
    fn scene_maniquante_sans_panique() {
        let doc = LayoutDoc::new();
        let mut scene = Scene::new();
        let computed = layout(&doc, &NullMeasurer);
        assert_eq!(apply_to_scene(&computed, &doc, &mut scene), 0);
    }
}
