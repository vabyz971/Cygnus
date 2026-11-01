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

use crate::command::{Command, HistoryAction};
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

/// One applied command in the history.
#[derive(Debug, Clone)]
pub struct Entry<C> {
    /// Revision the command produced.
    pub revision: Revision,
    /// Name of the command.
    pub name: &'static str,
    /// The command (for an undone entry: the command that redoes it).
    pub command: C,
    /// Its inverse, applied to undo it. Unused for undone entries.
    pub inverse: C,
    /// Who issued it.
    pub source: Source,
    /// When it was applied.
    pub time: SystemTime,
    /// Gesture it belongs to, if any.
    pub group: Option<GroupId>,
    /// Coalescing key of the command, see [`Command::coalesce_key`].
    key: Option<String>,
    retained_bytes: usize,
}

/// Bounds of the bus memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusLimits {
    /// Number of change sets kept for consumers that lag behind.
    pub journal_capacity: usize,
    /// Number of history entries kept; older ones are removed.
    pub max_entries: usize,
    /// Memory retained by the commands and inverses of the history, in
    /// bytes. Above it, the oldest entries are removed.
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

/// Applies the commands of catalog `C`, records them, undoes and redoes
/// them (docs/decisions/0010 and 0022).
///
/// History: `entries` (done, oldest first) and `undone` (most recently
/// undone last). Undo applies the inverse of the last entry, or of the
/// whole last gesture; applying an inverse yields the command that redoes
/// it. A new command clears what could be redone. Memory is bounded: past
/// the limits, the oldest entries are removed entirely, a gesture as a
/// whole; the newest entry (or gesture) is always kept, even above budget
/// ([`Self::over_budget`]). Removals are counted in [`Self::truncated`] for
/// the interface to warn about.
pub struct CommandBus<C: Command> {
    limits: BusLimits,
    revision: Revision,
    entries: VecDeque<Entry<C>>,
    undone: Vec<Entry<C>>,
    journal: Journal<C::Changes>,
    retained_bytes: usize,
    truncated: u64,
    next_group: u64,
    gesture: Option<GroupId>,
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
            undone: Vec::new(),
            journal: Journal::new(limits.journal_capacity),
            retained_bytes: 0,
            truncated: 0,
            next_group: 0,
            gesture: None,
        }
    }

    /// Current revision.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Applies `command` to `target`, inside the open gesture if any. On
    /// error, nothing changes: neither the target, nor the revision, nor
    /// the history. History commands undo or redo.
    pub fn execute(
        &mut self,
        target: &mut C::Target,
        command: C,
        source: Source,
    ) -> Result<Revision, C::Error> {
        let group = self.gesture;
        self.run(target, command, source, group)
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

    /// Opens a gesture: the commands executed until [`Self::end_gesture`]
    /// form one history entry.
    pub fn begin_gesture(&mut self) -> GroupId {
        let group = self.new_group();
        self.gesture = Some(group);
        group
    }

    /// Closes the open gesture.
    pub fn end_gesture(&mut self) {
        self.gesture = None;
        self.enforce_limits();
    }

    fn run(
        &mut self,
        target: &mut C::Target,
        command: C,
        source: Source,
        group: Option<GroupId>,
    ) -> Result<Revision, C::Error> {
        match command.history_action() {
            Some(HistoryAction::Undo) => {
                self.undo(target, source)?;
                return Ok(self.revision);
            }
            Some(HistoryAction::Redo) => {
                self.redo(target, source)?;
                return Ok(self.revision);
            }
            None => {}
        }
        let name = command.name();
        let key = command.coalesce_key();
        let recorded = command.clone();
        let applied = command.apply(target)?;

        self.revision = self.revision.next();
        self.journal.push(self.revision, applied.changes);
        self.clear_undone();

        // Same property again in the same gesture: keep one entry with the
        // first inverse and the latest command.
        let merges = group.is_some()
            && key.is_some()
            && self
                .entries
                .back()
                .is_some_and(|last| last.group == group && last.key == key);
        if merges {
            if let Some(last) = self.entries.back_mut() {
                let payload = recorded.payload_bytes();
                let old_payload = last.command.payload_bytes();
                self.retained_bytes =
                    self.retained_bytes + payload - old_payload.min(self.retained_bytes);
                last.retained_bytes =
                    last.retained_bytes + payload - old_payload.min(last.retained_bytes);
                last.command = recorded;
                last.revision = self.revision;
                last.time = SystemTime::now();
            }
        } else {
            let retained_bytes = recorded.payload_bytes() + applied.inverse_bytes;
            self.retained_bytes += retained_bytes;
            self.entries.push_back(Entry {
                revision: self.revision,
                name,
                command: recorded,
                inverse: applied.inverse,
                source,
                time: SystemTime::now(),
                group,
                key,
                retained_bytes,
            });
        }
        self.enforce_limits();
        Ok(self.revision)
    }

    fn clear_undone(&mut self) {
        for entry in self.undone.drain(..) {
            self.retained_bytes -= entry.retained_bytes.min(self.retained_bytes);
        }
    }

    /// Whether something can be undone.
    pub fn can_undo(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Whether something can be redone.
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Undoes the last entry, or the whole last gesture. Returns whether
    /// something was undone. `source` is who asked (HIS-1).
    pub fn undo(&mut self, target: &mut C::Target, source: Source) -> Result<bool, C::Error> {
        let _ = source;
        self.gesture = None;
        let Some(group) = self.entries.back().map(|entry| entry.group) else {
            return Ok(false);
        };
        loop {
            let Some(entry) = self.entries.pop_back() else {
                break;
            };
            self.retained_bytes -= entry.retained_bytes.min(self.retained_bytes);
            let applied = match entry.inverse.clone().apply(target) {
                Ok(applied) => applied,
                Err(error) => {
                    self.retained_bytes += entry.retained_bytes;
                    self.entries.push_back(entry);
                    return Err(error);
                }
            };
            self.revision = self.revision.next();
            self.journal.push(self.revision, applied.changes);
            let retained_bytes = applied.inverse.payload_bytes() + applied.inverse_bytes;
            self.retained_bytes += retained_bytes;
            self.undone.push(Entry {
                revision: self.revision,
                command: applied.inverse,
                retained_bytes,
                ..entry
            });
            let same_gesture =
                group.is_some() && self.entries.back().is_some_and(|last| last.group == group);
            if !same_gesture {
                break;
            }
        }
        self.enforce_limits();
        Ok(true)
    }

    /// Redoes the last undone entry, or the whole last undone gesture.
    /// Returns whether something was redone.
    pub fn redo(&mut self, target: &mut C::Target, source: Source) -> Result<bool, C::Error> {
        let _ = source;
        self.gesture = None;
        let Some(group) = self.undone.last().map(|entry| entry.group) else {
            return Ok(false);
        };
        loop {
            let Some(entry) = self.undone.pop() else {
                break;
            };
            self.retained_bytes -= entry.retained_bytes.min(self.retained_bytes);
            let applied = match entry.command.clone().apply(target) {
                Ok(applied) => applied,
                Err(error) => {
                    self.retained_bytes += entry.retained_bytes;
                    self.undone.push(entry);
                    return Err(error);
                }
            };
            self.revision = self.revision.next();
            self.journal.push(self.revision, applied.changes);
            let retained_bytes = entry.command.payload_bytes() + applied.inverse_bytes;
            self.retained_bytes += retained_bytes;
            self.entries.push_back(Entry {
                revision: self.revision,
                inverse: applied.inverse,
                retained_bytes,
                ..entry
            });
            let same_gesture =
                group.is_some() && self.undone.last().is_some_and(|last| last.group == group);
            if !same_gesture {
                break;
            }
        }
        self.enforce_limits();
        Ok(true)
    }

    fn over_limits(&self) -> bool {
        self.retained_bytes > self.limits.retained_bytes
            || self.entries.len() + self.undone.len() > self.limits.max_entries
    }

    fn enforce_limits(&mut self) {
        // Oldest done entries first, a gesture always as a whole (undoing
        // part of it would be meaningless). The newest entry (or gesture)
        // is always kept, even above budget: `over_budget` reports it.
        while self.over_limits() {
            let Some(front) = self.entries.front() else {
                break;
            };
            let group = front.group;
            let newest = match self.entries.back() {
                Some(last) => self.entries.len() == 1 || (group.is_some() && last.group == group),
                None => true,
            };
            if newest {
                break;
            }
            loop {
                let Some(entry) = self.entries.pop_front() else {
                    break;
                };
                self.retained_bytes -= entry.retained_bytes.min(self.retained_bytes);
                self.truncated += 1;
                let same_gesture =
                    group.is_some() && self.entries.front().is_some_and(|next| next.group == group);
                if !same_gesture {
                    break;
                }
            }
        }
        // Then the undone entries farthest from the present.
        while self.over_limits() && !self.undone.is_empty() {
            let entry = self.undone.remove(0);
            self.retained_bytes -= entry.retained_bytes.min(self.retained_bytes);
            self.truncated += 1;
        }
    }

    /// The done entries, oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &Entry<C>> {
        self.entries.iter()
    }

    /// The undone entries, the next one to redo first.
    pub fn undone(&self) -> impl Iterator<Item = &Entry<C>> {
        self.undone.iter().rev()
    }

    /// Memory currently retained by the history, in bytes.
    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    /// Whether the history retains more memory than its budget: the newest
    /// entry alone exceeds it and is kept anyway. The interface warns.
    pub fn over_budget(&self) -> bool {
        self.retained_bytes > self.limits.retained_bytes
    }

    /// Number of history entries removed to respect the limits since the
    /// bus was created. Consumers compare it with the value they last saw
    /// to warn the user (docs/decisions/0022).
    pub fn truncated(&self) -> u64 {
        self.truncated
    }

    /// Changes after `cursor`, for one consumer. Each consumer keeps its own
    /// cursor and passes back the returned revision next time.
    pub fn changes_since(&self, cursor: Revision) -> ChangesSince<C::Changes> {
        self.journal.since(cursor, self.revision)
    }
}
