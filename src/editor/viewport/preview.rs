//! A single coalesced CPU preparation worker; GPU upload stays on the UI thread.

use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

use super::picking::{PickObject, objects};
use crate::engine::level::LevelDef;
use crate::engine::loader::{LevelEntry, LevelSourceType, LoadedLevel, PropCatalog};
use crate::engine::materials::{TextureCache, resolve_materials};
use crate::engine::props::PropAssets;
use crate::engine::render::{
    CharacterScene, LevelBuild, build_authoring_environment_preview, build_authoring_preview,
};

pub(super) struct PreparedPreview {
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
}

struct Response {
    generation: u64,
    result: Result<PreparedPreview, String>,
}

pub(super) struct PreviewWorker {
    request: mpsc::Sender<Option<Request>>,
    response: mpsc::Receiver<Response>,
    thread: Option<JoinHandle<()>>,
    generation: u64,
    pending: Option<(LevelDef, bool)>,
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
                while let Ok(Some(request)) = request_rx.recv() {
                    let result = prepare(
                        request.level,
                        request.environment,
                        &catalog,
                        &mut assets,
                        &mut textures,
                    );
                    if response_tx
                        .send(Response {
                            generation: request.generation,
                            result,
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
            busy: false,
            error: None,
        })
    }

    pub fn request(&mut self, level: &LevelDef, environment: bool) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = Some((level.clone(), environment));
        self.error = None;
    }

    pub fn poll(&mut self) -> Option<PreparedPreview> {
        let mut ready = None;
        match self.response.try_recv() {
            Ok(response) => {
                self.busy = false;
                if response.generation == self.generation {
                    match response.result {
                        Ok(preview) => ready = Some(preview),
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
            && let Some((level, environment)) = self.pending.take()
        {
            if self
                .request
                .send(Some(Request {
                    generation: self.generation,
                    level,
                    environment,
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
) -> Result<PreparedPreview, String> {
    let started = std::time::Instant::now();
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
    textures.begin_level();
    let paths = level
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
        .collect();
    assets.retain_preview_models(&paths);
    let materials = resolve_materials(&level, catalog.assets(), None, Some(&root), textures)?;
    let light_sheets = crate::engine::loader::resolve_fixture_sheets(
        &level,
        catalog.assets(),
        None,
        textures,
        &materials,
    )?;
    let build = Arc::new(if environment {
        build_authoring_environment_preview(&level, catalog, assets, &materials)
    } else {
        build_authoring_preview(&level, catalog, assets, &materials)
    });
    let caustics_started = std::time::Instant::now();
    let caustics = crate::engine::render::PreparedCaustics::prepare(&level, &build.lighting);
    let caustics_ms = caustics_started.elapsed().as_secs_f64() * 1_000.0_f64;
    let object_bounds = objects(&level, catalog, assets, &build.lighting);
    let characters = CharacterScene::spawn_characters(&level, catalog, assets, &build.lighting);
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
    let stats = assets.stats();
    let metrics = serde_json::json!({"cpu_prepare_ms":started.elapsed().as_secs_f64()*1_000.0_f64,"caustics_prepare_ms":caustics_ms,"cached_models":stats.models_loaded,"model_texture_bytes":stats.texture_bytes,"cached_images":textures.len(),"image_decodes":textures.decoded_count()});
    Ok(PreparedPreview {
        metrics,
        caustics,
        loaded,
        build,
        assets: assets.clone(),
        objects: object_bounds,
        characters,
    })
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
    for environment in [false, false, true] {
        let started = Instant::now();
        let preview = prepare(
            level.clone(),
            environment,
            &catalog_props,
            &mut assets,
            &mut textures,
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
