//! Cooperative load cancellation and aggregate process-memory admission.
//! Operating budgets are separate from immutable package/decoder safety limits.
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const MIB: u64 = 1024 * 1024;

/// A refused old/new world overlap, before either live owner is changed.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct MapAdmissionError {
    pub displayed: u64,
    pub incoming: u64,
    pub budget: u64,
}

impl MapAdmissionError {
    /// Unloading can help only when the incoming world fits on its own.
    pub const fn can_retry_after_unload(self) -> bool {
        self.displayed > 0 && self.incoming <= self.budget
    }
}

impl std::fmt::Display for MapAdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Map GPU budget exceeded: displayed {} + incoming {} charged bytes, budget {}. The current map is preserved.",
            self.displayed, self.incoming, self.budget
        )
    }
}

pub fn cpu_budget() -> u64 {
    configured("PLACES_LOAD_MEMORY_MIB", 4096)
}
pub fn gpu_budget() -> u64 {
    configured("PLACES_MAP_GPU_MIB", 2048)
}
fn configured(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(fallback)
        .clamp(128, 8192)
        .saturating_mul(MIB)
}

/// One worker request and its explicitly inherited decoder tasks share reservations.
pub struct DecodeBudget {
    pub cancelled: Arc<AtomicBool>,
    pub decoded: AtomicU64,
    refused: AtomicBool,
    reserved: AtomicU64,
    limit: u64,
}
impl DecodeBudget {
    pub fn new(cancelled: Arc<AtomicBool>) -> Arc<Self> {
        Arc::new(Self {
            cancelled,
            decoded: AtomicU64::new(0),
            refused: AtomicBool::new(false),
            reserved: AtomicU64::new(0),
            limit: cpu_budget(),
        })
    }
}
thread_local! { static CURRENT: RefCell<Option<Arc<DecodeBudget>>> = const { RefCell::new(None) }; }
pub struct DecodeScope(Option<Arc<DecodeBudget>>);
impl Drop for DecodeScope {
    fn drop(&mut self) {
        CURRENT.with(|current| {
            let _previous = current.replace(self.0.take());
        });
    }
}
pub fn current() -> Option<Arc<DecodeBudget>> {
    CURRENT.with(|current| current.borrow().clone())
}
pub fn enter(budget: Option<Arc<DecodeBudget>>) -> DecodeScope {
    DecodeScope(CURRENT.with(|current| current.replace(budget)))
}
pub fn checkpoint() -> Result<(), String> {
    CURRENT.with(|current| {
        if let Some(budget) = current.borrow().as_ref() {
            if budget.cancelled.load(Ordering::Relaxed) { return Err("Map preparation cancelled".into()); }
            if budget.refused.load(Ordering::Relaxed) { return Err("Map memory budget exceeded; the current map is preserved. Return to the menu before retrying.".into()); }
        }
        Ok(())
    })
}
pub fn decoded(bytes: usize) {
    if let Some(budget) = current() {
        let _previous = budget
            .decoded
            .fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
    }
}

/// A short-lived allocation reservation. Existing worlds are never evicted by it.
/// Measured process commit includes active maps, cache owners, driver allocations,
/// and other CPU state. Reservations cover simultaneous decoders before allocation.
pub struct Reservation {
    budget: Option<Arc<DecodeBudget>>,
    bytes: u64,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(budget) = &self.budget {
            let _previous = budget.reserved.fetch_sub(self.bytes, Ordering::AcqRel);
        }
    }
}
pub fn allocation(bytes: usize) -> Result<Reservation, String> {
    checkpoint()?;
    let budget = current();
    let requested_bytes = u64::try_from(bytes).map_err(|error| error.to_string())?;
    if let Some(state) = &budget {
        state.reserve(process_memory_bytes().unwrap_or(0), requested_bytes)?;
    }
    Ok(Reservation {
        budget,
        bytes: requested_bytes,
    })
}

/// Windows private commit; Linux/macOS resident bytes where available. These are
/// process measures, never added to GPU bytes (unified memory would double count).
#[cfg(target_os = "windows")]
pub fn process_memory_bytes() -> Option<u64> {
    #[repr(C)]
    struct Counters {
        cb: u32,
        faults: u32,
        peak_rss: usize,
        rss: usize,
        peak_page: usize,
        page: usize,
        peak_nonpage: usize,
        nonpage: usize,
        commit: usize,
        peak_commit: usize,
        private: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut Counters,
            size: u32,
        ) -> i32;
    }
    let size = u32::try_from(std::mem::size_of::<Counters>()).ok()?;
    let mut counters = Counters {
        cb: size,
        faults: 0,
        peak_rss: 0,
        rss: 0,
        peak_page: 0,
        page: 0,
        peak_nonpage: 0,
        nonpage: 0,
        commit: 0,
        peak_commit: 0,
        private: 0,
    };
    // SAFETY: the pseudo-handle belongs to this process; the initialized repr(C)
    // structure and its exact size remain exclusively borrowed for the call.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: the process pseudo-handle is valid and counters has exactly size bytes.
    let success = unsafe { GetProcessMemoryInfo(process, &raw mut counters, size) };
    (success != 0)
        .then(|| u64::try_from(counters.private).ok())
        .flatten()
}
#[cfg(not(target_os = "windows"))]
pub fn process_memory_bytes() -> Option<u64> {
    crate::engine::perf::process_resident_bytes()
}

pub fn decode<T>(bytes: &[u8], read: impl FnOnce(&[u8]) -> Result<T, String>) -> Result<T, String> {
    let _reservation = allocation(bytes.len().saturating_mul(2))?;
    read(bytes)
}

impl DecodeBudget {
    fn reserve(&self, used: u64, requested_bytes: u64) -> Result<(), String> {
        let result = self
            .reserved
            .try_update(Ordering::AcqRel, Ordering::Acquire, |reserved| {
                let _total = used
                    .checked_add(reserved)?
                    .checked_add(requested_bytes)
                    .filter(|total| *total <= self.limit)?;
                reserved.checked_add(requested_bytes)
            });
        if result.is_err() {
            self.refused.store(true, Ordering::Relaxed);
            return Err(format!(
                "Map memory budget exceeded: process {used} + pending {} + allocation {requested_bytes} bytes, budget {}. The current map is preserved; close other applications or return to the menu before retrying.",
                self.reserved.load(Ordering::Acquire),
                self.limit
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/resource_admission.rs"]
mod tests;

/// Read-only Windows heap ownership/commit snapshot for explicit resource traces.
/// This includes native/driver heaps, not only Rust, and excludes `VirtualAlloc`
/// allocations outside those heaps. It never compacts or trims the allocator.
#[cfg(target_os = "windows")]
pub fn heap_diagnostics() -> Option<serde_json::Value> {
    #[repr(C)]
    #[derive(Default)]
    struct Summary {
        cb: u32,
        allocated: usize,
        committed: usize,
        reserved: usize,
        max_reserve: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetProcessHeaps(count: u32, heaps: *mut *mut std::ffi::c_void) -> u32;
        fn HeapSummary(heap: *mut std::ffi::c_void, flags: u32, summary: *mut Summary) -> i32;
    }
    // SAFETY: A null buffer and zero count query only the current process's count.
    let count = unsafe { GetProcessHeaps(0, std::ptr::null_mut()) };
    if count == 0 || count > 1_024 {
        return None;
    }
    let mut heaps = vec![std::ptr::null_mut(); usize::try_from(count).ok()?];
    // SAFETY: The buffer holds count handles and is exclusively borrowed.
    let copied = unsafe { GetProcessHeaps(count, heaps.as_mut_ptr()) };
    if copied == 0 || copied > count {
        return None;
    }
    let mut allocated = 0_usize;
    let mut committed = 0_usize;
    let mut reserved = 0_usize;
    let mut reported = 0_u32;
    for heap in heaps.iter().take(usize::try_from(copied).ok()?) {
        let mut summary = Summary {
            cb: u32::try_from(std::mem::size_of::<Summary>()).ok()?,
            ..Summary::default()
        };
        // SAFETY: The handle was returned for this process. The initialized
        // repr(C) structure has the exact cb size and is exclusively borrowed.
        let success = unsafe { HeapSummary(*heap, 0, &raw mut summary) };
        if success != 0_i32 {
            allocated = allocated.saturating_add(summary.allocated);
            committed = committed.saturating_add(summary.committed);
            reserved = reserved.saturating_add(summary.reserved);
            reported = reported.saturating_add(1);
        }
    }
    if reported == 0 {
        return None;
    }
    Some(
        serde_json::json!({"heaps":copied,"reported":reported,"allocated_bytes":allocated,"committed_bytes":committed,"reserved_bytes":reserved,"scope":"All process heaps; native/driver allocations may be included. VirtualAlloc allocations outside heaps are excluded. Not added to process/GPU bytes."}),
    )
}
#[cfg(not(target_os = "windows"))]
pub fn heap_diagnostics() -> Option<serde_json::Value> {
    None
}
