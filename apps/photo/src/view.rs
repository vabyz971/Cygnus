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

//! State of the view that is not part of the document (docs/decisions/
//! 0025): selection, folded groups, the name being edited, the curve
//! channel shown. It never goes through the command bus.

use std::collections::BTreeSet;

use cygnus_photo_core::{Document, NodeId};

/// What the panels show and edit.
#[derive(Default)]
pub struct ViewState {
    /// Selected layer.
    pub selected: Option<NodeId>,
    /// The mask of the selected layer is the target, not its pixels.
    pub mask: bool,
    /// Groups shown folded.
    pub collapsed: BTreeSet<NodeId>,
    /// Layer being renamed, with the text typed so far.
    pub renaming: Option<(NodeId, String)>,
    /// Curve shown by the curve editor: 0 master, 1 red, 2 green, 3 blue.
    pub curve_channel: usize,
    /// Curve point being dragged.
    pub curve_point: Option<usize>,
}

impl ViewState {
    /// Forgets what no longer exists in `document`.
    pub fn follow(&mut self, document: &Document) {
        if self.selected.is_some_and(|id| document.node(id).is_none()) {
            self.selected = None;
            self.mask = false;
        }
        if let Some(id) = self.selected
            && document.node(id).is_some_and(|node| node.mask.is_none())
        {
            self.mask = false;
        }
        self.collapsed.retain(|id| document.node(*id).is_some());
    }
}
