//! Optional preparation feedback shared by engine workers and tool front ends.
//!
//! Workers publish stages and atomic counts through an installed observer.
//! Presentation, signal handling and application completion policy belong to
//! that observer; the engine has no dependency on a compiler or native UI.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

static ACTIVE: Mutex<Option<Arc<dyn Reporter>>> = Mutex::new(None);
static ENABLED: AtomicBool = AtomicBool::new(false);

/// A front end's adapter for preparation feedback and cancellation.
pub(crate) trait Reporter: Send + Sync {
    fn stage(&self, label: &str, unit: &'static str);
    fn current_work(&self, total: usize) -> Option<Arc<Counter>>;
    fn start_work(&self, label: &str, total: usize, unit: &'static str) -> Option<Arc<Counter>>;
    fn refresh(&self);
    fn cancellation_flag(&self) -> Arc<AtomicBool>;
}

/// Keeps one observer installed until its front end stops the session.
pub(crate) struct Subscription;

/// Installs the sole active preparation observer.
pub(crate) fn subscribe(reporter: Arc<dyn Reporter>) -> Result<Subscription, String> {
    let mut active = ACTIVE.lock().map_err(|error| error.to_string())?;
    if active.is_some() {
        return Err("a preparation progress observer is already active".into());
    }
    *active = Some(reporter);
    drop(active);
    ENABLED.store(true, Ordering::Relaxed);
    Ok(Subscription)
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE.lock() {
            ENABLED.store(false, Ordering::Relaxed);
            *active = None;
        }
    }
}

fn active() -> Option<Arc<dyn Reporter>> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    ACTIVE.lock().ok()?.as_ref().map(Arc::clone)
}

pub(crate) fn cancellation_flag() -> Arc<AtomicBool> {
    active().map_or_else(
        || Arc::new(AtomicBool::new(false)),
        |reporter| reporter.cancellation_flag(),
    )
}

pub(crate) fn check_cancelled() -> Result<(), String> {
    if active().is_some_and(|reporter| reporter.cancellation_flag().load(Ordering::Relaxed)) {
        Err("build interrupted".into())
    } else {
        Ok(())
    }
}

pub(crate) fn stage(label: impl AsRef<str>) {
    stage_with_unit(label.as_ref(), "receivers");
}

pub(crate) fn stage_with_unit(label: &str, unit: &'static str) {
    if let Some(reporter) = active() {
        reporter.stage(label, unit);
    }
}

/// Completed items in one pass; the observer owns the human-readable view.
pub(crate) struct Counter {
    pub(crate) done: AtomicUsize,
    pub(crate) total: usize,
    pub(crate) unit: &'static str,
}

/// Acquired once per parallel pass; updates are a relaxed atomic per batch.
pub(crate) struct Work(pub(crate) Option<Arc<Counter>>);

impl Work {
    pub(crate) fn current(total: usize) -> Self {
        Self(active().and_then(|reporter| reporter.current_work(total)))
    }

    pub(crate) fn start(label: &str, total: usize, unit: &'static str) -> Self {
        Self(active().and_then(|reporter| reporter.start_work(label, total, unit)))
    }

    pub(crate) fn advance(&self, count: usize) {
        if let Some(counter) = &self.0 {
            let _previous = counter.done.fetch_add(count, Ordering::Relaxed);
        }
    }

    pub(crate) fn finish(&self) {
        if self.0.is_some()
            && let Some(reporter) = active()
        {
            reporter.refresh();
        }
    }
}
