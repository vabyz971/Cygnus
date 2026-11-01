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

//! Command bus behaviour, exercised with a toy catalog: a map of named
//! counters.

mod common;

use common::*;
use cygnus_commands::{
    BusLimits, Catalog, ChangeSet, ChangesSince, Command, CommandBus, Revision, Source,
};

#[test]
fn applied_command_changes_target_and_revision() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::new();
    assert_eq!(bus.revision(), Revision::ZERO);
    let revision = bus
        .execute(&mut counters, add("a", 3), Source::User)
        .unwrap();
    assert_eq!(counters["a"], 3);
    assert_eq!(revision, bus.revision());
    assert!(revision > Revision::ZERO);
}

#[test]
fn refused_command_changes_nothing() {
    let mut counters = Counters::from([("a".to_owned(), i64::MAX)]);
    let mut bus = CommandBus::<Toy>::new();
    let error = bus
        .execute(&mut counters, add("a", 1), Source::User)
        .unwrap_err();
    assert_eq!(error, ToyError::Overflow);
    assert_eq!(counters["a"], i64::MAX);
    assert_eq!(bus.revision(), Revision::ZERO);
    assert_eq!(bus.entries().count(), 0);
}

#[test]
fn log_records_command_inverse_and_source() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::new();
    bus.execute(&mut counters, add("a", 3), Source::User)
        .unwrap();
    bus.execute(&mut counters, add("a", 4), Source::Agent)
        .unwrap();
    let entries: Vec<_> = bus.entries().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].name, "counter.add");
    assert_eq!(entries[1].source, Source::Agent);
    assert_eq!(entries[1].command, add("a", 4));
    let inverse = entries[1].inverse.clone();
    // Applying the inverse restores the previous state.
    inverse.apply(&mut counters).unwrap();
    assert_eq!(counters["a"], 3);
}

#[test]
fn group_is_recorded() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::new();
    let group = bus.new_group();
    bus.execute_in_group(&mut counters, add("a", 1), Source::User, group)
        .unwrap();
    bus.execute(&mut counters, add("a", 1), Source::User)
        .unwrap();
    let entries: Vec<_> = bus.entries().collect();
    assert_eq!(entries[0].group, Some(group));
    assert_eq!(entries[1].group, None);
}

#[test]
fn independent_consumers_see_the_same_changes() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::new();
    let fast_start = bus.revision();
    let slow_start = bus.revision();

    bus.execute(&mut counters, add("a", 1), Source::User)
        .unwrap();
    // The fast consumer reads after each command.
    let first = bus.changes_since(fast_start);
    assert_eq!(first.changes, Touched::one("a"));
    let fast = first.revision;

    bus.execute(&mut counters, add("b", 1), Source::User)
        .unwrap();
    let second = bus.changes_since(fast);
    assert_eq!(second.changes, Touched::one("b"));

    // The slow consumer reads once, later: reading by the fast one consumed
    // nothing.
    let slow = bus.changes_since(slow_start);
    let mut both = Touched::one("a");
    both.merge(&Touched::one("b"));
    assert_eq!(slow.changes, both);
    assert_eq!(slow.revision, second.revision);

    // Up to date: nothing new.
    let none: ChangesSince<Touched> = bus.changes_since(slow.revision);
    assert!(none.changes.is_empty());
}

#[test]
fn consumer_behind_the_journal_gets_everything() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::with_limits(BusLimits {
        journal_capacity: 2,
        ..BusLimits::default()
    });
    let start = bus.revision();
    for _ in 0..5 {
        bus.execute(&mut counters, add("a", 1), Source::User)
            .unwrap();
    }
    let late = bus.changes_since(start);
    assert_eq!(late.changes, Touched::everything());
    // A cursor still inside the journal gets the precise changes.
    let recent = bus.changes_since(Revision::from_raw(late.revision.raw() - 1));
    assert_eq!(recent.changes, Touched::one("a"));
}

#[test]
fn unknown_cursor_gets_everything() {
    let bus = CommandBus::<Toy>::new();
    let future = bus.changes_since(Revision::from_raw(42));
    assert_eq!(future.changes, Touched::everything());
}

#[test]
fn catalog_describes_every_command_with_its_schema() {
    let json = cygnus_commands::catalog_json::<Toy>();
    let commands = json.as_array().unwrap();
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0]["name"], "counter.add");
    assert_eq!(commands[0]["stability"], "stable");
    assert_eq!(commands[1]["stability"], "development");
    let properties = &commands[0]["params"]["properties"];
    assert!(properties.get("counter").is_some());
    assert!(properties.get("amount").is_some());
}

#[test]
fn catalog_names_match_serialized_names() {
    // The name given by the catalog is the one used on the wire.
    for command in [
        add("a", 1),
        Toy::Set(SetParams {
            counter: "a".to_owned(),
            value: 0,
            payload: 0,
        }),
    ] {
        let json = serde_json::to_value(&command).unwrap();
        assert_eq!(json["command"], command.name());
        assert!(
            Toy::commands()
                .iter()
                .any(|info| info.name == command.name())
        );
    }
}

#[test]
fn saturation_removes_the_oldest_entries_with_a_warning() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::with_limits(BusLimits {
        retained_bytes: 1000,
        ..BusLimits::default()
    });
    // Each command retains 200 bytes of payload and 200 of inverse.
    for _ in 0..5 {
        bus.execute(&mut counters, heavy("a", 200), Source::User)
            .unwrap();
        assert!(bus.retained_bytes() <= 1000);
    }
    // The three oldest entries are gone entirely, and counted (0022).
    assert_eq!(bus.entries().count(), 2);
    assert_eq!(bus.truncated(), 3);
    // Everything still listed can be undone; then nothing is left.
    assert!(bus.undo(&mut counters, Source::User).unwrap());
    assert!(bus.undo(&mut counters, Source::User).unwrap());
    assert!(!bus.can_undo());
    assert_eq!(counters["a"], 3);
}

#[test]
fn single_command_above_budget_is_applied_but_not_kept() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::with_limits(BusLimits {
        retained_bytes: 100,
        ..BusLimits::default()
    });
    bus.execute(&mut counters, heavy("a", 500), Source::User)
        .unwrap();
    assert_eq!(counters["a"], 1);
    assert!(bus.retained_bytes() <= 100);
    assert_eq!(bus.entries().count(), 0);
    assert_eq!(bus.truncated(), 1);
    assert!(!bus.can_undo());
}

#[test]
fn entry_count_is_bounded() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::with_limits(BusLimits {
        max_entries: 3,
        ..BusLimits::default()
    });
    for _ in 0..10 {
        bus.execute(&mut counters, add("a", 1), Source::User)
            .unwrap();
    }
    assert_eq!(bus.entries().count(), 3);
    assert_eq!(bus.truncated(), 7);
    assert_eq!(counters["a"], 10);
}
