// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Translation keys of Cygnus Photo. Every key must have an entry in each
//! catalog of `i18n/`; the tests in `i18n.rs` check it.

cygnus_ui_kit::define_keys! {
    APP_NAME = "app.name";
    MENU_FILE = "menu.file";
    MENU_EDIT = "menu.edit";
    MENU_DOCUMENT = "menu.document";
    MENU_LAYER = "menu.layer";
    MENU_SELECT = "menu.select";
    MENU_FILTERS = "menu.filters";
    MENU_VIEW = "menu.view";
    MENU_WINDOW = "menu.window";
    MENU_HELP = "menu.help";
    FILE_NEW = "file.new";
    FILE_OPEN = "file.open";
    FILE_SAVE = "file.save";
    FILE_EXPORT = "file.export";
    FILE_QUIT = "file.quit";
    EDIT_UNDO = "edit.undo";
    EDIT_REDO = "edit.redo";
    EDIT_PREFERENCES = "edit.preferences";
    DOCUMENT_SETTINGS = "document.settings";
    DOCUMENT_IMAGE_SIZE = "document.image_size";
    LAYER_NEW = "layer.new";
    LAYER_DUPLICATE = "layer.duplicate";
    LAYER_DELETE = "layer.delete";
    LAYER_MERGE_DOWN = "layer.merge_down";
    SELECT_ALL = "select.all";
    SELECT_NONE = "select.none";
    SELECT_INVERT = "select.invert";
    FILTERS_GAUSSIAN_BLUR = "filters.gaussian_blur";
    FILTERS_UNSHARP_MASK = "filters.unsharp_mask";
    VIEW_ZOOM_IN = "view.zoom_in";
    VIEW_ZOOM_OUT = "view.zoom_out";
    VIEW_FIT = "view.fit";
    VIEW_ACTUAL_SIZE = "view.actual_size";
    WINDOW_RESET_LAYOUT = "window.reset_layout";
    HELP_ABOUT = "help.about";
    PERSONA_PHOTO = "persona.photo";
    PERSONA_DEVELOP = "persona.develop";
    PERSONA_LIQUIFY = "persona.liquify";
    PERSONA_TONE_MAPPING = "persona.tone_mapping";
    PERSONA_EXPORT = "persona.export";
    PERSONA_UNAVAILABLE = "persona.unavailable";
    ACTION_EXPORT = "action.export";
    TOOL_MOVE = "tool.move";
    TOOL_CROP = "tool.crop";
    TOOL_RECT_SELECT = "tool.rect_select";
    TOOL_LASSO = "tool.lasso";
    TOOL_MAGIC_WAND = "tool.magic_wand";
    TOOL_BRUSH = "tool.brush";
    TOOL_CLONE = "tool.clone";
    TOOL_ERASER = "tool.eraser";
    TOOL_EYEDROPPER = "tool.eyedropper";
    TOOL_PAN = "tool.pan";
    TOOL_ZOOM = "tool.zoom";
    TOOL_COLORS = "tool.colors";
    TOOL_SWAP_COLORS = "tool.swap_colors";
    OPTION_SIZE = "option.size";
    OPTION_HARDNESS = "option.hardness";
    OPTION_OPACITY = "option.opacity";
    OPTION_FLOW = "option.flow";
    OPTION_MODE = "option.mode";
    OPTION_STABILIZER = "option.stabilizer";
    OPTION_MORE = "option.more";
    UNIT_PX = "unit.px";
    UNIT_PERCENT = "unit.percent";
    BLEND_NORMAL = "blend.normal";
    BLEND_MULTIPLY = "blend.multiply";
    BLEND_SCREEN = "blend.screen";
    BLEND_OVERLAY = "blend.overlay";
    BLEND_SOFT_LIGHT = "blend.soft_light";
    BLEND_DARKEN = "blend.darken";
    BLEND_LIGHTEN = "blend.lighten";
    BLEND_DIFFERENCE = "blend.difference";
    BLEND_HUE = "blend.hue";
    BLEND_SATURATION = "blend.saturation";
    BLEND_COLOR = "blend.color";
    BLEND_LUMINOSITY = "blend.luminosity";
    DOCUMENT_CLOSE = "document.close";
    DOCUMENT_DIMENSIONS = "document.dimensions";
    DOCUMENT_INFO = "document.info";
    DOCUMENT_FORMAT_8 = "document.format_8";
    DOCUMENT_FORMAT_16 = "document.format_16";
    NUMBER_DECIMAL_SEPARATOR = "number.decimal_separator";
    DEMO_TITLE = "demo.title";
    DEMO_LAYER_SKY = "demo.layer_sky";
    DEMO_LAYER_SUN = "demo.layer_sun";
    DEMO_LAYER_MOUNTAINS = "demo.layer_mountains";
    DEMO_LAYER_LAKE = "demo.layer_lake";
    DEMO_LAYER_MIST = "demo.layer_mist";
    CANVAS_ERROR = "canvas.error";
    ZOOM_IN = "zoom.in";
    ZOOM_OUT = "zoom.out";
    ZOOM_FIT = "zoom.fit";
    STATUS_SAVED = "status.saved";
    STATUS_GPU = "status.gpu";
    STATUS_SHORTCUTS = "status.shortcuts";
    PANEL_HISTOGRAM = "panel.histogram";
    PANEL_ADJUSTMENTS = "panel.adjustments";
    PANEL_LAYERS = "panel.layers";
    PANEL_HISTORY = "panel.history";
    PANEL_MENU = "panel.menu";
    HISTOGRAM_RGB = "histogram.rgb";
    HISTOGRAM_MIDTONES = "histogram.midtones";
    HISTOGRAM_MEAN = "histogram.mean";
    HISTOGRAM_STD_DEV = "histogram.std_dev";
    ADJUSTMENT_CURVES = "adjustment.curves";
    ADJUSTMENT_AXES = "adjustment.axes";
    ADJUSTMENT_STRENGTH = "adjustment.strength";
    ADJUSTMENT_MASK_DENSITY = "adjustment.mask_density";
    LAYERS_OPACITY = "layers.opacity";
    LAYERS_VISIBILITY = "layers.visibility";
    LAYERS_LOCKED = "layers.locked";
    LAYERS_MASK = "layers.mask";
    LAYERS_SETTINGS = "layers.settings";
    LAYERS_ADD = "layers.add";
    LAYERS_ADD_GROUP = "layers.add_group";
    LAYERS_ADD_ADJUSTMENT = "layers.add_adjustment";
    LAYERS_ADD_MASK = "layers.add_mask";
    LAYERS_ADD_FILTER = "layers.add_filter";
    LAYERS_DELETE = "layers.delete";
    PLACEHOLDER_EXIF = "placeholder.exif";
    PLACEHOLDER_MASK_TARGET = "placeholder.mask_target";
    PLACEHOLDER_STATUS_ACTION = "placeholder.status_action";
    PLACEHOLDER_LAYER_SKY = "placeholder.layer_sky";
    PLACEHOLDER_LAYER_SKY_KIND = "placeholder.layer_sky_kind";
    PLACEHOLDER_LAYER_CURVES = "placeholder.layer_curves";
    PLACEHOLDER_LAYER_CURVES_KIND = "placeholder.layer_curves_kind";
    PLACEHOLDER_LAYER_BALANCE = "placeholder.layer_balance";
    PLACEHOLDER_LAYER_BALANCE_KIND = "placeholder.layer_balance_kind";
    PLACEHOLDER_LAYER_SOURCE = "placeholder.layer_source";
    PLACEHOLDER_LAYER_SOURCE_KIND = "placeholder.layer_source_kind";
    PLACEHOLDER_HISTORY_ENTRY = "placeholder.history_entry";
    PLACEHOLDER_HISTORY_TIME = "placeholder.history_time";
}
