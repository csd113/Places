//! The wgpu backend: SDL surface integration, device lifecycle, and the
//! complete Places renderer.
//!
//! Stages 0-3 built the renderer-neutral boundary and kept the OpenGL
//! implementation behind it. Stage 4 added the wgpu instance, surface, adapter,
//! device, queue, surface configuration and main depth target. Stage 5 added
//! the static Places world, Stage 6 the texture system, Stage 7 the material
//! system and Stage 8 the baked lighting and its display-space sheen. Stage 9
//! completed the feature migration:
//!
//! * the lightmap atlas (the reference's normal/default baked-light path),
//!   with the neutral bake and its content-keyed disk cache shared with the
//!   OpenGL path ([`lightmap`]);
//! * static reflection probes and the half-size planar mirror
//!   ([`reflections`]), the group-3 environment binding that carries the
//!   lightmaps, probe, planar image, fog and planar projection
//!   ([`environment`]);
//! * props/GLB models ([`props`]) and dynamic objects with their per-object
//!   baked-light probes ([`dynamic`]);
//! * fixture geometry and emission through the world draw set;
//! * the offscreen scene target, emissive bloom pass, two separable blurs and
//!   the resolve grade plus the plain present copy ([`postprocess`]);
//! * decals with the reference's depth bias ([`decals`]);
//! * fog inside the world shader, and the 480x272 renderer-owned HUD
//!   ([`ui`]).
//!
//! Colour space: every offscreen target is raw `Rgba8Unorm` and the shaders
//! write the reference's display-space values directly, so blending, filtering
//! and sampling match the OpenGL reference exactly; only the sRGB surface is
//! converted, once, at the final copy. See `docs/WGPU_STAGE9.md`.
//!
//! Module list:
//!
//! * [`lightmap`] — the baked atlas as raw RGBA8 textures;
//! * [`environment`] — group 3: lightmaps, reflections, fog, planar mirror;
//! * [`reflections`] — probe cubemaps, the planar target and capture maths;
//! * [`props`] — the neutral prop batches as GPU buffers and materials;
//! * [`dynamic`] — the dynamic-object meshes and per-object environments;
//! * [`decals`] — the decal sheets, geometry and pass;
//! * [`postprocess`] — scene/presented/emissive/blur targets and the resolve;
//! * [`ui`] — the renderer-owned HUD pass;
//! * [`world`] — the world vertex layout, pipelines and frame encode;
//! * [`material`] — the resolved material GPU records;
//! * [`texture`] — the texture cache, samplers and fallback sheet;
//! * [`surface`] — SDL raw-window-handle surface creation and recovery.

pub mod decals;
pub mod dynamic;
pub mod environment;
pub mod lightmap;
pub mod material;
pub mod postprocess;
pub mod props;
pub mod reflections;
pub mod renderer;
pub mod surface;
pub mod texture;
pub mod ui;
pub mod world;

pub use renderer::WgpuRenderer;
