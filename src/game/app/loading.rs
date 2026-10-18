//! One bounded CPU worker that decodes compiled map packages; SDL and GPU
//! installation remain owned by the frame loop.
//!
//! The player performs no static preparation. A request names a package and a
//! lightmap quality; the worker resolves the level's texture *pixels* through
//! the installed content bundle and decodes the package's prepared records
//! (geometry, prop batches, baked lighting, lightmap atlas, collision, probe
//! captures) concurrently, then hands the frame loop a world it can install.
//! Geometry emission, the lighting bake, chart planning, atlas filling, probe
//! capture and static collision derivation all happen offline in
//! `places-compile`.
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use crate::engine::loader::{LevelEntry, LevelManager, LevelSourceType, LoadedLevel};
use crate::engine::package::collision::CompiledCollision;
use crate::engine::package::world::{ProbeCaptures, load_variant};
use crate::engine::props::PropAssets;
use crate::engine::quality::LightmapQuality;
use crate::engine::render::{CharacterScene, LevelBuild};
use crate::game::CollisionWorld;

/// Where one preparation stands, for the loading screen and the load trace.
///
/// `Reading` spans the level's own assets *and* its compiled variant: the two
/// come from different files and caches and are read concurrently, so the
/// variant's read/decode overlaps the material and texture resolution. The
/// `Geometry` boundary is published once both halves are done, before the
/// records are assembled into an installable world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Queued = 0,
    Reading = 1,
    Geometry = 2,
    Lightmaps = 3,
    Collision = 4,
    Characters = 5,
    Ready = 6,
}
impl Phase {
    const fn as_byte(self) -> u8 {
        match self {
            Self::Queued => 0,
            Self::Reading => 1,
            Self::Geometry => 2,
            Self::Lightmaps => 3,
            Self::Collision => 4,
            Self::Characters => 5,
            Self::Ready => 6,
        }
    }

    const fn from_byte(value: u8) -> Self {
        match value {
            1 => Self::Reading,
            2 => Self::Geometry,
            3 => Self::Lightmaps,
            4 => Self::Collision,
            5 => Self::Characters,
            6 => Self::Ready,
            _ => Self::Queued,
        }
    }
}

/// Content identity of one compiled package captured when its request value
/// was created.
///
/// This is the package's own compiled manifest digest (the same
/// `package_identity` the preparation worker keys its cache on): it covers the
/// declared dependencies and the compiler fingerprint, so a rebuilt file with
/// the same path, size and timestamp still gets a different identity. A value
/// that could not be captured is never coalesced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceIdentity {
    /// The package was inspected and hashed at request time.
    Known(String),
    /// The package could not be inspected; no outstanding preparation may be
    /// reused for it.
    Unknown,
}

impl SourceIdentity {
    fn capture(entry: &LevelEntry) -> Self {
        crate::engine::package::world::package_identity(entry).map_or(Self::Unknown, Self::Known)
    }
}

/// True when two captured identities are both known and equal.
fn identities_match(left: &SourceIdentity, right: &SourceIdentity) -> bool {
    match (left, right) {
        (SourceIdentity::Known(left_identity), SourceIdentity::Known(right_identity)) => {
            left_identity == right_identity
        }
        _ => false,
    }
}

/// What the frame loop asks the worker to prepare.
#[derive(Clone)]
pub enum Source {
    /// One discovered package, with the content identity captured when this
    /// request value was created.
    Entry(LevelEntry, SourceIdentity),
    /// A world whose definition and materials must be reused (graphics-only
    /// changes, e.g. a lightmap quality switch). The package is re-read from
    /// its recorded location.
    Retained(Box<LoadedLevel>, SourceIdentity),
}
impl Source {
    /// A discovered-package request that snapshots the package identity now.
    #[must_use]
    pub fn entry(entry: LevelEntry) -> Self {
        let identity = SourceIdentity::capture(&entry);
        Self::Entry(entry, identity)
    }

    /// A retained-world request that snapshots the package identity now.
    #[must_use]
    pub fn retained(loaded: LoadedLevel) -> Self {
        let identity = SourceIdentity::capture(&loaded.entry);
        Self::Retained(Box::new(loaded), identity)
    }

    pub fn level_id(&self) -> &str {
        match self {
            Self::Entry(entry, _) => &entry.id,
            Self::Retained(loaded, _) => &loaded.level.id,
        }
    }

    /// True when both requests name the same world content.
    ///
    /// The comparison uses the content identities captured independently when
    /// each value was created, never a fresh stat of the two paths: a request
    /// created after an in-place edit carries a different identity and must
    /// supersede the outstanding preparation instead of promoting it. A
    /// different path, changed entry metadata, or a request after completion
    /// does not establish the defect, and an identity that could not be
    /// captured never coalesces.
    ///
    /// This coalesces in-flight requests only; retained-build reuse is the
    /// worker cache's decision.
    #[must_use]
    pub fn same_preparation(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Entry(a, a_identity), Self::Entry(b, b_identity)) => {
                a == b && identities_match(a_identity, b_identity)
            }
            (Self::Retained(a, a_identity), Self::Retained(b, b_identity)) => {
                a.entry == b.entry && identities_match(a_identity, b_identity)
            }
            _ => false,
        }
    }
}

/// One decoded package variant plus the level it belongs to.
pub struct Request {
    pub source: Source,
    pub lightmaps: LightmapQuality,
}

/// Keeps all decoded records of a pending or displayed world reusable through
/// the worker's weak identity index, including worlds too large for its LRU.
#[derive(Clone)]
pub struct PreparedLease {
    records: Arc<PreparedRecords>,
}

impl PreparedLease {
    /// CPU bytes retained by the active prepared records, including lazy diagnostics.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.records.retained_bytes()
    }
}

/// One internally compatible decoded world. The caller installs this only while
/// its generation is current.
pub struct PreparedWorld {
    pub preparation_millis: f64,
    pub cache_hit: bool,
    pub loaded: LoadedLevel,
    pub build: Arc<LevelBuild>,
    pub collision: CollisionWorld,
    pub characters: CharacterScene,
    pub assets: PropAssets,
    pub lightmaps: LightmapQuality,
    pub probes: ProbeCaptures,
    pub lease: PreparedLease,
}

#[derive(Clone)]
struct Control {
    cancelled: Arc<AtomicBool>,
    phase: Arc<AtomicU8>,
}
impl Control {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            phase: Arc::new(AtomicU8::new(Phase::Queued.as_byte())),
        }
    }
    fn checkpoint(&self, phase: Phase) -> bool {
        if self.cancelled.load(Ordering::Relaxed) {
            return false;
        }
        self.phase.store(phase.as_byte(), Ordering::Relaxed);
        true
    }
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

struct Job<I> {
    id: u64,
    input: I,
    control: Control,
}
struct Finished<O> {
    id: u64,
    outcome: Result<Option<O>, String>,
}

enum Message<I, O> {
    Prepare(Job<I>),
    Retire(O),
}

/// Scheduling stays generic so cancellation tests never need a renderer, assets, or real bake.
struct Worker<I, O> {
    sender: Option<mpsc::SyncSender<Message<I, O>>>,
    receiver: Option<mpsc::Receiver<Finished<O>>>,
    handle: Option<JoinHandle<()>>,
    current: u64,
    pending: Option<Job<I>>,
    running: Option<(u64, Control)>,
}
impl<I: Send + 'static, O: Send + 'static> Worker<I, O> {
    fn spawn(
        mut work: impl FnMut(I, &Control) -> Result<Option<O>, String> + Send + 'static,
    ) -> Result<Self, String> {
        let (sender, requests) = mpsc::sync_channel::<Message<I, O>>(1);
        let (results, receiver) = mpsc::sync_channel(1);
        let handle = std::thread::Builder::new()
            .name("places-prepare".to_string())
            .spawn(move || {
                while let Ok(message) = requests.recv() {
                    let job = match message {
                        Message::Prepare(job) => job,
                        Message::Retire(value) => {
                            drop(value);
                            continue;
                        }
                    };
                    let outcome = if job.control.cancelled.load(Ordering::Relaxed) {
                        Ok(None)
                    } else {
                        work(job.input, &job.control)
                    };
                    // At most one request is dispatched until its result is received.
                    // Closing the result receiver releases this send during shutdown.
                    if results
                        .send(Finished {
                            id: job.id,
                            outcome,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| format!("Could not start level preparation: {error}"))?;
        Ok(Self {
            sender: Some(sender),
            receiver: Some(receiver),
            handle: Some(handle),
            current: 0,
            pending: None,
            running: None,
        })
    }

    fn request(&mut self, input: I) -> Result<u64, String> {
        if self.sender.is_none() {
            return Err("Level preparation is shutting down".to_string());
        }
        let next = self
            .current
            .checked_add(1)
            .ok_or_else(|| "Level request counter exhausted".to_string())?;
        self.cancel();
        self.current = next;
        self.pending = Some(Job {
            id: next,
            input,
            control: Control::new(),
        });
        self.dispatch()?;
        Ok(next)
    }

    fn cancel(&mut self) {
        self.pending = None;
        if let Some((_, control)) = &self.running {
            control.cancel();
        }
    }

    fn dispatch(&mut self) -> Result<(), String> {
        if self.running.is_some() {
            return Ok(());
        }
        let Some(job) = self.pending.take() else {
            return Ok(());
        };
        let running = (job.id, job.control.clone());
        let Some(sender) = self.sender.as_ref() else {
            return Ok(());
        };
        match sender.try_send(Message::Prepare(job)) {
            Ok(()) => self.running = Some(running),
            Err(mpsc::TrySendError::Full(Message::Prepare(pending_job))) => {
                self.pending = Some(pending_job);
            }
            Err(mpsc::TrySendError::Full(Message::Retire(_))) => {
                return Err("Unexpected retirement message during dispatch".to_string());
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err("Level preparation worker stopped".to_string());
            }
        }
        Ok(())
    }

    fn poll(&mut self) -> Option<(u64, Result<O, String>)> {
        let received = self.receiver.as_ref()?.try_recv();
        match received {
            Ok(done) => {
                let accepted = self.running.as_ref().is_some_and(|(id, control)| {
                    *id == done.id
                        && done.id == self.current
                        && !control.cancelled.load(Ordering::Relaxed)
                });
                self.running = None;
                if !accepted {
                    if let Ok(Some(value)) = done.outcome
                        && let Some(sender) = &self.sender
                    {
                        // No next preparation is dispatched before this result is
                        // consumed, so the bounded request slot is available.
                        if let Err(error) = sender.try_send(Message::Retire(value)) {
                            // Retiring a cancelled result is best effort during
                            // shutdown; a failed send owns and drops the value.
                            crate::engine::logging::warn(format!(
                                "[loading] could not retire cancelled preparation: {error}"
                            ));
                        }
                    }
                    if let Err(error) = self.dispatch() {
                        return Some((self.current, Err(error)));
                    }
                    return None;
                }
                if let Err(error) = self.dispatch() {
                    return Some((self.current, Err(error)));
                }
                match done.outcome {
                    Ok(Some(value)) => Some((done.id, Ok(value))),
                    Ok(None) => None,
                    Err(error) => Some((done.id, Err(error))),
                }
            }
            Err(mpsc::TryRecvError::Empty) => self
                .dispatch()
                .err()
                .map(|error| (self.current, Err(error))),
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                let (id, control) = self.running.take()?;
                (!control.cancelled.load(Ordering::Relaxed)).then(|| {
                    (
                        id,
                        Err("Level preparation worker stopped before completing".to_string()),
                    )
                })
            }
        }
    }

    fn phase(&self) -> Option<Phase> {
        if self.pending.is_some() {
            return Some(Phase::Queued);
        }
        self.running.as_ref().and_then(|(_, control)| {
            (!control.cancelled.load(Ordering::Relaxed))
                .then(|| Phase::from_byte(control.phase.load(Ordering::Relaxed)))
        })
    }
}
impl<I, O> Worker<I, O> {
    fn shutdown(&mut self) {
        self.pending = None;
        if let Some((_, control)) = &self.running {
            control.cancel();
        }
        self.sender = None;
        self.receiver = None;
    }
    fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }
    fn join_finished(&mut self) -> Result<bool, String> {
        if !self.is_finished() {
            return Ok(false);
        }
        if let Some(handle) = self.handle.take() {
            handle.join().map_err(|payload| {
                format!(
                    "Level preparation worker panicked: {}",
                    crate::engine::logging::panic_message(payload.as_ref())
                )
            })?;
        }
        Ok(true)
    }
}
impl<I, O> Drop for Worker<I, O> {
    fn drop(&mut self) {
        // Normal shutdown keeps pumping until is_finished and calls join_finished.
        // Unwinding still closes channels and cancels, never blocks the UI in Drop.
        self.shutdown();
        if let Err(error) = self.join_finished() {
            crate::engine::logging::warn(format!("[loading] shutdown: {error}"));
        }
    }
}

/// Identity of one decoded package variant: the canonical manifest SHA-256 and
/// lightmap quality. Archive recompression does not change prepared content.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PackageKey {
    package_sha256: String,
    quality: LightmapQuality,
}

impl PackageKey {
    fn for_entry(entry: &LevelEntry, quality: LightmapQuality) -> Result<Self, String> {
        Ok(Self {
            package_sha256: crate::engine::package::world::package_identity(entry)?,
            quality,
        })
    }

    fn check_manifest(&self, manifest: &crate::engine::package::Manifest) -> Result<(), String> {
        if crate::engine::package::world::manifest_identity(manifest)? != self.package_sha256 {
            return Err("Map package changed during loading; select the map again".to_string());
        }
        Ok(())
    }

    fn check_loaded(&self, loaded: &LoadedLevel) -> Result<(), String> {
        if loaded.package_identity.as_deref() != Some(self.package_sha256.as_str()) {
            return Err(
                "Map package changed since its level was read; reload the map before changing lighting"
                    .to_string(),
            );
        }
        Ok(())
    }
}

/// One decoded package variant retained by the worker cache.
struct PreparedRecords {
    build: Arc<LevelBuild>,
    collision: CompiledCollision,
    /// The decoded baked navigation grid; the runtime mesh is built from it
    /// once per install.
    navigation: crate::engine::package::navigation::NavGrid,
    probes: ProbeCaptures,
}

impl PreparedRecords {
    fn retained_bytes(&self) -> usize {
        let probe_bytes = [&self.probes.medium, &self.probes.full]
            .into_iter()
            .flatten()
            .map(|capture| {
                let bytes = allocation_bytes(&capture.points)
                    .saturating_add(allocation_bytes(&capture.chains));
                capture.chains.iter().fold(bytes, |sum, chain| {
                    chain.iter().fold(
                        sum.saturating_add(allocation_bytes(chain)),
                        |chain_bytes, faces| {
                            faces.iter().fold(chain_bytes, |face_bytes, face| {
                                face_bytes.saturating_add(allocation_bytes(face))
                            })
                        },
                    )
                })
            })
            .fold(0_usize, usize::saturating_add);
        std::mem::size_of::<Self>()
            .saturating_add(std::mem::size_of::<usize>().saturating_mul(2))
            .saturating_add(self.build.retained_bytes())
            .saturating_add(self.collision.retained_heap_bytes())
            .saturating_add(self.navigation.retained_heap_bytes())
            .saturating_add(probe_bytes)
    }
}

const fn allocation_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity().saturating_mul(std::mem::size_of::<T>())
}

/// Small worker-owned LRU. Eviction releases only the cache's Arc; a displayed
/// world remains valid. Oversized worlds are prepared normally but not retained.
#[derive(Default)]
struct BuildCache {
    entries: std::collections::VecDeque<(PackageKey, Arc<PreparedRecords>, usize)>,
    limits: Option<(usize, usize)>,
}
impl BuildCache {
    fn refresh_charges(&mut self) {
        for (key, records, bytes) in &mut self.entries {
            *bytes = records
                .retained_bytes()
                .saturating_add(key.package_sha256.capacity());
        }
    }

    fn enforce_limits(&mut self) {
        self.refresh_charges();
        if let Some((max_entries, max_bytes)) = self.limits {
            while self.entries.len() > max_entries
                || self
                    .entries
                    .iter()
                    .fold(0_usize, |total, (_, _, bytes)| total.saturating_add(*bytes))
                    > max_bytes
            {
                drop(self.entries.pop_front());
            }
        }
    }

    fn get(&mut self, key: &PackageKey) -> Option<Arc<PreparedRecords>> {
        self.enforce_limits();
        let position = self
            .entries
            .iter()
            .position(|(stored, _, _)| stored == key)?;
        let entry = self.entries.remove(position)?;
        let records = Arc::clone(&entry.1);
        self.entries.push_back(entry);
        Some(records)
    }

    fn insert(&mut self, key: PackageKey, records: Arc<PreparedRecords>) {
        self.insert_with_limits(key, records, 3, 192 * 1024 * 1024);
    }

    fn insert_with_limits(
        &mut self,
        key: PackageKey,
        records: Arc<PreparedRecords>,
        max_entries: usize,
        max_bytes: usize,
    ) {
        self.limits = Some((max_entries, max_bytes));
        self.enforce_limits();
        let bytes = records
            .retained_bytes()
            .saturating_add(key.package_sha256.capacity());
        if max_entries == 0 || bytes > max_bytes {
            return;
        }
        while self.entries.len() >= max_entries
            || self
                .entries
                .iter()
                .fold(bytes, |total, (_, _, size)| total.saturating_add(*size))
                > max_bytes
        {
            drop(self.entries.pop_front());
        }
        self.entries.push_back((key, records, bytes));
    }
}

fn test_delay() -> Result<u64, String> {
    let delay = if std::env::var("PLACES_BENCH").as_deref() == Ok("1") {
        std::env::var("PLACES_PREPARE_DELAY_MS")
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or(0)
            .min(60_000)
    } else {
        0
    };
    Ok(delay)
}

pub struct Loader {
    worker: Worker<Request, PreparedWorld>,
}

fn remember_build(
    identities: &mut std::collections::VecDeque<(PackageKey, std::sync::Weak<PreparedRecords>)>,
    key: PackageKey,
    records: &Arc<PreparedRecords>,
) {
    // Weak identities keep an oversized displayed world reusable after a
    // different preparation is cancelled, without retaining its geometry.
    identities.retain(|(previous, cached_records)| {
        previous != &key && cached_records.strong_count() != 0
    });
    while identities.len() >= 8 {
        drop(identities.pop_front());
    }
    identities.push_back((key, Arc::downgrade(records)));
}

/// The prepared records a previous preparation of `key` can satisfy, if any.
///
/// Weak identities keep an oversized displayed world reusable after a
/// different preparation is cancelled, without retaining its geometry.
fn reusable_records(
    key: &PackageKey,
    builds: &mut BuildCache,
    identities: &mut std::collections::VecDeque<(PackageKey, std::sync::Weak<PreparedRecords>)>,
) -> Option<Arc<PreparedRecords>> {
    identities.retain(|(_, records)| records.strong_count() != 0);
    builds.get(key).or_else(|| {
        identities
            .iter()
            .find_map(|(previous, records)| (previous == key).then(|| records.upgrade()).flatten())
    })
}

/// Propagates a worker-panic join failure as an ordinary preparation error.
fn joined_variant(
    joined: std::thread::Result<Result<crate::engine::package::world::LoadedVariant, String>>,
) -> Result<crate::engine::package::world::LoadedVariant, String> {
    joined.map_err(|payload| {
        format!(
            "level preparation worker panicked: {}",
            crate::engine::logging::panic_message(payload.as_ref())
        )
    })?
}

/// Decodes one request into an installable world, reusing the worker cache.
#[expect(
    clippy::too_many_lines,
    reason = "one cohesive decode-to-world pipeline"
)] // one cohesive decode-to-world pipeline
fn prepare_world(
    manager: &mut LevelManager,
    request: Request,
    control: &Control,
    assets: &mut PropAssets,
    builds: &mut BuildCache,
    identities: &mut std::collections::VecDeque<(PackageKey, std::sync::Weak<PreparedRecords>)>,
    delay: u64,
) -> Result<Option<PreparedWorld>, String> {
    let started = std::time::Instant::now();
    if !control.checkpoint(Phase::Reading) {
        return Ok(None);
    }
    let delay_started = std::time::Instant::now();
    while delay_started.elapsed() < std::time::Duration::from_millis(delay) {
        if control.cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // An installable entry's variant cache key is known before the read, so a
    // repeated load can skip the compiled-variant read entirely.
    let mut planned: Option<(PackageKey, Option<Arc<PreparedRecords>>)> = None;
    // The variant records, already read while the level's own assets resolved.
    let mut prefetched_variant: Option<crate::engine::package::world::LoadedVariant> = None;
    let loaded = match request.source {
        Source::Entry(entry, _) => {
            manager.refresh_catalog();
            let key = PackageKey::for_entry(&entry, request.lightmaps)?;
            let reusable = reusable_records(&key, builds, identities);
            let loaded = if reusable.is_some() {
                manager.load_level(&entry)?
            } else {
                // The level's texture pixels and the compiled variant come from
                // different files and caches, so read them concurrently: a load
                // pays the longer half instead of their sum. Phase::Reading
                // spans both halves; the Geometry boundary follows the join.
                let (loaded, variant) = std::thread::scope(|scope| {
                    let reader = scope.spawn(|| load_entry_variant(&entry, &key, assets));
                    let loaded = manager.load_level(&entry);
                    (loaded, reader.join())
                });
                // A failed level keeps its own error; the variant read reports
                // only when the level itself was readable.
                let loaded_level = loaded?;
                prefetched_variant = Some(joined_variant(variant)?);
                loaded_level
            };
            planned = Some((key, reusable));
            loaded
        }
        Source::Retained(loaded, _) => *loaded,
    };
    if loaded.level.requires_full_lighting && request.lightmaps != LightmapQuality::Full {
        return Err(format!(
            "{} requires Full lighting for its authored circuits. Choose Lightmaps: Full and open the map again.",
            loaded.level.name
        ));
    }
    // The injected delay runs on both sides of the read so a test script can
    // coordinate an in-place package edit *after* the worker has read the
    // original bytes; that is the in-flight content-change race the request
    // identity guards. The delay is inert unless PLACES_BENCH=1. The Geometry
    // checkpoint is published between the two halves, so the trace's phase
    // mark is what proves the bytes were consumed.
    if !control.checkpoint(Phase::Geometry) {
        return Ok(None);
    }
    let geometry_delay_started = std::time::Instant::now();
    while geometry_delay_started.elapsed() < std::time::Duration::from_millis(delay) {
        if control.cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let (key, reusable) = if let Some(planned_build) = planned {
        planned_build
    } else {
        let key = PackageKey::for_entry(&loaded.entry, request.lightmaps)?;
        let reusable = reusable_records(&key, builds, identities);
        (key, reusable)
    };
    // Semantics, prepared records and the cache key must describe the same
    // manifest, including retained worlds used for graphics-only changes.
    key.check_loaded(&loaded)?;
    crate::engine::logging::info(format_args!(
        "[loading] package identity level={} variant={} sha256={} source={}",
        loaded.level.id,
        request.lightmaps.name(),
        key.package_sha256,
        loaded.entry.path.display()
    ));
    // Variant decoding synchronizes model ownership on cache misses. Hits also
    // release unrelated models and retain only this request's dependency hashes.
    if reusable.is_some() {
        let requested_manifest = crate::engine::package::world::inspect(&loaded.entry.path)?;
        key.check_manifest(&requested_manifest)?;
        assets.synchronize_dependencies(&requested_manifest.dependencies);
    }
    let cache_hit = reusable.is_some();
    let records = if let Some(records) = reusable {
        crate::engine::logging::info(format_args!(
            "[loading] compiled-cache hit level={} variant={}",
            loaded.level.id,
            request.lightmaps.name()
        ));
        records
    } else {
        crate::engine::logging::info(format_args!(
            "[loading] compiled-cache miss level={} variant={}",
            loaded.level.id,
            request.lightmaps.name()
        ));
        let variant = match prefetched_variant {
            Some(variant) => variant,
            None => load_entry_variant(&loaded.entry, &key, assets)?,
        };
        if !request.lightmaps.is_off() {
            crate::engine::package::world::validate_probe_captures(
                &variant.mesh,
                &loaded.materials,
                &variant.probes,
            )?;
        }
        let build = Arc::new(LevelBuild {
            static_light_off: variant.static_light_off,
            mesh: variant.mesh,
            batches: variant.props,
            lighting: variant.lighting,
            timings: crate::engine::render::BuildTimings::default(),
            lightmaps: variant.lightmaps,
            probes: variant.irradiance,
            lightmap_failure: None,
            lightmap_millis: 0.0,
        });
        let records = Arc::new(PreparedRecords {
            build,
            collision: variant.collision,
            navigation: variant.navigation,
            probes: variant.probes,
        });
        if control.cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        builds.insert(key.clone(), Arc::clone(&records));
        records
    };
    remember_build(identities, key, &records);
    if !control.checkpoint(Phase::Collision) {
        return Ok(None);
    }
    // A malformed navigation record fails the load: there is no runtime
    // bake or repair path, and a world without navigation would silently
    // disable every AI behavior.
    let navigation = crate::engine::nav::NavMesh::from_record(records.navigation.clone())
        .map_err(|error| format!("navigation record: {error}"))?;
    let collision =
        CollisionWorld::from_compiled(&loaded.level, records.collision.clone(), Some(navigation))
            .with_ground_materials(&loaded.level, &loaded.materials);
    if !control.checkpoint(Phase::Characters) {
        return Ok(None);
    }
    let characters = CharacterScene::spawn_characters_with_field(
        &loaded.level,
        loaded.catalog.as_ref(),
        assets,
        &records.build.lighting,
        records.build.probes.as_deref(),
    );
    if !control.checkpoint(Phase::Ready) {
        return Ok(None);
    }
    Ok(Some(PreparedWorld {
        preparation_millis: started.elapsed().as_secs_f64() * 1000.0,
        cache_hit,
        loaded,
        build: Arc::clone(&records.build),
        collision,
        characters,
        assets: assets.clone(),
        lightmaps: request.lightmaps,
        probes: records.probes.clone(),
        lease: PreparedLease { records },
    }))
}

impl Loader {
    /// Transfers the existing manager (including decoded images) to one persistent worker.
    pub fn new(mut manager: LevelManager) -> Result<Self, String> {
        let delay = test_delay()?;
        let mut assets = PropAssets::load_default();
        let mut builds = BuildCache::default();
        // The renderer owns the active world even when it is too large for the
        // LRU. A weak reference permits texture-only refits without retaining
        // another oversized world after it is no longer displayed.
        let mut identities: std::collections::VecDeque<(
            PackageKey,
            std::sync::Weak<PreparedRecords>,
        )> = std::collections::VecDeque::new();
        let worker = Worker::spawn(move |request: Request, control| {
            prepare_world(
                &mut manager,
                request,
                control,
                &mut assets,
                &mut builds,
                &mut identities,
                delay,
            )
        })?;
        Ok(Self { worker })
    }
    pub fn request(&mut self, request: Request) -> Result<u64, String> {
        self.worker.request(request)
    }
    pub fn cancel(&mut self) {
        self.worker.cancel();
    }
    pub fn poll(&mut self) -> Option<(u64, Result<PreparedWorld, String>)> {
        self.worker.poll()
    }
    pub fn phase(&self) -> Option<Phase> {
        self.worker.phase()
    }
    pub fn shutdown(&mut self) {
        self.worker.shutdown();
    }
    pub fn is_finished(&self) -> bool {
        self.worker.is_finished()
    }
    pub fn join_finished(&mut self) -> Result<bool, String> {
        self.worker.join_finished()
    }
}

/// Decodes one package variant from its discovered disk path.
///
/// A package that does not declare the requested variant cannot serve it at
/// any quality, so the error is the actionable one: it names the package, the
/// variant it lacks and the variants it has, so the player can pick a
/// supported Lightmaps setting instead of only seeing the load fail.
fn load_entry_variant(
    entry: &LevelEntry,
    key: &PackageKey,
    assets: &mut PropAssets,
) -> Result<crate::engine::package::world::LoadedVariant, String> {
    match entry.source_type {
        LevelSourceType::Bundled | LevelSourceType::Installed => {
            let opened = crate::engine::package::world::open_level(&entry.path, &entry.id)?;
            key.check_manifest(&opened.manifest)?;
            ensure_variant_supported(entry, &opened.manifest, key.quality)?;
            load_variant(&entry.path, &opened.manifest, key.quality, assets)
        }
    }
}

/// The actionable error for a package that cannot serve `quality`.
///
/// `Ok(())` when the variant exists. The message names the package, the
/// missing variant and every variant the package does provide, including the
/// compiler command that would add it, because an unsupported selection is a
/// content fact the player has to be told about — never a reason to silently
/// rewrite the selection.
fn ensure_variant_supported(
    entry: &LevelEntry,
    manifest: &crate::engine::package::manifest::Manifest,
    quality: LightmapQuality,
) -> Result<(), String> {
    if manifest.variant(quality.name()).is_some() {
        return Ok(());
    }
    let available: Vec<&str> = manifest
        .variants
        .iter()
        .map(|variant| variant.lightmap_quality.as_str())
        .collect();
    let available_variants = if available.is_empty() {
        "none".to_string()
    } else {
        available.join(", ")
    };
    Err(format!(
        "package '{}' ({}) has no '{}' lightmap variant; it provides {}. \
         Choose a Lighting setting it provides, or rebuild it with \
         'places-compile build <source> --variants off,full'",
        manifest.id,
        entry.path.display(),
        quality.name(),
        available_variants
    ))
}
