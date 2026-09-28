//! One bounded CPU worker that decodes compiled map packages; SDL and GPU
//! installation remain owned by the frame loop.
//!
//! The player performs no static preparation. A request names a package and a
//! lightmap quality; the worker resolves the level's texture *pixels* through
//! the installed content bundle, decodes the package's prepared records
//! (geometry, prop batches, baked lighting, lightmap atlas, collision, probe
//! captures) and hands the frame loop a world it can install. Geometry
//! emission, the lighting bake, chart planning, atlas filling, probe capture
//! and static collision derivation all happen offline in `places-compile`.
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use crate::game::CollisionWorld;
use crate::loader::{LevelEntry, LevelManager, LevelSourceType, LoadedLevel};
use crate::package::collision::CompiledCollision;
use crate::package::world::{ProbeCaptures, load_variant, load_variant_bytes};
use crate::props::PropAssets;
use crate::quality::LightmapQuality;
use crate::render::{CharacterScene, LevelBuild};

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
        crate::package::world::package_identity(entry).map_or(Self::Unknown, Self::Known)
    }
}

/// True when two captured identities are both known and equal.
fn identities_match(left: &SourceIdentity, right: &SourceIdentity) -> bool {
    match (left, right) {
        (SourceIdentity::Known(left), SourceIdentity::Known(right)) => left == right,
        _ => false,
    }
}

/// What the frame loop asks the worker to prepare.
#[derive(Clone)]
pub enum Source {
    /// The default level: the bundled demo package (or the embedded copy).
    Default,
    /// One discovered package, with the content identity captured when this
    /// request value was created.
    Entry(LevelEntry, SourceIdentity),
    /// A world whose definition and materials must be reused (graphics-only
    /// changes, e.g. a lightmap quality switch). The package is re-read from
    /// its recorded location; the embedded fallback reads its embedded bytes.
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
            Self::Default => crate::loader::DEMO_LEVEL_ID,
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
            (Self::Default, Self::Default) => true,
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
            phase: Arc::new(AtomicU8::new(Phase::Queued as u8)),
        }
    }
    fn checkpoint(&self, phase: Phase) -> bool {
        if self.cancelled.load(Ordering::Relaxed) {
            return false;
        }
        self.phase.store(phase as u8, Ordering::Relaxed);
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
            Err(mpsc::TrySendError::Full(Message::Prepare(job))) => self.pending = Some(job),
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
                        let _ = sender.try_send(Message::Retire(value));
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
            handle
                .join()
                .map_err(|_| "Level preparation worker panicked".to_string())?;
        }
        Ok(true)
    }
}
impl<I, O> Drop for Worker<I, O> {
    fn drop(&mut self) {
        // Normal shutdown keeps pumping until is_finished and calls join_finished.
        // Unwinding still closes channels and cancels, never blocks the UI in Drop.
        self.shutdown();
        let _ = self.join_finished();
    }
}

/// Identity of one decoded package variant: the package file's SHA-256 and the
/// lightmap quality. Two packages with identical bytes decode identically.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PackageKey {
    package_sha256: String,
    quality: LightmapQuality,
}

impl PackageKey {
    fn for_entry(entry: &LevelEntry, quality: LightmapQuality) -> Result<Self, String> {
        Ok(Self {
            package_sha256: crate::package::world::package_identity(entry)?,
            quality,
        })
    }
}

/// One decoded package variant retained by the worker cache.
struct PreparedRecords {
    build: Arc<LevelBuild>,
    collision: CompiledCollision,
    probes: ProbeCaptures,
    retained_bytes: usize,
}

impl PreparedRecords {
    fn retained_bytes(
        build: &LevelBuild,
        collision: &CompiledCollision,
        probes: &ProbeCaptures,
    ) -> usize {
        let collision_bytes = collision
            .walls
            .len()
            .saturating_mul(std::mem::size_of::<crate::collision::WallAabb>());
        let probe_bytes = [&probes.medium, &probes.full]
            .into_iter()
            .flatten()
            .map(|capture| {
                capture.chains.iter().fold(0_usize, |sum, chain| {
                    chain.iter().fold(sum, |sum, faces| {
                        faces
                            .iter()
                            .fold(sum, |sum, face| sum.saturating_add(face.len()))
                    })
                })
            })
            .fold(0_usize, usize::saturating_add);
        build
            .retained_bytes()
            .saturating_add(collision_bytes)
            .saturating_add(probe_bytes)
    }
}

/// Small worker-owned LRU. Eviction releases only the cache's Arc; a displayed
/// world remains valid. Oversized worlds are prepared normally but not retained.
#[derive(Default)]
struct BuildCache {
    entries: std::collections::VecDeque<(PackageKey, Arc<PreparedRecords>, usize)>,
}
impl BuildCache {
    fn get(&mut self, key: &PackageKey) -> Option<Arc<PreparedRecords>> {
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
        let bytes = records
            .retained_bytes
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
            self.entries.pop_front();
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
    identities.retain(|(previous, records)| previous != &key && records.strong_count() != 0);
    while identities.len() >= 8 {
        identities.pop_front();
    }
    identities.push_back((key, Arc::downgrade(records)));
}

/// Decodes one request into an installable world, reusing the worker cache.
#[allow(clippy::too_many_lines)] // one cohesive decode-to-world pipeline
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
    let loaded = match request.source {
        Source::Default => {
            manager.refresh_catalog();
            manager.load_default()?
        }
        Source::Entry(entry, _) => {
            manager.refresh_catalog();
            manager.load_level(&entry)?
        }
        Source::Retained(loaded, _) => *loaded,
    };
    // The injected delay runs on both sides of the read so a test script can
    // coordinate an in-place package edit *after* the worker has read the
    // original bytes; that is the in-flight content-change race the request
    // identity guards. The delay is inert unless PLACES_BENCH=1. The Geometry
    // checkpoint is published between the two halves, so the trace's phase
    // mark is what proves the bytes were consumed.
    if !control.checkpoint(Phase::Geometry) {
        return Ok(None);
    }
    let delay_started = std::time::Instant::now();
    while delay_started.elapsed() < std::time::Duration::from_millis(delay) {
        if control.cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let key = PackageKey::for_entry(&loaded.entry, request.lightmaps)?;
    identities.retain(|(_, records)| records.strong_count() != 0);
    let reusable = builds.get(&key).or_else(|| {
        identities
            .iter()
            .find_map(|(previous, records)| (previous == &key).then(|| records.upgrade()).flatten())
    });
    let cache_hit = reusable.is_some();
    let records = if let Some(records) = reusable {
        crate::logging::info(format_args!(
            "[loading] compiled-cache hit level={} variant={}",
            loaded.level.id,
            request.lightmaps.name()
        ));
        records
    } else {
        crate::logging::info(format_args!(
            "[loading] compiled-cache miss level={} variant={}",
            loaded.level.id,
            request.lightmaps.name()
        ));
        let variant = load_entry_variant(&loaded.entry, request.lightmaps, assets)?;
        crate::package::world::validate_probe_captures(
            &variant.mesh,
            &loaded.materials,
            &variant.probes,
        )?;
        let build = Arc::new(LevelBuild {
            mesh: variant.mesh,
            batches: variant.props,
            lighting: variant.lighting,
            timings: crate::render::BuildTimings::default(),
            lightmaps: variant.lightmaps,
            probes: variant.irradiance,
            lightmap_failure: None,
            lightmap_millis: 0.0,
        });
        let retained_bytes =
            PreparedRecords::retained_bytes(&build, &variant.collision, &variant.probes);
        let records = Arc::new(PreparedRecords {
            build,
            collision: variant.collision,
            probes: variant.probes,
            retained_bytes,
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
    let collision = CollisionWorld::from_compiled(&loaded.level, records.collision.clone());
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

/// Decodes one package variant, resolving the embedded fallback's bytes when
/// the entry is the embedded demo.
fn load_entry_variant(
    entry: &LevelEntry,
    quality: LightmapQuality,
    assets: &mut PropAssets,
) -> Result<crate::package::world::LoadedVariant, String> {
    match entry.source_type {
        LevelSourceType::Embedded => {
            let opened = crate::package::world::open_bytes(crate::loader::embedded_demo_package())?;
            load_variant_bytes(
                crate::loader::embedded_demo_package(),
                &opened.manifest,
                quality,
                assets,
            )
        }
        LevelSourceType::Bundled | LevelSourceType::Installed => {
            let opened = crate::package::world::open(&entry.path)?;
            load_variant(&entry.path, &opened.manifest, quality, assets)
        }
    }
}

#[cfg(test)]
mod tests {
    // Tests use expect to report a broken synchronization contract.
    #![allow(clippy::expect_used)]
    use super::*;

    /// Exact content identity prevents a replaced package with the same id from
    /// reusing stale records, for both the active records and the LRU cache.
    #[test]
    fn a_package_key_changes_with_the_package_bytes() {
        let key = |hash: &str| PackageKey {
            package_sha256: hash.to_string(),
            quality: LightmapQuality::Full,
        };
        let original = key("aaaa");
        assert!(original == key("aaaa"), "identical bytes reuse the build");
        assert!(original != key("bbbb"), "different bytes cannot match");
        assert!(
            BuildCache::default().get(&original).is_none(),
            "no retained build never matches"
        );
    }

    /// Coalescing reuses an outstanding request only for identical captured
    /// content.
    #[test]
    fn same_preparation_compares_captured_content_not_only_the_level_name() {
        let entry = |id: &str, path: &str| LevelEntry {
            id: id.to_string(),
            name: "Same Name".to_string(),
            author: "Author".to_string(),
            source_type: LevelSourceType::Installed,
            path: std::path::PathBuf::from(path),
        };
        let known = |value: &str| SourceIdentity::Known(value.to_string());
        let source = Source::Entry(
            entry("same_name", "/levels/a.placesmap"),
            known("content-1"),
        );
        assert!(
            source.same_preparation(&Source::Entry(
                entry("same_name", "/levels/a.placesmap"),
                known("content-1"),
            )),
            "an identical entry and content reuse their preparation"
        );
        assert!(
            !source.same_preparation(&Source::Entry(
                entry("same_name", "/levels/a.placesmap"),
                known("content-2"),
            )),
            "the same path with changed content must supersede, not coalesce"
        );
        assert!(
            !source.same_preparation(&Source::Entry(
                entry("same_name", "/levels/b.placesmap"),
                known("content-1"),
            )),
            "a different file with the same level id is different content"
        );
        assert!(
            !source.same_preparation(&Source::Entry(
                entry("other_level", "/levels/a.placesmap"),
                known("content-1"),
            )),
            "a different level id is different content"
        );
        assert!(
            !Source::Entry(
                entry("same_name", "/levels/a.placesmap"),
                SourceIdentity::Unknown
            )
            .same_preparation(&Source::Entry(
                entry("same_name", "/levels/a.placesmap"),
                SourceIdentity::Unknown,
            )),
            "an uninspectable package is never reused"
        );
        assert!(!source.same_preparation(&Source::Default));
        assert!(Source::Default.same_preparation(&Source::Default));
    }

    /// One structurally valid package whose manifest differs only by
    /// `compiler_fingerprint`; used to prove identity capture reads content.
    /// Entries are written uncompressed so two equal-length manifests also
    /// produce two equal-length files.
    fn write_identity_package(path: &std::path::Path, fingerprint: &str) {
        use crate::package::manifest::{Manifest, Variant, VariantEntries};
        use std::io::Write as _;
        let entry = |name: &str, role: &str, hash: char| crate::package::PackageEntry {
            name: name.to_string(),
            role: role.to_string(),
            bytes: 1,
            sha256: hash.to_string().repeat(64),
        };
        let entries = vec![
            entry("blobs/aa.mesh", "mesh", 'a'),
            entry("blobs/bb.props", "props", 'b'),
            entry("blobs/cc.lighting", "lighting", 'c'),
            entry("blobs/dd.collision", "collision", 'd'),
            entry("semantics.json", "semantics", 'e'),
        ];
        let manifest = Manifest {
            package_format: crate::package::FORMAT_VERSION,
            id: "identity_fixture".to_string(),
            name: "Identity Fixture".to_string(),
            author: String::new(),
            created_by: "identity-test".to_string(),
            compiler_fingerprint: fingerprint.to_string(),
            required_capabilities: vec![
                "geometry".to_string(),
                "props".to_string(),
                "lighting".to_string(),
                "collision".to_string(),
            ],
            dependencies: Vec::new(),
            entries: entries.clone(),
            variants: vec![Variant {
                lightmap_quality: "off".to_string(),
                quality_profile: "low".to_string(),
                lightmap_failure: None,
                entries: VariantEntries {
                    mesh: "blobs/aa.mesh".to_string(),
                    props: "blobs/bb.props".to_string(),
                    lighting: "blobs/cc.lighting".to_string(),
                    collision: "blobs/dd.collision".to_string(),
                    lightmaps: None,
                    lightmaps_meta: None,
                    irradiance: None,
                    probes: Vec::new(),
                },
            }],
        };
        let mut archive: Vec<(String, Vec<u8>)> = entries
            .iter()
            .map(|entry| (entry.name.clone(), vec![0_u8]))
            .collect();
        archive.push((
            "manifest.json".to_string(),
            manifest.to_json().expect("manifest serializes"),
        ));
        let file = std::fs::File::create(path).expect("create test package");
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        for (name, bytes) in archive {
            writer.start_file(name, options).expect("start entry");
            writer.write_all(&bytes).expect("write entry");
        }
        writer.finish().expect("finish test package");
    }

    /// AUD-005: an in-place edit between the worker read and a re-request is a
    /// different preparation. The two fixture files have the same path, the
    /// same size and the same preserved timestamp; only their manifest content
    /// differs, which is exactly what a metadata comparison misses.
    #[test]
    fn source_identity_changes_when_a_package_is_replaced_in_place() {
        let dir = std::env::temp_dir().join(format!(
            "places-source-identity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("scratch directory");
        let path = dir.join("identity_fixture.placesmap");
        write_identity_package(&path, "fingerprint-one");
        let entry = LevelEntry {
            id: "identity_fixture".to_string(),
            name: "Identity Fixture".to_string(),
            author: String::new(),
            source_type: LevelSourceType::Installed,
            path: path.clone(),
        };
        let before = Source::entry(entry.clone());
        assert!(
            before.same_preparation(&Source::entry(entry.clone())),
            "the unchanged package coalesces"
        );
        let metadata = std::fs::metadata(&path).expect("metadata");
        let size_before = metadata.len();
        let modified = metadata.modified().expect("mtime");

        write_identity_package(&path, "fingerprint-two");
        // Preserve the timestamp: content is then the only difference a
        // metadata-based reuse check could not see.
        std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("open for mtime restore")
            .set_modified(modified)
            .expect("restore mtime");
        let after_metadata = std::fs::metadata(&path).expect("metadata");
        assert_eq!(
            size_before,
            after_metadata.len(),
            "the negative control must not be detected by size alone"
        );

        let after = Source::entry(entry);
        assert!(
            !before.same_preparation(&after),
            "changed package content must supersede the outstanding preparation"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    // The gate deliberately ignores cancellation to emulate a completion racing a newer request.
    #[test]
    fn superseded_completion_is_discarded_and_only_latest_pending_runs() {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let (started, starts) = mpsc::sync_channel(1);
        let (release, releases) = mpsc::sync_channel(1);
        let mut worker = Worker::spawn(move |value: u32, _: &Control| {
            let _ = started.send(value);
            releases.recv().map_err(|error| error.to_string())?;
            Ok(Some(value))
        })
        .expect("worker starts");
        worker.request(1).expect("first request");
        assert_eq!(
            starts
                .recv_timeout(std::time::Duration::from_secs(30))
                .expect("started"),
            1
        );
        worker.request(2).expect("supersede");
        let newest = worker.request(3).expect("replace pending");
        release.send(()).expect("release first");
        // Readiness synchronization, not a wall-clock performance assertion.
        loop {
            assert!(worker.poll().is_none());
            if let Ok(value) = starts.try_recv() {
                assert_eq!(value, 3);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker synchronization timed out"
            );
            std::thread::yield_now();
        }
        release.send(()).expect("release latest");
        let (id, result) = loop {
            if let Some(result) = worker.poll() {
                break result;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker synchronization timed out"
            );
            std::thread::yield_now();
        };
        assert_eq!(id, newest);
        assert_eq!(result.expect("latest completes"), 3);
        worker.shutdown();
        while !worker.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "worker synchronization timed out"
            );
            std::thread::yield_now();
        }
        assert!(worker.join_finished().expect("joined"));
    }

    #[test]
    fn cancelled_completion_cannot_activate_and_worker_can_retry() {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut worker =
            Worker::spawn(|value: u32, _: &Control| Ok(Some(value))).expect("worker starts");
        worker.request(1).expect("request");
        worker.cancel();
        let latest = worker.request(2).expect("retry");
        let (id, value) = loop {
            if let Some(result) = worker.poll() {
                break result;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker synchronization timed out"
            );
            std::thread::yield_now();
        };
        assert_eq!(id, latest);
        assert_eq!(value.expect("success"), 2);
        worker.shutdown();
        assert!(worker.request(3).is_err());
        while !worker.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "worker synchronization timed out"
            );
            std::thread::yield_now();
        }
        assert!(worker.join_finished().expect("joined"));
    }

    #[test]
    fn failed_preparation_leaves_worker_available_for_retry() {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut worker = Worker::spawn(|value: u32, _: &Control| {
            if value == 0 {
                Err("invalid package".to_string())
            } else {
                Ok(Some(value))
            }
        })
        .expect("worker starts");
        worker.request(0).expect("request");
        let (_, outcome) = loop {
            if let Some(done) = worker.poll() {
                break done;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker synchronization timed out"
            );
            std::thread::yield_now();
        };
        assert_eq!(outcome, Err("invalid package".to_string()));
        worker.request(1).expect("retry");
        let (_, outcome) = loop {
            if let Some(done) = worker.poll() {
                break done;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker synchronization timed out"
            );
            std::thread::yield_now();
        };
        assert_eq!(outcome, Ok(1));
        worker.shutdown();
        while !worker.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "worker synchronization timed out"
            );
            std::thread::yield_now();
        }
        assert!(worker.join_finished().expect("joined"));
    }

    #[test]
    fn shutdown_does_not_join_a_live_preparation() {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let (started, starts) = mpsc::sync_channel(1);
        let (release, releases) = mpsc::sync_channel(1);
        let mut worker = Worker::spawn(move |(): (), control: &Control| {
            let _ = started.send(());
            releases.recv().map_err(|error| error.to_string())?;
            assert!(control.cancelled.load(Ordering::Relaxed));
            Ok(Some(()))
        })
        .expect("worker starts");
        worker.request(()).expect("request");
        starts
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("running");
        worker.shutdown();
        assert!(!worker.join_finished().expect("nonblocking join check"));
        release.send(()).expect("release worker");
        while !worker.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "worker synchronization timed out"
            );
            std::thread::yield_now();
        }
        assert!(worker.join_finished().expect("joined"));
    }
}

#[cfg(test)]
#[path = "loading/cache_tests.rs"]
mod cache_tests;
