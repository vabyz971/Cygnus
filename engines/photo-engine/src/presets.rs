// Cygnus — Professional open-source creative suite
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

//! Built-in adjustment presets (O005).
//!
//! A preset is a stable id + a COMPLETE parameter set: applying one
//! starts from the definition defaults and overlays the preset values,
//! so the result never depends on the previous settings. Every
//! adjustment type gets a `"default"` preset (reset). Ids are stable
//! (UI labels are translated app-side, the engine stays text-free).

use std::collections::HashMap;

use datatypes::ParamValue;

use crate::nodes::{curves, hsl, levels};

/// Id of the reset preset available on every registered type.
pub const DEFAULT_PRESET: &str = "default";

/// One preset: id + full parameter map to install.
#[derive(Clone, Debug, PartialEq)]
pub struct FilterPreset {
    /// Stable id (e.g. `"levels.high_contrast"`).
    pub id: &'static str,
    /// Complete parameters (defaults + overrides).
    pub params: HashMap<String, ParamValue>,
}

fn float(key: &str, v: f32) -> (&str, ParamValue) {
    (key, ParamValue::Float(v))
}

fn curve(points: &str) -> (&'static str, ParamValue) {
    (curves::POINTS_KEY, ParamValue::Text(points.to_owned()))
}

/// Preset overrides per type (the reset preset is added by
/// [`presets_for`]).
fn overrides(type_id: &str) -> Vec<(&'static str, Vec<(&'static str, ParamValue)>)> {
    match type_id {
        levels::TYPE_ID => vec![
            (
                "levels.high_contrast",
                vec![float("in_black", 25.0), float("in_white", 230.0)],
            ),
            ("levels.brighten", vec![float("gamma", 1.4)]),
            ("levels.darken", vec![float("gamma", 0.7)]),
            (
                "levels.faded",
                vec![float("out_black", 35.0), float("out_white", 230.0)],
            ),
        ],
        curves::TYPE_ID => vec![
            (
                "curves.soft_contrast",
                vec![curve("0,0;64,54;192,202;255,255")],
            ),
            (
                "curves.strong_contrast",
                vec![curve("0,0;64,40;192,215;255,255")],
            ),
            ("curves.brighten", vec![curve("0,0;128,160;255,255")]),
            ("curves.darken", vec![curve("0,0;128,96;255,255")]),
            ("curves.matte", vec![curve("0,40;64,72;192,200;255,240")]),
            ("curves.invert", vec![curve("0,255;255,0")]),
        ],
        hsl::TYPE_ID => vec![
            ("hsl.black_and_white", vec![float("saturation", -100.0)]),
            ("hsl.muted", vec![float("saturation", -40.0)]),
            ("hsl.vivid", vec![float("saturation", 45.0)]),
            ("hsl.brighten", vec![float("lightness", 15.0)]),
            ("hsl.complementary", vec![float("hue", 180.0)]),
        ],
        _ => Vec::new(),
    }
}

/// Presets of a registered type, reset first. Unknown type → empty.
#[must_use]
pub fn presets_for(type_id: &str) -> Vec<FilterPreset> {
    let Some(def) = crate::registry::definition_for(type_id) else {
        return Vec::new();
    };
    let mut out = vec![FilterPreset {
        id: DEFAULT_PRESET,
        params: def.default_params.clone(),
    }];
    for (id, values) in overrides(type_id) {
        let mut params = def.default_params.clone();
        for (key, value) in values {
            params.insert(key.to_owned(), value);
        }
        out.push(FilterPreset { id, params });
    }
    out
}

/// Looks a preset up by type and id.
#[must_use]
pub fn find_preset(type_id: &str, preset_id: &str) -> Option<FilterPreset> {
    presets_for(type_id).into_iter().find(|p| p.id == preset_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjustments_have_reset_and_presets() {
        for type_id in [levels::TYPE_ID, curves::TYPE_ID, hsl::TYPE_ID] {
            let presets = presets_for(type_id);
            assert!(presets.len() > 1, "{type_id}");
            assert_eq!(presets[0].id, DEFAULT_PRESET);
        }
    }

    #[test]
    fn preset_params_only_use_known_keys() {
        for type_id in [levels::TYPE_ID, curves::TYPE_ID, hsl::TYPE_ID] {
            let Some(def) = crate::registry::definition_for(type_id) else {
                panic!("type inconnu {type_id}");
            };
            for preset in presets_for(type_id) {
                assert_eq!(
                    preset.params.len(),
                    def.default_params.len(),
                    "{}",
                    preset.id
                );
                for key in preset.params.keys() {
                    assert!(def.default_params.contains_key(key), "{}: {key}", preset.id);
                }
            }
        }
    }

    #[test]
    fn preset_ids_are_unique() {
        for type_id in [levels::TYPE_ID, curves::TYPE_ID, hsl::TYPE_ID] {
            let presets = presets_for(type_id);
            let ids: std::collections::HashSet<_> = presets.iter().map(|p| p.id).collect();
            assert_eq!(ids.len(), presets.len(), "{type_id}");
        }
    }

    #[test]
    fn unknown_type_and_preset() {
        assert!(presets_for("inexistant").is_empty());
        assert!(find_preset(levels::TYPE_ID, "inexistant").is_none());
        assert!(find_preset(hsl::TYPE_ID, "hsl.vivid").is_some());
    }

    #[test]
    fn simple_filters_get_reset_only() {
        let presets = presets_for("blur");
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, DEFAULT_PRESET);
    }
}
