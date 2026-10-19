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

//! Render Graph integration point: pure effect overlay.
//!
//! [`effect_overlay`] exposes the document's filter chains in the shape
//! `render-graph` expects (`SceneNodeId → [EffectKey…]`, in application
//! order, disabled filters excluded), with stable keys derived from photo
//! ids (`SceneNodeId::from_uuid`).
//!
//! PURE read-only function over the [`Document`]: neither the renderer
//! (`renderer.rs`), nor compositing, nor history is touched. Once the
//! photo document is backed by a `Scene`, this overlay will feed
//! `RenderGraph::build`/`sync` directly — meanwhile, it is tested here,
//! with no render wiring.

use std::collections::HashMap;

use render_graph::{EffectKey, SceneNodeId};

use crate::document::Document;

/// Photo effect overlay: `SceneNodeId::from_uuid(layer_id) → type_ids`
/// of ACTIVE filter sub-layers, in application order. Only pixel layers
///
/// carrying at least one active filter appear (visible or not:
/// visibility is resolved at render time, like the snapshot
/// thumbnails).
#[must_use]
pub fn effect_overlay(doc: &Document) -> HashMap<SceneNodeId, Vec<EffectKey>> {
    let mut overlay = HashMap::new();
    for id in doc.all_pixel_ids() {
        if let Some(layer) = doc.pixel_layer(id) {
            let chain: Vec<EffectKey> = layer
                .filter_layers
                .iter()
                .filter(|f| f.enabled)
                .map(|f| EffectKey::new(f.type_id.clone()))
                .collect();
            if !chain.is_empty() {
                overlay.insert(SceneNodeId::from_uuid(id), chain);
            }
        }
    }
    overlay
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{FilterLayer, LayerNode, PixelLayer};
    use std::sync::Arc;

    fn doc_avec_filtres() -> (Document, uuid::Uuid) {
        let mut doc = Document::new(4, 4);
        let img = Arc::new(image::DynamicImage::new_rgba8(2, 2));
        let mut layer = PixelLayer::new("fond", img);
        layer.filter_layers.push(FilterLayer::neutral(
            "brightness_contrast",
            Default::default(),
        ));
        let mut off = FilterLayer::neutral("blur", Default::default());
        off.enabled = false;
        layer.filter_layers.push(off);
        let id = layer.id;
        doc.push_layer(LayerNode::Pixel(layer));
        (doc, id)
    }

    #[test]
    fn overlay_expose_filtres_actifs_dans_l_ordre() {
        let (doc, id) = doc_avec_filtres();
        let overlay = effect_overlay(&doc);
        assert_eq!(overlay.len(), 1);
        assert_eq!(
            overlay.get(&SceneNodeId::from_uuid(id)).expect("mapped"),
            &vec![EffectKey::new("brightness_contrast")]
        );
    }

    #[test]
    fn overlay_vide_sans_filtres_actifs() {
        let mut doc = Document::new(4, 4);
        let img = Arc::new(image::DynamicImage::new_rgba8(2, 2));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("nu", img)));
        assert!(effect_overlay(&doc).is_empty());
    }
}
