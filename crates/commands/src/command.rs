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

//! The command contract.

/// What changed in a document, as reported by a command. Change sets are
/// merged when a consumer reads several commands at once.
pub trait ChangeSet: Clone + Default {
    /// Adds the changes of `other` to `self`.
    fn merge(&mut self, other: &Self);

    /// A change set meaning "anything may have changed", given to consumers
    /// that cannot be told precisely (for example after missing journal
    /// entries). Consumers must then refresh everything.
    fn everything() -> Self;

    /// Whether nothing changed.
    fn is_empty(&self) -> bool;
}

/// Result of a successfully applied command.
#[derive(Debug, Clone)]
pub struct Applied<C: Command> {
    /// Command that restores the state from before the application. It may
    /// be an internal command of the catalog (for example a tile restore).
    pub inverse: C,
    /// Memory retained by `inverse` beyond its size, in bytes (copies of
    /// overwritten pixels, for instance). Counted in the bus memory budget.
    pub inverse_bytes: usize,
    /// What the command changed.
    pub changes: C::Changes,
}

/// A typed, serializable change to a document.
///
/// An implementation must leave the target untouched when it returns an
/// error: a refused command has no effect.
pub trait Command: Sized + Clone {
    /// The document the command applies to.
    type Target;
    /// Description of what a command changed.
    type Changes: ChangeSet;
    /// Why a command was refused.
    type Error;

    /// Stable name of the command, as in the catalog (for example
    /// `layer.create`).
    fn name(&self) -> &'static str;

    /// Memory carried by the command itself beyond its size, in bytes (pixel
    /// data, for instance). Counted in the bus memory budget.
    fn payload_bytes(&self) -> usize {
        0
    }

    /// Applies the command to `target`.
    fn apply(self, target: &mut Self::Target) -> Result<Applied<Self>, Self::Error>;

    /// For the history commands of the catalog (`history.undo`,
    /// `history.redo`): the action the bus performs itself instead of
    /// applying the command.
    fn history_action(&self) -> Option<HistoryAction> {
        None
    }

    /// Key under which consecutive commands of one gesture merge into one
    /// history entry (for example the property of a node a slider drags).
    /// `None`: the command never merges.
    fn coalesce_key(&self) -> Option<String> {
        None
    }
}

/// History action performed by the bus (docs/decisions/0022).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAction {
    /// Undo the last entry (a whole gesture at once).
    Undo,
    /// Redo the last undone entry.
    Redo,
}
