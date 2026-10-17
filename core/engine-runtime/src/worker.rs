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

//! Background engine thread.
//!
//! The UI owns a command `Sender` and a response `Receiver` and polls
//! the latter without blocking. The worker blocks on the first command,
//! drains everything already queued into ONE batch (the engine folds and
//! renders once), sends the responses, then calls the wake callback so an
//! idle UI loop picks them up immediately (egui only repaints on input
//! otherwise). Dropping the UI `Sender` stops the thread cleanly.

use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

/// Called by the worker after answering (the app requests a repaint;
/// a closure keeps the runtime free of any UI crate).
pub type Waker = Box<dyn Fn() + Send>;

/// An engine driven by [`spawn_engine`].
pub trait EngineLoop {
    /// Command sent by the UI.
    type Command: Send + 'static;
    /// Response sent back to the UI.
    type Response: Send + 'static;

    /// Applies every queued command (one render at most) and returns the
    /// responses, in order.
    fn apply_batch(&mut self, batch: Vec<Self::Command>) -> Vec<Self::Response>;
}

/// Starts the engine on its own thread. `make` builds the engine ON that
/// thread (engines holding non-`Sync` caches never cross threads).
pub fn spawn_engine<E, F>(
    make: F,
    commands: Receiver<E::Command>,
    responses: Sender<E::Response>,
    wake: Option<Waker>,
) -> JoinHandle<()>
where
    E: EngineLoop,
    F: FnOnce() -> E + Send + 'static,
{
    std::thread::spawn(move || {
        let mut engine = make();
        'worker: while let Ok(first) = commands.recv() {
            let mut batch = vec![first];
            while let Ok(next) = commands.try_recv() {
                batch.push(next);
            }
            for response in engine.apply_batch(batch) {
                if responses.send(response).is_err() {
                    break 'worker;
                }
            }
            if let Some(wake) = &wake {
                wake();
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    /// Echoes the batch size and the sum of its commands.
    struct Summer;

    impl EngineLoop for Summer {
        type Command = i32;
        type Response = (usize, i32);

        fn apply_batch(&mut self, batch: Vec<i32>) -> Vec<(usize, i32)> {
            vec![(batch.len(), batch.iter().sum())]
        }
    }

    #[test]
    fn answers_wakes_and_stops_when_the_ui_drops() {
        let (tx, rx) = channel();
        let (resp_tx, resp_rx) = channel();
        let woken = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&woken);
        let handle = spawn_engine(
            || Summer,
            rx,
            resp_tx,
            Some(Box::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            })),
        );
        tx.send(5).expect("envoi");
        let (_, sum) = resp_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("réponse");
        assert_eq!(sum, 5);
        drop(tx);
        handle.join().expect("arrêt propre");
        assert!(woken.load(Ordering::SeqCst) >= 1, "UI réveillée");
    }

    #[test]
    fn queued_commands_form_one_batch() {
        let (tx, rx) = channel();
        let (resp_tx, resp_rx) = channel();
        // Queue before the thread starts: the first batch sees all.
        for i in 1..=4 {
            tx.send(i).expect("envoi");
        }
        drop(tx);
        spawn_engine(|| Summer, rx, resp_tx, None)
            .join()
            .expect("arrêt");
        let all: Vec<(usize, i32)> = resp_rx.try_iter().collect();
        assert_eq!(all, vec![(4, 10)]);
    }
}
