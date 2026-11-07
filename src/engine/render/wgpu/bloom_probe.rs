//! Explicit numeric GPU acceptance control for complete bloom footprints.
//! Tiny attachments are diagnostic data, not generated artwork or level assets.
use super::{
    BLOOM_FORMAT, BlurParams, DrawableSize, PostProcess, create_target_texture,
    create_uniform_buffer, encode_fullscreen,
};
use crate::engine::logging;

pub(super) fn run(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), String> {
    let post = PostProcess::new(device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let cases =
        (0..8)
            .map(|row| (8, 8, row, row))
            .chain([(7, 5, 4, 6), (13, 9, 8, 12), (1, 1, 0, 0)]);
    for (width, height, row, column) in cases {
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let measured = measure(
            device,
            queue,
            &post,
            DrawableSize::new(width, height),
            row,
            column,
        );
        if let Some(error) = pollster::block_on(validation.pop()) {
            return Err(format!("bloom probe validation: {error}"));
        }
        measured?;
    }
    logging::event(
        "[bloom-probe] passed: all eight row/column phases, odd tails, constant energy and 1x1",
    );
    Ok(())
}

fn measure(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    post: &PostProcess,
    source_size: DrawableSize,
    hot_row: u32,
    hot_column: u32,
) -> Result<(), String> {
    let size = super::bloom_target_size(source_size);
    let sampled = wgpu::TextureUsages::TEXTURE_BINDING;
    let (source, source_view) = create_target_texture(
        device,
        "bloom-probe-data",
        source_size,
        BLOOM_FORMAT,
        sampled | wgpu::TextureUsages::COPY_DST,
    );
    let (reduced, reduced_view) = create_target_texture(
        device,
        "bloom-probe-reduced",
        size,
        BLOOM_FORMAT,
        sampled | wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let (output, output_view) = create_target_texture(
        device,
        "bloom-probe-output",
        size,
        format,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    upload_values(queue, &source, source_size, hot_row, hot_column);
    let uniform = create_uniform_buffer(device, "bloom-probe-size", &BlurParams::prefilter(size));
    let prefilter = post.blur_bind(
        device,
        "bloom-probe-prefilter",
        &source_view,
        &post.nearest_sampler,
        &uniform,
    );
    let copy = post.present_bind(device, &reduced_view);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("bloom-probe"),
    });
    encode_fullscreen(
        &mut encoder,
        "bloom-probe-prefilter",
        &reduced_view,
        &post.pipelines.prefilter,
        &prefilter,
        &post.quad_buffer,
        None,
    );
    encode_fullscreen(
        &mut encoder,
        "bloom-probe-copy",
        &output_view,
        &post.pipelines.present_raw,
        &copy,
        &post.quad_buffer,
        None,
    );
    let _submission = queue.submit([encoder.finish()]);
    let image = super::super::readback::read_back_rgba(
        device,
        queue,
        &output,
        size.width,
        size.height,
        format,
    )?;
    let mut largest_error = 0.0_f64;
    for (index, pixel) in image.rgba.as_chunks::<4>().0.iter().enumerate() {
        let pixel_index = u32::try_from(index).map_err(|error| error.to_string())?;
        let red = coverage(
            pixel_index.checked_div(size.width).unwrap_or(0),
            source_size.height,
            size.height,
            hot_row,
        );
        let green = coverage(
            pixel_index.checked_rem(size.width).unwrap_or(0),
            source_size.width,
            size.width,
            hot_column,
        );
        for (actual, expected) in
            pixel
                .iter()
                .zip([red * 31.875_f64, green * 31.875_f64, 31.875_f64, 255.0_f64])
        {
            largest_error = largest_error.max((f64::from(*actual) - expected).abs());
        }
    }
    logging::event(format!(
        "[bloom-probe] {}",
        serde_json::json!({"source": [source_size.width, source_size.height], "target": [size.width, size.height], "hot_row": hot_row, "hot_column": hot_column, "maximum_byte_error": largest_error})
    ));
    drop(reduced);
    if largest_error > 1.0_f64 {
        return Err(format!(
            "bloom footprint byte error {largest_error} exceeds 1"
        ));
    }
    Ok(())
}

fn upload_values(
    queue: &wgpu::Queue,
    source: &wgpu::Texture,
    source_size: DrawableSize,
    hot_row: u32,
    hot_column: u32,
) {
    let mut bytes = Vec::new();
    for y in 0..source_size.height {
        for x in 0..source_size.width {
            // Half-float numeric 0/1: red one row, green one column, blue constant.
            for component in [
                if y == hot_row { 0x3c00_u16 } else { 0 },
                if x == hot_column { 0x3c00_u16 } else { 0 },
                0x3c00,
                0x3c00,
            ] {
                bytes.extend_from_slice(&component.to_le_bytes());
            }
        }
    }
    queue.write_texture(
        source.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(source_size.width.saturating_mul(8)),
            rows_per_image: Some(source_size.height),
        },
        source.size(),
    );
}

fn coverage(pixel: u32, source: u32, destination: u32, hot: u32) -> f64 {
    let scale = f64::from(source) / f64::from(destination);
    let lower = f64::from(pixel) * scale;
    let upper = lower + scale;
    (upper.min(f64::from(hot) + 1.0) - lower.max(f64::from(hot))).max(0.0) / scale
}
