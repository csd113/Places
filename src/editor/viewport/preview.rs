//! A single coalesced CPU preparation worker; GPU upload stays on the UI thread.

pub(in crate::editor) mod audit;
mod local;
mod reuse;
mod timing;
pub(in crate::editor) use local::Progress;

use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

use super::picking::{PickObject, objects};
use crate::engine::level::LevelDef;
use crate::engine::loader::{LevelEntry, LevelSourceType, LoadedLevel, PropCatalog};
use crate::engine::materials::{TextureCache, resolve_materials};
use crate::engine::props::PropAssets;
use crate::engine::render::{CharacterScene, LevelBuild};

pub(super) struct PreparedPreview {
    pub geometry: crate::engine::render::PreparedGeometry,
    pub loaded: LoadedLevel,
    pub metrics: serde_json::Value,
    pub caustics: Option<crate::engine::render::PreparedCaustics>,
    pub build: Arc<LevelBuild>,
    pub assets: PropAssets,
    pub objects: Vec<PickObject>,
    pub characters: CharacterScene,
}

struct Request {
    generation: u64,
    level: LevelDef,
    environment: bool,
    scope: Option<[f32; 4]>,
    progress: Arc<Progress>,
    budget: Arc<crate::engine::resources::DecodeBudget>,
    queued: std::time::Instant,
}

struct Response {
    generation: u64,
    result: Result<PreparedPreview, String>,
    finished: std::time::Instant,
}

pub(super) struct PreviewWorker {
    request: mpsc::Sender<Option<Request>>,
    response: mpsc::Receiver<Response>,
    thread: Option<JoinHandle<()>>,
    generation: u64,
    pending: Option<(LevelDef, bool, Option<[f32; 4]>, std::time::Instant)>,
    pub progress: Arc<Progress>,
    budget: Option<Arc<crate::engine::resources::DecodeBudget>>,
    busy: bool,
    error: Option<String>,
}

impl PreviewWorker {
    pub fn new() -> Result<Self, String> {
        let (request_tx, request_rx) = mpsc::channel::<Option<Request>>();
        let (response_tx, response_rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("places-editor-preview".to_string())
            .spawn(move || {
                let catalog = PropCatalog::load_default();
                let mut assets = PropAssets::load_default();
                let mut textures = TextureCache::default();
                let mut reuse = reuse::Reuse::default();
                while let Ok(Some(request)) = request_rx.recv() {
                    let subscription = if request.scope.is_some() {
                        let reporter: Arc<dyn crate::engine::progress::Reporter> =
                            request.progress.clone();
                        crate::engine::progress::subscribe(reporter).map(Some)
                    } else {
                        Ok(None)
                    };
                    let _decode_scope =
                        crate::engine::resources::enter(Some(Arc::clone(&request.budget)));
                    let queue_wait_ms = request.queued.elapsed().as_secs_f64() * 1_000.0_f64;
                    let mut result = match subscription.as_ref() {
                        Ok(_) => prepare(
                            request.level,
                            request.environment,
                            &catalog,
                            &mut assets,
                            &mut textures,
                            &mut reuse,
                            request.scope,
                        ),
                        Err(error) => Err(format!("Cannot start local preview feedback: {error}")),
                    };
                    if let Ok(preview) = &mut result
                        && let Some(metrics) = preview.metrics.as_object_mut()
                    {
                        drop(metrics.insert(
                            "worker_queue_wait_ms".into(),
                            serde_json::json!(queue_wait_ms),
                        ));
                    }
                    if response_tx
                        .send(Response {
                            generation: request.generation,
                            result,
                            finished: std::time::Instant::now(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| format!("Cannot start scene preview worker: {error}"))?;
        Ok(Self {
            request: request_tx,
            response: response_rx,
            thread: Some(handle),
            generation: 0,
            pending: None,
            progress: Arc::new(Progress::default()),
            budget: None,
            busy: false,
            error: None,
        })
    }

    pub fn request(&mut self, level: &LevelDef, environment: bool) {
        self.request_scoped(level, environment, None);
    }

    pub fn request_scoped(&mut self, level: &LevelDef, environment: bool, scope: Option<[f32; 4]>) {
        if let Some(budget) = self.budget.take() {
            budget
                .cancelled
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.budget = Some(crate::engine::resources::DecodeBudget::new(Arc::new(
            std::sync::atomic::AtomicBool::new(false),
        )));
        self.generation = self.generation.wrapping_add(1);
        self.progress = Arc::new(Progress::new(
            self.budget.as_ref().map(|b| Arc::clone(&b.cancelled)),
        ));
        self.pending = Some((level.clone(), environment, scope, std::time::Instant::now()));
        self.error = None;
    }

    pub fn poll(&mut self) -> Option<PreparedPreview> {
        let mut ready = None;
        match self.response.try_recv() {
            Ok(response) => {
                self.busy = false;
                if response.generation == self.generation {
                    match response.result {
                        Ok(mut preview) => {
                            if let Some(metrics) = preview.metrics.as_object_mut() {
                                drop(metrics.insert(
                                    "ui_response_wait_ms".into(),
                                    serde_json::json!(
                                        response.finished.elapsed().as_secs_f64() * 1_000.0_f64
                                    ),
                                ));
                            }
                            ready = Some(preview);
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.busy = false;
                self.pending = None;
                self.error = Some("Scene preview worker stopped".to_string());
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if !self.busy
            && let Some((level, environment, scope, queued)) = self.pending.take()
        {
            if self
                .request
                .send(Some(Request {
                    generation: self.generation,
                    level,
                    environment,
                    scope,
                    progress: Arc::clone(&self.progress),
                    queued,
                    budget: self.budget.as_ref().map_or_else(
                        || {
                            crate::engine::resources::DecodeBudget::new(Arc::new(
                                std::sync::atomic::AtomicBool::new(false),
                            ))
                        },
                        Arc::clone,
                    ),
                }))
                .is_ok()
            {
                self.busy = true;
            } else {
                self.error = Some("Scene preview worker stopped".to_string());
            }
        }
        ready
    }

    pub const fn is_preparing(&self) -> bool {
        self.busy || self.pending.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        if let Some(budget) = &self.budget {
            budget
                .cancelled
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        // At most one CPU request is active. No GPU work or child process is
        // owned by this worker; joining releases its decoded asset cache.
        if self.request.send(None).is_err() {
            return;
        }
        if let Some(handle) = self.thread.take()
            && handle.join().is_err()
        {
            crate::engine::logging::warn_once(
                "editor-preview-worker",
                "Scene preview worker stopped unexpectedly",
            );
        }
    }
}

fn prepare(
    mut level: LevelDef,
    environment: bool,
    catalog: &PropCatalog,
    assets: &mut PropAssets,
    textures: &mut TextureCache,
    reuse: &mut reuse::Reuse,
    scope: Option<[f32; 4]>,
) -> Result<PreparedPreview, String> {
    let started = std::time::Instant::now();
    let cpu_started = timing::cpu_ms();
    let root = crate::engine::assets::resolve_asset_root()
        .ok_or_else(|| "Cannot locate built-in assets for the scene preview".to_string())?;
    // Preview the same derived trim and aligned fixtures as compilation while
    // keeping the authoritative document untouched.
    crate::map_compiler::prepare_level(&mut level, catalog.assets(), None);
    // Atmosphere is presentation policy rather than editable geometry. Keep the
    // inspection view clear without writing any of these changes to the file.
    if !environment {
        // Inspection keeps hidden actors authorable. Only this private preview
        // copy changes; Environment preview and Play retain runtime visibility.
        for prop in &mut level.props {
            prop.shadow_only = false;
        }
        level.environment = None;
        level.sky = None;
        level.weather = None;
        level.weather_alternate = None;
        level.weather_cycle = None;
        level.fog_regions.clear();
        level.ground_fog.clear();
    }
    let source_ms = started.elapsed().as_secs_f64() * 1_000.0_f64;
    let materials_started = std::time::Instant::now();
    textures.begin_level();
    let input_key = prepare_assets(&level, catalog, &root, assets, reuse)?;
    let materials = resolve_materials(&level, catalog.assets(), None, Some(&root), textures)?;
    let light_sheets = crate::engine::loader::resolve_fixture_sheets(
        &level,
        catalog.assets(),
        None,
        textures,
        &materials,
    )?;
    crate::engine::resources::checkpoint()?;
    let materials_ms = materials_started.elapsed().as_secs_f64() * 1_000.0_f64;
    let (build, reused_build, reused_props) = if let Some(bounds) = scope {
        (
            Arc::new(local::prepare(&level, catalog, assets, &materials, bounds)?),
            false,
            false,
        )
    } else {
        reuse.build(&level, catalog, assets, &materials, environment, &input_key)?
    };
    crate::engine::resources::checkpoint()?;
    let caustics_started = std::time::Instant::now();
    let caustics = crate::engine::render::PreparedCaustics::prepare(&level, &build.lighting);
    let caustics_ms = caustics_started.elapsed().as_secs_f64() * 1_000.0_f64;
    let picking_started = std::time::Instant::now();
    let object_bounds = objects(&level, catalog, assets, &build.lighting);
    let picking_ms = picking_started.elapsed().as_secs_f64() * 1_000.0_f64;
    let characters_started = std::time::Instant::now();
    let characters = CharacterScene::spawn_characters(&level, catalog, assets, &build.lighting);
    let characters_ms = characters_started.elapsed().as_secs_f64() * 1_000.0_f64;
    let entry = LevelEntry {
        id: level.id.clone(),
        name: level.name.clone(),
        author: level.author.clone(),
        source_type: LevelSourceType::Installed,
        path: std::path::PathBuf::new(),
    };
    let loaded = LoadedLevel {
        package_identity: None,
        catalog: Arc::new(catalog.clone()),
        level,
        materials,
        light_sheets,
        entry,
    };
    textures.begin_level();
    let geometry_started = std::time::Instant::now();
    let geometry = crate::engine::render::PreparedGeometry::new(&loaded, &build)?;
    let geometry_ms = geometry_started.elapsed().as_secs_f64() * 1_000.0_f64;
    let stats = assets.stats();
    let metrics = serde_json::json!({"local_preview":scope,"preview_bake_ms":build.lightmap_millis,"preview_charts":build.lightmaps.as_ref().map(|a|a.stats.charts),"preview_texels":build.lightmaps.as_ref().map(|a|a.stats.texels),"worker_thread_cpu_ms":cpu_started.zip(timing::cpu_ms()).map(|(begin,end)|end-begin),"cpu_prepare_ms":started.elapsed().as_secs_f64()*1_000.0_f64,"retained_build_bytes":build.retained_bytes(),"reused_build":reused_build,"reused_props":reused_props,"incremental":reuse.metrics,"source_ms":source_ms,"materials_ms":materials_ms,"lighting_ms":if reused_build {0.0_f64} else {build.timings.lighting_millis},"props_ms":if reused_build {0.0_f64} else {build.timings.props_millis},"surfaces_ms":if reused_build {0.0_f64} else {build.timings.surfaces_millis},"picking_ms":picking_ms,"characters_ms":characters_ms,"geometry_ms":geometry_ms,"caustics_prepare_ms":caustics_ms,"cached_models":stats.models_loaded,"model_texture_bytes":stats.texture_bytes,"cached_images":textures.len(),"image_decodes":textures.decoded_count()});
    Ok(PreparedPreview {
        geometry,
        metrics,
        caustics,
        loaded,
        build,
        assets: assets.clone(),
        objects: object_bounds,
        characters,
    })
}

fn prepare_assets(
    level: &LevelDef,
    catalog: &PropCatalog,
    root: &std::path::Path,
    assets: &mut PropAssets,
    reuse: &reuse::Reuse,
) -> Result<String, String> {
    let paths: std::collections::BTreeSet<_> = level
        .props
        .iter()
        .map(|prop| prop.model.as_str())
        .chain(
            level
                .spawn_templates
                .iter()
                .map(|template| template.model.as_str()),
        )
        .filter_map(|id| catalog.get(id).model)
        .chain(
            level
                .ceiling_lights
                .iter()
                .filter_map(|fixture| crate::engine::lighting::fixture_model_id(&fixture.fixture))
                .filter_map(|id| catalog.get(id).model),
        )
        .collect();
    let appearances: std::collections::BTreeSet<&str> = level
        .props
        .iter()
        .filter_map(|prop| prop.character_texture.as_deref())
        .chain(
            level
                .spawn_templates
                .iter()
                .filter_map(|template| template.character_texture.as_deref()),
        )
        .collect();
    let mut dependencies = Vec::with_capacity(paths.len());
    for path in &paths {
        crate::engine::resources::checkpoint()?;
        dependencies.push(crate::engine::package::PackageDependency {
            kind: crate::engine::package::DependencyKind::Model,
            bytes: std::fs::metadata(root.join(path)).map_or(0, |metadata| metadata.len()),
            path: path.clone(),
            sha256: crate::engine::package::hash::sha256_file(&root.join(path)).unwrap_or_default(),
        });
    }

    for appearance in &appearances {
        if let Some(path) = catalog.assets().texture_path(appearance) {
            dependencies.push(crate::engine::package::PackageDependency {
                kind: crate::engine::package::DependencyKind::Texture,
                path: path.to_owned(),
                sha256: crate::engine::package::hash::sha256_file(&root.join(path))
                    .unwrap_or_default(),
                bytes: std::fs::metadata(root.join(path)).map_or(0, |metadata| metadata.len()),
            });
        }
    }
    let inputs = format!("{dependencies:?}");
    if reuse.inputs != inputs {
        assets.synchronize_dependencies(&dependencies);
    }
    assets.retain_preview_models(&paths, &appearances);
    Ok(inputs)
}

/// Explicit CPU probe; never runs in the editor loop or automatic tests.
pub(in crate::editor) fn profile(level: &LevelDef) -> Result<Vec<serde_json::Value>, String> {
    use serde_json::json;
    use sha2::{Digest as _, Sha256};
    use std::time::Instant;
    let catalog_props = PropCatalog::load_default();
    let mut assets = PropAssets::load_default();
    let mut textures = TextureCache::default();
    let mut previews = Vec::new();
    let mut reuse = reuse::Reuse::default();
    for environment in [false, false, true] {
        let started = Instant::now();
        let preview = prepare(
            level.clone(),
            environment,
            &catalog_props,
            &mut assets,
            &mut textures,
            &mut reuse,
            None,
        )?;
        let ms = started.elapsed().as_secs_f64() * 1_000.0_f64;
        let mut mesh = Sha256::new();
        for range in &preview.build.mesh.ranges {
            mesh.update(format!("{:?}", range.vertices).as_bytes());
            mesh.update(bytemuck::cast_slice(&range.indices));
        }
        previews.push(json!({"environment":environment,"ms":ms,"metrics":preview.metrics,"mesh_sha256":crate::engine::package::hash::digest_hex(&mesh.finalize()),"objects":preview.objects.len()}));
    }
    Ok(previews)
}

#[cfg(test)]
#[path = "../tests/preview_reuse.rs"]
mod tests;
