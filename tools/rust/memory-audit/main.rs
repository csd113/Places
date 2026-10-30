//! Explicit Rust allocator attribution for real native player/editor sessions.
//! This binary alone installs the counting allocator; production binaries do not.
//! Compare its CSV with the explicit native campaign's separate OS process samples.
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct AuditAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: AuditAllocator = AuditAllocator;

fn allocated(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::Relaxed).saturating_add(size);
    let _previous = PEAK.fetch_max(live, Ordering::Relaxed);
}
fn released(size: usize) {
    let _previous = LIVE.fetch_sub(size, Ordering::Relaxed);
}
// SAFETY: All operations forward unchanged pointers/layouts to System. Counter
// updates allocate nothing, take no locks and never change allocation ownership.
unsafe impl GlobalAlloc for AuditAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies System's required valid allocation layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies System's required valid allocation layout.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        released(layout.size());
        // SAFETY: The original System allocation and layout are unchanged.
        unsafe {
            System.dealloc(pointer, layout);
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: The caller's original allocation/layout and new size are unchanged.
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            if size >= layout.size() {
                allocated(size.saturating_sub(layout.size()));
            } else {
                released(layout.size().saturating_sub(size));
            }
        }
        result
    }
}

fn record(file: &mut BufWriter<File>, start: Instant, phase: &str) -> std::io::Result<()> {
    let live = LIVE.load(Ordering::Relaxed);
    let peak = PEAK.load(Ordering::Relaxed);
    writeln!(
        file,
        "{:.3},{phase},{live},{peak}",
        start.elapsed().as_secs_f64() * 1_000.0_f64
    )?;
    file.flush()
}

fn run() -> Result<std::process::ExitCode, String> {
    let mode = std::env::var("PLACES_MEMORY_AUDIT_MODE")
        .map_err(|error| format!("{error}; set PLACES_MEMORY_AUDIT_MODE=game or editor"))?;
    if !matches!(mode.as_str(), "game" | "editor") {
        return Err("PLACES_MEMORY_AUDIT_MODE must be game or editor".into());
    }
    let path = std::env::var_os("PLACES_MEMORY_AUDIT_OUT")
        .ok_or("Set PLACES_MEMORY_AUDIT_OUT to a new CSV file")?;
    let output_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let mut file = BufWriter::with_capacity(4_096, output_file);
    writeln!(file, "elapsed_ms,phase,rust_live_bytes,rust_peak_bytes")
        .map_err(|error| error.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&stop);
    let start = Instant::now();
    let sampler = std::thread::Builder::new()
        .name("memory-audit".into())
        .spawn(move || {
            record(&mut file, start, "entry")?;
            while !done.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(250));
                record(&mut file, start, "sample")?;
            }
            record(&mut file, start, "application-returned")
        })
        .map_err(|error| error.to_string())?;
    let result = if mode == "editor" {
        Ok(places::editor::run())
    } else {
        std::env::args_os()
            .skip(1)
            .map(std::ffi::OsString::into_string)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|argument| {
                format!(
                    "Invalid Unicode argument ({} encoded bytes)",
                    argument.len()
                )
            })
            .and_then(|args| places::game::run(&args).map_err(|error| error.to_string()))
            .map(|()| std::process::ExitCode::SUCCESS)
    };
    stop.store(true, Ordering::Release);
    sampler
        .join()
        .map_err(|payload| {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("unknown panic payload");
            format!("Memory sampler panicked: {message}")
        })?
        .map_err(|error| error.to_string())?;
    result
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            places::engine::logging::warn(format_args!("Memory audit: {error}"));
            std::process::ExitCode::FAILURE
        }
    }
}
