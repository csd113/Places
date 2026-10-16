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

/// Five scalar cases exercise the failed-retry transition without a GPU;
/// loss and validation must survive the one-reconfiguration limit.
#[test]
fn a_failed_reconfigure_retry_keeps_loss_and_validation_actionable() {
    for (status, expected) in [
        (SurfaceStatus::Timeout, SurfaceRecovery::Skip),
        (SurfaceStatus::Occluded, SurfaceRecovery::Skip),
        (SurfaceStatus::Outdated, SurfaceRecovery::Skip),
        (SurfaceStatus::Lost, SurfaceRecovery::Recreate),
        (SurfaceStatus::Validation, SurfaceRecovery::Fatal),
    ] {
        assert_eq!(status.recovery_after_reconfigure(), expected, "{status:?}");
    }
}

/// Tiny capability lists reproduce recovery with a retained colour format
/// but changed alpha/presentation modes, and an incompatible replacement.
#[test]
fn recreation_refreshes_all_capability_settings_before_configuration() {
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: 1,
        height: 1,
        present_mode: wgpu::PresentMode::Immediate,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        view_formats: Vec::new(),
        desired_maximum_frame_latency: 2,
    };
    let mut replacement = capabilities(
        &[wgpu::TextureFormat::Rgba8UnormSrgb],
        &[wgpu::PresentMode::Fifo],
    );
    replacement.alpha_modes = vec![wgpu::CompositeAlphaMode::Inherit];
    assert!(refresh_configuration(&mut config, &replacement, false).is_ok());
    assert_eq!(config.format, wgpu::TextureFormat::Rgba8UnormSrgb);
    assert_eq!(config.present_mode, wgpu::PresentMode::Fifo);
    assert_eq!(config.alpha_mode, wgpu::CompositeAlphaMode::Inherit);

    replacement.formats = vec![wgpu::TextureFormat::Bgra8UnormSrgb];
    assert!(refresh_configuration(&mut config, &replacement, true).is_ok());
    assert_eq!(config.format, wgpu::TextureFormat::Bgra8UnormSrgb);
    replacement.formats.clear();
    assert!(refresh_configuration(&mut config, &replacement, true).is_err());
}
