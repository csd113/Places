//! Ephemeral local transport using the compiler's geometry, solver and atlas fill.
//! No `LightmapCache`, package writer or filesystem output is reachable here.
use crate::engine::{
    lighting::lightmap::LightmapPatch,
    progress::{Counter, Reporter},
    render::{self, LevelBuild, Vertex},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
pub(in crate::editor) struct Progress {
    state: Mutex<(String, Option<Arc<Counter>>)>,
    cancel: Arc<AtomicBool>,
}
impl Progress {
    pub fn new(cancel: Option<Arc<AtomicBool>>) -> Self {
        Self {
            state: Mutex::new(("Preparing full scene geometry".into(), None)),
            cancel: cancel.unwrap_or_default(),
        }
    }
    pub fn label(&self) -> String {
        self.state.lock().map_or_else(
            |_| "Preparing preview".into(),
            |state| {
                state.1.as_ref().map_or_else(
                    || state.0.clone(),
                    |work| {
                        format!(
                            "{} / {}/{} {}",
                            state.0,
                            work.done.load(Ordering::Relaxed).min(work.total),
                            work.total,
                            work.unit
                        )
                    },
                )
            },
        )
    }
}
impl Reporter for Progress {
    fn stage(&self, label: &str, _unit: &'static str) {
        if let Ok(mut state) = self.state.lock() {
            *state = (label.into(), None);
        }
    }
    fn current_work(&self, total: usize) -> Option<Arc<Counter>> {
        self.start_work("Current lighting pass", total, "receivers")
    }
    fn start_work(&self, label: &str, total: usize, unit: &'static str) -> Option<Arc<Counter>> {
        let work = Arc::new(Counter {
            done: std::sync::atomic::AtomicUsize::new(0),
            total,
            unit,
        });
        if let Ok(mut state) = self.state.lock() {
            let stage = if label == "Current lighting pass" {
                state.0.clone()
            } else {
                label.into()
            };
            *state = (stage, Some(Arc::clone(&work)));
        }
        Some(work)
    }
    fn refresh(&self) {}
    fn cancellation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }
}

pub(super) fn prepare(
    level: &crate::engine::level::LevelDef,
    catalog: &crate::engine::loader::PropCatalog,
    assets: &mut crate::engine::props::PropAssets,
    materials: &crate::engine::materials::MaterialTable,
    bounds: [f32; 4],
) -> Result<LevelBuild, String> {
    crate::engine::lighting::set_preparation_assets(catalog.clone(), assets.clone());
    let mut options =
        render::LightmapBuildOptions::for_lightmaps(crate::engine::quality::LightmapQuality::Full);
    // Preserve actual shadow geometry; reduce receiver density and gather budget only.
    options.config.texels_per_metre = options.config.texels_per_metre.min(4.0);
    options.solve.bounces = 1;
    options.solve.gather_samples = 16;
    let halo = 4.0_f32;
    let scope = [
        bounds[0] - halo,
        bounds[1] - halo,
        bounds[2] + halo,
        bounds[3] + halo,
    ];
    let mut prepared =
        render::prepare_local_lightmap_geometry(level, catalog, assets, materials, options, scope);
    let mut fill = prepared
        .fill
        .take()
        .ok_or("Local preview could not allocate a lightmap plan; use a smaller source")?;
    let selected: Vec<_> = fill
        .charts
        .iter()
        .map(|(patch, _)| intersects(patch, bounds, halo))
        .collect();
    let texels = fill
        .charts
        .iter()
        .zip(&selected)
        .filter(|(_, keep)| **keep)
        .fold(0usize, |sum, ((_, chart), _)| {
            sum.saturating_add(
                usize::try_from(chart.width.saturating_mul(chart.height)).unwrap_or(usize::MAX),
            )
        });
    if texels == 0 || texels > 262_144 {
        return Err(
            "Local preview receiver budget is 1..=262144 texels; select a smaller room / region"
                .into(),
        );
    }
    let scene = Arc::try_unwrap(fill.transport)
        .map_err(|_shared| "Preview transport is unexpectedly shared")?;
    fill.transport = Arc::new(scene.restrict_preview_receivers(&fill.charts, &selected));
    let retained: Vec<_> = fill
        .charts
        .iter()
        .zip(selected)
        .filter_map(|(chart, keep)| keep.then_some(*chart))
        .collect();
    fill.charts = retained;
    fill.content_key = "ephemeral-local-preview-not-for-export".into();
    fill.probe_bake = false;
    let cancel = crate::engine::progress::cancellation_flag();
    let product = match render::fill_lightmaps_cancellable(&fill, &cancel) {
        render::LightmapFillOutcome::Filled(product) => product,
        render::LightmapFillOutcome::Cancelled => return Err("Local preview cancelled".into()),
        render::LightmapFillOutcome::Failed(failure) => {
            return Err(format!("Local preview failed: {}", failure.name()));
        }
    };
    // Retained charts include a receiver halo. Whole chart domains stay lit;
    // other charts use subdued context, with no triangle crossing page sentinels.
    let domains: Vec<_> = fill
        .charts
        .iter()
        .map(|(_, chart)| {
            (
                u8::try_from(chart.page).unwrap_or(render::LIGHTMAP_NONE),
                chart.uv_at(fill.config.page_edge, 0.0, 0.0),
                chart.uv_at(fill.config.page_edge, 1.0, 1.0),
            )
        })
        .collect();
    let display = |vertex: &mut Vertex| {
        if vertex.lightmap_page != render::LIGHTMAP_NONE
            && !domains.iter().any(|(page, low, high)| {
                vertex.lightmap_page == *page
                    && vertex
                        .lightmap
                        .iter()
                        .zip(low.iter().zip(high))
                        .all(|(value, (a, b))| value >= a && value <= b)
            })
        {
            vertex.lightmap_page = render::LIGHTMAP_NONE;
            vertex.color = [0.08, 0.08, 0.08, 1.0];
        }
    };
    for range in &mut Arc::make_mut(&mut prepared.build.mesh).ranges {
        for vertex in &mut range.vertices {
            display(vertex);
        }
    }
    for batch in &mut prepared.build.batches {
        for vertex in batch.vertices.make_mut() {
            display(vertex);
        }
    }
    prepared.build.lightmap_millis = product.lightmaps.stats.bake_millis;
    prepared.build.lightmaps = Some(Arc::new(product.lightmaps));
    prepared.build.probes = None;
    Ok(prepared.build)
}

fn intersects(patch: &LightmapPatch, bounds: [f32; 4], halo: f32) -> bool {
    let points = [
        patch.point_at(0.0, 0.0),
        patch.point_at(1.0, 0.0),
        patch.point_at(0.0, 1.0),
        patch.point_at(1.0, 1.0),
    ];
    let low_x = points.iter().fold(f32::INFINITY, |v, p| v.min(p[0]));
    let high_x = points.iter().fold(f32::NEG_INFINITY, |v, p| v.max(p[0]));
    let low_z = points.iter().fold(f32::INFINITY, |v, p| v.min(p[2]));
    let high_z = points.iter().fold(f32::NEG_INFINITY, |v, p| v.max(p[2]));
    high_x >= bounds[0] - halo
        && low_x <= bounds[2] + halo
        && high_z >= bounds[1] - halo
        && low_z <= bounds[3] + halo
}
