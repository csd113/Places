//! Rendering: a renderer-neutral preparation layer and two backend
//! implementations behind one facade.
//!
//! The module is split into a neutral half, a facade, and the backends:
//!
//! - [`common`] — renderer-neutral engine preparation: level geometry emitters,
//!   meshes, materials-to-draw-state resolution, reflection routing and mirror
//!   maths, fog, emission animation, the frame/camera description and the view
//!   maths. Nothing here names a GPU type; it is the data the renderers draw.
//! - [`facade`] — [`Renderer`], the one rendering type the engine names. It
//!   dispatches to whichever implementation `PLACES_RENDERER` selected at
//!   startup.
//! - [`opengl`] — the OpenGL/GLES2 implementation: programs, textures, buffers,
//!   framebuffers, draws, captures and the GL context. It consumes `common`
//!   data and owns every GL object. This is the complete reference renderer.
//! - [`wgpu`] — the wgpu implementation: the Stage 4 instance, surface,
//!   adapter, device, queue and depth target, plus every rendering feature of
//!   the reference renderer added by Stages 5-9: the static world, the texture
//!   and material systems, the baked lightmap atlas, reflection probes and the
//!   planar mirror, props and dynamic objects, fixture emission, decals, fog,
//!   the offscreen bloom chain and resolve, and the HUD. It consumes the same
//!   `common` data as OpenGL; see `docs/WGPU_STAGE9.md`.
//!
//! The rest of the engine talks to rendering through the [`Renderer`] facade
//! re-exported here; it never imports a backend module. See
//! `docs/RENDERER_BOUNDARY.md` for the ownership rules and dependency
//! direction this split enforces, and `docs/WGPU_BOOTSTRAP.md` for the wgpu
//! bootstrap and the temporary renderer selector.

mod backend;
#[cfg(test)]
mod boundary_tests;
mod common;
mod facade;
mod opengl;
#[cfg(test)]
mod tests;
mod wgpu;

pub use backend::RendererBackend;
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
    AnimationEffect, BatchRange, BuildTimings, DECAL_EXTERNAL_BASE, DECAL_MATERIALS,
    DECAL_SURFACE_OFFSET_M, DECAL_TEST_MATERIAL, DEMO_DRUM_ID, DEMO_MACHINE_ID,
    DEMO_SPIN_DEGREES_PER_SECOND, DynamicId, DynamicMesh, DynamicObject, DynamicScene,
    DynamicSubmesh, DynamicUpdate, EXACT_VERTEX_STRIDE, EmissionAnimation, LIGHTMAP_NONE,
    LevelBuild, LevelMesh, LevelMeshBatches, LevelMeshRange, LightmapBuildOptions, MATERIAL_NONE,
    MAX_ANIMATION_DEPTH, MAX_DYNAMIC_MESHES, MAX_DYNAMIC_OBJECTS, MAX_FLICKER_HZ, MAX_PULSE_HZ,
    MaterialIndex, MaterialSlot, PROBE_EPSILON_M, PackedVertex, PropMeshBatch, StaticBatch,
    SurfaceKey, SurfaceKind, SurfaceShine, Vertex, VertexLayout, build_level_geometry,
    build_level_geometry_timed, build_level_geometry_timed_with_lightmaps,
    build_level_geometry_with_assets, build_level_geometry_with_assets_and_lighting,
    build_level_geometry_with_assets_and_lighting_and_materials, build_level_geometry_with_catalog,
    build_level_geometry_with_catalog_and_materials, build_level_geometry_with_materials,
    decal_external_sheet_ids, decal_material_slot, decal_quad_points, decal_sheet_index,
    decal_uv_rect, decal_uv_rect_full, dequantize_normal, dequantize_unit, exact_layout,
    logical_materials, packed_layout, spatial_cell_grid, tiled_uv,
};
#[cfg(test)]
pub(crate) use common::{MaterialLookup, SCENE_FAR_M, SCENE_NEAR_M, WallMaterialRun};
pub use facade::Renderer;
pub(crate) use opengl::context::{request_fallback_window_attributes, request_window_attributes};
pub use opengl::shaders::{DECAL_ALPHA_CUTOFF, DECAL_POLYGON_OFFSET, fragment_shader_source};

/// Marks the window being built for the selected backend.
///
/// OpenGL needs SDL to know the window will carry a GL context; wgpu needs the
/// macOS metal view for its raw-window-handle path and no OpenGL flag at all.
/// `main` calls this while building the window, before the backend exists.
pub fn apply_window_flags(builder: &mut sdl2::video::WindowBuilder, backend: RendererBackend) {
    match backend {
        RendererBackend::Opengl => opengl::context::apply_window_flags(builder),
        RendererBackend::Wgpu => wgpu::surface::apply_window_flags(builder),
    }
}
