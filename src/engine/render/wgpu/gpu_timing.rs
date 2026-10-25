//! Nonblocking hardware timing of the render passes submitted for one frame.
//!
//! Only pass-boundary timestamps are required: Apple Metal GPUs support those
//! without supporting arbitrary timestamp commands inside encoders. Three
//! persistent slots keep a pending map from being reused by the GPU; if every
//! slot is busy, timing skips a frame rather than waiting for it.

use std::cell::Cell;
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::engine::render::common::stats::{GpuTiming, GpuTimingSample, GpuTimingUnavailable};

const READBACK_SLOTS: usize = 3;
// A normal frame has at most eight passes: planar, scene, emission, two blurs,
// resolve, UI and surface copy. Leave bounded room for another small pass.
const MAX_QUERIES: u32 = 32;
const MAX_PASS_LABELS: usize = 16;
// 32 eight-byte query values also fill one resolve-alignment block.
const QUERY_BYTES: u64 = 256;
const RAW_TRACE_FIRST_FRAME: u64 = 600;
const RAW_TRACE_MAX_SAMPLES: u8 = 64;

/// The optional feature needed by all native render-pass timestamp writes.
pub(super) fn required_features(available: wgpu::Features) -> wgpu::Features {
    available & wgpu::Features::TIMESTAMP_QUERY
}

#[derive(Clone, Copy)]
struct FrameQueries {
    queries: u32,
    frame: u64,
    empty_pairs: u32,
    labels: [&'static str; MAX_PASS_LABELS],
    draws: [Option<usize>; MAX_PASS_LABELS],
}

/// Owns the bounded opt-in trace budget independently of timestamp decoding.
struct RawTrace {
    minimum_ms: Option<f32>,
    reports: u8,
    next_frame: u64,
}

impl RawTrace {
    fn new() -> Self {
        Self {
            minimum_ms: std::env::var("PLACES_BENCH_GPU_TRACE_MIN_MS")
                .ok()
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite() && (0.0..=1_000.0).contains(value)),
            reports: 0,
            next_frame: RAW_TRACE_FIRST_FRAME,
        }
    }

    fn record(
        &mut self,
        bytes: &[u8],
        frame: &FrameQueries,
        period_ns: f32,
        milliseconds: Option<f32>,
    ) {
        if self.reports >= RAW_TRACE_MAX_SAMPLES
            || frame.frame < self.next_frame
            || !milliseconds
                .is_some_and(|value| self.minimum_ms.is_none_or(|minimum| value >= minimum))
        {
            return;
        }
        trace_raw_sample(bytes, frame, period_ns);
        self.reports = self.reports.saturating_add(1);
        let interval = if self.minimum_ms.is_some() { 1 } else { 120 };
        self.next_frame = frame.frame.saturating_add(interval);
    }
}

struct PendingReadback {
    receiver: Receiver<Result<(), wgpu::BufferAsyncError>>,
    frame: FrameQueries,
}

struct PendingWrites {
    receiver: Receiver<()>,
    frame: FrameQueries,
}

enum PendingSlot {
    Written(PendingWrites),
    Readback(PendingReadback),
    /// An incomplete frame wrote queries; retain its slot until GPU completion
    /// without resolving, mapping or reporting any timing for that frame.
    Retired(Receiver<()>),
}

struct Slot {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    pending: Option<PendingSlot>,
}

impl Slot {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("places-gpu-render-timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: MAX_QUERIES,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("places-gpu-timestamp-resolve"),
                size: QUERY_BYTES,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("places-gpu-timestamp-readback"),
                size: QUERY_BYTES,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            pending: None,
        }
    }
}

/// Renderer-owned persistent timestamp resources; no device waits.
pub struct GpuTimer {
    slots: Vec<Slot>,
    active: Option<usize>,
    used_queries: Cell<u32>,
    empty_pairs: Cell<u32>,
    pass_labels: Cell<[&'static str; MAX_PASS_LABELS]>,
    pass_draws: Cell<[Option<usize>; MAX_PASS_LABELS]>,
    period_ns: f32,
    frame: u64,
    samples: u64,
    status: GpuTiming,
    rejected_sample_reports: u8,
    raw_trace: Option<RawTrace>,
}

impl GpuTimer {
    #[must_use]
    pub fn new(adapter: &wgpu::Adapter, device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        // Explicit benchmark controls are read once. Stray developer variables
        // cannot change normal gameplay or allocate a per-frame trace.
        let benchmark = std::env::var("PLACES_BENCH").is_ok_and(|value| value.trim() == "1");
        let timing_disabled = benchmark
            && std::env::var("PLACES_BENCH_GPU_TIMING")
                .is_ok_and(|value| value.trim().eq_ignore_ascii_case("off"));
        let raw_trace_enabled = benchmark
            && std::env::var("PLACES_BENCH_GPU_TRACE").is_ok_and(|value| value.trim() == "1");
        let period_ns = queue.get_timestamp_period();
        let unavailable = if timing_disabled {
            Some(GpuTimingUnavailable::BenchmarkDisabled)
        } else if !adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            Some(GpuTimingUnavailable::AdapterUnsupported)
        } else if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            Some(GpuTimingUnavailable::FeatureNotEnabled)
        } else if !period_ns.is_finite() || period_ns <= 0.0 {
            Some(GpuTimingUnavailable::InvalidTimestampPeriod)
        } else {
            None
        };
        Self {
            slots: if unavailable.is_none() {
                (0..READBACK_SLOTS).map(|_| Slot::new(device)).collect()
            } else {
                Vec::new()
            },
            active: None,
            used_queries: Cell::new(0),
            empty_pairs: Cell::new(0),
            pass_labels: Cell::new([""; MAX_PASS_LABELS]),
            pass_draws: Cell::new([None; MAX_PASS_LABELS]),
            period_ns,
            frame: 0,
            samples: 0,
            status: unavailable.map_or(GpuTiming::Pending, GpuTiming::Unavailable),
            rejected_sample_reports: 0,
            raw_trace: raw_trace_enabled.then(RawTrace::new),
        }
    }

    #[must_use]
    pub const fn status(&self) -> GpuTiming {
        self.status
    }

    #[must_use]
    pub const fn period_ns(&self) -> f32 {
        self.period_ns
    }

    /// Starts timing after consuming callbacks already delivered by polling.
    pub fn begin_frame(&mut self, queue: &wgpu::Queue) {
        // A scene-only facade submission has no final-pass completion point.
        // Discard its sample rather than resolving counters in a separate
        // command buffer, which can observe stale Metal stage boundaries.
        if let Some(slot) = self
            .active
            .take()
            .and_then(|index| self.slots.get_mut(index))
            && self.used_queries.get() > 0
        {
            slot.pending = Some(PendingSlot::Retired(completion_receiver(queue)));
        }
        self.frame = self.frame.saturating_add(1);
        self.used_queries.set(0);
        self.empty_pairs.set(0);
        self.pass_labels.set([""; MAX_PASS_LABELS]);
        self.pass_draws.set([None; MAX_PASS_LABELS]);
        // A failed sample says nothing about adapter support. A loading pass
        // may execute no vertex/fragment stages, so keep sampling supported
        // hardware and let a later complete result recover the meter.
        if !self.slots.is_empty() {
            self.active = self.slots.iter().position(|slot| slot.pending.is_none());
        }
    }

    /// Query position before encoding a pass whose draw count is not yet known.
    #[must_use]
    pub const fn pass_checkpoint(&self) -> u32 {
        self.used_queries.get()
    }

    /// Excludes only an authoritatively empty pass, never malformed data from
    /// a pass that rendered. Query indices remain unique even when excluded.
    pub fn record_pass_draws(&self, beginning: u32, draws: usize) {
        if self.used_queries.get() == beginning.saturating_add(2) {
            let mut counts = self.pass_draws.get();
            if let Some(slot) = usize::try_from(beginning / 2)
                .ok()
                .and_then(|index| counts.get_mut(index))
            {
                *slot = Some(draws);
                self.pass_draws.set(counts);
            }
        }
        if draws == 0
            && self.used_queries.get() == beginning.saturating_add(2)
            && let Some(bit) = 1_u32.checked_shl(beginning / 2)
        {
            self.empty_pairs.set(self.empty_pairs.get() | bit);
        }
    }

    /// Allocates one contiguous pair immediately before a real render pass.
    /// Static labels occupy fixed storage retained through asynchronous readback.
    #[must_use]
    pub fn pass_writes(&self, label: &'static str) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let slot = self.slots.get(self.active?)?;
        let beginning = self.used_queries.get();
        let end = beginning.checked_add(1)?;
        if end >= MAX_QUERIES {
            return None;
        }
        let mut labels = self.pass_labels.get();
        *labels.get_mut(usize::try_from(beginning / 2).ok()?)? = label;
        self.pass_labels.set(labels);
        self.used_queries.set(end.saturating_add(1));
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &slot.queries,
            beginning_of_pass_write_index: Some(beginning),
            end_of_pass_write_index: Some(end),
        })
    }

    /// Same pass-boundary query pair for compute deformation; no feature beyond
    /// `TIMESTAMP_QUERY` is needed on supported native backends.
    pub fn compute_writes(
        &self,
        label: &'static str,
    ) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        self.pass_writes(label)
            .map(|writes| wgpu::ComputePassTimestampWrites {
                query_set: writes.query_set,
                beginning_of_pass_write_index: writes.beginning_of_pass_write_index,
                end_of_pass_write_index: writes.end_of_pass_write_index,
            })
    }

    /// Registers completion after submission of the frame's final render pass.
    /// Metal stage-counter data is only reliable after GPU completion; even a
    /// resolve in the same command buffer can observe an unfinished boundary.
    /// The callback schedules no work and never waits.
    pub fn finish_frame_submitted(&mut self, queue: &wgpu::Queue) {
        let Some(slot) = self
            .active
            .take()
            .and_then(|index| self.slots.get_mut(index))
        else {
            return;
        };
        let queries = self.used_queries.get();
        let empty_pairs = self.empty_pairs.get();
        if queries == 0 {
            return;
        }
        let receiver = completion_receiver(queue);
        slot.pending = Some(if (pair_mask(queries) & !empty_pairs) == 0 {
            PendingSlot::Retired(receiver)
        } else {
            PendingSlot::Written(PendingWrites {
                receiver,
                frame: FrameQueries {
                    queries,
                    frame: self.frame,
                    empty_pairs,
                    labels: self.pass_labels.get(),
                    draws: self.pass_draws.get(),
                },
            })
        });
    }

    /// Reads ready mappings, drops each mapped view, then releases its slot.
    pub fn collect_ready(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        for slot in &mut self.slots {
            let Some(pending_state) = slot.pending.as_ref() else {
                continue;
            };
            let pending = match pending_state {
                PendingSlot::Written(written) => {
                    match written.receiver.try_recv() {
                        Ok(()) => {
                            let frame = written.frame;
                            slot.pending = Some(PendingSlot::Readback(resolve_completed(
                                slot, device, queue, &frame,
                            )));
                        }
                        Err(TryRecvError::Disconnected) => {
                            slot.pending = None;
                            self.status =
                                GpuTiming::Unavailable(GpuTimingUnavailable::ReadbackFailed);
                        }
                        Err(TryRecvError::Empty) => {}
                    }
                    continue;
                }
                PendingSlot::Readback(readback) => readback,
                PendingSlot::Retired(receiver) => {
                    if !matches!(receiver.try_recv(), Err(TryRecvError::Empty)) {
                        slot.pending = None;
                    }
                    continue;
                }
            };
            match pending.receiver.try_recv() {
                Ok(Ok(())) => {
                    let bytes = u64::from(pending.frame.queries)
                        .saturating_mul(u64::from(wgpu::QUERY_SIZE));
                    let mapped_milliseconds = slot
                        .readback
                        .slice(0..bytes)
                        .get_mapped_range()
                        .map_err(|error| TimestampDecodeError::MappedRange(error.to_string()))
                        .and_then(|mapped| {
                            let decoded = decode_milliseconds(
                                &mapped,
                                pending.frame.queries,
                                self.period_ns,
                                pending.frame.empty_pairs,
                            );
                            if let Some(trace) = self.raw_trace.as_mut() {
                                trace.record(
                                    &mapped,
                                    &pending.frame,
                                    self.period_ns,
                                    decoded.as_ref().copied().ok(),
                                );
                            }
                            decoded
                        });
                    let frame = pending.frame.frame;
                    let queries = pending.frame.queries;
                    slot.readback.unmap();
                    slot.pending = None;
                    if let Ok(milliseconds) = mapped_milliseconds {
                        self.samples = self.samples.saturating_add(1);
                        if !matches!(self.status, GpuTiming::Measured(sample) if sample.frame > frame)
                        {
                            self.status = GpuTiming::Measured(GpuTimingSample {
                                id: self.samples,
                                frame,
                                milliseconds,
                            });
                        }
                    } else {
                        if self.rejected_sample_reports < 3 {
                            self.rejected_sample_reports =
                                self.rejected_sample_reports.saturating_add(1);
                            crate::engine::logging::info(format!(
                                "[wgpu] GPU timestamp sample rejected: frame={frame} queries={queries} period_ns={} result={mapped_milliseconds:?}; retrying later frames",
                                self.period_ns,
                            ));
                        }
                        self.status =
                            GpuTiming::Unavailable(GpuTimingUnavailable::InvalidTimestampData);
                    }
                }
                Ok(Err(error)) => {
                    crate::engine::logging::warn_once(
                        "gpu-timestamp-map",
                        format!("[wgpu] GPU timestamp readback failed: {error}"),
                    );
                    slot.readback.unmap();
                    slot.pending = None;
                    self.status = GpuTiming::Unavailable(GpuTimingUnavailable::ReadbackFailed);
                }
                Err(TryRecvError::Disconnected) => {
                    slot.readback.unmap();
                    slot.pending = None;
                    self.status = GpuTiming::Unavailable(GpuTimingUnavailable::ReadbackFailed);
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }
}

/// At most 64 explicit benchmark records, each bounded to sixteen pairs.
/// Preserve labelled boundaries to inspect stage overlap independently of the
/// aggregate meter. This diagnostic never changes the measured rendering work.
fn trace_raw_sample(bytes: &[u8], frame: &FrameQueries, period_ns: f32) {
    let pairs: Vec<_> = bytes
        .as_chunks::<16>()
        .0
        .iter()
        .take(usize::try_from(MAX_QUERIES / 2).unwrap_or(0))
        .enumerate()
        .map(|(index, pair)| {
            let mut chunks = pair.as_chunks::<8>().0.iter();
            let beginning = chunks.next().map_or(0, |value| u64::from_ne_bytes(*value));
            let end = chunks.next().map_or(0, |value| u64::from_ne_bytes(*value));
            let empty = u32::try_from(index)
                .ok()
                .and_then(|shift| 1_u32.checked_shl(shift))
                .is_some_and(|bit| frame.empty_pairs & bit != 0);
            let duration_ms = end
                .checked_sub(beginning)
                .filter(|_| beginning != u64::MAX && end != u64::MAX)
                .map(|ticks| {
                    #[expect(
                        clippy::as_conversions,
                        clippy::cast_precision_loss,
                        reason = "Convert only elapsed hardware ticks to diagnostic milliseconds after integer subtraction"
                    )]
                    let elapsed = ticks as f64;
                    elapsed * f64::from(period_ns) / 1_000_000.0_f64
                });
            serde_json::json!({
                "pair": index,
                "label": frame.labels.get(index).copied().unwrap_or(""),
                "begin_ticks": beginning,
                "end_ticks": end,
                "duration_ms": duration_ms,
                "excluded_empty": empty,
                "submitted_draws": frame.draws.get(index).copied().flatten(),
            })
        })
        .collect();
    crate::engine::logging::info(format!(
        "[wgpu-gpu-trace] {}",
        serde_json::json!({
            "frame": frame.frame,
            "queries": frame.queries,
            "period_ns": period_ns,
            "empty_pairs": frame.empty_pairs,
            "pairs": pairs,
        })
    ));
}

/// Completion notification for already submitted GPU writes, without a wait.
fn completion_receiver(queue: &wgpu::Queue) -> Receiver<()> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    queue.on_submitted_work_done(move || {
        let _delivery_to_optional_receiver = sender.try_send(());
    });
    receiver
}

/// Only called after the write-completion callback; the slot stays busy until
/// mapping completes and its mapped view is dropped and unmapped.
fn resolve_completed(
    slot: &Slot,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    frame: &FrameQueries,
) -> PendingReadback {
    let bytes = u64::from(frame.queries).saturating_mul(u64::from(wgpu::QUERY_SIZE));
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("places-gpu-completed-timestamp-readback"),
    });
    encoder.resolve_query_set(&slot.queries, 0..frame.queries, &slot.resolve, 0);
    encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, bytes);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    encoder.map_buffer_on_submit(
        &slot.readback,
        wgpu::MapMode::Read,
        0..bytes,
        move |result| {
            let _delivery_to_optional_receiver = sender.try_send(result);
        },
    );
    let _submission = queue.submit([encoder.finish()]);
    PendingReadback {
        receiver,
        frame: *frame,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TimestampDecodeError {
    InvalidLayout,
    InvalidPeriod,
    NoRenderedPasses,
    MissingBoundary {
        pair: usize,
        beginning: u64,
        end: u64,
    },
    ReversedBoundary {
        pair: usize,
        beginning: u64,
        end: u64,
    },
    Overflow,
    NonFiniteDuration,
    MappedRange(String),
}

fn pair_mask(queries: u32) -> u32 {
    1_u32
        .checked_shl(queries / 2)
        .unwrap_or(0)
        .saturating_sub(1)
}

/// Converts hardware tick differences, never absolute timestamps or CPU time.
fn decode_milliseconds(
    bytes: &[u8],
    queries: u32,
    period_ns: f32,
    empty_pairs: u32,
) -> Result<f32, TimestampDecodeError> {
    if queries == 0 || queries > MAX_QUERIES || !queries.is_multiple_of(2) {
        return Err(TimestampDecodeError::InvalidLayout);
    }
    if !period_ns.is_finite() || period_ns <= 0.0 {
        return Err(TimestampDecodeError::InvalidPeriod);
    }
    let expected = usize::try_from(queries)
        .ok()
        .and_then(|count| count.checked_mul(8))
        .ok_or(TimestampDecodeError::InvalidLayout)?;
    if bytes.len() != expected {
        return Err(TimestampDecodeError::InvalidLayout);
    }
    if (pair_mask(queries) & !empty_pairs) == 0 {
        return Err(TimestampDecodeError::NoRenderedPasses);
    }
    let mut intervals = [(0_u64, 0_u64); MAX_PASS_LABELS];
    let mut count = 0_usize;
    for (pair_index, pair) in bytes.as_chunks::<16>().0.iter().enumerate() {
        if 1_u32
            .checked_shl(u32::try_from(pair_index).unwrap_or(u32::MAX))
            .is_some_and(|bit| empty_pairs & bit != 0)
        {
            continue;
        }
        let beginning = u64::from_ne_bytes(
            pair.get(..8)
                .and_then(|chunk| chunk.try_into().ok())
                .ok_or(TimestampDecodeError::InvalidLayout)?,
        );
        let end = u64::from_ne_bytes(
            pair.get(8..)
                .and_then(|chunk| chunk.try_into().ok())
                .ok_or(TimestampDecodeError::InvalidLayout)?,
        );
        // Metal resolves absent/error stage samples as MTLCounterErrorValue
        // (u64::MAX). Never turn them into a partial measured total. Zero is
        // permitted: a backend's hardware timestamp origin may be zero.
        if beginning == u64::MAX || end == u64::MAX {
            return Err(TimestampDecodeError::MissingBoundary {
                pair: pair_index,
                beginning,
                end,
            });
        }
        if end < beginning {
            return Err(TimestampDecodeError::ReversedBoundary {
                pair: pair_index,
                beginning,
                end,
            });
        }
        let interval = intervals
            .get_mut(count)
            .ok_or(TimestampDecodeError::InvalidLayout)?;
        *interval = (beginning, end);
        count = count.saturating_add(1);
    }
    // Metal samples vertex-start through fragment-end. Different passes can
    // overlap even in one command buffer; summing them charged the same time
    // up to seven times. Union the bounded intervals, excluding disjoint gaps.
    let rendered = intervals
        .get_mut(..count)
        .ok_or(TimestampDecodeError::InvalidLayout)?;
    rendered.sort_unstable();
    let mut current: Option<(u64, u64)> = None;
    let mut ticks = 0_u64;
    for &(beginning, end) in rendered.iter() {
        match current {
            Some((start, stop)) if beginning <= stop => {
                current = Some((start, stop.max(end)));
            }
            Some((start, stop)) => {
                ticks = ticks
                    .checked_add(stop.saturating_sub(start))
                    .ok_or(TimestampDecodeError::Overflow)?;
                current = Some((beginning, end));
            }
            None => current = Some((beginning, end)),
        }
    }
    if let Some((start, stop)) = current {
        ticks = ticks
            .checked_add(stop.saturating_sub(start))
            .ok_or(TimestampDecodeError::Overflow)?;
    }
    // Only elapsed render-pass ticks are converted, normally milliseconds of
    // work. The diagnostic retains f32 precision after the unit conversion.
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "A finite elapsed render-pass duration is converted from integer GPU ticks through f64 nanoseconds to f32 milliseconds; absolute timestamps are subtracted first"
    )]
    let milliseconds = ((ticks as f64) * f64::from(period_ns) / 1_000_000.0_f64) as f32;
    if milliseconds.is_finite() {
        Ok(milliseconds)
    } else {
        Err(TimestampDecodeError::NonFiniteDuration)
    }
}
