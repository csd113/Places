//! Opt-in archive attribution. Read/seek time includes the OS cache; the
//! remainder includes decompression, CRC, copies, admission and integrity work.
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[derive(Default)]
pub(super) struct Counters {
    nanos: AtomicU64,
    bytes: AtomicU64,
}

impl Counters {
    pub fn start(self: &Arc<Self>) -> Timing {
        Timing {
            counters: Arc::clone(self),
            started: Instant::now(),
            nanos: self.nanos.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

pub(super) struct Timing {
    counters: Arc<Counters>,
    started: Instant,
    nanos: u64,
    bytes: u64,
}

impl Timing {
    pub fn report(&self, name: &str, decoded: usize) {
        let io = std::time::Duration::from_nanos(
            self.counters
                .nanos
                .load(Ordering::Relaxed)
                .saturating_sub(self.nanos),
        );
        crate::engine::logging::info(format_args!(
            "[loading-archive] {}",
            serde_json::json!({
                "entry": name, "decoded_bytes": decoded,
                "read_bytes": self.counters.bytes.load(Ordering::Relaxed).saturating_sub(self.bytes),
                "read_seek_ms": io.as_secs_f64() * 1_000.0_f64,
                "decode_check_copy_ms": self.started.elapsed().saturating_sub(io).as_secs_f64() * 1_000.0_f64,
            }),
        ));
    }
}

pub(super) struct Reader<R> {
    inner: R,
    counters: Option<Arc<Counters>>,
}

impl<R> Reader<R> {
    pub const fn new(inner: R, counters: Option<Arc<Counters>>) -> Self {
        Self { inner, counters }
    }
}

impl<R: Read> Read for Reader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let Some(counters) = &self.counters else {
            return self.inner.read(buffer);
        };
        let started = Instant::now();
        let result = self.inner.read(buffer);
        let _nanos = counters.nanos.fetch_add(
            u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        if let Ok(count) = result {
            let _bytes = counters
                .bytes
                .fetch_add(u64::try_from(count).unwrap_or(u64::MAX), Ordering::Relaxed);
        }
        result
    }
}

impl<R: Seek> Seek for Reader<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let Some(counters) = &self.counters else {
            return self.inner.seek(position);
        };
        let started = Instant::now();
        let result = self.inner.seek(position);
        let _nanos = counters.nanos.fetch_add(
            u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        result
    }
}
