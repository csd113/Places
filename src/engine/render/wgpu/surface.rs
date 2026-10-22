//! SDL window integration and surface policy for the wgpu backend.
//!
//! Everything the backend decides *before* a device exists lives here: which
//! native backend the build targets, how the SDL window becomes a
//! `wgpu::Surface`, and the surface-format, present-mode, alpha-mode and
//! depth-format policies. The renderer in `super::renderer` owns the live
//! objects and the frame lifecycle.
//!
//! See `docs/RENDERER.md` section 1 for the policy rationale.

use sdl3::video::Window;

/// Direct non-sRGB 8-bit surfaces need a shader display encode. HDR targets
/// retain linear energy; sRGB surfaces encode in hardware.
pub const fn target_is_raw_display(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
    )
}

pub fn color_target_constants(format: wgpu::TextureFormat) -> [(&'static str, f64); 1] {
    [(
        "display_target",
        f64::from(u8::from(target_is_raw_display(format))),
    )]
}

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

/// Prefer the discrete adapter on hybrid Windows laptops; other hosts retain
/// wgpu's default adapter policy. This does not change any OS GPU setting.
pub const fn native_power_preference() -> wgpu::PowerPreference {
    if cfg!(target_os = "windows") {
        wgpu::PowerPreference::HighPerformance
    } else {
        wgpu::PowerPreference::None
    }
}

/// FXC cannot compile the world's bounded uniform-array loops. Require the
/// matching local DXC runtime instead of silently falling back and panicking.
#[cfg(target_os = "windows")]
pub fn configure_dx12_compiler(descriptor: &mut wgpu::InstanceDescriptor) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let compiler = executable.with_file_name("dxcompiler.dll");
    let validator = executable.with_file_name("dxil.dll");
    if !compiler.is_file() || !validator.is_file() {
        return Err("Windows rendering requires matching dxcompiler.dll (DXC 1.8.2502 or newer) and dxil.dll beside the executable; restore the complete Places runtime payload".into());
    }
    descriptor.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::DynamicDxc {
        dxc_path: compiler
            .to_str()
            .ok_or("DXC runtime path is not valid UTF-8")?
            .to_string(),
    };
    crate::engine::logging::info(format!(
        "[wgpu] Windows shader compiler: DXC ({})",
        compiler.display()
    ));
    Ok(())
}

/// Creates the surface for a created SDL3 window.
///
/// # Safety
///
/// The returned `Surface` holds copies of the SDL3 window's raw OS handles and
/// must not outlive that window. The renderer retains a clone of the exact SDL
/// window that supplied the handles in its final field, so the surface and any
/// pending frame drop before that shared window context. Surface recovery uses
/// the retained window and preserves the same ordering.
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
/// sRGB, exactly like the reference framebuffer). The list is short and
/// explicit so the presentation policy is reviewable in one place.
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
/// Raw 8-bit fallback surfaces receive software-encoded RGB.
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
/// Linear world output relies on hardware encoding for these formats.
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

/// Refreshes every capability-dependent setting after recreating a surface.
///
/// A supported format is retained so cached pipelines stay usable. The new
/// surface may expose different presentation or alpha modes even when its
/// colour format is unchanged.
///
/// # Errors
///
/// Returns an error when the new surface has no compatible colour format.
pub fn refresh_configuration(
    config: &mut wgpu::SurfaceConfiguration,
    capabilities: &wgpu::SurfaceCapabilities,
    vsync: bool,
) -> Result<(), String> {
    let format = if capabilities.formats.contains(&config.format) {
        config.format
    } else {
        select_surface_format(capabilities)
            .ok_or_else(|| "the recreated surface has no compatible colour format".to_string())?
    };
    config.format = format;
    config.present_mode = select_present_mode(capabilities, vsync);
    config.alpha_mode = select_alpha_mode(capabilities);
    Ok(())
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

/// The single main depth format the backend establishes.
///
/// `Depth32Float` is renderable on Metal, Vulkan and Direct3D 12 and needs no
/// optional feature. The same constant serves the world's depth testing and
/// the probe/planar capture depths and handheld spotlight visibility. There
/// is no separate depth format anywhere.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Linear form of the authored background sRGB `(0.08, 0.08, 0.09)`.
pub const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.007_194_4,
    g: 0.007_194_4,
    b: 0.008_540_382,
    a: 1.0,
};

/// Linear clear for an sRGB surface; hardware encodes it once.
pub const CLEAR_COLOR_SRGB: wgpu::Color = wgpu::Color {
    r: 0.007_194_4,
    g: 0.007_194_4,
    b: 0.008_540_382,
    a: 1.0,
};

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
    /// The recovery policy for this observation.
    #[must_use]
    pub const fn recovery(self) -> SurfaceRecovery {
        match self {
            Self::Timeout | Self::Occluded => SurfaceRecovery::Skip,
            Self::Outdated => SurfaceRecovery::Reconfigure,
            Self::Lost => SurfaceRecovery::Recreate,
            Self::Validation => SurfaceRecovery::Fatal,
        }
    }

    /// A retry must not reconfigure again, but loss and validation still need
    /// their ordinary recovery. Flattening all retry failures to `Skip` can
    /// leave a lost surface resident or conceal a fatal validation result.
    #[must_use]
    pub const fn recovery_after_reconfigure(self) -> SurfaceRecovery {
        match self {
            Self::Outdated => SurfaceRecovery::Skip,
            Self::Timeout | Self::Occluded | Self::Lost | Self::Validation => self.recovery(),
        }
    }
}
