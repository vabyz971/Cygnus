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

//! Journal of changes, read by several independent consumers.
//!
//! Each consumer keeps its own cursor (the last [`Revision`] it read) and
//! asks for the changes since then. Reading consumes nothing, so consumers
//! never interfere with each other. The journal is bounded: a consumer that
//! fell behind it receives [`ChangeSet::everything`].

use std::collections::VecDeque;

use crate::command::ChangeSet;

/// Number of applied commands since the bus was created. Each successful
/// command increments it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Revision(u64);

impl Revision {
    /// Revision of a bus before any command.
    pub const ZERO: Self = Self(0);

    /// Creates a revision from its number.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The revision number.
    pub const fn raw(self) -> u64 {
        self.0
    }

    pub(crate) fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Answer to [`crate::CommandBus::changes_since`].
#[derive(Debug, Clone, PartialEq)]
pub struct ChangesSince<C> {
    /// Revision the answer is up to: the consumer's next cursor.
    pub revision: Revision,
    /// Union of the changes after the requested cursor.
    pub changes: C,
}

#[derive(Debug, Clone)]
pub(crate) struct Journal<C> {
    capacity: usize,
    entries: VecDeque<(Revision, C)>,
}

impl<C: ChangeSet> Journal<C> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
        }
    }

    pub(crate) fn push(&mut self, revision: Revision, changes: C) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((revision, changes));
    }

    pub(crate) fn since(&self, cursor: Revision, current: Revision) -> ChangesSince<C> {
        let changes = if cursor == current {
            C::default()
        } else if cursor > current {
            // A cursor from elsewhere: nothing can be assumed.
            C::everything()
        } else {
            match self.entries.front() {
                // Every revision after the cursor is still in the journal.
                Some((oldest, _)) if oldest.raw() <= cursor.raw() + 1 => {
                    let mut merged = C::default();
                    for (revision, changes) in &self.entries {
                        if *revision > cursor {
                            merged.merge(changes);
                        }
                    }
                    merged
                }
                _ => C::everything(),
            }
        };
        ChangesSince {
            revision: current,
            changes,
        }
    }
}
