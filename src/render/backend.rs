//! Temporary migration selector for the renderer implementation.
//!
//! Places is migrating from its OpenGL/GLES2 renderer to a wgpu one. During
//! that migration the process can launch either implementation, chosen once at
//! startup by `PLACES_RENDERER`:
//!
//! ```text
//! PLACES_RENDERER=opengl   (default)  the complete reference renderer
//! PLACES_RENDERER=wgpu                the wgpu renderer: device/surface
//!                                     lifecycle plus the Stage 5 static world
//! ```
//!
//! This is a development mechanism, not a player-facing setting: there is no
//! UI switch, no saved preference and no runtime hot-swap. The choice is made
//! before the SDL window is built so the window can carry the flags its backend
//! needs. The wgpu path never falls back to OpenGL (and the OpenGL path never
//! starts wgpu): a failure is reported with the requested backend named.
//!
//! The default stays `opengl` until the wgpu renderer can draw the whole world;
//! see `docs/WGPU_WORLD_GEOMETRY.md`.

/// Environment variable that selects the renderer implementation.
pub const RENDERER_ENV: &str = "PLACES_RENDERER";

/// The renderer implementation a process should initialize.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RendererBackend {
    /// The complete, reference OpenGL/GLES2 renderer.
    #[default]
    Opengl,
    /// The wgpu renderer: device/surface lifecycle plus the Stage 5 static
    /// world geometry.
    Wgpu,
}

impl RendererBackend {
    /// The backend used when `PLACES_RENDERER` is unset or empty.
    pub const DEFAULT: Self = Self::Opengl;

    /// The `PLACES_RENDERER` value that selects this backend.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Opengl => "opengl",
            Self::Wgpu => "wgpu",
        }
    }

    /// True when this backend needs an SDL OpenGL context and window.
    #[must_use]
    pub const fn uses_opengl(self) -> bool {
        matches!(self, Self::Opengl)
    }

    /// Reads the backend selection from the environment.
    ///
    /// An unset or empty variable selects [`RendererBackend::DEFAULT`]; any
    /// other value must name a known backend exactly.
    ///
    /// # Errors
    ///
    /// Returns a message naming the variable, the rejected value and the
    /// accepted values when the selection is not understood.
    pub fn from_env() -> Result<Self, String> {
        match std::env::var(RENDERER_ENV) {
            Ok(value) => parse(value.trim()),
            Err(std::env::VarError::NotPresent) => Ok(Self::DEFAULT),
            Err(std::env::VarError::NotUnicode(_)) => Err(format!(
                "{RENDERER_ENV} is not valid Unicode; expected '{}' or '{}'",
                Self::Opengl.name(),
                Self::Wgpu.name()
            )),
        }
    }
}

/// Parses one `PLACES_RENDERER` value.
///
/// # Errors
///
/// Returns a message naming the rejected value and the accepted ones.
fn parse(value: &str) -> Result<RendererBackend, String> {
    match value.to_ascii_lowercase().as_str() {
        "" => Ok(RendererBackend::DEFAULT),
        "opengl" => Ok(RendererBackend::Opengl),
        "wgpu" => Ok(RendererBackend::Wgpu),
        other => Err(format!(
            "{RENDERER_ENV}: unknown renderer '{other}'; expected '{}' or '{}'",
            RendererBackend::Opengl.name(),
            RendererBackend::Wgpu.name()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{RendererBackend, parse};

    #[test]
    fn the_default_backend_is_the_reference_renderer() {
        assert_eq!(RendererBackend::DEFAULT, RendererBackend::Opengl);
        assert_eq!(parse("").unwrap_or_default(), RendererBackend::Opengl);
    }

    #[test]
    fn both_implementations_are_selectable_by_name() {
        assert_eq!(parse("opengl").unwrap_or_default(), RendererBackend::Opengl);
        assert_eq!(parse("wgpu").unwrap_or_default(), RendererBackend::Wgpu);
        assert_eq!(
            parse(" OpenGL ").unwrap_or_default(),
            RendererBackend::Opengl
        );
        assert_eq!(parse("WGPU").unwrap_or_default(), RendererBackend::Wgpu);
    }

    #[test]
    fn an_unknown_renderer_names_itself_and_the_choices() {
        let error = match parse("vulkan2") {
            Ok(backend) => format!("expected an error, got {backend:?}"),
            Err(error) => error,
        };
        assert!(error.contains("PLACES_RENDERER"), "{error}");
        assert!(error.contains("vulkan2"), "{error}");
        assert!(error.contains("opengl"), "{error}");
        assert!(error.contains("wgpu"), "{error}");
    }

    #[test]
    fn only_the_opengl_backend_owns_an_opengl_context() {
        assert!(RendererBackend::Opengl.uses_opengl());
        assert!(!RendererBackend::Wgpu.uses_opengl());
    }
}
