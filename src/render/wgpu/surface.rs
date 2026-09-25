//! SDL window integration and surface policy for the wgpu backend.
//!
//! Everything Stage 4 decides *before* a device exists lives here: which native
//! backend the build targets, how the SDL window becomes a `wgpu::Surface`, and
//! the surface-format, present-mode, alpha-mode and depth-format policies. The
//! renderer in `super::renderer` owns the live objects and the frame lifecycle.
//!
//! See `docs/WGPU_BOOTSTRAP.md` for the policy rationale.

use sdl3::video::Window;

/// The one native desktop backend this build targets.
///
/// On a platform Places does not ship a desktop build for, this is
/// [`wgpu::Backend::Noop`] and [`native_backend_is_compiled`] is false, so the
/// renderer reports an actionable error instead of panicking. The compiled wgpu
/// features match the real backends exactly (`metal`, `vulkan` or `dx12` in
/// `Cargo.toml`), so the instance cannot silently fall back to GL/GLES.
#[cfg(target_os = "macos")]
pub const NATIVE_BACKEND: wgpu::Backend = wgpu::Backend::Metal;
#[cfg(target_os = "linux")]
pub const NATIVE_BACKEND: wgpu::Backend = wgpu::Backend::Vulkan;
#[cfg(target_os = "windows")]
pub const NATIVE_BACKEND: wgpu::Backend = wgpu::Backend::Dx12;
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub const NATIVE_BACKEND: wgpu::Backend = wgpu::Backend::Noop;

/// The instance backend mask: exactly [`NATIVE_BACKEND`], never a set.
#[must_use]
pub const fn native_backends() -> wgpu::Backends {
    match NATIVE_BACKEND {
        wgpu::Backend::Metal => wgpu::Backends::METAL,
        wgpu::Backend::Vulkan => wgpu::Backends::VULKAN,
        wgpu::Backend::Dx12 => wgpu::Backends::DX12,
        wgpu::Backend::Noop | wgpu::Backend::Gl | wgpu::Backend::BrowserWebGpu => {
            wgpu::Backends::empty()
        }
    }
}

/// True when this build has a compiled backend for the target platform.
///
/// Checked before `Instance::new`, which panics without one.
#[must_use]
pub const fn native_backend_is_compiled() -> bool {
    let backends = native_backends();
    !backends.is_empty() && wgpu::Instance::enabled_backend_features().contains(backends)
}

/// A short platform name for diagnostics: `Metal`, `Vulkan` or `Direct3D 12`.
#[must_use]
pub const fn native_backend_label() -> &'static str {
    match NATIVE_BACKEND {
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Dx12 => "Direct3D 12",
        wgpu::Backend::Noop | wgpu::Backend::Gl | wgpu::Backend::BrowserWebGpu => "unsupported",
    }
}

/// Creates the surface for a created SDL3 window.
///
/// # Safety
///
/// The returned `Surface` holds copies of the SDL3 window's raw OS handles and
/// must not outlive that window. `main` upholds this invariant by construction:
/// the renderer is a local declared after the window, so it is dropped before
/// the window on every path (including error returns), and no frame is rendered
/// after the window is destroyed.
///
/// # Errors
///
/// Returns a message when SDL cannot report a raw window handle or wgpu refuses
/// the platform surface.
pub unsafe fn create(
    instance: &wgpu::Instance,
    window: &Window,
) -> Result<wgpu::Surface<'static>, String> {
    // SAFETY: `window` is a live SDL window; the handle copies are used only
    // to describe the surface and are valid for as long as `window` is, which
    // the caller guarantees encloses the surface's lifetime. Both handles are
    // passed because Vulkan on Linux requires the display connection
    // (`Wayland`/`Xlib`/`Xcb`) alongside the window.
    let target = unsafe { wgpu::SurfaceTargetUnsafe::from_display_and_window(window, window) }
        .map_err(|error| format!("SDL window has no raw-window-handle: {error}"))?;
    // SAFETY: the target's handles point at the live SDL window, and the caller
    // guarantees the window outlives every use of the returned surface.
    unsafe { instance.create_surface_unsafe(target) }
        .map_err(|error| format!("wgpu could not create a surface for the SDL window: {error}"))
}

/// Presentation formats the wgpu backend prefers, most preferred first.
///
/// Places presents 8-bit colour everywhere; an sRGB surface format preserves
/// the current presentation intent (values reaching the display are treated as
/// sRGB, exactly like the OpenGL framebuffer). The list is short and explicit
/// so later stages know what Stage 4 established.
const PREFERRED_SURFACE_FORMATS: [wgpu::TextureFormat; 2] = [
    wgpu::TextureFormat::Bgra8UnormSrgb,
    wgpu::TextureFormat::Rgba8UnormSrgb,
];

/// Chooses the surface format from the adapter's capabilities.
///
/// The first preferred sRGB format the surface supports wins; when neither is
/// supported the first format the surface reports is used so the backend is
/// never blocked on a hard-coded format. `None` means the surface is
/// incompatible with the adapter.
///
/// A non-sRGB fallback cannot honour the display-space contract Stage 7/8's
/// fragment stage is built on (it writes linear values expecting the hardware
/// to encode them), so [`surface_format_is_srgb`] reports it and the renderer
/// logs one warning instead of presenting silently wrong colours.
#[must_use]
pub fn select_surface_format(
    capabilities: &wgpu::SurfaceCapabilities,
) -> Option<wgpu::TextureFormat> {
    PREFERRED_SURFACE_FORMATS
        .iter()
        .copied()
        .find(|format| capabilities.formats.contains(format))
        .or_else(|| capabilities.formats.first().copied())
}

/// True when a surface format stores sRGB-encoded values.
///
/// The world shader's display-space assembly assumes the *surface* decodes the
/// shader's output: the surface-facing entry points convert once with
/// `srgb_to_linear` and rely on the hardware encode. (The capture read-back no
/// longer depends on this: since Stage 10 the post path copies the raw
/// presented image into a raw capture texture.) An adapter that offers only a
/// linear format cannot present the reference's display values without a shader
/// variant, which no stage has built.
#[must_use]
pub fn surface_format_is_srgb(format: wgpu::TextureFormat) -> bool {
    format.is_srgb()
}

/// Presentation modes the wgpu backend uses, most preferred first.
///
/// Both lists keep the synchronized FIFO path reachable; only a player who
/// explicitly switched `VSync` off may reach `Immediate`.
const VSYNC_PRESENT_MODES: [wgpu::PresentMode; 2] =
    [wgpu::PresentMode::Fifo, wgpu::PresentMode::FifoRelaxed];
const IMMEDIATE_PRESENT_MODES: [wgpu::PresentMode; 2] =
    [wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo];

/// Chooses the presentation mode from the adapter's capabilities.
///
/// With `VSync` on (the shipped default) the portable FIFO path is used. With
/// `VSync` explicitly off, `Immediate` is used when the surface supports it;
/// otherwise the synchronized path remains. The fallback is the first mode the
/// surface reports, and `Fifo` when it reports none.
#[must_use]
pub fn select_present_mode(
    capabilities: &wgpu::SurfaceCapabilities,
    vsync: bool,
) -> wgpu::PresentMode {
    let preference = if vsync {
        VSYNC_PRESENT_MODES
    } else {
        IMMEDIATE_PRESENT_MODES
    };
    preference
        .iter()
        .copied()
        .find(|mode| capabilities.present_modes.contains(mode))
        .or_else(|| capabilities.present_modes.first().copied())
        .unwrap_or(wgpu::PresentMode::Fifo)
}

/// Chooses the alpha/composite mode: an opaque window unless the platform only
/// offers something else.
#[must_use]
pub fn select_alpha_mode(capabilities: &wgpu::SurfaceCapabilities) -> wgpu::CompositeAlphaMode {
    if capabilities
        .alpha_modes
        .contains(&wgpu::CompositeAlphaMode::Opaque)
    {
        wgpu::CompositeAlphaMode::Opaque
    } else {
        capabilities
            .alpha_modes
            .first()
            .copied()
            .unwrap_or(wgpu::CompositeAlphaMode::Auto)
    }
}

/// A short presentation-mode name for diagnostics.
#[must_use]
pub const fn present_mode_label(mode: wgpu::PresentMode) -> &'static str {
    match mode {
        wgpu::PresentMode::Fifo => "Fifo",
        wgpu::PresentMode::FifoRelaxed => "FifoRelaxed",
        wgpu::PresentMode::Immediate => "Immediate",
        wgpu::PresentMode::Mailbox => "Mailbox",
        wgpu::PresentMode::AutoVsync => "AutoVsync",
        wgpu::PresentMode::AutoNoVsync => "AutoNoVsync",
    }
}

/// The single main depth format the Stage 4 backend establishes.
///
/// `Depth32Float` is renderable on Metal, Vulkan and Direct3D 12 and needs no
/// optional feature. Stage 5+ reuses this constant for the world's depth
/// testing and for the probe/planar capture depths; there are still no shadow
/// maps (the reference has none) and no separate depth format anywhere.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// What the clear pass writes into a **raw** (non-sRGB) colour target.
///
/// The reference renderer clears its non-sRGB framebuffer to the raw display
/// value `(0.08, 0.08, 0.09)` (the preserved reference renderer) and then composites,
/// blends, grades and reads that value back in display space. Every wgpu
/// offscreen colour target is raw `Rgba8Unorm` for exactly that reason, so the
/// scene, planar and probe clears write the raw value too — the same bytes the
/// reference's targets hold. Stage 9 left these targets on
/// [`CLEAR_COLOR_SRGB`], which the canonical `pool_entry` and
/// `wet_deck_shallow` views showed as a clear-colour-shaped hole; Stage 10
/// corrected it.
///
/// The world still has no background, sky or post-processing; a later stage
/// replaces this with the world's own background if one is ever authored.
pub const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.08,
    g: 0.08,
    b: 0.09,
    a: 1.0,
};

/// The same background for an **sRGB** surface target (the direct fallback and
/// the capture copy).
///
/// A written value on an sRGB attachment is linear and the hardware encodes
/// it, so presenting the reference's raw `(0.08, 0.08, 0.09)` background
/// requires the linear form (`srgb_to_linear(0.08) = 0.0071944`,
/// `srgb_to_linear(0.09) = 0.0085404`, IEC 61966-2-1). Stage 8 converted the
/// Stage 4 dark neutral to this value while all targets were sRGB; only the
/// surface-format paths use it now.
pub const CLEAR_COLOR_SRGB: wgpu::Color = wgpu::Color {
    r: 0.007_194_4,
    g: 0.007_194_4,
    b: 0.008_540_4,
    a: 1.0,
};

/// The reference's raw display-space clear colour, for the conversion test.
#[cfg(test)]
pub const REFERENCE_CLEAR_DISPLAY: [f64; 3] = [0.08, 0.08, 0.09];

/// What one surface acquisition attempt observed when it returned no texture.
///
/// A renderer-neutral mirror of the failure half of
/// `wgpu::CurrentSurfaceTexture`, so the recovery policy can be reviewed and
/// tested without a GPU. A successful or suboptimal acquisition yields a
/// texture and never reaches this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceStatus {
    /// Acquisition timed out.
    Timeout,
    /// The window is occluded (typically minimized or behind another window).
    Occluded,
    /// The stored surface configuration is stale.
    Outdated,
    /// The surface itself was lost and must be recreated.
    Lost,
    /// wgpu raised a validation error while acquiring.
    Validation,
}

/// What the frame loop should do after one [`SurfaceStatus`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceRecovery {
    /// Skip this frame and try again on the next one.
    Skip,
    /// Reconfigure the surface once, then retry the acquisition once.
    Reconfigure,
    /// Recreate the surface once, then resume on the next frame.
    Recreate,
    /// The surface cannot recover; stop the renderer with a diagnostic.
    Fatal,
}

impl SurfaceStatus {
    /// The Stage 4 recovery policy for this observation.
    #[must_use]
    pub const fn recovery(self) -> SurfaceRecovery {
        match self {
            Self::Timeout | Self::Occluded => SurfaceRecovery::Skip,
            Self::Outdated => SurfaceRecovery::Reconfigure,
            Self::Lost => SurfaceRecovery::Recreate,
            Self::Validation => SurfaceRecovery::Fatal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(
        formats: &[wgpu::TextureFormat],
        present_modes: &[wgpu::PresentMode],
    ) -> wgpu::SurfaceCapabilities {
        wgpu::SurfaceCapabilities {
            formats: formats.to_vec(),
            present_modes: present_modes.to_vec(),
            alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
            ..Default::default()
        }
    }

    #[test]
    fn the_native_backend_is_the_one_this_platform_ships() {
        #[cfg(target_os = "macos")]
        assert_eq!(NATIVE_BACKEND, wgpu::Backend::Metal);
        #[cfg(target_os = "linux")]
        assert_eq!(NATIVE_BACKEND, wgpu::Backend::Vulkan);
        #[cfg(target_os = "windows")]
        assert_eq!(NATIVE_BACKEND, wgpu::Backend::Dx12);
        assert!(
            !native_backends().contains(wgpu::Backends::GL),
            "the wgpu path must never request the GL backend"
        );
    }

    #[test]
    fn the_preferred_srgb_format_wins_when_supported() {
        let caps = capabilities(
            &[
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Bgra8UnormSrgb,
            ],
            &[wgpu::PresentMode::Fifo],
        );
        assert_eq!(
            select_surface_format(&caps),
            Some(wgpu::TextureFormat::Bgra8UnormSrgb)
        );
    }

    #[test]
    fn the_second_preference_is_used_without_the_first() {
        let caps = capabilities(
            &[
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ],
            &[wgpu::PresentMode::Fifo],
        );
        assert_eq!(
            select_surface_format(&caps),
            Some(wgpu::TextureFormat::Rgba8UnormSrgb)
        );
    }

    #[test]
    fn a_surface_without_an_srgb_format_falls_back_to_what_it_reports() {
        let caps = capabilities(
            &[wgpu::TextureFormat::Rgba8Unorm],
            &[wgpu::PresentMode::Fifo],
        );
        assert_eq!(
            select_surface_format(&caps),
            Some(wgpu::TextureFormat::Rgba8Unorm)
        );
        assert_eq!(select_surface_format(&capabilities(&[], &[])), None);
    }

    #[test]
    fn vsync_selects_the_synchronized_path() {
        let caps = capabilities(
            &[wgpu::TextureFormat::Bgra8UnormSrgb],
            &[wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo],
        );
        assert_eq!(select_present_mode(&caps, true), wgpu::PresentMode::Fifo);
        assert_eq!(
            select_present_mode(&caps, false),
            wgpu::PresentMode::Immediate
        );
    }

    #[test]
    fn a_surface_without_immediate_keeps_the_synchronized_path() {
        let caps = capabilities(
            &[wgpu::TextureFormat::Bgra8UnormSrgb],
            &[wgpu::PresentMode::Fifo],
        );
        assert_eq!(select_present_mode(&caps, false), wgpu::PresentMode::Fifo);
        assert_eq!(select_present_mode(&caps, true), wgpu::PresentMode::Fifo);
    }

    #[test]
    fn the_clear_colours_are_the_reference_display_value_in_both_spaces() {
        // One channel of the IEC 61966-2-1 decode, the shader's `srgb_to_linear`
        // and the rule an sRGB clear attachment follows.
        fn srgb_to_linear(value: f64) -> f64 {
            if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }
        // Raw targets take the reference's own display value byte for byte.
        let raw = [CLEAR_COLOR.r, CLEAR_COLOR.g, CLEAR_COLOR.b];
        for (actual, display) in raw.iter().zip(REFERENCE_CLEAR_DISPLAY.iter()) {
            assert!(
                (actual - display).abs() < f64::EPSILON,
                "raw clear {actual} must be the reference display value {display}"
            );
        }
        // The sRGB surface takes the linear form so the hardware encode lands
        // back on the same display value.
        let linear = [CLEAR_COLOR_SRGB.r, CLEAR_COLOR_SRGB.g, CLEAR_COLOR_SRGB.b];
        for (actual, display) in linear.iter().zip(REFERENCE_CLEAR_DISPLAY.iter()) {
            let expected = srgb_to_linear(*display);
            assert!(
                (actual - expected).abs() < 1.0e-6,
                "surface clear {actual} must be srgb_to_linear({display}) = {expected}"
            );
        }
        assert!((CLEAR_COLOR.a - 1.0).abs() < f64::EPSILON);
        assert!((CLEAR_COLOR_SRGB.a - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn an_opaque_surface_is_preferred_with_a_defined_fallback() {
        let caps = capabilities(&[wgpu::TextureFormat::Bgra8UnormSrgb], &[]);
        assert_eq!(select_alpha_mode(&caps), wgpu::CompositeAlphaMode::Opaque);
        let inherit_only = wgpu::SurfaceCapabilities {
            alpha_modes: vec![wgpu::CompositeAlphaMode::Inherit],
            ..Default::default()
        };
        assert_eq!(
            select_alpha_mode(&inherit_only),
            wgpu::CompositeAlphaMode::Inherit
        );
    }

    #[test]
    fn the_depth_format_really_is_a_depth_format() {
        assert!(DEPTH_FORMAT.has_depth_aspect());
        assert!(!DEPTH_FORMAT.has_color_aspect());
    }

    #[test]
    fn the_recovery_policy_covers_every_surface_status() {
        use SurfaceRecovery::*;
        for (status, expected) in [
            (SurfaceStatus::Timeout, Skip),
            (SurfaceStatus::Occluded, Skip),
            (SurfaceStatus::Outdated, Reconfigure),
            (SurfaceStatus::Lost, Recreate),
            (SurfaceStatus::Validation, Fatal),
        ] {
            assert_eq!(status.recovery(), expected, "{status:?}");
        }
    }
}
