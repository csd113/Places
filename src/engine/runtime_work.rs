//! Synchronous CPU preparation on one bounded, persistent worker pool.
//!
//! Consumers retain ordered ownership of their slots. A call joins all work
//! before returning, so GPU uploads, gameplay publication, map changes and
//! shutdown never inherit queued or stale results. Small batches stay inline.

use std::sync::OnceLock;

#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

const MAX_RUNTIME_WORKERS: usize = 8;
const DEFAULT_RUNTIME_WORKERS: usize = 4;

/// Reusable executor; one worker means execution on the calling thread.
pub struct RuntimeWork {
    #[cfg(not(target_arch = "wasm32"))]
    pool: Option<Arc<rayon::ThreadPool>>,
}

impl RuntimeWork {
    /// Creates a bounded executor, without starting threads for serial work.
    ///
    /// # Errors
    /// Returns an error for an unsupported worker count or thread creation failure.
    pub fn new(workers: usize) -> Result<Self, String> {
        if !(1..=MAX_RUNTIME_WORKERS).contains(&workers) {
            return Err(format!("runtime workers must be 1..={MAX_RUNTIME_WORKERS}"));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let pool = if workers == 1 {
                None
            } else {
                Some(Arc::new(
                    rayon::ThreadPoolBuilder::new()
                        .num_threads(workers)
                        .thread_name(|index| format!("places-cpu-{index}"))
                        .build()
                        .map_err(|error| format!("could not start runtime workers: {error}"))?,
                ))
            };
            Ok(Self { pool })
        }
        #[cfg(target_arch = "wasm32")]
        Ok(Self {})
    }

    /// Applies independent work to disjoint slots, joining before returning.
    /// `min_items` is the consumer's measured granularity threshold.
    pub fn for_each_mut<T, F>(&self, items: &mut [T], min_items: usize, operation: F)
    where
        T: Send,
        F: Fn(&mut T) + Send + Sync,
    {
        #[cfg(not(target_arch = "wasm32"))]
        if items.len() >= min_items.max(2)
            && let Some(pool) = &self.pool
        {
            pool.install(|| items.par_iter_mut().for_each(operation));
            return;
        }
        items.iter_mut().for_each(operation);
    }

    /// Number of persistent workers, or one for inline execution.
    #[must_use]
    pub fn workers(&self) -> usize {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.pool
                .as_ref()
                .map_or(1, |pool| pool.current_num_threads())
        }
        #[cfg(target_arch = "wasm32")]
        1
    }

    /// Shares the existing native executor with synchronous physics work.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn pool(&self) -> Option<Arc<rayon::ThreadPool>> {
        self.pool.clone()
    }
}

fn configured_workers() -> usize {
    if cfg!(target_arch = "wasm32") {
        return 1;
    }
    let budget = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .saturating_sub(1)
        .clamp(1, MAX_RUNTIME_WORKERS);
    if let Ok(value) = std::env::var("PLACES_RUNTIME_WORKERS") {
        match value.parse::<usize>() {
            Ok(workers @ 1..=MAX_RUNTIME_WORKERS) => return workers.min(budget),
            Ok(_) | Err(_) => crate::engine::logging::warn(format!(
                "[runtime] invalid PLACES_RUNTIME_WORKERS={value:?}; using automatic budget"
            )),
        }
    }
    budget.min(DEFAULT_RUNTIME_WORKERS)
}

/// The process-wide executor, shared by all runtime CPU consumers.
#[must_use]
pub fn global() -> &'static RuntimeWork {
    static EXECUTOR: OnceLock<RuntimeWork> = OnceLock::new();
    EXECUTOR.get_or_init(|| match RuntimeWork::new(configured_workers()) {
        Ok(executor) => {
            crate::engine::logging::info(format!("[runtime] CPU workers: {}", executor.workers()));
            executor
        }
        Err(error) => {
            crate::engine::logging::warn(format!("[runtime] {error}; using inline execution"));
            RuntimeWork {
                #[cfg(not(target_arch = "wasm32"))]
                pool: None,
            }
        }
    })
}

/// Shared native CPU pool; `None` requests inline application work.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pool() -> Option<Arc<rayon::ThreadPool>> {
    global().pool()
}

/// Explicit one-worker Rapier reference pool.
/// Rapier's `None` uses Rayon's global pool, so it is never a serial fallback.
///
/// # Errors
/// Returns the cached thread-creation error instead of using an unbounded pool.
#[cfg(all(feature = "parallel-physics", not(target_arch = "wasm32")))]
pub fn serial_pool() -> Result<Arc<rayon::ThreadPool>, String> {
    static SERIAL_POOL: OnceLock<Result<Arc<rayon::ThreadPool>, String>> = OnceLock::new();
    SERIAL_POOL
        .get_or_init(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .thread_name(|_| "places-physics-reference".to_string())
                .build()
                .map(Arc::new)
                .map_err(|error| format!("could not start serial physics worker: {error}"))
        })
        .clone()
}

/// Keeps small workloads inline without even initializing the worker pool.
pub fn for_each_mut<T, F>(items: &mut [T], min_items: usize, operation: F)
where
    T: Send,
    F: Fn(&mut T) + Send + Sync,
{
    if items.len() < min_items.max(2) {
        items.iter_mut().for_each(operation);
    } else {
        global().for_each_mut(items, min_items, operation);
    }
}
