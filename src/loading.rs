//! One bounded CPU preparation worker; SDL and GPU installation remain owned by the frame loop.
use std::fmt::Write;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use crate::game::CollisionWorld;
use crate::lighting::lightmap::LightmapCache;
use crate::loader::{LevelEntry, LevelManager, LoadedLevel};
use crate::props::PropAssets;
use crate::quality::LightmapQuality;
use crate::render::{
    CharacterScene, LevelBuild, LightmapBuildOptions, LightmapFillOutcome,
    fill_lightmaps_cancellable, prepare_level_geometry_with_lightmaps, rebuild_vertex_lit_level,
};

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

/// Retained definitions avoid disk access and preserve imported-pack assets on a graphics change.
#[derive(Clone)]
pub enum Source {
    Default,
    Entry(LevelEntry),
    Retained(Box<LoadedLevel>),
}
impl Source {
    pub fn level_id(&self) -> &str {
        match self {
            Self::Default => crate::loader::DEMO_LEVEL_ID,
            Self::Entry(entry) => &entry.id,
            Self::Retained(loaded) => &loaded.level.id,
        }
    }

    /// True when both requests name the same world content.
    ///
    /// This deliberately requires more than a matching level name: an `Entry`
    /// also compares the discovered file identity (including its size and
    /// modification time when it exists on disk), and a `Retained` world
    /// compares the serialised definition, so an edited level can never reuse
    /// preparation started for its previous content.
    #[must_use]
    pub fn same_preparation(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Default, Self::Default) => true,
            (Self::Entry(a), Self::Entry(b)) => {
                a == b && entry_file_identity(a) == entry_file_identity(b)
            }
            (Self::Retained(a), Self::Retained(b)) => {
                a.entry == b.entry
                    && serde_json::to_vec(&a.level).ok() == serde_json::to_vec(&b.level).ok()
            }
            _ => false,
        }
    }
}

/// `(len, modified)` of a discovered level file, or `None` for embedded and
/// missing paths whose definition cannot change on disk between requests.
fn entry_file_identity(entry: &LevelEntry) -> Option<(u64, std::time::SystemTime)> {
    let metadata = std::fs::metadata(&entry.path).ok()?;
    let modified = metadata.modified().ok()?;
    Some((metadata.len(), modified))
}
pub struct Request {
    pub source: Source,
    pub lightmaps: LightmapQuality,
}
/// One internally compatible CPU world. The caller installs this only while its generation is current.
pub struct PreparedWorld {
    pub preparation_millis: f64,
    pub cache_hit: bool,
    pub loaded: LoadedLevel,
    pub build: Arc<LevelBuild>,
    pub collision: CollisionWorld,
    pub characters: CharacterScene,
    pub assets: PropAssets,
    pub lightmaps: LightmapQuality,
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

/// The key stores exact dependency bytes; file times and hash collisions cannot
/// make a changed GLB reuse old prepared geometry. Pixels for world sheets live
/// in `LoadedLevel` and are uploaded independently using their content identity.
#[derive(Clone, PartialEq)]
struct BuildKey {
    definition: Vec<u8>,
    materials: String,
    catalog: String,
    root: Option<std::path::PathBuf>,
    models: Vec<(String, crate::props::ModelInput)>,
    quality: LightmapQuality,
}
impl BuildKey {
    fn for_request(
        loaded: &LoadedLevel,
        catalog: &crate::loader::PropCatalog,
        assets: &mut PropAssets,
        quality: LightmapQuality,
    ) -> Result<Self, String> {
        let entries: Vec<_> = loaded
            .level
            .props
            .iter()
            .map(|prop| catalog.get(&prop.model))
            .collect();
        let mut paths: Vec<_> = entries
            .iter()
            .filter_map(|entry| entry.model.clone())
            .collect();
        paths.sort();
        paths.dedup();
        let models = assets.refresh_inputs(&paths);
        let mut materials = String::new();
        for entry in loaded.materials.entries() {
            let mut logical = entry.clone();
            logical.image = None;
            write!(materials, "{logical:?}").map_err(|error| error.to_string())?;
        }
        Ok(Self {
            definition: serde_json::to_vec(&loaded.level).map_err(|error| error.to_string())?,
            materials,
            catalog: format!("{entries:?}"),
            root: assets.root().map(std::path::Path::to_path_buf),
            models,
            quality,
        })
    }
}

/// Small worker-owned LRU. Eviction releases only the cache's Arc; a displayed
/// world remains valid. Oversized worlds are prepared normally but not retained.
#[derive(Default)]
struct BuildCache {
    entries: std::collections::VecDeque<(BuildKey, Arc<LevelBuild>, usize)>,
}
impl BuildCache {
    fn get(&mut self, key: &BuildKey) -> Option<Arc<LevelBuild>> {
        let position = self
            .entries
            .iter()
            .position(|(stored, _, _)| stored == key)?;
        let entry = self.entries.remove(position)?;
        let build = Arc::clone(&entry.1);
        self.entries.push_back(entry);
        Some(build)
    }
    fn insert(&mut self, key: BuildKey, build: Arc<LevelBuild>) {
        self.insert_with_limits(key, build, 3, 192 * 1024 * 1024);
    }

    fn insert_with_limits(
        &mut self,
        key: BuildKey,
        build: Arc<LevelBuild>,
        max_entries: usize,
        max_bytes: usize,
    ) {
        let bytes = build
            .retained_bytes()
            .saturating_add(key.definition.capacity())
            .saturating_add(key.materials.capacity())
            .saturating_add(key.catalog.capacity())
            .saturating_add(
                key.models
                    .iter()
                    .map(|(path, input)| {
                        path.capacity().saturating_add(
                            input
                                .as_ref()
                                .map_or_else(String::capacity, |bytes| bytes.len()),
                        )
                    })
                    .sum::<usize>(),
            );
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
        self.entries.push_back((key, build, bytes));
    }
}

fn prepare_build(
    loaded: &LoadedLevel,
    assets: &mut PropAssets,
    cache: &mut LightmapCache,
    quality: LightmapQuality,
    control: &Control,
) -> Option<LevelBuild> {
    crate::lighting::set_preparation_assets(loaded.catalog.as_ref().clone(), assets.clone());
    let prepared = prepare_level_geometry_with_lightmaps(
        &loaded.level,
        loaded.catalog.as_ref(),
        assets,
        &loaded.materials,
        LightmapBuildOptions::for_lightmaps(quality),
        Some(cache),
    );
    let mut build = prepared.build;
    if !control.checkpoint(Phase::Lightmaps) {
        return None;
    }
    if let Some(fill) = prepared.fill {
        match fill_lightmaps_cancellable(&fill, &control.cancelled) {
            LightmapFillOutcome::Filled(atlas) => {
                let atlas = Arc::new(atlas);
                if !control.checkpoint(Phase::Lightmaps) {
                    return None;
                }
                cache.insert(&fill.content_key, Arc::clone(&atlas));
                crate::render::dump_lightmaps_for_level(&loaded.level, &atlas);
                build.lightmap_millis = atlas.stats.bake_millis;
                build.lightmaps = Some(atlas);
                build.lightmap_failure = None;
            }
            LightmapFillOutcome::Failed(failure) => {
                build.lightmap_failure = Some(failure);
                build.lightmaps = None;
                let began = std::time::Instant::now();
                build.mesh = rebuild_vertex_lit_level(
                    &loaded.level,
                    loaded.catalog.as_ref(),
                    assets,
                    &loaded.materials,
                    &build.lighting,
                );
                build.timings.surfaces_millis = began
                    .elapsed()
                    .as_secs_f64()
                    .mul_add(1000.0, build.timings.surfaces_millis);
            }
            LightmapFillOutcome::Cancelled => return None,
        }
    }
    Some(build)
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
    identities: &mut std::collections::VecDeque<(BuildKey, std::sync::Weak<LevelBuild>)>,
    key: BuildKey,
    build: &Arc<LevelBuild>,
) {
    // Weak identities keep an oversized displayed world reusable after a
    // different preparation is cancelled, without retaining its geometry.
    identities.retain(|(previous, build)| previous != &key && build.strong_count() != 0);
    while identities.len() >= 8 {
        identities.pop_front();
    }
    identities.push_back((key, Arc::downgrade(build)));
}

impl Loader {
    /// Transfers the existing manager (including decoded images) to one persistent worker.
    pub fn new(mut manager: LevelManager) -> Result<Self, String> {
        let delay = test_delay()?;
        let mut assets = PropAssets::load_default();
        let mut cache = LightmapCache::with_disk();
        let mut builds = BuildCache::default();
        // The renderer owns the active world even when it is too large for the
        // LRU. A weak reference permits texture-only refits without retaining
        // another oversized world after it is no longer displayed.
        let mut identities: std::collections::VecDeque<(BuildKey, std::sync::Weak<LevelBuild>)> =
            std::collections::VecDeque::new();
        let worker = Worker::spawn(move |request: Request, control| {
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
                Source::Entry(entry) => {
                    manager.refresh_catalog();
                    manager.load_level(&entry)?
                }
                Source::Retained(loaded) => *loaded,
            };
            if !control.checkpoint(Phase::Geometry) {
                return Ok(None);
            }
            let key = BuildKey::for_request(
                &loaded,
                loaded.catalog.as_ref(),
                &mut assets,
                request.lightmaps,
            )?;
            identities.retain(|(_, build)| build.strong_count() != 0);
            let reusable = builds.get(&key).or_else(|| {
                identities.iter().find_map(|(previous, build)| {
                    (previous == &key).then(|| build.upgrade()).flatten()
                })
            });
            let cache_hit = reusable.is_some();
            let build = if let Some(build) = reusable {
                crate::logging::info(format_args!(
                    "[loading] prepared-cache hit level={}",
                    loaded.level.id
                ));
                build
            } else {
                crate::logging::info(format_args!(
                    "[loading] prepared-cache miss level={}",
                    loaded.level.id
                ));
                let Some(build) =
                    prepare_build(&loaded, &mut assets, &mut cache, request.lightmaps, control)
                else {
                    return Ok(None);
                };
                let build = Arc::new(build);
                if control.cancelled.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                builds.insert(key.clone(), Arc::clone(&build));
                build
            };
            remember_build(&mut identities, key, &build);
            if !control.checkpoint(Phase::Collision) {
                return Ok(None);
            }
            let collision = CollisionWorld::from_level(&loaded.level);
            if !control.checkpoint(Phase::Characters) {
                return Ok(None);
            }
            let characters = CharacterScene::spawn_characters(
                &loaded.level,
                loaded.catalog.as_ref(),
                &mut assets,
                &build.lighting,
            );
            if !control.checkpoint(Phase::Ready) {
                return Ok(None);
            }
            Ok(Some(PreparedWorld {
                preparation_millis: started.elapsed().as_secs_f64() * 1000.0,
                cache_hit,
                loaded,
                build,
                collision,
                characters,
                assets: assets.clone(),
                lightmaps: request.lightmaps,
            }))
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

#[cfg(test)]
mod tests {
    // Tests use expect to report a broken synchronization contract.
    #![allow(clippy::expect_used)]
    use super::*;

    /// Exact content identity prevents an edited level with the same id from
    /// reusing stale geometry, for both the active build and the LRU cache.
    #[test]
    fn a_retained_build_only_matches_the_same_level_content() {
        let level = crate::level::LevelDef::from_json(
            r#"{
                "format_version": 2, "id": "build_identity", "name": "Build Identity",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("identity fixture parses");
        let loaded = LoadedLevel {
            materials: crate::render::logical_materials(&level),
            catalog: Arc::new(crate::loader::PropCatalog::builtin()),
            entry: crate::loader::LevelEntry {
                id: level.id.clone(),
                name: level.name.clone(),
                author: String::new(),
                source_type: crate::loader::LevelSourceType::Embedded,
                path: std::path::PathBuf::new(),
            },
            level,
            light_sheets: Vec::new(),
        };
        let mut assets = PropAssets::default();
        let mut key_for = |loaded: &LoadedLevel| {
            BuildKey::for_request(loaded, &loaded.catalog, &mut assets, LightmapQuality::Full)
                .expect("identity serializes")
        };
        let original = key_for(&loaded);
        assert!(
            original == key_for(&loaded.clone()),
            "identical content reuses the build"
        );
        let mut edited = loaded.clone();
        edited.level.ceiling_lights.first_mut().expect("fixture").x += 0.25;
        assert!(
            original != key_for(&edited),
            "a moved fixture invalidates the same id"
        );
        let mut renamed = loaded.clone();
        renamed.level.id = "other_level".to_string();
        assert!(original != key_for(&renamed), "a different id cannot match");
        assert!(
            BuildCache::default().get(&original).is_none(),
            "no retained build never matches"
        );
    }

    /// Coalescing reuses an outstanding request only for identical content.
    #[test]
    fn same_preparation_compares_content_not_only_the_level_name() {
        let entry = |id: &str, path: &str| LevelEntry {
            id: id.to_string(),
            name: "Same Name".to_string(),
            author: "Author".to_string(),
            source_type: crate::loader::LevelSourceType::CustomJson,
            path: std::path::PathBuf::from(path),
        };
        let source = Source::Entry(entry("same_name", "/levels/a.json"));
        assert!(
            source.same_preparation(&Source::Entry(entry("same_name", "/levels/a.json"))),
            "an identical entry reuses its preparation"
        );
        assert!(
            !source.same_preparation(&Source::Entry(entry("same_name", "/levels/b.json"))),
            "a different file with the same level id is different content"
        );
        assert!(
            !source.same_preparation(&Source::Entry(entry("other_level", "/levels/a.json"))),
            "a different level id is different content"
        );
        assert!(!source.same_preparation(&Source::Default));
        assert!(Source::Default.same_preparation(&Source::Default));
    }

    /// A retained world's serialised definition decides reuse; an edit that
    /// keeps the id cannot silently answer the new request with the old build.
    #[test]
    fn retained_sources_only_match_identical_definitions() {
        let level = crate::level::LevelDef::from_json(
            r#"{
                "format_version": 2, "id": "retained_identity", "name": "Retained Identity",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("identity fixture parses");
        let loaded = LoadedLevel {
            materials: crate::render::logical_materials(&level),
            catalog: Arc::new(crate::loader::PropCatalog::builtin()),
            entry: crate::loader::LevelEntry {
                id: level.id.clone(),
                name: level.name.clone(),
                author: String::new(),
                source_type: crate::loader::LevelSourceType::CustomJson,
                path: std::path::PathBuf::new(),
            },
            level,
            light_sheets: Vec::new(),
        };
        let source = Source::Retained(Box::new(loaded.clone()));
        assert!(
            source.same_preparation(&Source::Retained(Box::new(loaded.clone()))),
            "an identical retained world reuses its preparation"
        );
        let mut edited = loaded;
        edited.level.ceiling_lights.first_mut().expect("fixture").x += 0.25;
        assert!(
            !source.same_preparation(&Source::Retained(Box::new(edited))),
            "an edited retained world is different content"
        );
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
                Err("invalid level".to_string())
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
        assert_eq!(outcome, Err("invalid level".to_string()));
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
