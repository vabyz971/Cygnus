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

//! Photo domain strings (menu bar…).
//!
//! Same principle as the generic catalog: stable keys
//! ([`PhotoTextKey`]) + fr/en tables (`&'static str`, zero
//! allocation) via [`PhotoCatalog`]. Lives here — not in the app —
//! so photo / video / audio share one translation crate with no
//! per-app duplicates.

use crate::Language;

/// Stable text key specific to Photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhotoTextKey {
    /// Close the active document.
    CloseDocument,
    /// Open a `.cygp` project (new tab).
    OpenProject,
    /// Save as (`.cygp` project).
    SaveAs,
    /// New empty layer.
    NewEmptyLayer,
    /// Layer from an image.
    LayerFromImage,
    /// Duplicate the layer.
    DuplicateLayer,
    /// Horizontal mirror (O004).
    FlipHorizontal,
    /// Vertical mirror (O004).
    FlipVertical,
    /// 90° clockwise rotation (O004).
    RotateClockwise,
    /// 90° counterclockwise rotation (O004).
    RotateCounterclockwise,
    /// Crop the layer to the document (O004).
    CropToDocument,
    /// Transformations submenu (mirror, rotate, crop).
    Transform,
    /// Expand a group (tree chevron).
    ExpandGroup,
    /// Collapse a group.
    CollapseGroup,
    /// Expand a layer's attachments.
    ExpandAttachments,
    /// Collapse the attachments.
    CollapseAttachments,
    /// Enable / disable the filter (eye).
    ToggleFilterEnabled,
    /// Enable / disable the mask (eye).
    ToggleMaskEnabled,
    /// Delete the filter.
    DeleteFilter,
    /// Delete the mask.
    DeleteMask,
    /// Show / hide (layer eye).
    ShowHideLayer,
    /// Solo a layer (isolate it).
    IsolateLayer,
    /// Empty list (layers header).
    NoLayers,
    /// No layer selected (inspector).
    NoLayerSelected,
    /// "Live filter" label (inspector).
    FilterLiveLabel,
    /// "Mask" label (inspector).
    MaskLabel,
    /// "Filter enabled" switch.
    FilterActive,
    /// "Mask enabled" switch.
    MaskActive,
    /// "Brightness" slider (filter).
    FilterParamBrightness,
    /// "Contrast" slider (filter).
    FilterParamContrast,
    /// "Radius" slider (filter).
    FilterParamRadius,
    /// "Saturation" slider (filter).
    FilterParamSaturation,
    /// "Hue" slider (filter).
    FilterParamHue,
    /// "Opacity" label.
    OpacityLabel,
    /// "Blend" label.
    BlendLabel,
    /// "Filter list" tooltip.
    FilterList,
    /// "Open an image…" button.
    OpenImageFile,
    /// Move tool title.
    MoveHint,
    /// Hand tool title.
    PanHint,
    /// Zoom tool title.
    ZoomHint,
    /// Brush tool title.
    BrushHint,
    /// Eraser tool title.
    EraserHint,
    /// Eyedropper tool title.
    EyedropperHint,
    /// Move contextual help (status bar).
    MoveHintText,
    /// Hand contextual help.
    PanHintText,
    /// Zoom contextual help.
    ZoomHintText,
    /// Brush contextual help.
    BrushHintText,
    /// Eraser contextual help.
    EraserHintText,
    /// Eyedropper contextual help.
    EyedropperHintText,
    /// "Size" label.
    SizeLabel,
    /// "Color" label.
    ColorLabel,
    /// "Move" title (options bar).
    MoveTitle,
    /// "Grid" label.
    GridLabel,
    /// Welcome screen: no document.
    NoDocumentOpen,
    /// Welcome screen: invitation text.
    WelcomeBody,
    /// Empty dock panel.
    NoDocument,
    /// "Create document" title.
    CreateDocumentTitle,
    /// "Print" tab.
    PrintTab,
    /// "Digital" tab.
    DigitalTab,
    /// "Unit" label.
    UnitLabel,
    /// "Width" label.
    WidthLabel,
    /// "Height" label.
    HeightLabel,
    /// "Orientation" label.
    OrientationLabel,
    /// "Portrait" button.
    PortraitLabel,
    /// "Landscape" button.
    LandscapeLabel,
    /// "A4 Portrait" preset.
    PresetA4Portrait,
    /// "A4 Landscape" preset.
    PresetA4Landscape,
    /// "Letter Portrait" preset.
    PresetLetterPortrait,
    /// "Letter Landscape" preset.
    PresetLetterLandscape,
    /// "Square" preset.
    PresetSquare,
    /// "4K UHD" preset (invariant).
    Preset4kUhd,
    /// "8K UHD" preset (invariant).
    Preset8kUhd,
    /// "16:9 Full HD" preset (invariant).
    Preset16by9FullHd,
    /// "Export" title.
    ExportTitle,
    /// "Folder" label.
    FolderLabel,
    /// "Export folder" placeholder.
    FolderPlaceholder,
    /// "Name" label.
    NameLabel,
    /// "File name" placeholder.
    FileNamePlaceholder,
    /// "Format" label.
    FormatLabel,
    /// "Quality" label.
    QualityLabel,
    /// "Destination:" prefix.
    DestinationLabel,
    /// "Photo help" title.
    HelpTitle,
    /// Help: tools paragraph.
    HelpToolsText,
    /// Help: layers paragraph.
    HelpLayersText,
    /// "Pixels" kind (inspector).
    LayerKindPixel,
    /// "Group" kind.
    LayerKindGroup,
    /// "Adjustment" kind.
    LayerKindAdjustment,
    /// "Vector" kind.
    LayerKindVector,
    /// Help: file/export paragraph.
    HelpFileText,
    /// Add a mask.
    AddMask,
    /// Delete the layer.
    DeleteLayer,
    /// Clip the preview to the document (`[x]`/`[ ]` check added by the caller).
    CropPreviewToDocument,
    /// Zoom 100%.
    Zoom100,
    /// Reset the dock layout.
    ResetLayout,
    /// Empty History panel.
    HistoryEmpty,
    /// Opening-state row (undo everything).
    HistoryInitial,
    /// About.
    About,
    /// New vector shape layer (Layers panel button).
    NewVectorLayer,
    /// Rectangle shape (vector creation menu).
    ShapeRect,
    /// Ellipse shape (vector creation menu).
    ShapeEllipse,
    /// Polygon shape (vector creation menu).
    ShapePolygon,
    /// Star shape (vector creation menu).
    ShapeStar,
    /// Node tool title.
    NodeHint,
    /// Node tool contextual help.
    NodeHintText,
    /// Mask out-of-document content (mode bar toggle).
    MaskView,
    /// Input black point slider (levels).
    FilterParamInBlack,
    /// Input white point slider (levels).
    FilterParamInWhite,
    /// Midtone gamma slider (levels).
    FilterParamGamma,
    /// Output black slider (levels).
    FilterParamOutBlack,
    /// Output white slider (levels).
    FilterParamOutWhite,
    /// Lightness slider (HSL).
    FilterParamLightness,
    /// Preset picker label (adjustments).
    PresetLabel,
    /// Current settings match no preset.
    PresetCustom,
    /// Reset preset (definition defaults).
    PresetDefault,
    /// Levels preset.
    PresetLevelsHighContrast,
    /// Levels preset.
    PresetLevelsBrighten,
    /// Levels preset.
    PresetLevelsDarken,
    /// Levels preset.
    PresetLevelsFaded,
    /// Curves preset.
    PresetCurvesSoftContrast,
    /// Curves preset.
    PresetCurvesStrongContrast,
    /// Curves preset.
    PresetCurvesBrighten,
    /// Curves preset.
    PresetCurvesDarken,
    /// Curves preset.
    PresetCurvesMatte,
    /// Curves preset.
    PresetCurvesInvert,
    /// HSL preset.
    PresetHslBlackAndWhite,
    /// HSL preset.
    PresetHslMuted,
    /// HSL preset.
    PresetHslVivid,
    /// HSL preset.
    PresetHslBrighten,
    /// HSL preset.
    PresetHslComplementary,
    /// Curve editor contextual help.
    CurveEditorHint,
    /// Reset the curve to the diagonal.
    CurveReset,
    /// Histogram with no image.
    HistogramEmpty,
    /// Luminance channel toggle.
    HistogramLuma,
    /// RGB channels toggle.
    HistogramRgb,
    /// Export dialog size estimate prefix.
    EstimatedSizeLabel,
    /// Size estimate in flight.
    EstimatingSize,
    /// WebP export mode note.
    WebpLosslessNote,
    /// TIFF export mode note.
    TiffNote,
    /// Kilobyte unit suffix.
    SizeUnitKilobytes,
    /// Megabyte unit suffix.
    SizeUnitMegabytes,
    /// History step: Visibility.
    HistoryOpVisibility,
    /// History step: Isolate.
    HistoryOpIsolate,
    /// History step: Reorder.
    HistoryOpReorder,
    /// History step: Nest.
    HistoryOpNest,
    /// History step: Fold / unfold.
    HistoryOpFold,
    /// History step: Opacity.
    HistoryOpOpacity,
    /// History step: Blend mode.
    HistoryOpBlend,
    /// History step: Brush stroke.
    HistoryOpBrushStroke,
    /// History step: Paint mask.
    HistoryOpPaintMask,
    /// History step: Move.
    HistoryOpMove,
    /// History step: Transform.
    HistoryOpTransform,
    /// History step: Vector nodes.
    HistoryOpVectorNodes,
    /// History step: Open image.
    HistoryOpOpenImage,
    /// History step: New layer.
    HistoryOpNewLayer,
    /// History step: New shape.
    HistoryOpNewShape,
    /// History step: Duplicate.
    HistoryOpDuplicate,
    /// History step: Duplicate filter.
    HistoryOpDuplicateFilter,
    /// History step: Delete.
    HistoryOpDelete,
    /// History step: Add filter.
    HistoryOpAddFilter,
    /// History step: Rename.
    HistoryOpRename,
    /// History step: Add mask.
    HistoryOpAddMask,
    /// History step: Remove mask.
    HistoryOpRemoveMask,
    /// History step: Move filter.
    HistoryOpMoveFilter,
    /// History step: Move mask.
    HistoryOpMoveMask,
    /// History step: Remove filter.
    HistoryOpRemoveFilter,
    /// History step: Flip horizontal.
    HistoryOpFlipHorizontal,
    /// History step: Flip vertical.
    HistoryOpFlipVertical,
    /// History step: Rotate clockwise.
    HistoryOpRotateClockwise,
    /// History step: Rotate counterclockwise.
    HistoryOpRotateCounterclockwise,
    /// History step: Crop to document.
    HistoryOpCropToDocument,
    /// History step: Toggle filter.
    HistoryOpToggleFilter,
    /// History step: Filter setting.
    HistoryOpFilterSetting,
    /// History step: Preset.
    HistoryOpPreset,
    /// History step: Toggle mask.
    HistoryOpToggleMask,
    /// History step: Edit.
    HistoryOpEdit,
    /// Separator between a label and its value.
    LabelSeparator,
    /// Status: document ready.
    StatusReady,
    /// Status: export written.
    StatusExported,
    /// Status: project saved.
    StatusSaved,
    /// Status: project save in progress.
    StatusSaving,
    /// Status: project load in progress.
    StatusLoading,
    /// Status: native file dialog unavailable.
    StatusPickerUnavailable,
    /// Error: image cannot be opened.
    ErrorOpenImage,
    /// Error: mirror failed.
    ErrorFlip,
    /// Error: rotation failed.
    ErrorRotate,
    /// Error: crop on a transformed layer.
    ErrorCropTransformed,
    /// Error: crop on a layer outside the document.
    ErrorCropOutside,
    /// Error: crop failed.
    ErrorCrop,
    /// Error: unknown filter type.
    ErrorUnknownFilter,
    /// Error: unknown preset.
    ErrorUnknownPreset,
    /// Error: empty document export.
    ErrorNothingToExport,
    /// Error: export failed.
    ErrorExport,
    /// Error: project save failed.
    ErrorSaveProject,
    /// Error: foreign project format version.
    ErrorUnsupportedProjectVersion,
    /// Hint after an unsupported project version.
    ErrorProjectVersionHint,
    /// Error: project load failed.
    ErrorLoadProject,
    /// Base name of a new pixel layer.
    DefaultLayerName,
    /// Base name of a new vector shape.
    DefaultShapeName,
    /// Base name of a new mask.
    DefaultMaskName,
    /// Name of a new group.
    DefaultGroupName,
    /// Title of a new unsaved document.
    UntitledDocument,
    /// Live filter name.
    FilterNameBrightnessContrast,
    /// Live filter name.
    FilterNameBlur,
    /// Live filter name.
    FilterNameColorCorrect,
    /// Live filter name.
    FilterNameLevels,
    /// Live filter name.
    FilterNameCurves,
    /// Live filter name.
    FilterNameHsl,
    /// Minimize the window.
    WindowMinimize,
    /// Maximize / restore the window.
    WindowMaximize,
    /// Adjustments section title (focused filter or mask).
    AdjustmentsTitle,
    /// Adjustments section caption.
    NonDestructive,
    /// Adjustments section empty state.
    AdjustmentsEmpty,
    /// Histogram axis: middle tones.
    HistogramMidtones,
    /// Histogram statistic: mean.
    HistogramMean,
    /// Histogram statistic: standard deviation.
    HistogramStdDev,
    /// Hide a dock panel (section menu).
    HidePanel,
    /// Document channel depth (8 bits).
    BitDepth8,
    /// Tool switcher tooltip.
    ToolLabel,
    /// Swap primary and secondary colors.
    SwapColors,
    /// Primary (brush) color swatch.
    PrimaryColor,
    /// Secondary color swatch.
    SecondaryColor,
    /// Selection menu title.
    SelectionMenu,
    /// Select the whole document.
    SelectAll,
    /// Drop the selection.
    Deselect,
    /// Invert the selection.
    InvertSelection,
    /// Mask built from the selection.
    MaskFromSelection,
    /// Selection tool name.
    SelectHint,
    /// Selection tool usage.
    SelectHintText,
    /// Rectangle marquee.
    SelectionRect,
    /// Ellipse marquee.
    SelectionEllipse,
    /// History: mask from selection.
    HistoryOpMaskFromSelection,
    /// Brush hardness field.
    HardnessLabel,
    /// Brush flow field.
    FlowLabel,
    /// Brush dab spacing field.
    SpacingLabel,
    /// Stroke stabilizer strength field.
    StabilizerLabel,
    /// Lasso tool name.
    LassoHint,
    /// Lasso tool usage.
    LassoHintText,
    /// Magic wand tool name.
    WandHint,
    /// Magic wand usage.
    WandHintText,
    /// Magic wand tolerance field.
    ToleranceLabel,
    /// Magic wand contiguous option.
    ContiguousLabel,
    /// Mask density slider.
    MaskDensityLabel,
    /// Mask feather slider.
    MaskFeatherLabel,
    /// Mask inversion switch.
    MaskInvertLabel,
    /// History: mask density / feather / inversion.
    HistoryOpMaskSettings,
    /// Paint bucket tool name.
    BucketHint,
    /// Paint bucket usage.
    BucketHintText,
    /// Gradient tool name.
    GradientHint,
    /// Gradient usage.
    GradientHintText,
    /// History: paint bucket fill.
    HistoryOpFill,
    /// History: gradient fill.
    HistoryOpGradient,
    /// Blend mode name.
    BlendNormal,
    /// Blend mode name.
    BlendMultiply,
    /// Blend mode name.
    BlendScreen,
    /// Blend mode name.
    BlendOverlay,
    /// Blend mode name.
    BlendDarken,
    /// Blend mode name.
    BlendLighten,
    /// Blend mode name.
    BlendColorDodge,
    /// Blend mode name.
    BlendColorBurn,
    /// Blend mode name.
    BlendHardLight,
    /// Blend mode name.
    BlendSoftLight,
    /// Blend mode name.
    BlendDifference,
    /// Blend mode name.
    BlendExclusion,
    /// Blend mode name.
    BlendLinearDodge,
    /// Blend mode name.
    BlendLinearBurn,
    /// Clone stamp tool name.
    CloneHint,
    /// Clone stamp usage.
    CloneHintText,
    /// History: clone stamp stroke.
    HistoryOpClone,
}

/// Photo catalog bound to a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhotoCatalog {
    lang: Language,
}

/// Every key (update on each addition: the
/// `all_keys_translated` test requires it non-empty in both languages).
#[cfg(test)]
pub(crate) const ALL_PHOTO_TEXT_KEYS: &[PhotoTextKey] = &[
    PhotoTextKey::CloseDocument,
    PhotoTextKey::NewEmptyLayer,
    PhotoTextKey::LayerFromImage,
    PhotoTextKey::DuplicateLayer,
    PhotoTextKey::AddMask,
    PhotoTextKey::DeleteLayer,
    PhotoTextKey::CropPreviewToDocument,
    PhotoTextKey::Zoom100,
    PhotoTextKey::ResetLayout,
    PhotoTextKey::About,
    PhotoTextKey::OpenProject,
    PhotoTextKey::SaveAs,
    PhotoTextKey::HistoryEmpty,
    PhotoTextKey::HistoryInitial,
    PhotoTextKey::FlipHorizontal,
    PhotoTextKey::FlipVertical,
    PhotoTextKey::RotateClockwise,
    PhotoTextKey::RotateCounterclockwise,
    PhotoTextKey::CropToDocument,
    PhotoTextKey::Transform,
    PhotoTextKey::ExpandGroup,
    PhotoTextKey::CollapseGroup,
    PhotoTextKey::ExpandAttachments,
    PhotoTextKey::CollapseAttachments,
    PhotoTextKey::ToggleFilterEnabled,
    PhotoTextKey::ToggleMaskEnabled,
    PhotoTextKey::DeleteFilter,
    PhotoTextKey::DeleteMask,
    PhotoTextKey::ShowHideLayer,
    PhotoTextKey::IsolateLayer,
    PhotoTextKey::NoLayers,
    PhotoTextKey::NoLayerSelected,
    PhotoTextKey::FilterLiveLabel,
    PhotoTextKey::MaskLabel,
    PhotoTextKey::FilterActive,
    PhotoTextKey::MaskActive,
    PhotoTextKey::FilterParamBrightness,
    PhotoTextKey::FilterParamContrast,
    PhotoTextKey::FilterParamRadius,
    PhotoTextKey::FilterParamSaturation,
    PhotoTextKey::FilterParamHue,
    PhotoTextKey::OpacityLabel,
    PhotoTextKey::BlendLabel,
    PhotoTextKey::FilterList,
    PhotoTextKey::OpenImageFile,
    PhotoTextKey::MoveHint,
    PhotoTextKey::PanHint,
    PhotoTextKey::ZoomHint,
    PhotoTextKey::BrushHint,
    PhotoTextKey::EraserHint,
    PhotoTextKey::EyedropperHint,
    PhotoTextKey::MoveHintText,
    PhotoTextKey::PanHintText,
    PhotoTextKey::ZoomHintText,
    PhotoTextKey::BrushHintText,
    PhotoTextKey::EraserHintText,
    PhotoTextKey::EyedropperHintText,
    PhotoTextKey::SizeLabel,
    PhotoTextKey::ColorLabel,
    PhotoTextKey::MoveTitle,
    PhotoTextKey::GridLabel,
    PhotoTextKey::NoDocumentOpen,
    PhotoTextKey::WelcomeBody,
    PhotoTextKey::NoDocument,
    PhotoTextKey::CreateDocumentTitle,
    PhotoTextKey::PrintTab,
    PhotoTextKey::DigitalTab,
    PhotoTextKey::UnitLabel,
    PhotoTextKey::WidthLabel,
    PhotoTextKey::HeightLabel,
    PhotoTextKey::OrientationLabel,
    PhotoTextKey::PortraitLabel,
    PhotoTextKey::LandscapeLabel,
    PhotoTextKey::PresetA4Portrait,
    PhotoTextKey::PresetA4Landscape,
    PhotoTextKey::PresetLetterPortrait,
    PhotoTextKey::PresetLetterLandscape,
    PhotoTextKey::PresetSquare,
    PhotoTextKey::Preset4kUhd,
    PhotoTextKey::Preset8kUhd,
    PhotoTextKey::Preset16by9FullHd,
    PhotoTextKey::ExportTitle,
    PhotoTextKey::FolderLabel,
    PhotoTextKey::FolderPlaceholder,
    PhotoTextKey::NameLabel,
    PhotoTextKey::FileNamePlaceholder,
    PhotoTextKey::FormatLabel,
    PhotoTextKey::QualityLabel,
    PhotoTextKey::DestinationLabel,
    PhotoTextKey::HelpTitle,
    PhotoTextKey::HelpToolsText,
    PhotoTextKey::HelpLayersText,
    PhotoTextKey::HelpFileText,
    PhotoTextKey::LayerKindPixel,
    PhotoTextKey::LayerKindGroup,
    PhotoTextKey::LayerKindAdjustment,
    PhotoTextKey::LayerKindVector,
    PhotoTextKey::NewVectorLayer,
    PhotoTextKey::ShapeRect,
    PhotoTextKey::ShapeEllipse,
    PhotoTextKey::ShapePolygon,
    PhotoTextKey::ShapeStar,
    PhotoTextKey::NodeHint,
    PhotoTextKey::NodeHintText,
    PhotoTextKey::MaskView,
    PhotoTextKey::FilterParamInBlack,
    PhotoTextKey::FilterParamInWhite,
    PhotoTextKey::FilterParamGamma,
    PhotoTextKey::FilterParamOutBlack,
    PhotoTextKey::FilterParamOutWhite,
    PhotoTextKey::FilterParamLightness,
    PhotoTextKey::PresetLabel,
    PhotoTextKey::PresetCustom,
    PhotoTextKey::PresetDefault,
    PhotoTextKey::PresetLevelsHighContrast,
    PhotoTextKey::PresetLevelsBrighten,
    PhotoTextKey::PresetLevelsDarken,
    PhotoTextKey::PresetLevelsFaded,
    PhotoTextKey::PresetCurvesSoftContrast,
    PhotoTextKey::PresetCurvesStrongContrast,
    PhotoTextKey::PresetCurvesBrighten,
    PhotoTextKey::PresetCurvesDarken,
    PhotoTextKey::PresetCurvesMatte,
    PhotoTextKey::PresetCurvesInvert,
    PhotoTextKey::PresetHslBlackAndWhite,
    PhotoTextKey::PresetHslMuted,
    PhotoTextKey::PresetHslVivid,
    PhotoTextKey::PresetHslBrighten,
    PhotoTextKey::PresetHslComplementary,
    PhotoTextKey::CurveEditorHint,
    PhotoTextKey::CurveReset,
    PhotoTextKey::HistogramEmpty,
    PhotoTextKey::HistogramLuma,
    PhotoTextKey::HistogramRgb,
    PhotoTextKey::EstimatedSizeLabel,
    PhotoTextKey::EstimatingSize,
    PhotoTextKey::WebpLosslessNote,
    PhotoTextKey::TiffNote,
    PhotoTextKey::SizeUnitKilobytes,
    PhotoTextKey::SizeUnitMegabytes,
    PhotoTextKey::HistoryOpVisibility,
    PhotoTextKey::HistoryOpIsolate,
    PhotoTextKey::HistoryOpReorder,
    PhotoTextKey::HistoryOpNest,
    PhotoTextKey::HistoryOpFold,
    PhotoTextKey::HistoryOpOpacity,
    PhotoTextKey::HistoryOpBlend,
    PhotoTextKey::HistoryOpBrushStroke,
    PhotoTextKey::HistoryOpPaintMask,
    PhotoTextKey::HistoryOpMove,
    PhotoTextKey::HistoryOpTransform,
    PhotoTextKey::HistoryOpVectorNodes,
    PhotoTextKey::HistoryOpOpenImage,
    PhotoTextKey::HistoryOpNewLayer,
    PhotoTextKey::HistoryOpNewShape,
    PhotoTextKey::HistoryOpDuplicate,
    PhotoTextKey::HistoryOpDuplicateFilter,
    PhotoTextKey::HistoryOpDelete,
    PhotoTextKey::HistoryOpAddFilter,
    PhotoTextKey::HistoryOpRename,
    PhotoTextKey::HistoryOpAddMask,
    PhotoTextKey::HistoryOpRemoveMask,
    PhotoTextKey::HistoryOpMoveFilter,
    PhotoTextKey::HistoryOpMoveMask,
    PhotoTextKey::HistoryOpRemoveFilter,
    PhotoTextKey::HistoryOpFlipHorizontal,
    PhotoTextKey::HistoryOpFlipVertical,
    PhotoTextKey::HistoryOpRotateClockwise,
    PhotoTextKey::HistoryOpRotateCounterclockwise,
    PhotoTextKey::HistoryOpCropToDocument,
    PhotoTextKey::HistoryOpToggleFilter,
    PhotoTextKey::HistoryOpFilterSetting,
    PhotoTextKey::HistoryOpPreset,
    PhotoTextKey::HistoryOpToggleMask,
    PhotoTextKey::HistoryOpEdit,
    PhotoTextKey::LabelSeparator,
    PhotoTextKey::StatusReady,
    PhotoTextKey::StatusExported,
    PhotoTextKey::StatusSaved,
    PhotoTextKey::StatusSaving,
    PhotoTextKey::StatusLoading,
    PhotoTextKey::StatusPickerUnavailable,
    PhotoTextKey::ErrorOpenImage,
    PhotoTextKey::ErrorFlip,
    PhotoTextKey::ErrorRotate,
    PhotoTextKey::ErrorCropTransformed,
    PhotoTextKey::ErrorCropOutside,
    PhotoTextKey::ErrorCrop,
    PhotoTextKey::ErrorUnknownFilter,
    PhotoTextKey::ErrorUnknownPreset,
    PhotoTextKey::ErrorNothingToExport,
    PhotoTextKey::ErrorExport,
    PhotoTextKey::ErrorSaveProject,
    PhotoTextKey::ErrorUnsupportedProjectVersion,
    PhotoTextKey::ErrorProjectVersionHint,
    PhotoTextKey::ErrorLoadProject,
    PhotoTextKey::DefaultLayerName,
    PhotoTextKey::DefaultShapeName,
    PhotoTextKey::DefaultMaskName,
    PhotoTextKey::DefaultGroupName,
    PhotoTextKey::UntitledDocument,
    PhotoTextKey::FilterNameBrightnessContrast,
    PhotoTextKey::FilterNameBlur,
    PhotoTextKey::FilterNameColorCorrect,
    PhotoTextKey::FilterNameLevels,
    PhotoTextKey::FilterNameCurves,
    PhotoTextKey::FilterNameHsl,
    PhotoTextKey::WindowMinimize,
    PhotoTextKey::WindowMaximize,
    PhotoTextKey::AdjustmentsTitle,
    PhotoTextKey::NonDestructive,
    PhotoTextKey::AdjustmentsEmpty,
    PhotoTextKey::HistogramMidtones,
    PhotoTextKey::HistogramMean,
    PhotoTextKey::HistogramStdDev,
    PhotoTextKey::HidePanel,
    PhotoTextKey::BitDepth8,
    PhotoTextKey::ToolLabel,
    PhotoTextKey::SwapColors,
    PhotoTextKey::PrimaryColor,
    PhotoTextKey::SecondaryColor,
    PhotoTextKey::SelectionMenu,
    PhotoTextKey::SelectAll,
    PhotoTextKey::Deselect,
    PhotoTextKey::InvertSelection,
    PhotoTextKey::MaskFromSelection,
    PhotoTextKey::SelectHint,
    PhotoTextKey::SelectHintText,
    PhotoTextKey::SelectionRect,
    PhotoTextKey::SelectionEllipse,
    PhotoTextKey::HistoryOpMaskFromSelection,
    PhotoTextKey::HardnessLabel,
    PhotoTextKey::FlowLabel,
    PhotoTextKey::SpacingLabel,
    PhotoTextKey::StabilizerLabel,
    PhotoTextKey::LassoHint,
    PhotoTextKey::LassoHintText,
    PhotoTextKey::WandHint,
    PhotoTextKey::WandHintText,
    PhotoTextKey::ToleranceLabel,
    PhotoTextKey::ContiguousLabel,
    PhotoTextKey::MaskDensityLabel,
    PhotoTextKey::MaskFeatherLabel,
    PhotoTextKey::MaskInvertLabel,
    PhotoTextKey::HistoryOpMaskSettings,
    PhotoTextKey::BucketHint,
    PhotoTextKey::BucketHintText,
    PhotoTextKey::GradientHint,
    PhotoTextKey::GradientHintText,
    PhotoTextKey::HistoryOpFill,
    PhotoTextKey::HistoryOpGradient,
    PhotoTextKey::BlendNormal,
    PhotoTextKey::BlendMultiply,
    PhotoTextKey::BlendScreen,
    PhotoTextKey::BlendOverlay,
    PhotoTextKey::BlendDarken,
    PhotoTextKey::BlendLighten,
    PhotoTextKey::BlendColorDodge,
    PhotoTextKey::BlendColorBurn,
    PhotoTextKey::BlendHardLight,
    PhotoTextKey::BlendSoftLight,
    PhotoTextKey::BlendDifference,
    PhotoTextKey::BlendExclusion,
    PhotoTextKey::BlendLinearDodge,
    PhotoTextKey::BlendLinearBurn,
    PhotoTextKey::CloneHint,
    PhotoTextKey::CloneHintText,
    PhotoTextKey::HistoryOpClone,
];

impl PhotoCatalog {
    /// Catalog for language `lang`.
    pub fn new(lang: Language) -> Self {
        Self { lang }
    }

    /// Translates `key` into the catalog language.
    pub fn get(self, key: PhotoTextKey) -> &'static str {
        match self.lang {
            Language::En => translate_en(key),
            Language::Fr => translate_fr(key),
        }
    }
}

/// French table (historical rendering, unchanged).
fn translate_fr(key: PhotoTextKey) -> &'static str {
    match key {
        PhotoTextKey::CloseDocument => "Fermer le document",
        PhotoTextKey::OpenProject => "Ouvrir un projet",
        PhotoTextKey::SaveAs => "Enregistrer sous",
        PhotoTextKey::NewEmptyLayer => "Nouveau calque vide",
        PhotoTextKey::LayerFromImage => "Calque depuis une image",
        PhotoTextKey::DuplicateLayer => "Dupliquer le calque",
        PhotoTextKey::FlipHorizontal => "Miroir horizontal",
        PhotoTextKey::FlipVertical => "Miroir vertical",
        PhotoTextKey::RotateClockwise => "Rotation 90° horaire",
        PhotoTextKey::RotateCounterclockwise => "Rotation 90° antihoraire",
        PhotoTextKey::CropToDocument => "Rogner au document",
        PhotoTextKey::Transform => "Transformation",
        PhotoTextKey::ExpandGroup => "Déplier",
        PhotoTextKey::CollapseGroup => "Replier",
        PhotoTextKey::ExpandAttachments => "Déplier les effets",
        PhotoTextKey::CollapseAttachments => "Replier les effets",
        PhotoTextKey::ToggleFilterEnabled => "Activer / désactiver le filtre",
        PhotoTextKey::ToggleMaskEnabled => "Activer / désactiver le masque",
        PhotoTextKey::DeleteFilter => "Supprimer le filtre",
        PhotoTextKey::DeleteMask => "Supprimer le masque",
        PhotoTextKey::ShowHideLayer => "Afficher / masquer",
        PhotoTextKey::IsolateLayer => "Solo",
        PhotoTextKey::NoLayers => "Aucun calque",
        PhotoTextKey::NoLayerSelected => "Aucun calque sélectionné",
        PhotoTextKey::FilterLiveLabel => "Filtre live",
        PhotoTextKey::MaskLabel => "Masque",
        PhotoTextKey::FilterActive => "Filtre actif",
        PhotoTextKey::MaskActive => "Masque actif",
        PhotoTextKey::FilterParamBrightness => "Luminosité",
        PhotoTextKey::FilterParamContrast => "Contraste",
        PhotoTextKey::FilterParamRadius => "Rayon",
        PhotoTextKey::FilterParamSaturation => "Saturation",
        PhotoTextKey::FilterParamHue => "Teinte",
        PhotoTextKey::OpacityLabel => "Opacité",
        PhotoTextKey::BlendLabel => "Fusion",
        PhotoTextKey::FilterList => "Liste des filtres",
        PhotoTextKey::OpenImageFile => "Ouvrir une image…",
        PhotoTextKey::MoveHint => "Déplacer / sélectionner",
        PhotoTextKey::PanHint => "Main (déplacer la vue)",
        PhotoTextKey::ZoomHint => "Loupe",
        PhotoTextKey::BrushHint => "Pinceau",
        PhotoTextKey::EraserHint => "Gomme",
        PhotoTextKey::EyedropperHint => "Pipette",
        PhotoTextKey::MoveHintText => "Glisser : déplacer le calque — molette : zoom",
        PhotoTextKey::PanHintText => "Glisser : déplacer la vue — molette : zoom",
        PhotoTextKey::ZoomHintText => "Clic : zoom avant — clic droit : zoom arrière",
        PhotoTextKey::BrushHintText => "Glisser : peindre — relâcher : valider le trait",
        PhotoTextKey::EraserHintText => "Glisser : effacer — relâcher : valider",
        PhotoTextKey::EyedropperHintText => "Cliquer : échantillonner la couleur",
        PhotoTextKey::SizeLabel => "Taille",
        PhotoTextKey::ColorLabel => "Couleur",
        PhotoTextKey::MoveTitle => "Déplacement",
        PhotoTextKey::GridLabel => "Grille",
        PhotoTextKey::NoDocumentOpen => "Aucun document ouvert",
        PhotoTextKey::WelcomeBody => "Créez un document ou ouvrez une image pour commencer.",
        PhotoTextKey::NoDocument => "Aucun document",
        PhotoTextKey::CreateDocumentTitle => "Créer un document",
        PhotoTextKey::PrintTab => "Impression",
        PhotoTextKey::DigitalTab => "Numérique",
        PhotoTextKey::UnitLabel => "Unité",
        PhotoTextKey::WidthLabel => "Largeur",
        PhotoTextKey::HeightLabel => "Hauteur",
        PhotoTextKey::OrientationLabel => "Orientation",
        PhotoTextKey::PortraitLabel => "Portrait",
        PhotoTextKey::LandscapeLabel => "Paysage",
        PhotoTextKey::PresetA4Portrait => "A4 Portrait",
        PhotoTextKey::PresetA4Landscape => "A4 Paysage",
        PhotoTextKey::PresetLetterPortrait => "Lettre Portrait",
        PhotoTextKey::PresetLetterLandscape => "Lettre Paysage",
        PhotoTextKey::PresetSquare => "Carré",
        PhotoTextKey::Preset4kUhd => "4K UHD",
        PhotoTextKey::Preset8kUhd => "8K UHD",
        PhotoTextKey::Preset16by9FullHd => "16:9 Full HD",
        PhotoTextKey::ExportTitle => "Exportation",
        PhotoTextKey::FolderLabel => "Dossier",
        PhotoTextKey::FolderPlaceholder => "Dossier d'exportation",
        PhotoTextKey::NameLabel => "Nom",
        PhotoTextKey::FileNamePlaceholder => "Nom du fichier",
        PhotoTextKey::FormatLabel => "Format",
        PhotoTextKey::QualityLabel => "Qualité",
        PhotoTextKey::DestinationLabel => "Destination :",
        PhotoTextKey::HelpTitle => "Aide de Photo",
        PhotoTextKey::HelpToolsText => {
            "Outils : déplacement, main, loupe, pinceau, gomme, pipette."
        }
        PhotoTextKey::HelpLayersText => {
            "Calques : clic = sélection, double-clic sur le nom = renommer, glisser-déposer = réordonner. Bas du panneau : image, vide, dupliquer, masque, filtres, supprimer."
        }
        PhotoTextKey::HelpFileText => {
            "Fichier > Nouveau document : format, dimensions (px, in, cm, pica), orientation portrait/paysage. Exportation : dossier, format PNG/JPEG/WEBP/TIFF/GIF, validation."
        }
        PhotoTextKey::LayerKindPixel => "Pixels",
        PhotoTextKey::LayerKindGroup => "Groupe",
        PhotoTextKey::LayerKindAdjustment => "Réglage",
        PhotoTextKey::LayerKindVector => "Vecteur",
        PhotoTextKey::AddMask => "Ajouter un masque",
        PhotoTextKey::DeleteLayer => "Supprimer le calque",
        PhotoTextKey::CropPreviewToDocument => "Rogner l'aperçu au document",
        PhotoTextKey::Zoom100 => "Zoom 100 %",
        PhotoTextKey::ResetLayout => "Réinitialiser la disposition",
        PhotoTextKey::HistoryEmpty => "Aucune modification",
        PhotoTextKey::HistoryInitial => "État initial",
        PhotoTextKey::About => "À propos",
        PhotoTextKey::NewVectorLayer => "Nouvelle forme vectorielle",
        PhotoTextKey::ShapeRect => "Rectangle",
        PhotoTextKey::ShapeEllipse => "Ellipse",
        PhotoTextKey::ShapePolygon => "Polygone",
        PhotoTextKey::ShapeStar => "Étoile",
        PhotoTextKey::NodeHint => "Nœuds",
        PhotoTextKey::NodeHintText => "Glisser : déplacer le nœud — molette : zoom",
        PhotoTextKey::MaskView => "Masquer",
        PhotoTextKey::FilterParamInBlack => "Noir d'entrée",
        PhotoTextKey::FilterParamInWhite => "Blanc d'entrée",
        PhotoTextKey::FilterParamGamma => "Gamma",
        PhotoTextKey::FilterParamOutBlack => "Noir de sortie",
        PhotoTextKey::FilterParamOutWhite => "Blanc de sortie",
        PhotoTextKey::FilterParamLightness => "Luminosité",
        PhotoTextKey::PresetLabel => "Préréglage",
        PhotoTextKey::PresetCustom => "Personnalisé",
        PhotoTextKey::PresetDefault => "Par défaut",
        PhotoTextKey::PresetLevelsHighContrast => "Contraste élevé",
        PhotoTextKey::PresetLevelsBrighten => "Éclaircir",
        PhotoTextKey::PresetLevelsDarken => "Assombrir",
        PhotoTextKey::PresetLevelsFaded => "Délavé",
        PhotoTextKey::PresetCurvesSoftContrast => "Contraste doux",
        PhotoTextKey::PresetCurvesStrongContrast => "Contraste fort",
        PhotoTextKey::PresetCurvesBrighten => "Éclaircir",
        PhotoTextKey::PresetCurvesDarken => "Assombrir",
        PhotoTextKey::PresetCurvesMatte => "Mat",
        PhotoTextKey::PresetCurvesInvert => "Négatif",
        PhotoTextKey::PresetHslBlackAndWhite => "Noir et blanc",
        PhotoTextKey::PresetHslMuted => "Atténué",
        PhotoTextKey::PresetHslVivid => "Éclatant",
        PhotoTextKey::PresetHslBrighten => "Éclaircir",
        PhotoTextKey::PresetHslComplementary => "Complémentaire",
        PhotoTextKey::CurveEditorHint => {
            "Clic : ajouter un point · glisser : déplacer · double-clic : supprimer"
        }
        PhotoTextKey::CurveReset => "Réinitialiser la courbe",
        PhotoTextKey::HistogramEmpty => "Aucune image à analyser",
        PhotoTextKey::HistogramLuma => "Luminance",
        PhotoTextKey::HistogramRgb => "RVB",
        PhotoTextKey::EstimatedSizeLabel => "Taille estimée :",
        PhotoTextKey::EstimatingSize => "estimation…",
        PhotoTextKey::WebpLosslessNote => "WebP sans perte, transparence conservée",
        PhotoTextKey::TiffNote => "TIFF non compressé, transparence conservée",
        PhotoTextKey::SizeUnitKilobytes => "Ko",
        PhotoTextKey::SizeUnitMegabytes => "Mo",
        PhotoTextKey::HistoryOpVisibility => "Visibilité",
        PhotoTextKey::HistoryOpIsolate => "Isoler",
        PhotoTextKey::HistoryOpReorder => "Réordonner",
        PhotoTextKey::HistoryOpNest => "Imbriquer",
        PhotoTextKey::HistoryOpFold => "Replier / déplier",
        PhotoTextKey::HistoryOpOpacity => "Opacité",
        PhotoTextKey::HistoryOpBlend => "Fusion",
        PhotoTextKey::HistoryOpBrushStroke => "Coup de pinceau",
        PhotoTextKey::HistoryOpPaintMask => "Peindre le masque",
        PhotoTextKey::HistoryOpMove => "Déplacer",
        PhotoTextKey::HistoryOpTransform => "Transformer",
        PhotoTextKey::HistoryOpVectorNodes => "Nœuds vecteur",
        PhotoTextKey::HistoryOpOpenImage => "Ouvrir une image",
        PhotoTextKey::HistoryOpNewLayer => "Nouveau calque",
        PhotoTextKey::HistoryOpNewShape => "Nouvelle forme",
        PhotoTextKey::HistoryOpDuplicate => "Dupliquer",
        PhotoTextKey::HistoryOpDuplicateFilter => "Dupliquer le filtre",
        PhotoTextKey::HistoryOpDelete => "Supprimer",
        PhotoTextKey::HistoryOpAddFilter => "Ajouter un filtre",
        PhotoTextKey::HistoryOpRename => "Renommer",
        PhotoTextKey::HistoryOpAddMask => "Ajouter un masque",
        PhotoTextKey::HistoryOpRemoveMask => "Supprimer le masque",
        PhotoTextKey::HistoryOpMoveFilter => "Déplacer le filtre",
        PhotoTextKey::HistoryOpMoveMask => "Déplacer le masque",
        PhotoTextKey::HistoryOpRemoveFilter => "Supprimer le filtre",
        PhotoTextKey::HistoryOpFlipHorizontal => "Miroir horizontal",
        PhotoTextKey::HistoryOpFlipVertical => "Miroir vertical",
        PhotoTextKey::HistoryOpRotateClockwise => "Rotation horaire",
        PhotoTextKey::HistoryOpRotateCounterclockwise => "Rotation antihoraire",
        PhotoTextKey::HistoryOpCropToDocument => "Rogner au document",
        PhotoTextKey::HistoryOpToggleFilter => "Basculer le filtre",
        PhotoTextKey::HistoryOpFilterSetting => "Réglage du filtre",
        PhotoTextKey::HistoryOpPreset => "Préréglage",
        PhotoTextKey::HistoryOpToggleMask => "Basculer le masque",
        PhotoTextKey::HistoryOpEdit => "Édition",
        PhotoTextKey::LabelSeparator => " : ",
        PhotoTextKey::StatusReady => "Prêt",
        PhotoTextKey::StatusExported => "Exporté",
        PhotoTextKey::StatusSaved => "Enregistré",
        PhotoTextKey::StatusSaving => "Enregistrement",
        PhotoTextKey::StatusLoading => "Chargement",
        PhotoTextKey::StatusPickerUnavailable => "Dialogue de fichier indisponible",
        PhotoTextKey::ErrorOpenImage => "Ouverture impossible",
        PhotoTextKey::ErrorFlip => "Miroir impossible",
        PhotoTextKey::ErrorRotate => "Rotation impossible",
        PhotoTextKey::ErrorCropTransformed => "Rogner impossible : redressez le calque d'abord",
        PhotoTextKey::ErrorCropOutside => "Rogner impossible : calque hors document",
        PhotoTextKey::ErrorCrop => "Rogner impossible",
        PhotoTextKey::ErrorUnknownFilter => "Filtre inconnu",
        PhotoTextKey::ErrorUnknownPreset => "Préréglage inconnu",
        PhotoTextKey::ErrorNothingToExport => "Rien à exporter (document vide)",
        PhotoTextKey::ErrorExport => "Export impossible",
        PhotoTextKey::ErrorSaveProject => "Enregistrement impossible",
        PhotoTextKey::ErrorUnsupportedProjectVersion => "Version de projet non supportée",
        PhotoTextKey::ErrorProjectVersionHint => {
            "les projets au format 1 et 2 ne sont plus lisibles"
        }
        PhotoTextKey::ErrorLoadProject => "Ouverture du projet impossible",
        PhotoTextKey::DefaultLayerName => "Calque",
        PhotoTextKey::DefaultShapeName => "Forme",
        PhotoTextKey::DefaultMaskName => "Masque",
        PhotoTextKey::DefaultGroupName => "Groupe",
        PhotoTextKey::UntitledDocument => "Sans titre",
        PhotoTextKey::FilterNameBrightnessContrast => "Luminosité / Contraste",
        PhotoTextKey::FilterNameBlur => "Flou",
        PhotoTextKey::FilterNameColorCorrect => "Correction couleur",
        PhotoTextKey::FilterNameLevels => "Niveaux",
        PhotoTextKey::FilterNameCurves => "Courbes",
        PhotoTextKey::FilterNameHsl => "Teinte / Saturation / Luminosité",
        PhotoTextKey::WindowMinimize => "Réduire",
        PhotoTextKey::WindowMaximize => "Agrandir",
        PhotoTextKey::AdjustmentsTitle => "Réglages",
        PhotoTextKey::NonDestructive => "Non destructif",
        PhotoTextKey::AdjustmentsEmpty => "Sélectionnez un filtre ou un masque dans les calques",
        PhotoTextKey::HistogramMidtones => "Tons moyens",
        PhotoTextKey::HistogramMean => "Moyenne",
        PhotoTextKey::HistogramStdDev => "Écart type",
        PhotoTextKey::HidePanel => "Masquer le panneau",
        PhotoTextKey::BitDepth8 => "8 bits",
        PhotoTextKey::ToolLabel => "Outil",
        PhotoTextKey::SwapColors => "Permuter les couleurs",
        PhotoTextKey::PrimaryColor => "Couleur principale",
        PhotoTextKey::SecondaryColor => "Couleur secondaire",
        PhotoTextKey::SelectionMenu => "Sélection",
        PhotoTextKey::SelectAll => "Tout sélectionner",
        PhotoTextKey::Deselect => "Désélectionner",
        PhotoTextKey::InvertSelection => "Intervertir la sélection",
        PhotoTextKey::MaskFromSelection => "Masque depuis la sélection",
        PhotoTextKey::SelectHint => "Sélection",
        PhotoTextKey::SelectHintText => {
            "Glisser : sélectionner — Maj : ajouter, Alt : soustraire — clic : désélectionner"
        }
        PhotoTextKey::SelectionRect => "Rectangle",
        PhotoTextKey::SelectionEllipse => "Ellipse",
        PhotoTextKey::HistoryOpMaskFromSelection => "Masque depuis la sélection",
        PhotoTextKey::HardnessLabel => "Dureté",
        PhotoTextKey::FlowLabel => "Flux",
        PhotoTextKey::SpacingLabel => "Espacement",
        PhotoTextKey::StabilizerLabel => "Stabilisateur",
        PhotoTextKey::LassoHint => "Lasso",
        PhotoTextKey::LassoHintText => {
            "Glisser : tracer la sélection — Maj : ajouter, Alt : soustraire"
        }
        PhotoTextKey::WandHint => "Baguette magique",
        PhotoTextKey::WandHintText => {
            "Clic : sélectionner les couleurs proches — Maj : ajouter, Alt : soustraire"
        }
        PhotoTextKey::ToleranceLabel => "Tolérance",
        PhotoTextKey::ContiguousLabel => "Contiguë",
        PhotoTextKey::MaskDensityLabel => "Densité",
        PhotoTextKey::MaskFeatherLabel => "Contour progressif",
        PhotoTextKey::MaskInvertLabel => "Inverser",
        PhotoTextKey::HistoryOpMaskSettings => "Réglages du masque",
        PhotoTextKey::BucketHint => "Pot de peinture",
        PhotoTextKey::BucketHintText => "Clic : remplir la zone de couleur proche",
        PhotoTextKey::GradientHint => "Dégradé",
        PhotoTextKey::GradientHintText => {
            "Glisser : dégradé de la couleur principale à la secondaire"
        }
        PhotoTextKey::HistoryOpFill => "Remplissage",
        PhotoTextKey::HistoryOpGradient => "Dégradé",
        PhotoTextKey::BlendNormal => "Normal",
        PhotoTextKey::BlendMultiply => "Produit",
        PhotoTextKey::BlendScreen => "Superposition",
        PhotoTextKey::BlendOverlay => "Incrustation",
        PhotoTextKey::BlendDarken => "Obscurcir",
        PhotoTextKey::BlendLighten => "Éclaircir",
        PhotoTextKey::BlendColorDodge => "Densité couleur −",
        PhotoTextKey::BlendColorBurn => "Densité couleur +",
        PhotoTextKey::BlendHardLight => "Lumière crue",
        PhotoTextKey::BlendSoftLight => "Lumière tamisée",
        PhotoTextKey::BlendDifference => "Différence",
        PhotoTextKey::BlendExclusion => "Exclusion",
        PhotoTextKey::BlendLinearDodge => "Densité linéaire − (Addition)",
        PhotoTextKey::BlendLinearBurn => "Densité linéaire +",
        PhotoTextKey::CloneHint => "Tampon de duplication",
        PhotoTextKey::CloneHintText => "Alt+clic : définir la source — glisser : recopier",
        PhotoTextKey::HistoryOpClone => "Tampon de duplication",
    }
}

/// English table.
fn translate_en(key: PhotoTextKey) -> &'static str {
    match key {
        PhotoTextKey::CloseDocument => "Close document",
        PhotoTextKey::OpenProject => "Open project",
        PhotoTextKey::SaveAs => "Save as",
        PhotoTextKey::NewEmptyLayer => "New empty layer",
        PhotoTextKey::LayerFromImage => "Layer from image",
        PhotoTextKey::DuplicateLayer => "Duplicate layer",
        PhotoTextKey::FlipHorizontal => "Flip horizontal",
        PhotoTextKey::FlipVertical => "Flip vertical",
        PhotoTextKey::RotateClockwise => "Rotate 90° clockwise",
        PhotoTextKey::RotateCounterclockwise => "Rotate 90° counterclockwise",
        PhotoTextKey::CropToDocument => "Crop to document",
        PhotoTextKey::Transform => "Transform",
        PhotoTextKey::ExpandGroup => "Expand",
        PhotoTextKey::CollapseGroup => "Collapse",
        PhotoTextKey::ExpandAttachments => "Expand effects",
        PhotoTextKey::CollapseAttachments => "Collapse effects",
        PhotoTextKey::ToggleFilterEnabled => "Enable / disable filter",
        PhotoTextKey::ToggleMaskEnabled => "Enable / disable mask",
        PhotoTextKey::DeleteFilter => "Delete filter",
        PhotoTextKey::DeleteMask => "Delete mask",
        PhotoTextKey::ShowHideLayer => "Show / hide",
        PhotoTextKey::IsolateLayer => "Solo",
        PhotoTextKey::NoLayers => "No layers",
        PhotoTextKey::NoLayerSelected => "No layer selected",
        PhotoTextKey::FilterLiveLabel => "Live filter",
        PhotoTextKey::MaskLabel => "Mask",
        PhotoTextKey::FilterActive => "Filter enabled",
        PhotoTextKey::MaskActive => "Mask enabled",
        PhotoTextKey::FilterParamBrightness => "Brightness",
        PhotoTextKey::FilterParamContrast => "Contrast",
        PhotoTextKey::FilterParamRadius => "Radius",
        PhotoTextKey::FilterParamSaturation => "Saturation",
        PhotoTextKey::FilterParamHue => "Hue",
        PhotoTextKey::OpacityLabel => "Opacity",
        PhotoTextKey::BlendLabel => "Blend",
        PhotoTextKey::FilterList => "Filter list",
        PhotoTextKey::OpenImageFile => "Open an image…",
        PhotoTextKey::MoveHint => "Move / select",
        PhotoTextKey::PanHint => "Hand (pan view)",
        PhotoTextKey::ZoomHint => "Zoom",
        PhotoTextKey::BrushHint => "Brush",
        PhotoTextKey::EraserHint => "Eraser",
        PhotoTextKey::EyedropperHint => "Eyedropper",
        PhotoTextKey::MoveHintText => "Drag: move layer — wheel: zoom",
        PhotoTextKey::PanHintText => "Drag: pan view — wheel: zoom",
        PhotoTextKey::ZoomHintText => "Click: zoom in — right-click: zoom out",
        PhotoTextKey::BrushHintText => "Drag: paint — release: commit stroke",
        PhotoTextKey::EraserHintText => "Drag: erase — release: commit",
        PhotoTextKey::EyedropperHintText => "Click canvas to sample color",
        PhotoTextKey::SizeLabel => "Size",
        PhotoTextKey::ColorLabel => "Color",
        PhotoTextKey::MoveTitle => "Move",
        PhotoTextKey::GridLabel => "Grid",
        PhotoTextKey::NoDocumentOpen => "No open document",
        PhotoTextKey::WelcomeBody => "Create a document or open an image to start.",
        PhotoTextKey::NoDocument => "No document",
        PhotoTextKey::CreateDocumentTitle => "New document",
        PhotoTextKey::PrintTab => "Print",
        PhotoTextKey::DigitalTab => "Digital",
        PhotoTextKey::UnitLabel => "Unit",
        PhotoTextKey::WidthLabel => "Width",
        PhotoTextKey::HeightLabel => "Height",
        PhotoTextKey::OrientationLabel => "Orientation",
        PhotoTextKey::PortraitLabel => "Portrait",
        PhotoTextKey::LandscapeLabel => "Landscape",
        PhotoTextKey::PresetA4Portrait => "A4 Portrait",
        PhotoTextKey::PresetA4Landscape => "A4 Landscape",
        PhotoTextKey::PresetLetterPortrait => "Letter Portrait",
        PhotoTextKey::PresetLetterLandscape => "Letter Landscape",
        PhotoTextKey::PresetSquare => "Square",
        PhotoTextKey::Preset4kUhd => "4K UHD",
        PhotoTextKey::Preset8kUhd => "8K UHD",
        PhotoTextKey::Preset16by9FullHd => "16:9 Full HD",
        PhotoTextKey::ExportTitle => "Export",
        PhotoTextKey::FolderLabel => "Folder",
        PhotoTextKey::FolderPlaceholder => "Export folder",
        PhotoTextKey::NameLabel => "Name",
        PhotoTextKey::FileNamePlaceholder => "File name",
        PhotoTextKey::FormatLabel => "Format",
        PhotoTextKey::QualityLabel => "Quality",
        PhotoTextKey::DestinationLabel => "Destination:",
        PhotoTextKey::HelpTitle => "Photo help",
        PhotoTextKey::HelpToolsText => "Tools: move, hand, zoom, brush, eraser, eyedropper.",
        PhotoTextKey::HelpLayersText => {
            "Layers: click = select, double-click name = rename, drag and drop = reorder. Panel bottom: image, empty, duplicate, mask, filters, delete."
        }
        PhotoTextKey::HelpFileText => {
            "File > New document: format, dimensions (px, in, cm, pica), portrait/landscape orientation. Export: folder, PNG/JPEG/JPG/GIF format, confirm."
        }
        PhotoTextKey::LayerKindPixel => "Pixels",
        PhotoTextKey::LayerKindGroup => "Group",
        PhotoTextKey::LayerKindAdjustment => "Adjustment",
        PhotoTextKey::LayerKindVector => "Vector",
        PhotoTextKey::AddMask => "Add mask",
        PhotoTextKey::DeleteLayer => "Delete layer",
        PhotoTextKey::CropPreviewToDocument => "Crop preview to document",
        PhotoTextKey::Zoom100 => "Zoom 100%",
        PhotoTextKey::ResetLayout => "Reset layout",
        PhotoTextKey::HistoryEmpty => "No changes yet",
        PhotoTextKey::HistoryInitial => "Initial state",
        PhotoTextKey::About => "About",
        PhotoTextKey::NewVectorLayer => "New vector shape",
        PhotoTextKey::ShapeRect => "Rectangle",
        PhotoTextKey::ShapeEllipse => "Ellipse",
        PhotoTextKey::ShapePolygon => "Polygon",
        PhotoTextKey::ShapeStar => "Star",
        PhotoTextKey::NodeHint => "Nodes",
        PhotoTextKey::NodeHintText => "Drag: move node — wheel: zoom",
        PhotoTextKey::MaskView => "Mask",
        PhotoTextKey::FilterParamInBlack => "Input black",
        PhotoTextKey::FilterParamInWhite => "Input white",
        PhotoTextKey::FilterParamGamma => "Gamma",
        PhotoTextKey::FilterParamOutBlack => "Output black",
        PhotoTextKey::FilterParamOutWhite => "Output white",
        PhotoTextKey::FilterParamLightness => "Lightness",
        PhotoTextKey::PresetLabel => "Preset",
        PhotoTextKey::PresetCustom => "Custom",
        PhotoTextKey::PresetDefault => "Default",
        PhotoTextKey::PresetLevelsHighContrast => "High contrast",
        PhotoTextKey::PresetLevelsBrighten => "Brighten",
        PhotoTextKey::PresetLevelsDarken => "Darken",
        PhotoTextKey::PresetLevelsFaded => "Faded",
        PhotoTextKey::PresetCurvesSoftContrast => "Soft contrast",
        PhotoTextKey::PresetCurvesStrongContrast => "Strong contrast",
        PhotoTextKey::PresetCurvesBrighten => "Brighten",
        PhotoTextKey::PresetCurvesDarken => "Darken",
        PhotoTextKey::PresetCurvesMatte => "Matte",
        PhotoTextKey::PresetCurvesInvert => "Invert",
        PhotoTextKey::PresetHslBlackAndWhite => "Black and white",
        PhotoTextKey::PresetHslMuted => "Muted",
        PhotoTextKey::PresetHslVivid => "Vivid",
        PhotoTextKey::PresetHslBrighten => "Brighten",
        PhotoTextKey::PresetHslComplementary => "Complementary",
        PhotoTextKey::CurveEditorHint => "Click: add point · drag: move · double-click: remove",
        PhotoTextKey::CurveReset => "Reset curve",
        PhotoTextKey::HistogramEmpty => "No image to analyze",
        PhotoTextKey::HistogramLuma => "Luminance",
        PhotoTextKey::HistogramRgb => "RGB",
        PhotoTextKey::EstimatedSizeLabel => "Estimated size:",
        PhotoTextKey::EstimatingSize => "estimating…",
        PhotoTextKey::WebpLosslessNote => "Lossless WebP, transparency kept",
        PhotoTextKey::TiffNote => "Uncompressed TIFF, transparency kept",
        PhotoTextKey::SizeUnitKilobytes => "KB",
        PhotoTextKey::SizeUnitMegabytes => "MB",
        PhotoTextKey::HistoryOpVisibility => "Visibility",
        PhotoTextKey::HistoryOpIsolate => "Isolate",
        PhotoTextKey::HistoryOpReorder => "Reorder",
        PhotoTextKey::HistoryOpNest => "Nest",
        PhotoTextKey::HistoryOpFold => "Fold / unfold",
        PhotoTextKey::HistoryOpOpacity => "Opacity",
        PhotoTextKey::HistoryOpBlend => "Blend mode",
        PhotoTextKey::HistoryOpBrushStroke => "Brush stroke",
        PhotoTextKey::HistoryOpPaintMask => "Paint mask",
        PhotoTextKey::HistoryOpMove => "Move",
        PhotoTextKey::HistoryOpTransform => "Transform",
        PhotoTextKey::HistoryOpVectorNodes => "Vector nodes",
        PhotoTextKey::HistoryOpOpenImage => "Open image",
        PhotoTextKey::HistoryOpNewLayer => "New layer",
        PhotoTextKey::HistoryOpNewShape => "New shape",
        PhotoTextKey::HistoryOpDuplicate => "Duplicate",
        PhotoTextKey::HistoryOpDuplicateFilter => "Duplicate filter",
        PhotoTextKey::HistoryOpDelete => "Delete",
        PhotoTextKey::HistoryOpAddFilter => "Add filter",
        PhotoTextKey::HistoryOpRename => "Rename",
        PhotoTextKey::HistoryOpAddMask => "Add mask",
        PhotoTextKey::HistoryOpRemoveMask => "Remove mask",
        PhotoTextKey::HistoryOpMoveFilter => "Move filter",
        PhotoTextKey::HistoryOpMoveMask => "Move mask",
        PhotoTextKey::HistoryOpRemoveFilter => "Remove filter",
        PhotoTextKey::HistoryOpFlipHorizontal => "Flip horizontal",
        PhotoTextKey::HistoryOpFlipVertical => "Flip vertical",
        PhotoTextKey::HistoryOpRotateClockwise => "Rotate clockwise",
        PhotoTextKey::HistoryOpRotateCounterclockwise => "Rotate counterclockwise",
        PhotoTextKey::HistoryOpCropToDocument => "Crop to document",
        PhotoTextKey::HistoryOpToggleFilter => "Toggle filter",
        PhotoTextKey::HistoryOpFilterSetting => "Filter setting",
        PhotoTextKey::HistoryOpPreset => "Preset",
        PhotoTextKey::HistoryOpToggleMask => "Toggle mask",
        PhotoTextKey::HistoryOpEdit => "Edit",
        PhotoTextKey::LabelSeparator => ": ",
        PhotoTextKey::StatusReady => "Ready",
        PhotoTextKey::StatusExported => "Exported",
        PhotoTextKey::StatusSaved => "Saved",
        PhotoTextKey::StatusSaving => "Saving",
        PhotoTextKey::StatusLoading => "Loading",
        PhotoTextKey::StatusPickerUnavailable => "File dialog unavailable",
        PhotoTextKey::ErrorOpenImage => "Cannot open image",
        PhotoTextKey::ErrorFlip => "Cannot flip",
        PhotoTextKey::ErrorRotate => "Cannot rotate",
        PhotoTextKey::ErrorCropTransformed => "Cannot crop: straighten the layer first",
        PhotoTextKey::ErrorCropOutside => "Cannot crop: the layer is outside the document",
        PhotoTextKey::ErrorCrop => "Cannot crop",
        PhotoTextKey::ErrorUnknownFilter => "Unknown filter",
        PhotoTextKey::ErrorUnknownPreset => "Unknown preset",
        PhotoTextKey::ErrorNothingToExport => "Nothing to export (empty document)",
        PhotoTextKey::ErrorExport => "Export failed",
        PhotoTextKey::ErrorSaveProject => "Cannot save the project",
        PhotoTextKey::ErrorUnsupportedProjectVersion => "Unsupported project version",
        PhotoTextKey::ErrorProjectVersionHint => "projects in format 1 and 2 can no longer be read",
        PhotoTextKey::ErrorLoadProject => "Cannot open the project",
        PhotoTextKey::DefaultLayerName => "Layer",
        PhotoTextKey::DefaultShapeName => "Shape",
        PhotoTextKey::DefaultMaskName => "Mask",
        PhotoTextKey::DefaultGroupName => "Group",
        PhotoTextKey::UntitledDocument => "Untitled",
        PhotoTextKey::FilterNameBrightnessContrast => "Brightness / Contrast",
        PhotoTextKey::FilterNameBlur => "Blur",
        PhotoTextKey::FilterNameColorCorrect => "Color correction",
        PhotoTextKey::FilterNameLevels => "Levels",
        PhotoTextKey::FilterNameCurves => "Curves",
        PhotoTextKey::FilterNameHsl => "Hue / Saturation / Lightness",
        PhotoTextKey::WindowMinimize => "Minimize",
        PhotoTextKey::WindowMaximize => "Maximize",
        PhotoTextKey::AdjustmentsTitle => "Adjustments",
        PhotoTextKey::NonDestructive => "Non-destructive",
        PhotoTextKey::AdjustmentsEmpty => "Select a filter or mask in the layers",
        PhotoTextKey::HistogramMidtones => "Midtones",
        PhotoTextKey::HistogramMean => "Mean",
        PhotoTextKey::HistogramStdDev => "Std. dev.",
        PhotoTextKey::HidePanel => "Hide panel",
        PhotoTextKey::BitDepth8 => "8-bit",
        PhotoTextKey::ToolLabel => "Tool",
        PhotoTextKey::SwapColors => "Swap colors",
        PhotoTextKey::PrimaryColor => "Primary color",
        PhotoTextKey::SecondaryColor => "Secondary color",
        PhotoTextKey::SelectionMenu => "Select",
        PhotoTextKey::SelectAll => "Select all",
        PhotoTextKey::Deselect => "Deselect",
        PhotoTextKey::InvertSelection => "Invert selection",
        PhotoTextKey::MaskFromSelection => "Mask from selection",
        PhotoTextKey::SelectHint => "Selection",
        PhotoTextKey::SelectHintText => {
            "Drag: select — Shift: add, Alt: subtract — click: deselect"
        }
        PhotoTextKey::SelectionRect => "Rectangle",
        PhotoTextKey::SelectionEllipse => "Ellipse",
        PhotoTextKey::HistoryOpMaskFromSelection => "Mask from selection",
        PhotoTextKey::HardnessLabel => "Hardness",
        PhotoTextKey::FlowLabel => "Flow",
        PhotoTextKey::SpacingLabel => "Spacing",
        PhotoTextKey::StabilizerLabel => "Stabilizer",
        PhotoTextKey::LassoHint => "Lasso",
        PhotoTextKey::LassoHintText => "Drag: draw the selection — Shift: add, Alt: subtract",
        PhotoTextKey::WandHint => "Magic wand",
        PhotoTextKey::WandHintText => "Click: select similar colors — Shift: add, Alt: subtract",
        PhotoTextKey::ToleranceLabel => "Tolerance",
        PhotoTextKey::ContiguousLabel => "Contiguous",
        PhotoTextKey::MaskDensityLabel => "Density",
        PhotoTextKey::MaskFeatherLabel => "Feather",
        PhotoTextKey::MaskInvertLabel => "Invert",
        PhotoTextKey::HistoryOpMaskSettings => "Mask settings",
        PhotoTextKey::BucketHint => "Paint bucket",
        PhotoTextKey::BucketHintText => "Click: fill the similar-color area",
        PhotoTextKey::GradientHint => "Gradient",
        PhotoTextKey::GradientHintText => "Drag: gradient from primary to secondary color",
        PhotoTextKey::HistoryOpFill => "Fill",
        PhotoTextKey::HistoryOpGradient => "Gradient",
        PhotoTextKey::BlendNormal => "Normal",
        PhotoTextKey::BlendMultiply => "Multiply",
        PhotoTextKey::BlendScreen => "Screen",
        PhotoTextKey::BlendOverlay => "Overlay",
        PhotoTextKey::BlendDarken => "Darken",
        PhotoTextKey::BlendLighten => "Lighten",
        PhotoTextKey::BlendColorDodge => "Color Dodge",
        PhotoTextKey::BlendColorBurn => "Color Burn",
        PhotoTextKey::BlendHardLight => "Hard Light",
        PhotoTextKey::BlendSoftLight => "Soft Light",
        PhotoTextKey::BlendDifference => "Difference",
        PhotoTextKey::BlendExclusion => "Exclusion",
        PhotoTextKey::BlendLinearDodge => "Linear Dodge (Add)",
        PhotoTextKey::BlendLinearBurn => "Linear Burn",
        PhotoTextKey::CloneHint => "Clone stamp",
        PhotoTextKey::CloneHintText => "Alt+click: set the source — drag: clone",
        PhotoTextKey::HistoryOpClone => "Clone stamp",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// French keeps the historical rendering character-for-character
    /// (including accent-less `apercu` and the `100 %` space).
    #[test]
    fn french_matches_legacy_menu_strings() {
        let texts = PhotoCatalog::new(Language::Fr);
        assert_eq!(texts.get(PhotoTextKey::CloseDocument), "Fermer le document");
        assert_eq!(texts.get(PhotoTextKey::OpenProject), "Ouvrir un projet");
        assert_eq!(texts.get(PhotoTextKey::SaveAs), "Enregistrer sous");
        assert_eq!(
            texts.get(PhotoTextKey::NewEmptyLayer),
            "Nouveau calque vide"
        );
        assert_eq!(
            texts.get(PhotoTextKey::LayerFromImage),
            "Calque depuis une image"
        );
        assert_eq!(
            texts.get(PhotoTextKey::DuplicateLayer),
            "Dupliquer le calque"
        );
        assert_eq!(texts.get(PhotoTextKey::FlipHorizontal), "Miroir horizontal");
        assert_eq!(texts.get(PhotoTextKey::FlipVertical), "Miroir vertical");
        assert_eq!(
            texts.get(PhotoTextKey::RotateClockwise),
            "Rotation 90° horaire"
        );
        assert_eq!(
            texts.get(PhotoTextKey::RotateCounterclockwise),
            "Rotation 90° antihoraire"
        );
        assert_eq!(
            texts.get(PhotoTextKey::CropToDocument),
            "Rogner au document"
        );
        assert_eq!(texts.get(PhotoTextKey::Transform), "Transformation");
        assert_eq!(texts.get(PhotoTextKey::AddMask), "Ajouter un masque");
        assert_eq!(texts.get(PhotoTextKey::DeleteLayer), "Supprimer le calque");
        assert_eq!(
            texts.get(PhotoTextKey::CropPreviewToDocument),
            "Rogner l'aperçu au document"
        );
        assert_eq!(texts.get(PhotoTextKey::Zoom100), "Zoom 100 %");
        assert_eq!(
            texts.get(PhotoTextKey::ResetLayout),
            "Réinitialiser la disposition"
        );
        assert_eq!(texts.get(PhotoTextKey::HistoryEmpty), "Aucune modification");
        assert_eq!(texts.get(PhotoTextKey::About), "À propos");
    }

    #[test]
    fn english_translates_every_key() {
        let texts = PhotoCatalog::new(Language::En);
        for key in ALL_PHOTO_TEXT_KEYS {
            assert!(!texts.get(*key).is_empty(), "clé non traduite : {key:?}");
        }
        assert_eq!(texts.get(PhotoTextKey::CloseDocument), "Close document");
    }

    #[test]
    fn all_keys_translated_in_both_languages() {
        for lang in [Language::Fr, Language::En] {
            let texts = PhotoCatalog::new(lang);
            for key in ALL_PHOTO_TEXT_KEYS {
                assert!(
                    !texts.get(*key).is_empty(),
                    "clé vide : {key:?} en {lang:?}"
                );
            }
        }
    }
}
