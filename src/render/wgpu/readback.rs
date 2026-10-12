//! Texture staging and row compaction shared by frame and HDR probe captures.
//! Readback is explicit developer/compiler work, outside ordinary frame draws.

use super::reflections::probe_mip_edge;

#[derive(Clone, Copy)]
enum ReadbackKind {
    Capture,
    Probe,
}

impl ReadbackKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Probe => "probe",
        }
    }

    const fn bytes_per_texel(self) -> u32 {
        match self {
            Self::Capture => 4,
            Self::Probe => 8,
        }
    }

    const fn buffer_label(self) -> &'static str {
        match self {
            Self::Capture => "places-wgpu-capture-readback",
            Self::Probe => "places-wgpu-probe-readback",
        }
    }

    const fn copy_label(self) -> &'static str {
        match self {
            Self::Capture => "places-wgpu-capture-copy",
            Self::Probe => "places-wgpu-probe-copy",
        }
    }
}

/// Reads one cube face of one mip level as tightly packed RGBA16F bytes.
pub(super) fn read_back_cube_face(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    face_size: u32,
    level: u32,
    face: u32,
) -> Result<Vec<u8>, String> {
    let edge = probe_mip_edge(face_size, level)
        .ok_or_else(|| format!("probe has no mip level {level}"))?;
    read_texture(
        device,
        queue,
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: face,
            },
            aspect: wgpu::TextureAspect::All,
        },
        edge,
        edge,
        ReadbackKind::Probe,
    )
}

/// Reads the presented target as a top-down RGBA8 image, restoring RGBA order
/// when the configured surface format stores BGRA bytes.
pub(super) fn read_back_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> Result<crate::loader::RawImage, String> {
    let mut rgba = read_texture(
        device,
        queue,
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        width,
        height,
        ReadbackKind::Capture,
    )?;
    if matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for texel in rgba.as_chunks_mut::<4>().0 {
            texel.swap(0, 2);
        }
    }
    Ok(crate::loader::RawImage::new(width, height, rgba))
}

/// The source must already have been rendered and submitted. This submits
/// only the copy, maps its aligned staging buffer, and removes row padding.
fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: wgpu::TexelCopyTextureInfo<'_>,
    width: u32,
    height: u32,
    kind: ReadbackKind,
) -> Result<Vec<u8>, String> {
    let name = kind.name();
    let unpadded_bytes_per_row = width.saturating_mul(kind.bytes_per_texel());
    let padded_bytes_per_row = unpadded_bytes_per_row
        .div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        .saturating_mul(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(kind.buffer_label()),
        size: u64::from(padded_bytes_per_row).saturating_mul(u64::from(height)),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some(kind.copy_label()),
    });
    encoder.copy_texture_to_buffer(
        source,
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let _submission = queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        // A closed receiver means the caller gave up.
        let _delivery_to_optional_receiver = sender.send(result);
    });
    let _poll_status = device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| format!("wgpu could not wait for the {name} read-back: {error}"))?;
    receiver
        .recv()
        .map_err(|error| format!("the wgpu {name} read-back callback was lost: {error}"))?
        .map_err(|error| format!("wgpu could not map the {name} buffer: {error}"))?;

    let height_usize = usize::try_from(height).unwrap_or(usize::MAX);
    let row_usize = usize::try_from(unpadded_bytes_per_row).unwrap_or(usize::MAX);
    let padded_usize = usize::try_from(padded_bytes_per_row).unwrap_or(usize::MAX);
    let mut pixels = vec![0_u8; row_usize.saturating_mul(height_usize)];
    {
        let mapped = slice
            .get_mapped_range()
            .map_err(|error| format!("wgpu could not read the mapped {name} buffer: {error}"))?;
        for row in 0..height_usize {
            let source_offset = row.saturating_mul(padded_usize);
            let target_offset = row.saturating_mul(row_usize);
            let from = mapped
                .get(source_offset..source_offset.saturating_add(row_usize))
                .ok_or_else(|| format!("{name} staging row runs past its buffer"))?;
            let to = pixels
                .get_mut(target_offset..target_offset.saturating_add(row_usize))
                .ok_or_else(|| format!("{name} row runs past its image"))?;
            to.copy_from_slice(from);
        }
    }
    buffer.unmap();
    Ok(pixels)
}
