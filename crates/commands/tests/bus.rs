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

use std::collections::{BTreeMap, BTreeSet};

use cygnus_commands::{
    Applied, BusLimits, Catalog, ChangeSet, ChangesSince, Command, CommandBus, CommandInfo,
    Revision, Source, Stability,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

type Counters = BTreeMap<String, i64>;

/// Names of the counters touched, or every counter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Touched {
    all: bool,
    names: BTreeSet<String>,
}

impl Touched {
    fn one(name: &str) -> Self {
        Self {
            all: false,
            names: [name.to_owned()].into(),
        }
    }
}

impl ChangeSet for Touched {
    fn merge(&mut self, other: &Self) {
        self.all |= other.all;
        self.names.extend(other.names.iter().cloned());
    }

    fn everything() -> Self {
        Self {
            all: true,
            names: BTreeSet::new(),
        }
    }

    fn is_empty(&self) -> bool {
        !self.all && self.names.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
struct AddParams {
    counter: String,
    amount: i64,
    /// Bytes the command pretends to carry, to exercise the memory budget.
    #[serde(default)]
    payload: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
struct SetParams {
    counter: String,
    value: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", content = "params")]
enum Toy {
    #[serde(rename = "counter.add")]
    Add(AddParams),
    #[serde(rename = "counter.set")]
    Set(SetParams),
}

#[derive(Debug, PartialEq)]
enum ToyError {
    Overflow,
}

impl Command for Toy {
    type Target = Counters;
    type Changes = Touched;
    type Error = ToyError;

    fn name(&self) -> &'static str {
        match self {
            Self::Add(_) => "counter.add",
            Self::Set(_) => "counter.set",
        }
    }

    fn payload_bytes(&self) -> usize {
        match self {
            Self::Add(params) => params.payload,
            Self::Set(_) => 0,
        }
    }

    fn apply(self, target: &mut Counters) -> Result<Applied<Self>, ToyError> {
        match self {
            Self::Add(params) => {
                let current = target.get(&params.counter).copied().unwrap_or(0);
                // Checked before any change: a refused command changes nothing.
                let value = current
                    .checked_add(params.amount)
                    .ok_or(ToyError::Overflow)?;
                target.insert(params.counter.clone(), value);
                Ok(Applied {
                    inverse: Self::Set(SetParams {
                        counter: params.counter.clone(),
                        value: current,
                    }),
                    inverse_bytes: params.payload,
                    changes: Touched::one(&params.counter),
                })
            }
            Self::Set(params) => {
                let previous = target.insert(params.counter.clone(), params.value);
                Ok(Applied {
                    inverse: Self::Set(SetParams {
                        counter: params.counter.clone(),
                        value: previous.unwrap_or(0),
                    }),
                    inverse_bytes: 0,
                    changes: Touched::one(&params.counter),
                })
            }
        }
    }
}

impl Catalog for Toy {
    fn commands() -> Vec<CommandInfo> {
        vec![
            CommandInfo::new::<AddParams>("counter.add", "Adds to a counter.", Stability::Stable),
            CommandInfo::new::<SetParams>("counter.set", "Sets a counter.", Stability::Development),
        ]
    }
}

fn add(counter: &str, amount: i64) -> Toy {
    Toy::Add(AddParams {
        counter: counter.to_owned(),
        amount,
        payload: 0,
    })
}

fn heavy(counter: &str, payload: usize) -> Toy {
    Toy::Add(AddParams {
        counter: counter.to_owned(),
        amount: 1,
        payload,
    })
}

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
    assert_eq!(entries[1].command.as_ref(), Some(&add("a", 4)));
    let inverse = entries[1].inverse.clone().unwrap();
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
fn memory_budget_is_respected() {
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
    let entries: Vec<_> = bus.entries().collect();
    assert_eq!(
        entries.len(),
        5,
        "entries are kept, only their data dropped"
    );
    // The oldest entries lost their command and inverse: no longer undoable.
    assert!(!entries[0].is_undoable());
    assert!(entries[0].command.is_none());
    assert_eq!(entries[0].name, "counter.add");
    // The most recent ones are intact.
    assert!(entries[4].is_undoable());
    assert_eq!(bus.forgotten(), 3);
}

#[test]
fn single_command_above_budget_is_kept_but_not_undoable() {
    let mut counters = Counters::new();
    let mut bus = CommandBus::<Toy>::with_limits(BusLimits {
        retained_bytes: 100,
        ..BusLimits::default()
    });
    bus.execute(&mut counters, heavy("a", 500), Source::User)
        .unwrap();
    assert_eq!(counters["a"], 1);
    assert!(bus.retained_bytes() <= 100);
    assert!(!bus.entries().next().unwrap().is_undoable());
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
    assert_eq!(counters["a"], 10);
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
