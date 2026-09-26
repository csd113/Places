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
//! * the lightmap atlas (the reference's normal/default baked-light path),
//!   with the neutral bake and its content-keyed disk cache ([`lightmap`]);
//! * static reflection probes and the half-size planar mirror
//!   ([`reflections`]), the group-3 environment binding that carries the
//!   lightmaps, probe, planar image, fog and planar projection
//!   ([`environment`]);
//! * props/GLB models ([`props`]), dynamic objects with their per-object
//!   baked-light probes ([`dynamic`]) and skinned characters with
//!   per-character CPU-skinned vertex buffers ([`character`]);
//! * fixture geometry and emission through the world draw set;
//! * the offscreen scene target, emissive bloom pass, two separable blurs and
//!   the resolve grade plus the plain present copy ([`postprocess`]);
//! * decals with the reference's depth bias ([`decals`]);
//! * fog inside the world shader, and the 480x272 renderer-owned HUD
//!   ([`ui`]).
//!
//! Colour space: every offscreen target is raw `Rgba8Unorm` and the shaders
//! write the reference's display-space values directly, so blending, filtering
//! and sampling match the reference exactly; only the sRGB surface is
//! converted, once, at the final copy. See `docs/RENDERER.md`.
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

pub mod character;
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
