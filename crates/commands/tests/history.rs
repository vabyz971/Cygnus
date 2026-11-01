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

//! Undo, redo, gestures, coalescing and saturation of the history
//! (docs/decisions/0022), with the toy catalog.

mod common;

use common::*;
use cygnus_commands::{BusLimits, ChangeSet, CommandBus, Revision, Source};

fn bus() -> CommandBus<Toy> {
    CommandBus::new()
}

#[test]
fn undo_restores_and_redo_reapplies() {
    let mut counters = Counters::new();
    let mut bus = bus();
    bus.execute(&mut counters, add("a", 3), Source::User)
        .unwrap();
    bus.execute(&mut counters, add("a", 4), Source::User)
        .unwrap();
    assert!(bus.can_undo() && !bus.can_redo());

    assert!(bus.undo(&mut counters, Source::User).unwrap());
    assert_eq!(counters["a"], 3);
    assert!(bus.can_redo());
    assert!(bus.undo(&mut counters, Source::User).unwrap());
    assert_eq!(counters["a"], 0);
    assert!(
        !bus.undo(&mut counters, Source::User).unwrap(),
        "nothing left"
    );

    assert!(bus.redo(&mut counters, Source::User).unwrap());
    assert!(bus.redo(&mut counters, Source::User).unwrap());
    assert_eq!(counters["a"], 7);
    assert!(
        !bus.redo(&mut counters, Source::User).unwrap(),
        "nothing left"
    );
}

#[test]
fn undo_and_redo_are_revisions_with_changes() {
    let mut counters = Counters::new();
    let mut bus = bus();
    bus.execute(&mut counters, add("a", 1), Source::User)
        .unwrap();
    let cursor = bus.revision();
    bus.undo(&mut counters, Source::User).unwrap();
    assert!(bus.revision() > cursor);
    assert_eq!(bus.changes_since(cursor).changes, Touched::one("a"));
    let cursor = bus.revision();
    bus.redo(&mut counters, Source::User).unwrap();
    assert_eq!(bus.changes_since(cursor).changes, Touched::one("a"));
    // Undo with nothing to undo is not a revision.
    let mut empty = CommandBus::<Toy>::new();
    assert!(!empty.undo(&mut counters, Source::User).unwrap());
    assert_eq!(empty.revision(), Revision::ZERO);
}

#[test]
fn a_new_command_clears_what_could_be_redone() {
    let mut counters = Counters::new();
    let mut bus = bus();
    bus.execute(&mut counters, add("a", 1), Source::User)
        .unwrap();
    bus.undo(&mut counters, Source::User).unwrap();
    assert!(bus.can_redo());
    bus.execute(&mut counters, add("b", 1), Source::User)
        .unwrap();
    assert!(!bus.can_redo());
    assert_eq!(bus.undone().count(), 0);
}

#[test]
fn history_commands_go_through_execute() {
    let mut counters = Counters::new();
    let mut bus = bus();
    bus.execute(&mut counters, add("a", 5), Source::User)
        .unwrap();
    bus.execute(&mut counters, Toy::Undo, Source::Agent)
        .unwrap();
    assert_eq!(counters["a"], 0);
    bus.execute(&mut counters, Toy::Redo, Source::Agent)
        .unwrap();
    assert_eq!(counters["a"], 5);
    // They create no history entry of their own.
    assert_eq!(bus.entries().count(), 1);
    assert_eq!(bus.entries().next().unwrap().name, "counter.add");
}

#[test]
fn a_gesture_is_one_entry_undone_whole() {
    let mut counters = Counters::new();
    let mut bus = bus();
    bus.execute(&mut counters, add("a", 1), Source::User)
        .unwrap();
    let gesture = bus.begin_gesture();
    bus.execute(&mut counters, add("a", 10), Source::User)
        .unwrap();
    bus.execute(&mut counters, add("b", 10), Source::User)
        .unwrap();
    bus.end_gesture();
    assert!(
        bus.entries()
            .skip(1)
            .all(|entry| entry.group == Some(gesture))
    );

    bus.undo(&mut counters, Source::User).unwrap();
    assert_eq!(
        (counters["a"], counters["b"]),
        (1, 0),
        "the whole gesture is undone"
    );
    bus.redo(&mut counters, Source::User).unwrap();
    assert_eq!((counters["a"], counters["b"]), (11, 10), "and redone");
}

#[test]
fn repeated_changes_in_a_gesture_merge() {
    let mut counters = Counters::from([("a".to_owned(), 2)]);
    let mut bus = bus();
    bus.begin_gesture();
    for value in 10..70 {
        bus.execute(&mut counters, set("a", value), Source::User)
            .unwrap();
    }
    bus.end_gesture();
    // 60 sets of the same counter are one entry, whatever the duration.
    assert_eq!(bus.entries().count(), 1);
    assert_eq!(counters["a"], 69);
    bus.undo(&mut counters, Source::User).unwrap();
    assert_eq!(counters["a"], 2, "back before the gesture");
    bus.redo(&mut counters, Source::User).unwrap();
    assert_eq!(counters["a"], 69, "the final value");
}

#[test]
fn changes_merge_only_inside_a_gesture() {
    let mut counters = Counters::new();
    let mut bus = bus();
    bus.execute(&mut counters, set("a", 1), Source::User)
        .unwrap();
    bus.execute(&mut counters, set("a", 2), Source::User)
        .unwrap();
    assert_eq!(bus.entries().count(), 2);
    // Different counters in one gesture do not merge either.
    bus.begin_gesture();
    bus.execute(&mut counters, set("a", 3), Source::User)
        .unwrap();
    bus.execute(&mut counters, set("b", 3), Source::User)
        .unwrap();
    bus.end_gesture();
    assert_eq!(bus.entries().count(), 4);
}

#[test]
fn saturation_never_removes_the_open_gesture() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::with_limits(BusLimits {
        retained_bytes: 1000,
        ..BusLimits::default()
    });
    bus.execute(&mut counters, heavy("a", 200), Source::User)
        .unwrap();
    bus.begin_gesture();
    for _ in 0..4 {
        bus.execute(&mut counters, heavy("b", 200), Source::User)
            .unwrap();
    }
    // Over budget, but only the entry outside the gesture may go.
    assert_eq!(bus.truncated(), 1);
    assert_eq!(bus.entries().count(), 4);
    // Once closed and still over budget, the gesture goes as a whole,
    // never in part.
    bus.end_gesture();
    assert_eq!(bus.entries().count(), 0);
    assert_eq!(bus.truncated(), 5);
    assert!(!bus.undo(&mut counters, Source::User).unwrap());
    assert_eq!(counters["b"], 4);
}

#[test]
fn a_closed_gesture_within_budget_stays_whole() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::with_limits(BusLimits {
        retained_bytes: 2000,
        ..BusLimits::default()
    });
    bus.execute(&mut counters, heavy("a", 200), Source::User)
        .unwrap();
    bus.begin_gesture();
    for _ in 0..3 {
        bus.execute(&mut counters, heavy("b", 200), Source::User)
            .unwrap();
    }
    bus.end_gesture();
    // 4 × 400 bytes: within budget, nothing removed.
    assert_eq!(bus.truncated(), 0);
    bus.undo(&mut counters, Source::User).unwrap();
    assert_eq!(
        (counters["a"], counters["b"]),
        (1, 0),
        "the gesture is undone whole"
    );
}

#[test]
fn undone_entries_count_in_the_memory_budget() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::with_limits(BusLimits {
        retained_bytes: 1000,
        ..BusLimits::default()
    });
    bus.execute(&mut counters, heavy("a", 200), Source::User)
        .unwrap();
    bus.execute(&mut counters, heavy("a", 200), Source::User)
        .unwrap();
    bus.undo(&mut counters, Source::User).unwrap();
    bus.undo(&mut counters, Source::User).unwrap();
    assert!(bus.retained_bytes() <= 1000);
    assert_eq!(bus.undone().count(), 2);
    assert!(bus.retained_bytes() > 0);
    // The journal still tells consumers what changed.
    assert!(!bus.changes_since(Revision::ZERO).changes.is_empty());
}
