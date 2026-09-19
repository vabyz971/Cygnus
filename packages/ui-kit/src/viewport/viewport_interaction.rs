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

//! Interactions souris du canvas : pan, zoom molette et outils.
//!
//! [`zoom_factor_for_scroll`] est une fonction pure (delta molette →
//! facteur), testable sans contexte UI. [`handle_pointer`] applique
//! pan (bouton milieu, ou drag avec l'outil main), zoom (molette
//! ancrée au pointeur, clic avec l'outil loupe) et rapporte les
//! positions monde pour les outils de dessin.

use super::viewport_state::ViewportState;

/// Outil actif du canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewportTool {
    /// Déplacement / sélection.
    #[default]
    Move,
    /// Main : le drag déplace la vue.
    Pan,
    /// Loupe : clic = zoom avant, clic droit = zoom arrière.
    Zoom,
    /// Pinceau : le drag rapporte les positions monde.
    Brush,
}

impl ViewportTool {
    /// Curseur associé à l'outil.
    pub fn cursor(self) -> egui::CursorIcon {
        match self {
            Self::Move => egui::CursorIcon::Move,
            Self::Pan => egui::CursorIcon::Grab,
            Self::Zoom => egui::CursorIcon::ZoomIn,
            Self::Brush => egui::CursorIcon::Crosshair,
        }
    }
}

/// Action rapportée par [`handle_pointer`], à traiter par l'app
/// (mise à jour d'état local, ou envoi au moteur via channel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewportAction {
    /// La vue a été déplacée (pan).
    Panned,
    /// Le zoom a changé.
    Zoomed,
    /// Position monde du pointeur (clic ou drag avec un outil de
    /// dessin/déplacement), en pixels image.
    PointerAtWorld(egui::Vec2),
}

/// Facteur de zoom pour un delta de molette vertical.
///
/// Convention : molette haut (`delta_y > 0`) = zoom avant (facteur
/// `> 1`), molette bas = zoom arrière (`< 1`), aucun scroll = `1.0`.
/// Le facteur par événement est borné à [1/4, 4] (garde-fou
/// trackpads) ; les bornes de zoom restent appliquées par
/// [`ViewportState`].
pub fn zoom_factor_for_scroll(delta_y: f32) -> f32 {
    if delta_y == 0.0 {
        return 1.0;
    }
    (2.0f32.powf(delta_y * 0.002)).clamp(0.25, 4.0)
}

/// Applique pan/zoom/outils depuis les entrées pointeur.
///
/// À appeler après l'allocation du canvas, avec sa `response`.
/// Le zoom molette n'est pris en compte que si le canvas est survolé.
/// `viewport_center` est le centre écran du widget (géométrie réelle
/// `screen = center + offset + world * zoom`) : le zoom molette et la
/// loupe y sont ancrés pour que le point image sous la souris reste
/// fixe (zoom au niveau du curseur).
pub fn handle_pointer(
    ui: &mut egui::Ui,
    response: &egui::Response,
    state: &mut ViewportState,
    tool: ViewportTool,
    viewport_center: egui::Pos2,
) -> Vec<ViewportAction> {
    let mut actions = Vec::new();

    // Pan : bouton milieu toujours, bouton principal avec l'outil main.
    if response.dragged_by(egui::PointerButton::Middle)
        || (tool == ViewportTool::Pan && response.dragged_by(egui::PointerButton::Primary))
    {
        state.pan_by(response.drag_delta());
        actions.push(ViewportAction::Panned);
    }

    // Zoom molette ancré au pointeur (canvas survolé uniquement).
    if response.hovered() {
        let delta_y = ui.input(|input| input.smooth_scroll_delta()).y;
        let factor = zoom_factor_for_scroll(delta_y);
        if factor != 1.0
            && let Some(anchor) = ui.ctx().pointer_latest_pos()
        {
            state.zoom_by_around(factor, anchor, viewport_center);
            actions.push(ViewportAction::Zoomed);
        }
    }

    // Clic loupe : avant (gauche) / arrière (droit), ancré au pointeur.
    if tool == ViewportTool::Zoom
        && let Some(anchor) = ui.ctx().pointer_latest_pos()
    {
        if response.clicked_by(egui::PointerButton::Primary) {
            state.zoom_by_around(1.25, anchor, viewport_center);
            actions.push(ViewportAction::Zoomed);
        } else if response.clicked_by(egui::PointerButton::Secondary) {
            state.zoom_by_around(0.8, anchor, viewport_center);
            actions.push(ViewportAction::Zoomed);
        }
    }

    // Outils de dessin/déplacement : rapporter la position monde.
    if (tool == ViewportTool::Brush || tool == ViewportTool::Move)
        && (response.dragged_by(egui::PointerButton::Primary)
            || response.clicked_by(egui::PointerButton::Primary))
        && let Some(pointer) = ui.ctx().pointer_latest_pos()
    {
        actions.push(ViewportAction::PointerAtWorld(
            state.screen_to_world(pointer),
        ));
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_zoom_direction() {
        // Molette haut = zoom avant.
        assert!(zoom_factor_for_scroll(50.0) > 1.0);
        // Molette bas = zoom arrière.
        assert!(zoom_factor_for_scroll(-50.0) < 1.0);
        // Pas de scroll = pas de zoom.
        assert_eq!(zoom_factor_for_scroll(0.0), 1.0);
    }

    #[test]
    fn wheel_factor_bounded() {
        // Rafale trackpad : facteur borné, jamais de saut brutal.
        assert_eq!(zoom_factor_for_scroll(10_000.0), 4.0);
        assert_eq!(zoom_factor_for_scroll(-10_000.0), 0.25);
    }

    #[test]
    fn tool_cursors_are_distinct() {
        assert_ne!(ViewportTool::Pan.cursor(), ViewportTool::Zoom.cursor());
        assert_ne!(ViewportTool::Brush.cursor(), ViewportTool::Move.cursor());
    }
}
