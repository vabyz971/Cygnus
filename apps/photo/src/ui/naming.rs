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

//! Default names in the interface language: the engine creates items
//! with the names the app supplies (`photo_engine::session::naming`),
//! the filter catalog shows translated effect names.

use cygnus_i18n::{Language, PhotoCatalog, PhotoTextKey};
use photo_engine::session::DefaultNames;

/// Live filters offered by the registry (`type_id` → catalog key).
const FILTER_NAMES: &[(&str, PhotoTextKey)] = &[
    (
        "brightness_contrast",
        PhotoTextKey::FilterNameBrightnessContrast,
    ),
    ("blur", PhotoTextKey::FilterNameBlur),
    ("color_correct", PhotoTextKey::FilterNameColorCorrect),
    ("levels", PhotoTextKey::FilterNameLevels),
    ("curves", PhotoTextKey::FilterNameCurves),
    ("hsl", PhotoTextKey::FilterNameHsl),
];

/// Translated name of a live filter (`None` for an effect without a
/// catalog entry: callers fall back to the registry name).
#[must_use]
pub fn filter_display_name(type_id: &str, texts: PhotoCatalog) -> Option<&'static str> {
    FILTER_NAMES
        .iter()
        .find(|(id, _)| *id == type_id)
        .map(|(_, key)| texts.get(*key))
}

/// Names the engine gives to created items, in `language`.
#[must_use]
pub fn default_names(language: Language) -> DefaultNames {
    let texts = PhotoCatalog::new(language);
    DefaultNames {
        layer: texts.get(PhotoTextKey::DefaultLayerName).to_owned(),
        shape: texts.get(PhotoTextKey::DefaultShapeName).to_owned(),
        mask: texts.get(PhotoTextKey::DefaultMaskName).to_owned(),
        group: texts.get(PhotoTextKey::DefaultGroupName).to_owned(),
        filters: FILTER_NAMES
            .iter()
            .map(|(id, key)| ((*id).to_owned(), texts.get(*key).to_owned()))
            .collect(),
    }
}

/// Name of the first layer of a blank document ("Calque 1").
#[must_use]
pub fn first_layer_name(language: Language) -> String {
    DefaultNames::numbered(
        PhotoCatalog::new(language).get(PhotoTextKey::DefaultLayerName),
        1,
    )
}

/// Title of the `n`-th untitled document ("Sans titre 2").
#[must_use]
pub fn untitled_title(language: Language, n: usize) -> String {
    DefaultNames::numbered(
        PhotoCatalog::new(language).get(PhotoTextKey::UntitledDocument),
        n,
    )
}

/// Translated name of a blend mode (the engine label is English).
#[must_use]
pub fn blend_label(
    mode: photo_engine::BlendMode,
    texts: cygnus_i18n::PhotoCatalog,
) -> &'static str {
    use cygnus_i18n::PhotoTextKey as K;
    use photo_engine::BlendMode as B;
    texts.get(match mode {
        B::Normal => K::BlendNormal,
        B::Multiply => K::BlendMultiply,
        B::Screen => K::BlendScreen,
        B::Overlay => K::BlendOverlay,
        B::Darken => K::BlendDarken,
        B::Lighten => K::BlendLighten,
        B::ColorDodge => K::BlendColorDodge,
        B::ColorBurn => K::BlendColorBurn,
        B::HardLight => K::BlendHardLight,
        B::SoftLight => K::BlendSoftLight,
        B::Difference => K::BlendDifference,
        B::Exclusion => K::BlendExclusion,
        B::LinearDodge => K::BlendLinearDodge,
        B::LinearBurn => K::BlendLinearBurn,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_filterable_effect_has_a_translated_name() {
        let texts = PhotoCatalog::new(Language::Fr);
        for def in photo_engine::filterable_types() {
            assert!(
                filter_display_name(&def.type_id, texts).is_some(),
                "nom traduit manquant : {}",
                def.type_id
            );
        }
    }

    #[test]
    fn names_follow_the_language() {
        assert_eq!(first_layer_name(Language::Fr), "Calque 1");
        assert_eq!(first_layer_name(Language::En), "Layer 1");
        assert_eq!(untitled_title(Language::En, 2), "Untitled 2");
        let fr = default_names(Language::Fr);
        assert_eq!(fr.mask, "Masque");
        assert_eq!(fr.filter("levels", "Levels"), "Niveaux");
    }

    #[test]
    fn every_blend_mode_has_a_distinct_french_name() {
        let texts = cygnus_i18n::PhotoCatalog::new(cygnus_i18n::Language::Fr);
        let names: std::collections::HashSet<&str> = photo_engine::BlendMode::ALL
            .iter()
            .map(|mode| blend_label(*mode, texts))
            .collect();
        assert_eq!(names.len(), photo_engine::BlendMode::ALL.len());
        assert_eq!(
            blend_label(photo_engine::BlendMode::Multiply, texts),
            "Produit"
        );
    }
}
