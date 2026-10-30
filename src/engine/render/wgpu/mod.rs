//! The wgpu backend: SDL3 surface integration, device lifecycle, and the
//! complete Places renderer.
//!
//! It owns the wgpu instance, surface, adapter, device, queue, surface
//! configuration and main depth target, and every part of the frame:
//!
//! * the static Places world, textured through the base-colour, material and
//!   baked-lighting paths; the world vertex layout, pipelines and frame encode
//!   live in [`world`], the material records in [`material`] and the texture
//!   cache in [`texture`];
//! * the lightmap atlas,
//!   with the neutral bake and its content-keyed disk cache ([`lightmap`]);
//! * static reflection probes and the half-size planar mirror
//!   ([`reflections`]), the group-3 environment binding that carries the
//!   lightmaps, probe, planar image, fog and planar projection
//!   ([`environment`]);
//! * props/GLB models ([`props`]), dynamic objects with their per-object
//!   baked-light probes ([`dynamic`]) and skinned characters with
//!   per-character deformed vertex buffers ([`character`]);
//! * fixture geometry and emission through the world draw set;
//! * the offscreen scene target, emissive bloom pass, two separable blurs and
//!   the resolve grade plus the plain present copy ([`postprocess`]);
//! * depth-biased decals ([`decals`]);
//! * fog inside the world shader, and the 480x272 renderer-owned HUD
//!   ([`ui`]).
//!
//! Colour space: scene, emission, bloom and reflection targets hold linear HDR
//! in `Rgba16Float`. Resolve encodes the final display values into the
//! `Rgba8Unorm` presented target, where the HUD blends. The final surface copy
//! preserves those display values on either sRGB or raw surfaces. See
//! `docs/RENDERER.md`.
//!
//! Module list:
//!
//! * [`lightmap`] — the prepared HDR atlas as linear `Rgba16Float` layer pairs;
//! * [`environment`] — group 3: lightmaps, reflections, fog, planar mirror;
//! * [`reflections`] — probe cubemaps, the planar target and capture maths;
//! * [`props`] — the neutral prop batches as GPU buffers and materials;
//! * [`dynamic`] — the dynamic-object meshes and per-object environments;
//! * [`effects`] — the ambient effect billboards (steam plumes);
//! * [`decals`] — the decal sheets, geometry and pass;
//! * [`postprocess`] — scene/presented/emissive/blur targets and the resolve;
//! * [`ui`] — the renderer-owned HUD pass;
//! * [`world`] — the world vertex layout, pipelines and frame encode;
//! * [`material`] — the resolved material GPU records;
//! * [`texture`] — the texture cache, samplers and fallback sheet;
//! * [`surface`] — SDL raw-window-handle surface creation and recovery.

pub mod caustics;
pub mod character;
mod character_transparency;
pub mod decals;
#[cfg(feature = "visual-diagnostics")]
pub mod diagnostics;

#[cfg(feature = "visual-diagnostics")]
fn diagnostic_model_hidden(model: &str) -> bool {
    diagnostics::model_hidden(model)
}

#[cfg(not(feature = "visual-diagnostics"))]
const fn diagnostic_model_hidden(_model: &str) -> bool {
    false
}
pub mod dynamic;
pub mod effects;
pub mod environment;
mod flashlight;
mod gpu_timing;
pub mod lightmap;
pub mod material;
pub mod postprocess;
#[cfg(feature = "visual-diagnostics")]
mod probe_visualization;
pub mod prop_instances;
pub mod props;
mod readback;
pub mod reflections;
pub mod renderer;
mod skinning;
pub mod sky;
pub mod surface;
pub mod texture;
pub mod ui;
mod uv_receivers;
pub mod water;
pub mod world;

pub use renderer::WgpuRenderer;

/// Byte size of one GPU record in a const buffer layout.
#[expect(
    clippy::as_conversions,
    reason = "Places' desktop targets use at most 64-bit usize, so a GPU record's size converts exactly to BufferAddress during const evaluation"
)]
pub(super) const fn buffer_element_bytes<T>() -> wgpu::BufferAddress {
    std::mem::size_of::<T>() as wgpu::BufferAddress
}
