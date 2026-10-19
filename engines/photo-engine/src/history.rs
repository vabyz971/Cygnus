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

//! HYBRID document history (undo/redo) — UI-independent. Two entry
//!
//! kinds depending on the edit nature:
//! - [`HistoryEntry::Snapshot`] — destructive/structural ops (paint,
//!   flip, crop, layer add/remove/reorder, project open). Snapshots are
//!   nearly free for PIXELS (shared via `Arc`) but clone the tree
//!   structure.
//! - [`HistoryEntry::Command`] — parameter micro-edits (opacity,
//!   transform, blend, visibility, rename, filter parameter). A few bytes
//!   per entry: ZERO tree cloning, and the returned [`UndoAction`]
//!   allows TARGETED render invalidation.
//!
//! Time coalescing (800 ms) kept for continuous gestures, on both
//! commands AND snapshots. Command twist: coalescing MERGES
//! transitions (keeps the gesture start's `old`, updates the final
//! `new`) so undo restores the gesture start AND redo reapplies
//! its FINAL value.

use std::time::{Duration, Instant};

use crate::command::Command;
use crate::document::{Document, LayerNode};

/// Fully restorable document state (tree + dimensions).
#[derive(Clone)]
pub struct Snapshot {
    pub doc_size: (u32, u32),
    pub root: Vec<LayerNode>,
}

/// Hybrid history entry.
#[derive(Clone)]
pub enum HistoryEntry {
    /// Full snapshot (destructive / structural ops)
    Snapshot(Snapshot),
    /// Lightweight old→new command (micro-edits)
    Command(Command),
}

/// What the app must do to the render after an undo/redo:
/// - [`UndoAction::FullRestore`]: re-render EVERYTHING (structure changed)
/// - [`UndoAction::Applied`]: the now-effective command — `cmd.render_event()`
///   tells targeted invalidation from recomposite
#[derive(Debug)]
pub enum UndoAction {
    FullRestore,
    Applied(Command),
}

/// Coalescing window: two same-key pushes within this window create a
/// single restore point (the gesture start's).
const COALESCE_WINDOW: Duration = Duration::from_millis(800);

#[derive(Default)]
pub struct History {
    /// STRICT past states (the current state never appears there), oldest
    /// to newest.
    undo: Vec<HistoryEntry>,
    /// Undone future states (newest to oldest).
    redo: Vec<HistoryEntry>,
    limit: usize,
    last_key: Option<(u64, Instant)>,
}

impl History {
    /// Create a new history with default limit.
    #[must_use]
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: 50,
            last_key: None,
        }
    }

    /// Resets history (new document, project open): no going back before
    /// the next push.
    pub fn reset(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_key = None;
    }

    // -- Pushes ------------------------------------------------------------

    /// Pushes a PRE-mutation snapshot (destructive ops). Call BEFORE
    /// modifying the document.
    pub fn push_snapshot(&mut self, pre: Snapshot) {
        self.push_inner(HistoryEntry::Snapshot(pre));
        self.last_key = None;
    }

    /// Pushes a lightweight command WITH coalescing (continuous gestures:
    /// sliders, name typing, filter settings…). The command must describe
    ///
    /// the CURRENT old→new transition; if the same key is re-pushed within
    /// the window, the existing entry is MERGED (initial `old` kept,
    /// `new` refreshed) rather than duplicated — else redo would restore
    /// an intermediate value. Applying to the document stays the caller's
    ///
    /// job ([`Document::apply_command`]),
    /// BEFORE or AFTER this push.
    pub fn push_command(&mut self, key: u64, command: Command) {
        match self.last_key {
            Some((k, t)) if k == key && t.elapsed() < COALESCE_WINDOW => {
                let merged = match self.undo.last_mut() {
                    Some(HistoryEntry::Command(top)) => top.merge_forward(&command),
                    _ => false,
                };
                if !merged {
                    // Same key but incompatible edit: distinct entry
                    self.push_inner(HistoryEntry::Command(command));
                }
                self.last_key = Some((k, Instant::now()));
            }
            _ => {
                self.push_inner(HistoryEntry::Command(command));
                self.last_key = Some((key, Instant::now()));
            }
        }
    }

    /// Pushes a lightweight command WITHOUT coalescing (discrete edits:
    /// visibility toggle, ±90° rotation, mode change…).
    pub fn push_command_immediate(&mut self, command: Command) {
        self.push_inner(HistoryEntry::Command(command));
        self.last_key = None;
    }

    fn push_inner(&mut self, entry: HistoryEntry) {
        self.redo.clear();
        self.undo.push(entry);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
    }

    // -- Undo / redo ------------------------------------------

    /// Undoes: applies the last command's INVERSE to the document (or
    /// restores the previous snapshot) and returns the render action.
    ///
    /// Convention: both stacks store FORWARD commands (the transition
    /// toward the future). Undo therefore applies [`Command::inverse`],
    /// redo reapplies as is.
    pub fn undo(&mut self, doc: &mut Document) -> Option<UndoAction> {
        match self.undo.pop()? {
            HistoryEntry::Snapshot(snapshot) => {
                let current = doc.snapshot();
                self.redo.push(HistoryEntry::Snapshot(current));
                doc.restore_snapshot(snapshot);
                self.last_key = None;
                Some(UndoAction::FullRestore)
            }
            HistoryEntry::Command(forward) => {
                let backward = forward.inverse();
                doc.apply_command(backward.clone());
                self.redo.push(HistoryEntry::Command(forward.clone()));
                self.last_key = None;
                Some(UndoAction::Applied(backward))
            }
        }
    }

    /// Redoes: reapplies the last undone command (or restores the undone
    /// snapshot) and pushes the entry onto the undo stack.
    pub fn redo(&mut self, doc: &mut Document) -> Option<UndoAction> {
        match self.redo.pop()? {
            HistoryEntry::Snapshot(snapshot) => {
                let current = doc.snapshot();
                self.undo.push(HistoryEntry::Snapshot(current));
                doc.restore_snapshot(snapshot);
                self.last_key = None;
                Some(UndoAction::FullRestore)
            }
            HistoryEntry::Command(forward) => {
                doc.apply_command(forward.clone());
                self.undo.push(HistoryEntry::Command(forward.clone()));
                self.last_key = None;
                Some(UndoAction::Applied(forward))
            }
        }
    }

    /// Whether an undo is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether a redo is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Number of entries in the UNDO stack (useful for tests proving the
    /// ABSENCE of an entry, where `can_undo`/`can_redo` may already be
    /// `false` before the test).
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::PixelLayer;
    use datatypes::ParamValue;
    use image::DynamicImage;
    use std::sync::Arc;
    use uuid::Uuid;

    fn pixel(value: u8) -> LayerNode {
        let img = DynamicImage::ImageRgba8(image::ImageBuffer::from_pixel(
            2,
            2,
            image::Rgba([value, 0, 0, 255]),
        ));
        LayerNode::Pixel(PixelLayer::new(format!("c{value}"), Arc::new(img)))
    }

    fn snap(values: &[u8]) -> Snapshot {
        Snapshot {
            doc_size: (4, 4),
            root: values.iter().map(|&v| pixel(v)).collect(),
        }
    }

    fn doc_with(values: &[u8]) -> Document {
        let mut doc = Document::new(4, 4);
        doc.restore_snapshot(snap(values));
        doc
    }

    fn opacity_cmd(layer_id: Uuid, old: f32, new: f32) -> Command {
        Command::SetOpacity { layer_id, old, new }
    }

    #[test]
    fn snapshot_undo_redo_aller_retour() {
        let mut h = History::new();
        let mut doc = doc_with(&[1]);
        assert!(!h.can_undo());

        h.push_snapshot(doc.snapshot());
        assert!(h.can_undo());

        // The app meanwhile added a layer; undoing
        doc.push_layer(pixel(2));
        let action = h.undo(&mut doc).expect("undo");
        assert!(matches!(action, UndoAction::FullRestore));
        assert_eq!(doc.root.len(), 1);
        assert!(!h.can_undo());
        assert!(h.can_redo());

        // Redo
        let action = h.redo(&mut doc).expect("redo");
        assert!(matches!(action, UndoAction::FullRestore));
        assert_eq!(doc.root.len(), 2);
        assert!(!h.can_redo());
        assert!(h.can_undo(), "va-et-vient possible");
    }

    #[test]
    fn commande_undo_redo_retablit_les_deux_valeurs() {
        let mut h = History::new();
        let mut doc = doc_with(&[1]);
        let id = doc.root[0].id();

        // 50 → 80 edit recorded as a command
        let cmd = opacity_cmd(id, 50.0, 80.0);
        h.push_command_immediate(cmd);
        let _inverse = doc.apply_command(opacity_cmd(id, 50.0, 80.0));
        assert!(
            (doc.find(id)
                .expect("layer should exist after command")
                .opacity()
                - 80.0)
                .abs()
                < f32::EPSILON
        );

        // Undo → 50, Redo → 80
        let action = h.undo(&mut doc).expect("undo");
        assert!(matches!(&action, UndoAction::Applied(c)
                if c.render_event() == crate::command::RenderEvent::NodeInvalidated(id)));
        assert!(
            (doc.find(id)
                .expect("layer should exist after undo")
                .opacity()
                - 50.0)
                .abs()
                < f32::EPSILON
        );

        let action = h.redo(&mut doc).expect("redo");
        assert!(matches!(action, UndoAction::Applied(_)));
        assert!(
            (doc.find(id)
                .expect("layer should exist after redo")
                .opacity()
                - 80.0)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn coalescence_fusionne_et_le_restaurateur_est_correct() {
        let mut h = History::new();
        let mut doc = doc_with(&[1]);
        let id = doc.root[0].id();

        // Continuous 50→60→70→80 gesture: three same-key pushes
        for (old, new) in [(50.0, 60.0), (60.0, 70.0), (70.0, 80.0)] {
            h.push_command(coalesce(id), opacity_cmd(id, old, new));
            let _ = doc.apply_command(opacity_cmd(id, old, new));
        }
        // A single entry pushed
        assert_eq!(h.undo.len(), 1, "coalescence = une seule entrée");

        // Undo → gesture start (50)
        h.undo(&mut doc).expect("undo");
        assert!(
            (doc.find(id)
                .expect("layer should exist after undo coalesced")
                .opacity()
                - 50.0)
                .abs()
                < f32::EPSILON
        );
        assert!(!h.can_undo());

        // Redo → gesture's FINAL value (80), not the first (60)
        h.redo(&mut doc).expect("redo");
        assert!(
            (doc.find(id)
                .expect("layer should exist after redo coalesced")
                .opacity()
                - 80.0)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn coalescence_snapshot_conservee_aussi() {
        // Snapshots keep their own window via shared last_key: two snapshot
        // pushes do NOT coalesce (push_snapshot resets), but a command followed
        // by a snapshot starts a new point.
        let mut h = History::new();
        h.push_command(coalesce(Uuid::nil()), opacity_cmd(Uuid::nil(), 1.0, 2.0));
        h.reset(); // isolates the next test
        h.push_snapshot(snap(&[1]));
        h.push_snapshot(snap(&[2]));
        assert_eq!(h.undo.len(), 2, "snapshots jamais fusionnés");
    }

    #[test]
    fn melange_hybride_sequencement_coherent() {
        let mut h = History::new();
        let mut doc = doc_with(&[1]);
        let id = doc.root[0].id();

        // 1) pre-structural snapshot then layer addition
        h.push_snapshot(doc.snapshot());
        doc.push_layer(pixel(2));

        // 2) 100 → 40 opacity micro-edit as a command
        h.push_command_immediate(opacity_cmd(id, 100.0, 40.0));
        let _ = doc.apply_command(opacity_cmd(id, 100.0, 40.0));

        // Undo #1: opacity back to 100 (command)
        h.undo(&mut doc).expect("undo cmd");
        assert!(
            (doc.find(id)
                .expect("layer should exist after undo hybrid")
                .opacity()
                - 100.0)
                .abs()
                < f32::EPSILON
        );

        // Undo #2: the added layer disappears (snapshot)
        h.undo(&mut doc).expect("undo snap");
        assert_eq!(doc.root.len(), 1);
        assert!(!h.can_undo());

        // Redo ×2: snapshot (layer returns) then command (opacity 40)
        h.redo(&mut doc).expect("redo snap");
        assert_eq!(doc.root.len(), 2);
        h.redo(&mut doc).expect("redo cmd");
        assert!(
            (doc.find(id)
                .expect("layer should exist after redo hybrid")
                .opacity()
                - 40.0)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn commande_sur_noeud_absent_est_un_no_op_sur() {
        let mut h = History::new();
        let mut doc = doc_with(&[1]);
        let fantome = uuid::Uuid::new_v4();

        h.push_command_immediate(opacity_cmd(fantome, 10.0, 90.0));
        h.undo(&mut doc).expect("undo s'exécute sans panic");
        // Nothing moved, no crash
        assert_eq!(doc.root.len(), 1);
    }

    #[test]
    fn limite_memoire_et_base_preservee_hybride() {
        let mut h = History::new();
        let mut doc = doc_with(&[1]);
        for i in 0..200u64 {
            if i % 2 == 0 {
                h.push_snapshot(doc.snapshot());
            } else {
                let id = doc.root[0].id();
                let v = (i % 100) as f32;
                h.push_command_immediate(opacity_cmd(id, v, v + 1.0));
            }
        }
        let mut steps = 0;
        while h.undo(&mut doc).is_some() {
            steps += 1;
            assert!(steps < 300, "boucle infinie");
        }
        assert!((40..=51).contains(&steps), "limite non respectée: {steps}");
    }

    #[test]
    fn filtre_param_commande_invalide_le_cache_apparence() {
        use crate::document::FilterLayer;
        let mut doc = Document::new(2, 2);
        let img = DynamicImage::ImageRgba8(image::ImageBuffer::from_pixel(
            2,
            2,
            image::Rgba([100, 100, 100, 255]),
        ));
        let mut layer = PixelLayer::new("f", Arc::new(img));
        layer.filter_layers.push(FilterLayer::neutral(
            "brightness_contrast",
            Default::default(),
        ));
        doc.push_layer(LayerNode::Pixel(layer));
        let layer_id = doc.root[0].id();
        let pixel_layer = doc.pixel_layer(layer_id).expect("layer should exist");
        let filter_id = pixel_layer.filter_layers[0].id;

        let version_avant = pixel_layer.appearance_version;
        let cmd = Command::SetFilterParam {
            layer_id,
            filter_id,
            param_name: "brightness".into(),
            old: ParamValue::Float(0.0),
            new: ParamValue::Float(50.0),
        };
        let inverse = doc.apply_command(cmd);
        // Bumped version → the appearance cache knows it is stale
        assert!(
            doc.pixel_layer(layer_id)
                .expect("layer should still exist")
                .appearance_version
                > version_avant,
            "apply_command doit invalider l'apparence"
        );
        // Consistent inverse
        match inverse {
            Command::SetFilterParam { old, new, .. } => {
                assert_eq!(old, ParamValue::Float(50.0));
                assert_eq!(new, ParamValue::Float(0.0));
            }
            other => panic!("variante inattendue : {other:?}"),
        }
    }

    // -- Helpers -----------------------------------------------------------

    fn coalesce(id: Uuid) -> u64 {
        (id.as_u128() as u64).wrapping_mul(16).wrapping_add(1)
    }
}
