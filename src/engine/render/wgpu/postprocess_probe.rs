//! Explicit one-texel native control for the final presentation transfer.
//! Uses the player's existing device and numeric attachment clears; no assets,
//! additional adapter, map preparation, ordinary tests or frame work.

use super::{
    DrawableSize, POST_PRESENT_FRAGMENT_ENTRY, POST_SHADER_SRC, PostProcess, QualityLevel,
    build_pass_pipeline, create_target_texture, encode_fullscreen, surface_present_fragment_entry,
};
use crate::engine::logging;

const DISPLAY_RGBA: [u8; 4] = [128, 64, 191, 255];
const DECODED_RGBA: [u8; 4] = [55, 13, 133, 255];

pub(super) fn run(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), String> {
    logging::event("[present-probe] begin: existing native device, four 1x1 format controls");
    for format in [
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
    ] {
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let measured = measure(device, queue, format);
        let validation_error = pollster::block_on(validation.pop());
        if let Some(error) = validation_error {
            return Err(format!(
                "presentation probe {format:?} validation failed: {error}"
            ));
        }
        let (before, after) =
            measured.map_err(|error| format!("presentation probe {format:?}: {error}"))?;
        let before_expected = if format.is_srgb() {
            DISPLAY_RGBA
        } else {
            DECODED_RGBA
        };
        let before_error = max_byte_error(before, before_expected);
        let after_error = max_byte_error(after, DISPLAY_RGBA);
        let passed = before_error <= 1 && after_error <= 1;
        logging::event(format!(
            "[present-probe] {}",
            serde_json::json!({
                "format": format!("{format:?}"),
                "size": [1_u32, 1_u32],
                "before_entry": POST_PRESENT_FRAGMENT_ENTRY,
                "before_rgba": before,
                "before_expected_rgba": before_expected,
                "before_max_byte_error": before_error,
                "after_entry": surface_present_fragment_entry(format),
                "after_rgba": after,
                "expected_rgba": DISPLAY_RGBA,
                "after_max_byte_error": after_error,
                "tolerance_bytes": 1_u8,
                "passed": passed,
            })
        ));
        if !passed {
            return Err(format!(
                "presentation probe {format:?} byte mismatch: old {before:?}, corrected {after:?}"
            ));
        }
    }
    logging::event(
        "[present-probe] passed: raw fallback corrected; hardware-sRGB output preserved",
    );
    Ok(())
}

fn max_byte_error(actual: [u8; 4], expected: [u8; 4]) -> u8 {
    actual
        .into_iter()
        .zip(expected)
        .map(|(value, reference)| value.abs_diff(reference))
        .max()
        .unwrap_or(0)
}

fn measure(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> Result<([u8; 4], [u8; 4]), String> {
    let mut post = PostProcess::new(device, format);
    let size = DrawableSize::new(1, 1);
    let _targets_created = post.ensure(device, QualityLevel::High, size);
    let targets = post
        .targets
        .as_ref()
        .ok_or_else(|| "presentation probe targets were not created".to_owned())?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("places-present-probe-old-shader"),
        source: wgpu::ShaderSource::Wgsl(POST_SHADER_SRC.into()),
    });
    // Deliberately reproduce the former unconditional surface entry point.
    let old_pipeline = build_pass_pipeline(
        device,
        "places-present-probe-old",
        &shader,
        &post.layouts.present,
        POST_PRESENT_FRAGMENT_ENTRY,
        format,
    );
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
    let (before, before_view) =
        create_target_texture(device, "places-present-probe-before", size, format, usage);
    let (after, after_view) =
        create_target_texture(device, "places-present-probe-after", size, format, usage);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("places-present-probe"),
    });
    {
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("places-present-probe-display-values"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.presented_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.5,
                        g: 0.25,
                        b: 0.75,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    encode_fullscreen(
        &mut encoder,
        "places-present-probe-before",
        &before_view,
        &old_pipeline,
        &targets.presented_present_bind,
        &post.quad_buffer,
        None,
    );
    // Exercise the actual format-selected pipeline, rather than a shader copy.
    post.encode_present_to(&mut encoder, &after_view, None);
    let _submitted = queue.submit([encoder.finish()]);
    Ok((
        read_pixel(device, queue, &before, format)?,
        read_pixel(device, queue, &after, format)?,
    ))
}

fn read_pixel(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
) -> Result<[u8; 4], String> {
    let image = super::super::readback::read_back_rgba(device, queue, texture, 1, 1, format)?;
    image.rgba.as_slice().try_into().map_err(|error| {
        format!("presentation probe readback did not contain exactly one RGBA pixel: {error}")
    })
}
