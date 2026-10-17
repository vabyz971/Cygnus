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

//! Latest-value-wins folding of queued commands.
//!
//! When the UI streams a gesture faster than the worker renders, the
//! queue holds several values of the same setting. Folding keeps only
//! what matters (the last opacity, the summed move…) BEFORE applying, so
//! intermediate values never reach the renderer. Only ADJACENT commands
//! fold: an `Undo` (or anything else) between two ticks keeps both.

/// A command that can absorb the next queued one.
pub trait Coalesce {
    /// Merges `next` into `self` when they are two values of the same
    /// continuous setting; returns false (nothing changed) otherwise.
    fn absorb(&mut self, next: &Self) -> bool;

    /// True when the (possibly merged) command no longer does anything
    /// and can be dropped (e.g. a move summing to zero).
    fn is_noop(&self) -> bool {
        false
    }
}

/// Folds adjacent absorbable commands, then drops no-ops. Order of
/// everything else is preserved.
#[must_use]
pub fn fold_batch<C: Coalesce>(commands: Vec<C>) -> Vec<C> {
    let mut folded: Vec<C> = Vec::with_capacity(commands.len());
    for command in commands {
        let merged = folded.last_mut().is_some_and(|last| last.absorb(&command));
        if !merged {
            folded.push(command);
        }
    }
    folded.retain(|c| !c.is_noop());
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    enum Cmd {
        Set(u8, i32),
        Move(i32),
        Undo,
    }

    impl Coalesce for Cmd {
        fn absorb(&mut self, next: &Self) -> bool {
            match (self, next) {
                (Cmd::Set(a, v), Cmd::Set(b, w)) if a == b => {
                    *v = *w;
                    true
                }
                (Cmd::Move(d), Cmd::Move(e)) => {
                    *d += *e;
                    true
                }
                _ => false,
            }
        }

        fn is_noop(&self) -> bool {
            matches!(self, Cmd::Move(0))
        }
    }

    #[test]
    fn adjacent_values_fold_to_the_last() {
        let out = fold_batch(vec![Cmd::Set(1, 10), Cmd::Set(1, 20), Cmd::Set(1, 30)]);
        assert_eq!(out, vec![Cmd::Set(1, 30)]);
    }

    #[test]
    fn other_keys_and_barriers_do_not_fold() {
        let out = fold_batch(vec![
            Cmd::Set(1, 10),
            Cmd::Set(2, 5),
            Cmd::Set(1, 20),
            Cmd::Undo,
            Cmd::Set(1, 30),
        ]);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn summed_noops_are_dropped() {
        assert!(fold_batch(vec![Cmd::Move(3), Cmd::Move(-3)]).is_empty());
        assert_eq!(
            fold_batch(vec![Cmd::Move(3), Cmd::Move(2)]),
            vec![Cmd::Move(5)]
        );
    }
}
