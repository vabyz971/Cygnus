// Cygnus — Suite créative professionnelle open source
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

//! DEDICATED, CAPPED rayon pool for heavy image computation.
//!
//! RENDERING.md invariant #4: compositing NEVER uses the global rayon
//! pool — uncapped, it would saturate all cores and freeze the UI thread
//! even when the computation runs in `spawn_blocking`. [`run_parallel`]
//! confines all nested `par_iter`/`par_bridge` to the pool named here (≤ 4
//! threads), leaving CPU for the rest of the system.

use std::sync::OnceLock;

struct RenderExecutor {
    pool: rayon::ThreadPool,
}

impl RenderExecutor {
    fn new(pool: rayon::ThreadPool) -> Self {
        Self { pool }
    }
}

/// The engine's single bounded pool. `None` only if the OS refuses to
/// create threads (pathological system state) — then degrade to inline
/// execution rather than crashing the app.
static EXECUTOR: OnceLock<Option<RenderExecutor>> = OnceLock::new();

fn executor() -> &'static Option<RenderExecutor> {
    EXECUTOR.get_or_init(|| {
        // Deliberate cap: past 4 threads blending is memory-bound and gains are
        // marginal — while contention with the UI grows.
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(2, 4);
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("creative-render-{i}"))
            .build()
            .map(RenderExecutor::new)
            .ok()
    })
}

/// Runs `f` on the dedicated render pool. `par_*` calls made from `f`
/// (directly or in sub-functions) run on THIS pool, not the global one.
/// If `f` is already on the pool (nested install), rayon runs it
/// directly — no reentrancy risk.
pub fn run_parallel<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    match executor().as_ref() {
        Some(ex) => ex.pool.install(f),
        None => f(),
    }
}
