//! Neutral counters describing a level build and a submitted frame.
//!
//! The renderer fills these in; the benchmark harness and the developer logs
//! read them. They contain no GPU resources, only numbers.

/// Cost and shape of the last level build, split by stage so a hardware run can
/// tell an expensive geometry bake from an expensive prop instancing pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelBuildStats {
    /// Distinct vertices in the static level mesh (floors, ceilings, walls, fixtures).
    pub static_vertices: usize,
    /// Indices in the static level mesh.
    pub static_indices: usize,
    /// Distinct vertices in the prop mesh (every placed instance).
    pub prop_vertices: usize,
    /// Indices in the prop mesh.
    pub prop_indices: usize,
    /// Draw calls the level needs for real prop geometry.
    pub prop_draws: usize,
    /// Cullable static batches the level was partitioned into.
    pub static_batches: usize,
    /// Static GPU buffer pairs (a level past 65 536 vertices needs several).
    pub static_chunks: usize,
    /// Prop GPU buffer pairs.
    pub prop_chunks: usize,
    /// Bytes resident in vertex buffers.
    pub vbo_bytes: usize,
    /// Bytes resident in index buffers.
    pub index_bytes: usize,
    /// Wall-clock cost of the last level build (geometry + lighting bake), in ms.
    pub build_millis: f64,
    /// Time spent baking the static lighting, in ms.
    pub lighting_millis: f64,
    /// Time spent resolving, transforming and lit-shading every placed prop, in ms.
    pub props_millis: f64,
    /// Time spent emitting and spatially bucketing the static surfaces, in ms.
    pub surfaces_millis: f64,
    /// Summary of the baked static lighting.
    pub lighting: crate::lighting::LightingSummary,
    /// Lightmap atlas pages resident on the GPU.
    pub lightmap_pages: usize,
    /// Page texels uploaded, including gutters and unused page space.
    pub lightmap_texels: usize,
    /// Charts the lightmap bake filled.
    pub lightmap_charts: usize,
    /// Chart data texels the fill pass wrote, i.e. the light samples this level
    /// actually evaluated. Smaller than [`Self::lightmap_texels`], which counts
    /// whole pages including gutters and unused space.
    pub lightmap_chart_texels: usize,
    /// Time spent filling and packing the lightmap atlas, in ms.
    pub lightmap_millis: f64,
    /// True when a lightmap build failed and the level fell back to vertex
    /// lighting; the reason is on the level build, not in this counter.
    pub lightmap_fallback: bool,
}

/// Geometry counters for the frame that was most recently submitted.
///
/// Filled in by [`Renderer::render_scene`] and read by the debug-only benchmark
/// harness, so the numbers describe the actual draw path rather than a
/// reconstruction of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Vertices the level holds in total (static + props), visible or not.
    pub total_vertices: usize,
    /// Vertices inside the batches that were actually submitted this frame.
    pub visible_vertices: usize,
    /// Vertices belonging to batches the frustum rejected this frame.
    pub culled_vertices: usize,
    /// Render batches the level is split into.
    pub total_batches: usize,
    /// Batches that survived culling and were submitted.
    pub visible_batches: usize,
    /// Draw calls issued for the scene.
    pub draw_calls: usize,
    /// Draw calls the dynamic-object path submitted this frame: one per object
    /// per material, never one per vertex.
    pub dynamic_draws: usize,
    /// Distinct vertices the dynamic objects hold this frame.
    pub dynamic_vertices: usize,
    /// Bytes resident in static vertex buffers (level + props).
    pub vbo_bytes: usize,
    /// Bytes resident in element (index) buffers.
    pub index_bytes: usize,
    /// Texture binds the scene and presentation passes issued. One material
    /// change can cost several (albedo, emission mask, normal map); a run of
    /// batches that share a material costs none.
    pub texture_binds: usize,
    /// Surface-state changes the scene passes applied: one per material change
    /// per pass, plus the pass switches that invalidate the cache.
    pub material_changes: usize,
    /// Scene submissions the frame spent on reflections: one for the active
    /// planar plane, zero when only probes are drawn or nothing reflects.
    pub reflection_passes: u32,
}
