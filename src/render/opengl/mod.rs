//! The OpenGL/GLES2 implementation of the renderer.
//!
//! Every `glow`/GL object, every GL state change, every shader source and every
//! texture unit lives under this module. It consumes the renderer-neutral data
//! prepared by `crate::render::common` and exposes the `Renderer` facade
//! through `crate::render`.
//!
//! Modules stay `pub` so the integration tests in `crate::render::tests`
//! can pin the GL contract (attribute slots, shader sources, layout constants)
//! while nothing outside the crate — or outside `render` — can name them.

pub mod context;
pub mod framebuffer;
pub mod postprocess;
pub mod reflections;
pub mod renderer;
pub mod shaders;
