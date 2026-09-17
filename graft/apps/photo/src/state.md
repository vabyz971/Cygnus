# apps/photo/src/state.rs

- PhotoUiState · struct · L44-L77 — pub struct PhotoUiState
- FilterModalState · struct · L81-L86 — pub struct FilterModalState
- OpenDocument · struct · L89-L102 — pub struct OpenDocument
- PhotoShellState · struct · L105-L122 — pub struct PhotoShellState
- default · function · L128-L137 — fn default() -> Self
- PhotoRuntimeState · struct · L141-L148 — pub struct PhotoRuntimeState
- new · function · L152-L162 — pub fn new() -> Self
- default · function · L166-L168 — fn default() -> Self
- apply_response · function · L176-L222 — pub fn apply_response(ctx: &egui::Context, ui: &mut PhotoUiState, response: PhotoEngineResponse)
- sample_preview_color · function · L226-L241 — pub fn sample_preview_color(preview: &PreviewImage, x: f32, y: f32) -> Option<[u8; 3]>
- tests · module · L244-L309 — mod tests
- sample_preview_color_clamps · function · L248-L270 — fn sample_preview_color_clamps()
- engine_error_sets_status · function · L273-L284 — fn engine_error_sets_status()
- export_done_sets_status · function · L287-L298 — fn export_done_sets_status()
- shell_defaults_to_usable_workspace · function · L301-L308 — fn shell_defaults_to_usable_workspace()
