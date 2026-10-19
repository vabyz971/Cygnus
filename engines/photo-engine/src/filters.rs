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

//! Internal live-filter engine: a layer's sub-layer chain is evaluated
//! sequentially — each effect receives the image accumulated below it,
//! then its result is composited with its layer attributes (opacity,
//! blend, transform, masks), Affinity-style.
//!
//! An unknown effect type (newer-version project, removed effect) is
//! transparent: the image passes through as is.

use std::sync::Arc;

use datatypes::SocketType;
use image::DynamicImage;

use crate::document::{FilterLayer, FilterNode};

/// Creates a filter SUB-LAYER (neutral layer attributes: full opacity,
/// normal blend, no transform or mask) with the definition's DEFAULT
/// parameters. Returns None if type_id is unknown.
#[must_use]
pub fn new_filter_layer(type_id: &str) -> Option<FilterLayer> {
    let def = crate::registry::definition_for(type_id)?;
    Some(FilterLayer::new(
        def.type_id.clone(),
        def.name.clone(),
        def.default_params.clone(),
    ))
}

/// Effect types eligible as live filter / adjustment layer: only
/// single-input image → image effects (no input/output/mix/layer).
pub fn filterable_types() -> Vec<datatypes::NodeDefinition> {
    crate::registry::all_definitions()
        .into_iter()
        .filter(|d| {
            let mono_image = d.inputs.len() == 1
                && d.outputs.len() == 1
                && d.inputs[0].socket_type == SocketType::Image
                && d.outputs[0].socket_type == SocketType::Image;
            mono_image
                && matches!(
                    d.category,
                    datatypes::NodeCategory::Color | datatypes::NodeCategory::Filter
                )
        })
        .collect()
}

/// PLAIN fold over raw [`FilterNode`]s (no layer attributes) — reserved
/// for adjustment layers, whose opacity/blend live at the layer level.
/// Pixels use [`render_chain`] (sub-layers).
pub(crate) fn render_nodes(
    source: &Arc<DynamicImage>,
    filters: &[FilterNode],
) -> Arc<DynamicImage> {
    if !filters.iter().any(|f| f.enabled) {
        return Arc::clone(source);
    }

    let mut current: DynamicImage = (**source).clone();
    for f in filters.iter().filter(|f| f.enabled) {
        let Some(effect) = crate::nodes::find(&f.type_id) else {
            continue;
        };
        let ctx = crate::nodes::NodeCtx {
            params: &f.params,
            input_image: Some(&current),
            original: source,
        };
        if let Some(out) = (effect.apply)(&ctx) {
            current = out;
        }
    }
    Arc::new(current)
}

/// Applies the ACTIVE filter sub-layer chain to `source`. Empty chain
///
/// or all disabled → returns `source` as is (zero cost). Sequential
/// fold — each effect receives the accumulation, its result is
/// composited with its layer attributes
/// ([`crate::document::compositing::composite_filter_layer`]).
/// Unknown effect → graceful degradation: input passes through as is.
pub fn render_chain(source: &Arc<DynamicImage>, layers: &[FilterLayer]) -> Arc<DynamicImage> {
    if !layers.iter().any(|f| f.enabled) {
        return Arc::clone(source);
    }

    let mut current: DynamicImage = (**source).clone();
    for f in layers.iter().filter(|f| f.enabled) {
        let Some(effect) = crate::nodes::find(&f.type_id) else {
            continue; // unknown effect: forward as is (kept behavior)
        };
        let ctx = crate::nodes::NodeCtx {
            params: &f.params,
            input_image: Some(&current),
            original: source,
        };
        if let Some(out) = (effect.apply)(&ctx) {
            current = crate::document::compositing::composite_filter_layer(&current, out, f);
        }
    }
    Arc::new(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datatypes::ParamValue;
    use image::ImageBuffer;
    use image::Rgba;

    fn grey(value: u8) -> Arc<DynamicImage> {
        Arc::new(DynamicImage::ImageRgba8(ImageBuffer::from_pixel(
            2,
            2,
            Rgba([value, value, value, 255]),
        )))
    }

    fn layer(type_id: &str) -> FilterLayer {
        new_filter_layer(type_id).expect("définition connue")
    }

    fn lum(value: f32) -> FilterLayer {
        let mut f = layer("brightness_contrast");
        f.params
            .insert("brightness".into(), ParamValue::Float(value));
        f
    }

    #[test]
    fn chaine_vide_renvoie_la_source_partagee() {
        let src = grey(10);
        // Identical Arc: zero copy
        let out = render_chain(&src, &[]);
        assert!(Arc::ptr_eq(&src, &out));
    }

    #[test]
    fn filtre_desactive_est_transparent() {
        let src = grey(10);
        let mut f = layer("brightness_contrast");
        f.enabled = false;
        f.params
            .insert("brightness".into(), ParamValue::Float(100.0));
        let out = render_chain(&src, &[f]);
        assert!(Arc::ptr_eq(&src, &out));
    }

    #[test]
    fn brightness_applique_le_parametre() {
        let src = grey(100);
        let f = lum(50.0);
        let contrast_default = f.params.get("contrast").cloned();
        let out = render_chain(&src, &[f]);
        let rgba = out.to_rgba8();
        let p = rgba.get_pixel(0, 0);
        // 100 + 50*2.55 = 227 (CPU path: image too small for the GPU)
        assert!((p[0] as i16 - 227).abs() <= 2);
        // The definition's default parameters are properly propagated
        assert_eq!(
            contrast_default,
            Some(ParamValue::Float(0.0)),
            "new_filter_layer doit copier default_params"
        );
    }

    #[test]
    fn chaine_sequentielle_compose_les_effets() {
        let src = grey(100);
        let f1 = lum(40.0);
        let mut f2 = layer("color_correct");
        f2.params
            .insert("saturation".into(), ParamValue::Float(1.5));
        let out = render_chain(&src, &[f1, f2]);
        // Saturated gray stays gray; brightness was applied before
        let rgba = out.to_rgba8();
        let p = rgba.get_pixel(0, 0);
        let expected = 100.0 + 40.0 * 2.55;
        assert!((p[0] as f32 - expected).abs() <= 3.0);
    }

    #[test]
    fn effet_inconnu_propage_son_entree() {
        let src = grey(42);
        let f = FilterLayer::neutral("effet_inexistant", Default::default());
        let out = render_chain(&src, &[f]);
        let rgba = out.to_rgba8();
        let p = rgba.get_pixel(0, 0);
        assert_eq!(p[0], 42);
    }

    #[test]
    fn opacite_sous_calque_mixe_lineairement() {
        // Affinity-style: the filtered result is weighted over the accumulation.
        let src = grey(100);
        let mut f = lum(40.0); // full = 202
        f.opacity = 50.0;
        let out = render_chain(&src, &[f]);
        let rgba = out.to_rgba8();
        let p = rgba.get_pixel(0, 0);
        // mix 100 ↔ 202 at 50% ≈ 151
        assert!((p[0] as f32 - 151.0).abs() <= 3.0);
    }

    #[test]
    fn fusion_sous_calque_multiply() {
        let src = grey(100);
        let mut f = lum(40.0); // filtered = 202
        f.blend_mode = crate::document::BlendMode::Multiply;
        let out = render_chain(&src, &[f]);
        let rgba = out.to_rgba8();
        let p = rgba.get_pixel(0, 0);
        // (100/255) × (202/255) × 255 ≈ 79
        assert!((p[0] as f32 - 79.0).abs() <= 3.0);
    }

    #[test]
    fn passthrough_neutre_sans_cout() {
        let f = lum(10.0);
        assert!(f.is_passthrough());
        let mut g = lum(10.0);
        g.opacity = 50.0;
        assert!(!g.is_passthrough());
    }

    #[test]
    fn filterable_types_exclut_entrees_sorties() {
        let ids: Vec<String> = filterable_types().into_iter().map(|d| d.type_id).collect();
        assert!(!ids.contains(&"input_image".to_string()));
        assert!(!ids.contains(&"output".to_string()));
        assert!(ids.contains(&"brightness_contrast".to_string()));
        assert!(ids.contains(&"blur".to_string()) || ids.contains(&"color_correct".to_string()));
    }
}
