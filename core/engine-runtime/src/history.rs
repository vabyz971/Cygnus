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

//! Undo/redo history over snapshots, with gesture coalescing.
//!
//! Contract (same as the photo worker it was extracted from):
//! - [`History::record`] is called BEFORE mutating, with the snapshot of
//!   the state about to change; it drains redo.
//! - A continuous gesture (slider, drag) passes a gesture key: while the
//!   same key stays open, further ticks record nothing, so the whole
//!   gesture undoes in one step. Any other record, undo or redo closes it.
//! - At most one gesture is open at a time.
//! - A mutation that fails after recording rolls back with
//!   [`History::cancel_last`].
//!
//! Snapshots are expected to be cheap (shared buffers): the history only
//! moves them around.

/// One undoable step: the state to restore and its UI label.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryStep<S> {
    /// PRE-mutation state to restore.
    pub snapshot: S,
    /// Label shown in the History panel (travels with the snapshot:
    /// undo moves it to redo and back).
    pub label: String,
}

/// Bounded undo/redo stacks plus the open gesture.
#[derive(Clone, Debug)]
pub struct History<S, K> {
    undo: Vec<HistoryStep<S>>,
    redo: Vec<HistoryStep<S>>,
    limit: usize,
    gesture: Option<K>,
}

impl<S, K: PartialEq> History<S, K> {
    /// Empty history keeping at most `limit` undo steps (oldest dropped).
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            gesture: None,
        }
    }

    /// Records the PRE-mutation `snapshot` (built lazily) unless
    /// `gesture` continues the open gesture. Opens `gesture` (or closes
    /// any gesture when `None`). Returns true when a step was recorded.
    pub fn record(
        &mut self,
        gesture: Option<K>,
        label: impl Into<String>,
        snapshot: impl FnOnce() -> S,
    ) -> bool {
        if gesture.is_some() && gesture == self.gesture {
            return false;
        }
        if self.undo.len() >= self.limit {
            self.undo.remove(0);
        }
        self.undo.push(HistoryStep {
            snapshot: snapshot(),
            label: label.into(),
        });
        self.redo.clear();
        self.gesture = gesture;
        true
    }

    /// Open gesture, if any.
    #[must_use]
    pub fn gesture(&self) -> Option<&K> {
        self.gesture.as_ref()
    }

    /// Replaces the open gesture without recording (e.g. a drag frame
    /// keeps its slot, a commit releases it).
    pub fn set_gesture(&mut self, gesture: Option<K>) {
        self.gesture = gesture;
    }

    /// Closes the open gesture when `pred` matches it.
    pub fn release_gesture_if(&mut self, pred: impl FnOnce(&K) -> bool) {
        if self.gesture.as_ref().is_some_and(pred) {
            self.gesture = None;
        }
    }

    /// Drops the last recorded step (mutation failed after `record`):
    /// returns it, and closes the gesture so the next tick records again.
    pub fn cancel_last(&mut self) -> Option<HistoryStep<S>> {
        self.gesture = None;
        self.undo.pop()
    }

    /// One step back: returns the snapshot to restore; `current` (the
    /// live state) goes to redo with the step's label. `None` when there
    /// is nothing to undo.
    pub fn undo(&mut self, current: impl FnOnce() -> S) -> Option<S> {
        let step = self.undo.pop()?;
        self.redo.push(HistoryStep {
            snapshot: current(),
            label: step.label,
        });
        self.gesture = None;
        Some(step.snapshot)
    }

    /// One step forward, symmetric to [`Self::undo`].
    pub fn redo(&mut self, current: impl FnOnce() -> S) -> Option<S> {
        let step = self.redo.pop()?;
        self.undo.push(HistoryStep {
            snapshot: current(),
            label: step.label,
        });
        self.gesture = None;
        Some(step.snapshot)
    }

    /// Forgets everything (new editing session, e.g. a loaded project).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.gesture = None;
    }

    /// Undoable steps, oldest first.
    #[must_use]
    pub fn undo_steps(&self) -> &[HistoryStep<S>] {
        &self.undo
    }

    /// Redoable steps, in undo order (the last one is the next redo).
    #[must_use]
    pub fn redo_steps(&self) -> &[HistoryStep<S>] {
        &self.redo
    }

    /// True when something can be undone.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// True when something can be redone.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undo labels, oldest first.
    #[must_use]
    pub fn undo_labels(&self) -> Vec<String> {
        self.undo.iter().map(|s| s.label.clone()).collect()
    }

    /// Redo labels, in undo order.
    #[must_use]
    pub fn redo_labels(&self) -> Vec<String> {
        self.redo.iter().map(|s| s.label.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// State = a number, gesture keys = small ints.
    fn history() -> History<i32, u8> {
        History::new(3)
    }

    #[test]
    fn undo_redo_round_trip_with_labels() {
        let mut h = history();
        let mut state = 0;
        assert!(h.record(None, "un", || state));
        state = 1;
        assert!(h.record(None, "deux", || state));
        state = 2;
        assert_eq!(h.undo_labels(), ["un", "deux"]);
        state = h.undo(|| state).expect("annulable");
        assert_eq!(state, 1);
        assert_eq!(h.redo_labels(), ["deux"]);
        state = h.redo(|| state).expect("rétablissable");
        assert_eq!(state, 2);
        assert!(h.redo(|| state).is_none());
    }

    #[test]
    fn gesture_ticks_coalesce_until_another_record() {
        let mut h = history();
        assert!(h.record(Some(7), "curseur", || 0));
        assert!(!h.record(Some(7), "curseur", || 1), "même geste");
        assert!(h.record(Some(8), "autre", || 2), "autre clé");
        assert!(h.record(None, "discret", || 3));
        assert!(h.record(Some(8), "autre", || 4), "geste refermé");
        assert_eq!(h.undo_steps().len(), 3, "borne respectée");
    }

    #[test]
    fn undo_and_redo_close_the_gesture() {
        let mut h = history();
        h.record(Some(1), "a", || 0);
        let _ = h.undo(|| 1);
        assert_eq!(h.gesture(), None);
        assert!(h.record(Some(1), "a", || 0));
    }

    #[test]
    fn new_record_drains_redo() {
        let mut h = history();
        h.record(None, "a", || 0);
        let _ = h.undo(|| 1);
        assert!(h.can_redo());
        h.record(None, "b", || 0);
        assert!(!h.can_redo());
    }

    #[test]
    fn cancel_last_rolls_back_and_reopens() {
        let mut h = history();
        h.record(Some(2), "a", || 0);
        assert_eq!(h.cancel_last().map(|s| s.snapshot), Some(0));
        assert!(!h.can_undo());
        assert!(h.record(Some(2), "a", || 0), "le tick suivant enregistre");
    }

    #[test]
    fn gesture_slot_management() {
        let mut h = history();
        h.set_gesture(Some(5));
        h.release_gesture_if(|k| *k == 6);
        assert_eq!(h.gesture(), Some(&5));
        h.release_gesture_if(|k| *k == 5);
        assert_eq!(h.gesture(), None);
        h.record(Some(1), "a", || 0);
        h.clear();
        assert!(!h.can_undo() && h.gesture().is_none());
    }

    #[test]
    fn limit_drops_the_oldest() {
        let mut h = history();
        for i in 0..5 {
            h.record(None, i.to_string(), || i);
        }
        assert_eq!(h.undo_labels(), ["2", "3", "4"]);
    }
}
