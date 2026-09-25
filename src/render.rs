//! Rendering: a renderer-neutral preparation layer behind a narrow facade.
//!
//! The module is split into a neutral half and the wgpu backend:
//!
//! - [`common`] — renderer-neutral engine preparation: level geometry emitters,
//!   meshes, materials-to-draw-state resolution, reflection routing and mirror
//!   maths, fog, emission animation, the frame/camera description and the view
//!   maths. Nothing here names a GPU type; it is the data the renderer draws.
//! - [`facade`] — [`Renderer`], the one rendering type the engine names. It
//!   owns the wgpu renderer and exposes the engine's operations; the engine
//!   never imports a backend module.
//! - [`wgpu`] — the wgpu implementation: the instance, surface, adapter,
//!   device, queue and depth target, plus every rendering feature of the
//!   historical reference renderer — the static world, the texture and material
//!   systems, the baked lightmap atlas, reflection probes and the planar
//!   mirror, props and dynamic objects, fixture emission, decals, fog, the
//!   offscreen bloom chain and resolve, and the HUD. It consumes the
//!   renderer-neutral `common` data; see `docs/RENDERER.md`.
//!
//! The rest of the engine talks to rendering through the [`Renderer`] facade
//! re-exported here; it never imports a backend module. See
//! `docs/ARCHITECTURE.md` for the ownership rules and dependency direction
//! this split enforces. The deleted OpenGL/GLES2 reference renderer is
//! preserved at the `renderer-gles2-reference` tag; see
//! `docs/RENDERER_REFERENCE.md`.

#[cfg(test)]
mod boundary_tests;
mod common;
mod facade;
#[cfg(test)]
mod tests;
mod wgpu;

#[cfg(test)]
pub(in crate::render) use common::WallUnit;
pub use common::camera::RenderCamera;
pub use common::stats::{LevelBuildStats, RenderStats};
pub use common::view::{
    DrawableSize, UI_REFERENCE_HEIGHT, UI_REFERENCE_WIDTH, UiViewport, reference_aspect_ratio,
    vertical_fov_for_aspect,
};
#[cfg(test)]
pub(in crate::render) use common::wall_units;
pub use common::{
    AnimationEffect, BatchRange, BuildTimings, DECAL_ALPHA_CUTOFF, DECAL_EXTERNAL_BASE,
    DECAL_MATERIALS, DECAL_POLYGON_OFFSET, DECAL_SURFACE_OFFSET_M, DECAL_TEST_MATERIAL,
    DEMO_DRUM_ID, DEMO_MACHINE_ID, DEMO_SPIN_DEGREES_PER_SECOND, DynamicId, DynamicMesh,
    DynamicObject, DynamicScene, DynamicSubmesh, DynamicUpdate, EmissionAnimation,
    GraphicsTransition, LIGHTMAP_NONE, LevelBuild, LevelMesh, LevelMeshBatches, LevelMeshRange,
    LightmapBuildOptions, MATERIAL_NONE, MAX_ANIMATION_DEPTH, MAX_DYNAMIC_MESHES,
    MAX_DYNAMIC_OBJECTS, MAX_FLICKER_HZ, MAX_PULSE_HZ, MaterialIndex, MaterialSlot,
    PROBE_EPSILON_M, PropMeshBatch, SurfaceKey, SurfaceKind, SurfaceShine, Vertex,
    build_level_geometry, build_level_geometry_timed, build_level_geometry_timed_with_lightmaps,
    build_level_geometry_with_assets, build_level_geometry_with_assets_and_lighting,
    build_level_geometry_with_assets_and_lighting_and_materials, build_level_geometry_with_catalog,
    build_level_geometry_with_catalog_and_materials, build_level_geometry_with_materials,
    decal_external_sheet_ids, decal_material_slot, decal_quad_points, decal_sheet_index,
    decal_uv_rect, decal_uv_rect_full, dequantize_unit, logical_materials, spatial_cell_grid,
    tiled_uv,
};
#[cfg(test)]
pub(crate) use common::{MaterialLookup, SCENE_FAR_M, SCENE_NEAR_M, WallMaterialRun};
pub use facade::Renderer;

// The surface creation and ownership contract lives in `render::wgpu::surface`:
// the raw-window-handle implementation reports the window's content view and
// wgpu attaches its own Metal layer, so no backend-specific window flag has to
// be applied before the window is built.
