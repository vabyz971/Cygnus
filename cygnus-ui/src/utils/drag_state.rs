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

//! GENERIC index-based drag & drop state.
//!
//! Reusable: layers (photo), clips (video), tracks (audio).
//! Strictly domain-agnostic: no domain type here, only indexes.

/// Generic index-based drag & drop state.
#[derive(Debug, Clone, Default)]
pub struct ReorderDragState {
    /// Drag origin index, during a drag.
    pub dragging_index: Option<usize>,
    /// Last known mouse position during the drag.
    pub current_mouse_pos: egui::Pos2,
    /// Current insertion index, during a drag.
    pub target_index: Option<usize>,
    /// Current drop position (before / into / after the target).
    pub drop_position: DropPosition,
    /// True while a drag is in progress.
    pub is_dragging: bool,
}

/// A drop's position relative to its target row (hierarchical
/// lists: `Into` = nest inside, the target must accept it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DropPosition {
    /// Insert before the target row.
    #[default]
    Before,
    /// Drop inside the target (nesting).
    Into,
    /// Insert after the target row.
    After,
}

impl ReorderDragState {
    /// Starts dragging the item at `index`.
    pub fn start(&mut self, index: usize) {
        self.dragging_index = Some(index);
        self.is_dragging = true;
    }

    /// Resets the state (drag end or cancel).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Ends the drag and returns `(from, to)` when a reorder must be
    /// applied, `None` otherwise. Resets the state in all cases.
    pub fn end_drag(&mut self) -> Option<(usize, usize)> {
        let result = match (self.dragging_index, self.target_index) {
            (Some(from), Some(to)) if from != to => Some((from, to)),
            _ => None,
        };
        self.reset();
        result
    }

    /// Ends the drag and returns `(from, to, position)` for
    /// hierarchical lists (`Into` = nest, same index as the origin
    /// accepted). Resets in all cases.
    pub fn end_drag_hierarchical(&mut self) -> Option<(usize, usize, DropPosition)> {
        let result = match (self.dragging_index, self.target_index) {
            (Some(from), Some(to)) => match self.drop_position {
                DropPosition::Into if from != to => Some((from, to, DropPosition::Into)),
                DropPosition::Into => None,
                position if from != to => Some((from, to, position)),
                _ => None,
            },
            _ => None,
        };
        self.reset();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_reset() {
        let mut state = ReorderDragState::default();
        assert!(!state.is_dragging);
        state.start(2);
        assert!(state.is_dragging);
        assert_eq!(state.dragging_index, Some(2));
        state.target_index = Some(5);
        assert_eq!(state.end_drag(), Some((2, 5)));
        // Reset after end_drag.
        assert!(!state.is_dragging);
        assert_eq!(state.dragging_index, None);
        assert_eq!(state.target_index, None);
    }

    #[test]
    fn end_drag_same_index_returns_none() {
        let mut state = ReorderDragState::default();
        state.start(3);
        state.target_index = Some(3);
        assert_eq!(state.end_drag(), None);
        assert!(!state.is_dragging);
    }

    #[test]
    fn end_drag_without_target_returns_none() {
        let mut state = ReorderDragState::default();
        state.start(0);
        assert_eq!(state.end_drag(), None);
    }

    #[test]
    fn end_drag_hierarchical_keeps_position() {
        use super::DropPosition;
        let mut state = ReorderDragState::default();
        state.start(1);
        state.target_index = Some(3);
        state.drop_position = DropPosition::After;
        assert_eq!(
            state.end_drag_hierarchical(),
            Some((1, 3, DropPosition::After))
        );
        assert!(!state.is_dragging);
        // Into onto itself: no-op (never a command).
        state.start(2);
        state.target_index = Some(2);
        state.drop_position = DropPosition::Into;
        assert_eq!(state.end_drag_hierarchical(), None);
        // Into elsewhere: kept (the engine validates next).
        state.start(2);
        state.target_index = Some(5);
        state.drop_position = DropPosition::Into;
        assert_eq!(
            state.end_drag_hierarchical(),
            Some((2, 5, DropPosition::Into))
        );
    }
}
