//! Neutral counters describing a level build and a submitted frame.
//!
//! The renderer fills these in; the benchmark harness and the developer logs
//! read them. They contain no GPU resources, only numbers.

/// Why the hardware GPU render-pass meter cannot supply a measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuTimingUnavailable {
    BenchmarkDisabled,
    AdapterUnsupported,
    FeatureNotEnabled,
    InvalidTimestampPeriod,
    ReadbackFailed,
    InvalidTimestampData,
}

impl GpuTimingUnavailable {
    /// Compact reason shown by the performance overlay and benchmark output.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::BenchmarkDisabled => "disabled by benchmark control",
            Self::AdapterUnsupported => "adapter lacks timestamps",
            Self::FeatureNotEnabled => "timestamps not enabled",
            Self::InvalidTimestampPeriod => "invalid timestamp period",
            Self::ReadbackFailed => "timestamp readback failed",
            Self::InvalidTimestampData => "invalid timestamp data",
        }
    }
}

/// One completed asynchronous GPU timestamp sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuTimingSample {
    /// Unique completion id; unchanged while no newer result is available.
    pub id: u64,
    /// Renderer frame id measured by these queries, independent of readback lag.
    pub frame: u64,
    /// Union of rendered pass timestamp spans in milliseconds, including
    /// planar reflection, bloom, resolve, UI and the surface copy when drawn.
    /// Overlapping vertex/fragment stages count once; disjoint gaps are excluded.
    /// Stage spans can contain GPU scheduling stalls, so this is elapsed GPU
    /// timeline coverage, not active execution time or utilization.
    pub milliseconds: f32,
}

/// Hardware GPU timing status, separate from CPU timing and utilization.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum GpuTiming {
    #[default]
    Pending,
    Measured(GpuTimingSample),
    Unavailable(GpuTimingUnavailable),
}

impl GpuTiming {
    /// Stable status text for the machine-readable benchmark CSV.
    #[must_use]
    pub const fn status(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Measured(_) => "measured",
            Self::Unavailable(reason) => reason.reason(),
        }
    }
}

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
    /// Prepared prop draw groups, before individual instance culling.
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
    pub lighting: crate::engine::lighting::LightingSummary,
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

/// Source geometry in one family, before/after the main view's conservative
/// culling. Vertices count each placement once (props count primitive-instance
/// ranges); batches count source ranges before transparency splitting/merging.
/// Resident bytes and actual commands are deliberately separate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct GeometryVisibility {
    pub eligible_vertices: usize,
    pub visible_vertices: usize,
    pub eligible_batches: usize,
    pub visible_batches: usize,
}

impl GeometryVisibility {
    pub const fn record(&mut self, vertices: usize, batches: usize, visible: bool) {
        self.eligible_vertices = self.eligible_vertices.saturating_add(vertices);
        self.eligible_batches = self.eligible_batches.saturating_add(batches);
        if visible {
            self.visible_vertices = self.visible_vertices.saturating_add(vertices);
            self.visible_batches = self.visible_batches.saturating_add(batches);
        }
    }

    pub const fn absorb(&mut self, other: Self) {
        self.eligible_vertices = self
            .eligible_vertices
            .saturating_add(other.eligible_vertices);
        self.visible_vertices = self.visible_vertices.saturating_add(other.visible_vertices);
        self.eligible_batches = self.eligible_batches.saturating_add(other.eligible_batches);
        self.visible_batches = self.visible_batches.saturating_add(other.visible_batches);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct SceneVisibility {
    pub world: GeometryVisibility,
    pub props: GeometryVisibility,
    pub dynamic: GeometryVisibility,
    pub characters: GeometryVisibility,
    pub decals: GeometryVisibility,
    pub effects: GeometryVisibility,
}

impl SceneVisibility {
    pub fn total(self) -> GeometryVisibility {
        let mut total = GeometryVisibility::default();
        for family in [
            self.world,
            self.props,
            self.dynamic,
            self.characters,
            self.decals,
            self.effects,
        ] {
            total.absorb(family);
        }
        total
    }
}

/// Geometry counters for the frame that was most recently submitted.
///
/// Filled in by [`Renderer::render_scene`] and read by the debug-only benchmark
/// harness. Source visibility describes the main-view cull decisions; actual
/// indices and commands describe encoding, including transparency splits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Matching eligible/visible scopes per family.
    pub geometry: SceneVisibility,
    /// Eligible source vertex placements in the same scope as `visible_vertices`.
    pub total_vertices: usize,
    /// Source vertex placements surviving main-view culling, counted once per source.
    pub visible_vertices: usize,
    /// Indices submitted by the base scene, including repeated indexed vertices.
    pub visible_indices: usize,
    /// Eligible source vertex placements rejected by frustum/occlusion culling.
    pub culled_vertices: usize,
    /// Eligible source ranges before command splitting/merging.
    pub total_batches: usize,
    /// Source ranges surviving culling, independent of actual draw command count.
    pub visible_batches: usize,
    /// Draw calls issued for the scene.
    pub draw_calls: usize,
    /// Actual dynamic-object draw commands; compatible instances share one
    /// submission while lighting-incompatible objects retain separate draws.
    pub dynamic_draws: usize,
    /// Distinct vertices the dynamic objects hold this frame.
    pub dynamic_vertices: usize,
    /// Bytes resident in world, prop, dynamic, character and decal vertex buffers.
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
