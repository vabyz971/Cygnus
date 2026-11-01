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

//! Toy catalog shared by the bus tests: a map of named counters.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use cygnus_commands::{
    Applied, Catalog, ChangeSet, Command, CommandInfo, HistoryAction, Stability,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type Counters = BTreeMap<String, i64>;

/// Names of the counters touched, or every counter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Touched {
    pub all: bool,
    pub names: BTreeSet<String>,
}

impl Touched {
    pub fn one(name: &str) -> Self {
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
pub struct AddParams {
    pub counter: String,
    pub amount: i64,
    /// Bytes the command pretends to carry, to exercise the memory budget.
    #[serde(default)]
    pub payload: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetParams {
    pub counter: String,
    pub value: i64,
    /// Bytes the command pretends to carry, to exercise the memory budget.
    #[serde(default)]
    pub payload: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", content = "params")]
pub enum Toy {
    #[serde(rename = "counter.add")]
    Add(AddParams),
    #[serde(rename = "counter.set")]
    Set(SetParams),
    #[serde(rename = "history.undo")]
    Undo,
    #[serde(rename = "history.redo")]
    Redo,
}

#[derive(Debug, PartialEq)]
pub enum ToyError {
    Overflow,
}

pub fn set(counter: &str, value: i64) -> Toy {
    Toy::Set(SetParams {
        counter: counter.to_owned(),
        value,
        payload: 0,
    })
}

impl Command for Toy {
    type Target = Counters;
    type Changes = Touched;
    type Error = ToyError;

    fn name(&self) -> &'static str {
        match self {
            Self::Add(_) => "counter.add",
            Self::Set(_) => "counter.set",
            Self::Undo => "history.undo",
            Self::Redo => "history.redo",
        }
    }

    fn history_action(&self) -> Option<HistoryAction> {
        match self {
            Self::Undo => Some(HistoryAction::Undo),
            Self::Redo => Some(HistoryAction::Redo),
            _ => None,
        }
    }

    /// Consecutive sets of the same counter in one gesture merge.
    fn coalesce_key(&self) -> Option<String> {
        match self {
            Self::Set(params) => Some(format!("set:{}", params.counter)),
            _ => None,
        }
    }

    fn payload_bytes(&self) -> usize {
        match self {
            Self::Add(params) => params.payload,
            Self::Set(params) => params.payload,
            _ => 0,
        }
    }

    fn apply(self, target: &mut Counters) -> Result<Applied<Self>, ToyError> {
        match self {
            // Executed by the bus itself, never applied.
            Self::Undo | Self::Redo => Err(ToyError::Overflow),
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
                        payload: params.payload,
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
                        payload: params.payload,
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

pub fn add(counter: &str, amount: i64) -> Toy {
    Toy::Add(AddParams {
        counter: counter.to_owned(),
        amount,
        payload: 0,
    })
}

pub fn heavy(counter: &str, payload: usize) -> Toy {
    Toy::Add(AddParams {
        counter: counter.to_owned(),
        amount: 1,
        payload,
    })
}
