//! Nonblocking hardware timing of the render passes submitted for one frame.
//!
//! Only pass-boundary timestamps are required: Apple Metal GPUs support those
//! without supporting arbitrary timestamp commands inside encoders. Three
//! persistent slots keep a pending map from being reused by the GPU; if every
//! slot is busy, timing skips a frame rather than waiting for it.

use std::cell::Cell;
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::render::common::stats::{GpuTiming, GpuTimingSample, GpuTimingUnavailable};

const READBACK_SLOTS: usize = 3;
// A normal frame has at most eight passes: planar, scene, emission, two blurs,
// resolve, UI and surface copy. Leave bounded room for another small pass.
const MAX_QUERIES: u32 = 32;
// 32 eight-byte query values also fill one resolve-alignment block.
const QUERY_BYTES: u64 = 256;

/// The optional feature needed by all native render-pass timestamp writes.
pub(super) fn required_features(available: wgpu::Features) -> wgpu::Features {
    available & wgpu::Features::TIMESTAMP_QUERY
}

#[derive(Clone, Copy)]
struct FrameQueries {
    queries: u32,
    frame: u64,
    empty_pairs: u32,
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
    period_ns: f32,
    frame: u64,
    samples: u64,
    status: GpuTiming,
    rejected_sample_reports: u8,
}

impl GpuTimer {
    #[must_use]
    pub fn new(adapter: &wgpu::Adapter, device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let period_ns = queue.get_timestamp_period();
        let unavailable = if !adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
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
            period_ns,
            frame: 0,
            samples: 0,
            status: unavailable.map_or(GpuTiming::Pending, GpuTiming::Unavailable),
            rejected_sample_reports: 0,
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
        if draws == 0
            && self.used_queries.get() == beginning.saturating_add(2)
            && let Some(bit) = 1_u32.checked_shl(beginning / 2)
        {
            self.empty_pairs.set(self.empty_pairs.get() | bit);
        }
    }

    /// Allocates one contiguous pair immediately before a real render pass.
    #[must_use]
    pub fn pass_writes(&self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let slot = self.slots.get(self.active?)?;
        let beginning = self.used_queries.get();
        let end = beginning.checked_add(1)?;
        if end >= MAX_QUERIES {
            return None;
        }
        self.used_queries.set(end.saturating_add(1));
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &slot.queries,
            beginning_of_pass_write_index: Some(beginning),
            end_of_pass_write_index: Some(end),
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
                                slot, device, queue, frame,
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
                            decode_milliseconds(
                                &mapped,
                                pending.frame.queries,
                                self.period_ns,
                                pending.frame.empty_pairs,
                            )
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
                            crate::logging::info(format!(
                                "[wgpu] GPU timestamp sample rejected: frame={frame} queries={queries} period_ns={} result={mapped_milliseconds:?}; retrying later frames",
                                self.period_ns,
                            ));
                        }
                        self.status =
                            GpuTiming::Unavailable(GpuTimingUnavailable::InvalidTimestampData);
                    }
                }
                Ok(Err(error)) => {
                    crate::logging::warn_once(
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
    frame: FrameQueries,
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
    PendingReadback { receiver, frame }
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
    let mut ticks = 0_u64;
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
        let elapsed = end
            .checked_sub(beginning)
            .ok_or(TimestampDecodeError::ReversedBoundary {
                pair: pair_index,
                beginning,
                end,
            })?;
        ticks = ticks
            .checked_add(elapsed)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_boundary_timing_does_not_require_encoder_timestamps() {
        // Protects Apple GPUs: encoder timestamp writes are unavailable even
        // when pass-boundary timing is supported. Pure bitflags, no GPU setup.
        assert_eq!(
            required_features(wgpu::Features::TIMESTAMP_QUERY),
            wgpu::Features::TIMESTAMP_QUERY
        );
        assert_eq!(
            required_features(wgpu::Features::empty()),
            wgpu::Features::empty()
        );
    }

    #[test]
    fn timestamp_readback_sums_passes_in_milliseconds_and_rejects_invalid_data() {
        // Two 16-byte pairs protect units, subtraction before float conversion,
        // and exclusion of queue/CPU gaps. No map bake or device is needed.
        let origin = 9_007_199_254_740_992_u64;
        let values = [
            origin,
            origin.saturating_add(1_000_000),
            origin.saturating_add(50_000_000),
            origin.saturating_add(52_000_000),
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u64::to_ne_bytes).collect();
        assert!(
            matches!(decode_milliseconds(&bytes, 4, 2.0, 0), Ok(ms) if (ms - 6.0).abs() < f32::EPSILON)
        );
        let zero_origin: Vec<u8> = [0_u64, 1_000_000_u64]
            .into_iter()
            .flat_map(u64::to_ne_bytes)
            .collect();
        assert!(
            matches!(decode_milliseconds(&zero_origin, 2, 1.0, 0), Ok(ms) if (ms - 1.0).abs() < f32::EPSILON)
        );
        assert_eq!(
            decode_milliseconds(&bytes, 2, 2.0, 0),
            Err(TimestampDecodeError::InvalidLayout)
        );
        assert_eq!(
            decode_milliseconds(&bytes, 3, 2.0, 0),
            Err(TimestampDecodeError::InvalidLayout)
        );
        assert_eq!(
            decode_milliseconds(&bytes, 4, f32::NAN, 0),
            Err(TimestampDecodeError::InvalidPeriod)
        );
        let reversed: Vec<u8> = [2_u64, 1_u64]
            .into_iter()
            .flat_map(u64::to_ne_bytes)
            .collect();
        assert_eq!(
            decode_milliseconds(&reversed, 2, 1.0, 0),
            Err(TimestampDecodeError::ReversedBoundary {
                pair: 0,
                beginning: 2,
                end: 1
            })
        );
    }

    #[test]
    fn empty_pass_mask_skips_only_drawless_pairs_and_preserves_later_measurements() {
        // One maximum-capacity 256-byte readback protects Metal's missing
        // stage sentinel and exact empty-pass exclusions, including pair 15.
        // It rejects any missing boundary in a pass that actually drew.
        let bytes: Vec<u8> = (0_u32..16_u32)
            .flat_map(|pair| {
                if pair == 15_u32 {
                    [1_000_000_u64, 2_000_000_u64]
                } else {
                    [u64::MAX; 2]
                }
            })
            .flat_map(u64::to_ne_bytes)
            .collect();
        assert!(
            matches!(decode_milliseconds(&bytes, 32, 1.0, 0x7fff), Ok(ms) if (ms - 1.0).abs() < f32::EPSILON)
        );
        assert!(matches!(
            decode_milliseconds(&bytes, 32, 1.0, 0x7ffe),
            Err(TimestampDecodeError::MissingBoundary {
                pair: 0,
                beginning: _,
                end: _
            })
        ));
        assert_eq!(
            decode_milliseconds(&bytes, 32, 1.0, 0xffff),
            Err(TimestampDecodeError::NoRenderedPasses)
        );
    }
}
