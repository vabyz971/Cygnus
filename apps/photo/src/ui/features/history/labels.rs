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

//! History step labels: the engine records the stable operation id
//! (`PhotoEngineCommand::op_name`, e.g. `"set_opacity"`); the UI
//! translates it at display time, in the current language.

use cygnus_i18n::{PhotoCatalog, PhotoTextKey};

/// Catalog key of an operation id ("Edit" for unknown ids: a label is
/// never empty, even for operations added after this table).
#[must_use]
pub fn history_label_key(op: &str) -> PhotoTextKey {
    match op {
        "toggle_visibility" => PhotoTextKey::HistoryOpVisibility,
        "isolate_layer" => PhotoTextKey::HistoryOpIsolate,
        "reorder_nodes" => PhotoTextKey::HistoryOpReorder,
        "move_into_parent" => PhotoTextKey::HistoryOpNest,
        "toggle_collapsed" => PhotoTextKey::HistoryOpFold,
        "set_opacity" => PhotoTextKey::HistoryOpOpacity,
        "set_blend_mode" => PhotoTextKey::HistoryOpBlend,
        "paint_stroke" => PhotoTextKey::HistoryOpBrushStroke,
        "paint_mask" => PhotoTextKey::HistoryOpPaintMask,
        "move_layer" => PhotoTextKey::HistoryOpMove,
        "set_transform" => PhotoTextKey::HistoryOpTransform,
        "set_vector_paths" => PhotoTextKey::HistoryOpVectorNodes,
        "open_image" => PhotoTextKey::HistoryOpOpenImage,
        "add_empty_layer" => PhotoTextKey::HistoryOpNewLayer,
        "add_vector_layer" => PhotoTextKey::HistoryOpNewShape,
        "duplicate_layer" => PhotoTextKey::HistoryOpDuplicate,
        "duplicate_filter" => PhotoTextKey::HistoryOpDuplicateFilter,
        "delete_layer" => PhotoTextKey::HistoryOpDelete,
        "add_filter" => PhotoTextKey::HistoryOpAddFilter,
        "rename_layer" => PhotoTextKey::HistoryOpRename,
        "add_mask" => PhotoTextKey::HistoryOpAddMask,
        "add_mask_from_selection" => PhotoTextKey::HistoryOpMaskFromSelection,
        "set_mask_params" => PhotoTextKey::HistoryOpMaskSettings,
        "fill" => PhotoTextKey::HistoryOpFill,
        "set_layer_styles" => PhotoTextKey::HistoryOpLayerStyles,
        "set_selection" => PhotoTextKey::HistoryOpSelection,
        "magic_wand" => PhotoTextKey::HistoryOpSelection,
        "clone_stroke" => PhotoTextKey::HistoryOpClone,
        "gradient" => PhotoTextKey::HistoryOpGradient,
        "remove_mask" => PhotoTextKey::HistoryOpRemoveMask,
        "move_filter" => PhotoTextKey::HistoryOpMoveFilter,
        "move_mask" => PhotoTextKey::HistoryOpMoveMask,
        "remove_filter" => PhotoTextKey::HistoryOpRemoveFilter,
        "flip_horizontal" => PhotoTextKey::HistoryOpFlipHorizontal,
        "flip_vertical" => PhotoTextKey::HistoryOpFlipVertical,
        "rotate_clockwise" => PhotoTextKey::HistoryOpRotateClockwise,
        "rotate_counterclockwise" => PhotoTextKey::HistoryOpRotateCounterclockwise,
        "crop_to_document" => PhotoTextKey::HistoryOpCropToDocument,
        "toggle_filter" => PhotoTextKey::HistoryOpToggleFilter,
        "set_filter_param" => PhotoTextKey::HistoryOpFilterSetting,
        "apply_filter_preset" => PhotoTextKey::HistoryOpPreset,
        "toggle_mask" => PhotoTextKey::HistoryOpToggleMask,
        "move_filter_to" => PhotoTextKey::HistoryOpMoveFilter,
        "move_mask_to" => PhotoTextKey::HistoryOpMoveMask,
        "set_filter_text" => PhotoTextKey::HistoryOpFilterSetting,
        _ => PhotoTextKey::HistoryOpEdit,
    }
}

/// Translated label of an operation id.
#[must_use]
pub fn history_label(op: &str, texts: PhotoCatalog) -> &'static str {
    texts.get(history_label_key(op))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cygnus_i18n::Language;

    /// Every operation the worker records gets its own label.
    const RECORDED_OPS: &[&str] = &[
        "toggle_visibility",
        "isolate_layer",
        "reorder_nodes",
        "move_into_parent",
        "toggle_collapsed",
        "set_opacity",
        "set_blend_mode",
        "paint_stroke",
        "paint_mask",
        "move_layer",
        "set_transform",
        "set_vector_paths",
        "open_image",
        "add_empty_layer",
        "add_vector_layer",
        "duplicate_layer",
        "duplicate_filter",
        "delete_layer",
        "add_filter",
        "rename_layer",
        "add_mask",
        "remove_mask",
        "move_filter",
        "move_mask",
        "remove_filter",
        "flip_horizontal",
        "flip_vertical",
        "rotate_clockwise",
        "rotate_counterclockwise",
        "crop_to_document",
        "toggle_filter",
        "set_filter_param",
        "apply_filter_preset",
        "toggle_mask",
        "move_filter_to",
        "move_mask_to",
        "set_filter_text",
    ];

    #[test]
    fn every_recorded_operation_has_a_label() {
        for op in RECORDED_OPS {
            assert_ne!(
                history_label_key(op),
                PhotoTextKey::HistoryOpEdit,
                "libellé manquant pour {op}"
            );
        }
    }

    #[test]
    fn labels_follow_the_language() {
        let fr = PhotoCatalog::new(Language::Fr);
        let en = PhotoCatalog::new(Language::En);
        assert_eq!(history_label("set_opacity", fr), "Opacité");
        assert_eq!(history_label("set_opacity", en), "Opacity");
        assert_eq!(history_label("inconnu", fr), "Édition");
    }
}
