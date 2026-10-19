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

//! UI action queue drained once per frame by the app.
//!
//! Generic version of the old `PhotoCommandQueue`: panels
//! push actions, the app drains and routes them (local state
//! or engine worker). `A` is the app's action type (e.g.
//! `PhotoAction` on the photo side).

/// UI action queue (emission order preserved).
#[derive(Debug)]
pub struct ActionQueue<A> {
    actions: Vec<A>,
}

impl<A> Default for ActionQueue<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A> ActionQueue<A> {
    /// Empty queue.
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// Pushes an action at the back of the queue.
    pub fn push(&mut self, action: A) {
        self.actions.push(action);
    }

    /// Pushes several actions at the back of the queue.
    pub fn extend(&mut self, actions: impl IntoIterator<Item = A>) {
        self.actions.extend(actions);
    }

    /// Number of pending actions.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// True when no action is pending.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Drains the queue and returns the actions (emission order).
    pub fn drain(&mut self) -> Vec<A> {
        std::mem::take(&mut self.actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dummy action (the queue knows nothing about apps' actions).
    #[derive(Debug, Clone, PartialEq)]
    enum Probe {
        Undo,
        Redo,
        ZoomIn,
    }

    #[test]
    fn queue_preserves_emission_order() {
        let mut queue = ActionQueue::new();
        assert!(queue.is_empty());
        queue.push(Probe::Undo);
        queue.extend([Probe::Redo, Probe::ZoomIn]);
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.drain(), vec![Probe::Undo, Probe::Redo, Probe::ZoomIn]);
        assert!(queue.is_empty());
    }

    #[test]
    fn drain_on_empty_queue_returns_nothing() {
        let mut queue: ActionQueue<Probe> = ActionQueue::new();
        assert!(queue.drain().is_empty());
    }
}
