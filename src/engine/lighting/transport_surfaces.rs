//! Material coverage and sampled albedo for transport triangles.

use super::transport::{TransportAlphaSurface, TransportTextureAddress};
use crate::engine::level::WaterVolumes;
use crate::engine::materials::{AlphaMode, RawImage};

const WATER_SURFACE_EPS_M: f32 = 1.0e-3;

pub(in crate::engine) fn alpha_surface(
    alpha: crate::engine::materials::MaterialAlpha,
    image: Option<std::sync::Arc<RawImage>>,
    uv: [[f32; 2]; 3],
    vertex_alpha: [f32; 3],
    address: TransportTextureAddress,
) -> Option<TransportAlphaSurface> {
    (alpha.mode != AlphaMode::Opaque).then_some(TransportAlphaSurface {
        alpha,
        image,
        uv,
        vertex_alpha,
        address,
    })
}

/// True when a triangle is one water volume's drawn surface: all three corners
/// lie on one resolved footprint at its `surface_y` (so the triangle is
/// horizontal) and the range resolves to that volume's material id.
///
/// The surface is drawn only for [`WaterVolumes`] entries, so matching the
/// authored geometry back to the same resolution is what keeps the transmitted
/// surface and the attenuating body one thing.
pub(in crate::engine) fn is_water_surface(
    corners: [[f32; 3]; 3],
    material_id: Option<&str>,
    water: &WaterVolumes,
) -> bool {
    let Some(surface_material) = material_id else {
        return false;
    };
    water.volumes().iter().any(|volume| {
        volume.material_id() == surface_material
            && corners.iter().all(|corner| {
                (corner[1] - volume.surface_y).abs() <= WATER_SURFACE_EPS_M
                    && corner[0] >= volume.x0 - WATER_SURFACE_EPS_M
                    && corner[0] <= volume.x1 + WATER_SURFACE_EPS_M
                    && corner[2] >= volume.z0 - WATER_SURFACE_EPS_M
                    && corner[2] <= volume.z1 + WATER_SURFACE_EPS_M
            })
    })
}

/// The albedo of one triangle: the material tint (or the prop vertex's own
/// base colour) multiplied by the submesh texture's colour at the triangle
/// centre.
pub(in crate::engine) fn triangle_albedo(
    material_albedo: [f32; 3],
    [a, b, c]: [[f32; 4]; 3],
    [ua, ub, uc]: [[f32; 2]; 3],
    texture: Option<&RawImage>,
) -> [f32; 3] {
    let mut base = [
        material_albedo[0] * ((a[0] + b[0] + c[0]) / 3.0),
        material_albedo[1] * ((a[1] + b[1] + c[1]) / 3.0),
        material_albedo[2] * ((a[2] + b[2] + c[2]) / 3.0),
    ];
    if let Some(image) = texture {
        let centroid = [(ua[0] + ub[0] + uc[0]) / 3.0, (ua[1] + ub[1] + uc[1]) / 3.0];
        let sample = sample_texture(image, centroid);
        base = [
            base[0] * sample[0],
            base[1] * sample[1],
            base[2] * sample[2],
        ];
    }
    [
        base[0].clamp(0.0, 1.0),
        base[1].clamp(0.0, 1.0),
        base[2].clamp(0.0, 1.0),
    ]
}

/// The averaged albedo of one resolved level material.
pub(in crate::engine) fn material_albedo(
    mut tint: [f32; 3],
    texture: Option<&RawImage>,
) -> [f32; 3] {
    if let Some(image) = texture {
        let mean = mean_texture_color(image);
        tint = [tint[0] * mean[0], tint[1] * mean[1], tint[2] * mean[2]];
    }
    [
        tint[0].clamp(0.0, 1.0),
        tint[1].clamp(0.0, 1.0),
        tint[2].clamp(0.0, 1.0),
    ]
}

/// Mean colour of one texture, sampled on a fixed stride so a large image is
/// bounded work.
#[must_use]
fn mean_texture_color(image: &RawImage) -> [f32; 3] {
    if image.width == 0 || image.height == 0 {
        return [1.0; 3];
    }
    let texels = (usize::try_from(image.width).unwrap_or(0))
        .saturating_mul(usize::try_from(image.height).unwrap_or(0));
    if texels == 0 {
        return [1.0; 3];
    }
    let stride = (texels / 1024).max(1);
    let mut sum = [0.0_f32; 3];
    let mut count = 0_usize;
    let mut index = 0usize;
    while index < texels {
        let offset = index.saturating_mul(4);
        if let Some([r, g, b, a]) = image
            .rgba
            .get(offset..offset.saturating_add(4))
            .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
            && a > 0
        {
            sum[0] += crate::engine::materials::color::decode_byte(r);
            sum[1] += crate::engine::materials::color::decode_byte(g);
            sum[2] += crate::engine::materials::color::decode_byte(b);
            count = count.saturating_add(1);
        }
        index = index.saturating_add(stride);
    }
    if count == 0 {
        return [1.0; 3];
    }
    // The stride caps the sample count near 1024, so the divisor is bounded.
    let divisor = f32::from(u16::try_from(count).unwrap_or(u16::MAX)).max(1.0);
    [sum[0] / divisor, sum[1] / divisor, sum[2] / divisor]
}

/// Nearest-texel sample of a GLB sheet, clamped like the runtime sampler.
#[must_use]
fn sample_texture(image: &RawImage, uv: [f32; 2]) -> [f32; 3] {
    if image.width == 0 || image.height == 0 {
        return [1.0; 3];
    }
    let width = usize::try_from(image.width).unwrap_or(1).max(1);
    let height = usize::try_from(image.height).unwrap_or(1).max(1);
    let u_coordinate = if uv[0].is_finite() {
        uv[0].clamp(0.0, 1.0)
    } else {
        0.0
    };
    let v_coordinate = if uv[1].is_finite() {
        uv[1].clamp(0.0, 1.0)
    } else {
        0.0
    };
    let width_f =
        f32::from(u16::try_from(width.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)).max(1.0);
    let height_f =
        f32::from(u16::try_from(height.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)).max(1.0);
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Finite wrapped UVs become floored texel indices bounded by u16 texture dimensions and clamped to the last texel."
    )]
    // Both coordinates are finite and in `0..width` after the modulo and the
    // multiply, so the truncating cast is exact for the range it receives.
    let x_texel = ((u_coordinate * width_f).floor() as u32)
        .min(u32::try_from(width.saturating_sub(1)).unwrap_or(0));
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Finite wrapped UVs become floored texel indices bounded by u16 texture dimensions and clamped to the last texel."
    )]
    let y_texel = ((v_coordinate * height_f).floor() as u32)
        .min(u32::try_from(height.saturating_sub(1)).unwrap_or(0));
    let offset = usize::try_from(y_texel)
        .unwrap_or(0)
        .saturating_mul(width)
        .saturating_add(usize::try_from(x_texel).unwrap_or(0))
        .saturating_mul(4);
    match image
        .rgba
        .get(offset..offset.saturating_add(4))
        .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
    {
        Some([r, g, b, _]) => [
            crate::engine::materials::color::decode_byte(r),
            crate::engine::materials::color::decode_byte(g),
            crate::engine::materials::color::decode_byte(b),
        ],
        _ => [1.0; 3],
    }
}
