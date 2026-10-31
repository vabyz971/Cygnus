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

//! The command bus: applies commands, keeps their log and the journal of
//! changes.

use std::collections::VecDeque;
use std::time::SystemTime;

use serde::Serialize;

use crate::command::Command;
use crate::journal::{ChangesSince, Journal, Revision};

/// Who issued a command (HIS-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The user, through the interface.
    User,
    /// An AI agent, through the MCP server.
    Agent,
    /// The command line.
    Cli,
    /// A test or a development script.
    Test,
}

/// Identifier grouping several commands into one history entry (a whole
/// brush stroke, HIS-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(u64);

/// One applied command in the log.
#[derive(Debug, Clone)]
pub struct Entry<C> {
    /// Revision the command produced.
    pub revision: Revision,
    /// Name of the command, kept even when its data is dropped.
    pub name: &'static str,
    /// The command, unless dropped to respect the memory budget.
    pub command: Option<C>,
    /// Its inverse, unless dropped to respect the memory budget.
    pub inverse: Option<C>,
    /// Who issued it.
    pub source: Source,
    /// When it was applied.
    pub time: SystemTime,
    /// Group it belongs to, if any.
    pub group: Option<GroupId>,
    retained_bytes: usize,
}

impl<C> Entry<C> {
    /// Whether the entry can still be undone (its inverse is kept).
    pub fn is_undoable(&self) -> bool {
        self.inverse.is_some()
    }
}

/// Bounds of the bus memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusLimits {
    /// Number of change sets kept for consumers that lag behind.
    pub journal_capacity: usize,
    /// Number of log entries kept; older ones are removed.
    pub max_entries: usize,
    /// Memory retained by the commands and inverses of the log, in bytes.
    /// Above it, the oldest entries lose their data and become non undoable.
    pub retained_bytes: usize,
}

impl Default for BusLimits {
    fn default() -> Self {
        Self {
            journal_capacity: 1024,
            max_entries: 10_000,
            retained_bytes: 512 * 1024 * 1024,
        }
    }
}

/// Applies the commands of catalog `C` and records them.
pub struct CommandBus<C: Command> {
    limits: BusLimits,
    revision: Revision,
    entries: VecDeque<Entry<C>>,
    journal: Journal<C::Changes>,
    retained_bytes: usize,
    forgotten: u64,
    next_group: u64,
}

impl<C: Command> Default for CommandBus<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Command> CommandBus<C> {
    /// Creates a bus with the default limits.
    pub fn new() -> Self {
        Self::with_limits(BusLimits::default())
    }

    /// Creates a bus with the given limits.
    pub fn with_limits(limits: BusLimits) -> Self {
        Self {
            limits,
            revision: Revision::ZERO,
            entries: VecDeque::new(),
            journal: Journal::new(limits.journal_capacity),
            retained_bytes: 0,
            forgotten: 0,
            next_group: 0,
        }
    }

    /// Current revision.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Applies `command` to `target`. On error, nothing changes: neither the
    /// target, nor the revision, nor the log.
    pub fn execute(
        &mut self,
        target: &mut C::Target,
        command: C,
        source: Source,
    ) -> Result<Revision, C::Error> {
        self.run(target, command, source, None)
    }

    /// Like [`Self::execute`], recording the command as part of `group`.
    pub fn execute_in_group(
        &mut self,
        target: &mut C::Target,
        command: C,
        source: Source,
        group: GroupId,
    ) -> Result<Revision, C::Error> {
        self.run(target, command, source, Some(group))
    }

    /// Reserves a new group identifier.
    pub fn new_group(&mut self) -> GroupId {
        self.next_group += 1;
        GroupId(self.next_group)
    }

    fn run(
        &mut self,
        target: &mut C::Target,
        command: C,
        source: Source,
        group: Option<GroupId>,
    ) -> Result<Revision, C::Error> {
        let name = command.name();
        let recorded = command.clone();
        let applied = command.apply(target)?;

        self.revision = self.revision.next();
        self.journal.push(self.revision, applied.changes);
        let retained_bytes = recorded.payload_bytes() + applied.inverse_bytes;
        self.retained_bytes += retained_bytes;
        self.entries.push_back(Entry {
            revision: self.revision,
            name,
            command: Some(recorded),
            inverse: Some(applied.inverse),
            source,
            time: SystemTime::now(),
            group,
            retained_bytes,
        });
        self.enforce_limits();
        Ok(self.revision)
    }

    fn enforce_limits(&mut self) {
        while self.entries.len() > self.limits.max_entries {
            if let Some(entry) = self.entries.pop_front() {
                self.retained_bytes -= entry.retained_bytes;
            }
        }
        // Oldest first, so that the most recent actions stay undoable.
        for entry in &mut self.entries {
            if self.retained_bytes <= self.limits.retained_bytes {
                break;
            }
            if entry.inverse.is_some() {
                entry.command = None;
                entry.inverse = None;
                self.retained_bytes -= entry.retained_bytes;
                entry.retained_bytes = 0;
                self.forgotten += 1;
            }
        }
    }

    /// The log, oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &Entry<C>> {
        self.entries.iter()
    }

    /// Memory currently retained by the log, in bytes.
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Number of entries that lost their data to the memory budget.
    pub fn forgotten(&self) -> u64 {
        self.forgotten
    }

    /// Changes after `cursor`, for one consumer. Each consumer keeps its own
    /// cursor and passes back the returned revision next time.
    pub fn changes_since(&self, cursor: Revision) -> ChangesSince<C::Changes> {
        self.journal.since(cursor, self.revision)
    }
}
