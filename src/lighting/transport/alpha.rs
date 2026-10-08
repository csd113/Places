//! Authored alpha coverage for straight transport rays. Alpha is numeric data;
//! it never decodes as sRGB. Blend coverage attenuates without refraction or tint.
use std::sync::Arc;

use crate::materials::{AlphaMode, MaterialAlpha, RawImage};

use super::{PreparedRay, TransportScene, scale};

/// The authored texture's addressing convention at a triangle hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportTextureAddress {
    /// Model and fixture sheets clamp at their edges.
    Clamp,
    /// Architectural sheets repeat in both axes.
    Repeat,
}

/// Material coverage aligned with one transport triangle. Images are shared
/// with the compiler's resolved assets; no image is copied per triangle.
#[derive(Clone, Debug, PartialEq)]
pub struct TransportAlphaSurface {
    /// The renderer's coverage mode, opacity multiplier and MASK threshold.
    pub alpha: MaterialAlpha,
    /// Numeric source alpha; a missing image means unit texture coverage.
    pub image: Option<Arc<RawImage>>,
    /// UVs in the same corner order as the transport triangle.
    pub uv: [[f32; 2]; 3],
    /// Linear coverage factors in the same corner order.
    pub vertex_alpha: [f32; 3],
    /// Clamp for model sheets, repeat for architectural sheets.
    pub address: TransportTextureAddress,
}

impl TransportAlphaSurface {
    fn is_valid(&self) -> bool {
        self.uv.iter().flatten().all(|value| value.is_finite())
            && self
                .vertex_alpha
                .iter()
                .chain([&self.alpha.opacity, &self.alpha.cutoff])
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            && self.image.as_ref().is_none_or(|image| {
                image.width > 0
                    && image.height > 0
                    && usize::try_from(image.width)
                        .ok()
                        .and_then(|width| width.checked_mul(usize::try_from(image.height).ok()?))
                        .and_then(|pixels| pixels.checked_mul(4))
                        == Some(image.rgba.len())
            })
    }

    fn coverage(&self, corners: [[f32; 3]; 3], point: [f32; 3]) -> f32 {
        let weights = barycentric(corners, point);
        let uv = std::array::from_fn(|axis| {
            self.uv
                .iter()
                .zip(weights)
                .map(|(coordinate, weight)| coordinate[axis] * weight)
                .sum()
        });
        let vertex = self
            .vertex_alpha
            .iter()
            .zip(weights)
            .map(|(coverage, weight)| coverage * weight)
            .sum::<f32>();
        let texture = self
            .image
            .as_ref()
            .map_or(1.0, |image| sample_alpha(image, uv, self.address));
        (texture * vertex * self.alpha.opacity).clamp(0.0, 1.0)
    }
}

/// Barycentric interpolation uses double precision for thin model faces.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "Normalized, clamped barycentric weights intentionally round to the shader's f32 domain."
)]
fn barycentric(corners: [[f32; 3]; 3], point: [f32; 3]) -> [f32; 3] {
    let edge = |corner: [f32; 3]| {
        std::array::from_fn::<_, 3, _>(|axis| f64::from(corner[axis]) - f64::from(corners[0][axis]))
    };
    let u = edge(corners[1]);
    let v = edge(corners[2]);
    let p = edge(point);
    let dot = |left: [f64; 3], right: [f64; 3]| {
        left.into_iter().zip(right).map(|(a, b)| a * b).sum::<f64>()
    };
    let uu = dot(u, u);
    let uv = dot(u, v);
    let vv = dot(v, v);
    let determinant = uu.mul_add(vv, -(uv * uv));
    if !determinant.is_finite() || determinant <= 0.0_f64 {
        return [1.0, 0.0, 0.0];
    }
    let pu = dot(p, u);
    let pv = dot(p, v);
    let bu = (pu.mul_add(vv, -(pv * uv)) / determinant).clamp(0.0, 1.0);
    let bv = (pv.mul_add(uu, -(pu * uv)) / determinant).clamp(0.0, 1.0);
    let sum = (bu + bv).max(1.0);
    [
        (1.0 - (bu + bv) / sum) as f32,
        (bu / sum) as f32,
        (bv / sum) as f32,
    ]
}

/// Sample the level-zero alpha with the runtime bilinear texel-centre rule.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "Validated source image dimensions fit i64; bounded texel indices and unit interpolation fractions intentionally round to the GPU sampling domain."
)]
fn sample_alpha(image: &RawImage, uv: [f32; 2], address: TransportTextureAddress) -> f32 {
    let coordinate = |value: f32, extent: u32| {
        let unit = match address {
            TransportTextureAddress::Clamp => value.clamp(0.0, 1.0),
            TransportTextureAddress::Repeat => value.rem_euclid(1.0),
        };
        f64::from(unit).mul_add(f64::from(extent), -0.5)
    };
    let x = coordinate(uv[0], image.width);
    let y = coordinate(uv[1], image.height);
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let fx = (x - x.floor()) as f32;
    let fy = (y - y.floor()) as f32;
    let index = |value: i64, extent: u32| {
        let limit = i64::from(extent);
        let bounded = match address {
            TransportTextureAddress::Clamp => value.clamp(0, limit.saturating_sub(1)),
            TransportTextureAddress::Repeat => value.rem_euclid(limit),
        };
        usize::try_from(bounded).unwrap_or(0)
    };
    let texel = |column, row| {
        let pixel = index(row, image.height)
            .saturating_mul(usize::try_from(image.width).unwrap_or(0))
            .saturating_add(index(column, image.width));
        f32::from(
            image
                .rgba
                .get(pixel.saturating_mul(4).saturating_add(3))
                .copied()
                .unwrap_or(255),
        ) / 255.0
    };
    let top = texel(ix, iy) * (1.0 - fx) + texel(ix.saturating_add(1), iy) * fx;
    let bottom = texel(ix, iy.saturating_add(1)) * (1.0 - fx)
        + texel(ix.saturating_add(1), iy.saturating_add(1)) * fx;
    top * (1.0 - fy) + bottom * fy
}

pub(super) fn blocks(
    surfaces: &[Option<TransportAlphaSurface>],
    surface: u32,
    corners: [[f32; 3]; 3],
    point: impl FnOnce() -> [f32; 3],
) -> bool {
    surfaces
        .get(usize::try_from(surface).unwrap_or(usize::MAX))
        .and_then(Option::as_ref)
        .is_none_or(|material| match material.alpha.mode {
            AlphaMode::Opaque => true,
            AlphaMode::Cutout => material.coverage(corners, point()) >= material.alpha.cutoff,
            AlphaMode::Blend => false,
        })
}

impl TransportScene {
    /// Attach authored per-triangle alpha coverage. Returns `None` for a
    /// mismatched table, non-finite UVs or invalid coverage/image storage.
    #[must_use]
    pub fn with_surface_alpha(
        mut self,
        surfaces: Vec<Option<TransportAlphaSurface>>,
    ) -> Option<Self> {
        if surfaces.len() != self.triangles.len()
            || surfaces.iter().flatten().any(|surface| !surface.is_valid())
        {
            return None;
        }
        self.blend_nodes = vec![false; self.ray_nodes.len()];
        for index in (0..self.ray_nodes.len()).rev() {
            let node = &self.ray_nodes[index];
            self.blend_nodes[index] = if node.count > 0 {
                let start = usize::try_from(node.first).ok()?;
                let end = start.checked_add(usize::try_from(node.count).ok()?)?;
                self.ray_triangles.get(start..end)?.iter().any(|triangle| {
                    !triangle.transmissive
                        && surfaces
                            .get(usize::try_from(triangle.surface).unwrap_or(usize::MAX))
                            .and_then(Option::as_ref)
                            .is_some_and(|surface| surface.alpha.mode == AlphaMode::Blend)
                })
            } else {
                self.blend_nodes
                    .get(usize::try_from(node.first).ok()?)
                    .copied()
                    .unwrap_or(false)
                    || self
                        .blend_nodes
                        .get(usize::try_from(node.right).ok()?)
                        .copied()
                        .unwrap_or(false)
            };
        }
        self.surface_alpha = surfaces;
        Some(self)
    }

    /// Straight scalar transmission, including authored MASK/BLEND coverage.
    /// Cache-connectivity queries retain their opaque/MASK obstruction meaning.
    pub(super) fn transmittance(&self, a: [f32; 3], b: [f32; 3]) -> f32 {
        if self.occluded(a, b) {
            return 0.0;
        }
        let delta = super::sub(b, a);
        let distance = super::length(delta);
        if !distance.is_finite() || distance <= 0.0 {
            return 1.0;
        }
        let endpoint_scale = b
            .iter()
            .fold(distance.max(1.0), |scale, value| scale.max(value.abs()));
        self.blend_transmittance(
            a,
            scale(delta, 1.0 / distance),
            distance - super::SURFACE_OFFSET_M * endpoint_scale,
        )
    }

    /// First opaque/MASK hit plus throughput along the preceding straight path.
    pub(super) fn intersect_transport(
        &self,
        origin: [f32; 3],
        direction: [f32; 3],
    ) -> (Option<(f32, usize)>, f32) {
        let hit = self.intersect(origin, direction);
        let throughput = self.blend_transmittance(
            origin,
            direction,
            hit.map_or(f32::INFINITY, |(distance, _)| distance),
        );
        (hit, throughput)
    }

    fn blend_transmittance(&self, origin: [f32; 3], direction: [f32; 3], max_t: f32) -> f32 {
        if !self.blend_nodes.first().copied().unwrap_or(false) || max_t <= 0.0 {
            return 1.0;
        }
        let Some(ray) = PreparedRay::new(origin, direction) else {
            return 1.0;
        };
        let inverse = direction.map(super::safe_inverse);
        let mut stack = [0_u32; 64];
        let mut depth = 1_usize;
        let mut hits = Vec::new();
        while depth > 0 {
            depth = depth.saturating_sub(1);
            let index = usize::try_from(stack[depth]).unwrap_or(usize::MAX);
            if !self.blend_nodes.get(index).copied().unwrap_or(false) {
                continue;
            }
            let Some(node) = self.ray_nodes.get(index) else {
                continue;
            };
            if ray
                .node_entry::<true>(node, origin, inverse, max_t)
                .is_none()
            {
                continue;
            }
            if node.count == 0 {
                // The hierarchy depth is capped at 40, below the 64-slot walk.
                stack[depth] = node.first;
                stack[depth.saturating_add(1)] = node.right;
                depth = depth.saturating_add(2);
                continue;
            }
            let first = usize::try_from(node.first).unwrap_or(usize::MAX);
            let end = first.saturating_add(usize::try_from(node.count).unwrap_or(0));
            for triangle in self.ray_triangles.get(first..end).unwrap_or_default() {
                if triangle.transmissive {
                    continue;
                }
                let Some(surface) = self
                    .surface_alpha
                    .get(usize::try_from(triangle.surface).unwrap_or(usize::MAX))
                    .and_then(Option::as_ref)
                else {
                    continue;
                };
                if surface.alpha.mode != AlphaMode::Blend {
                    continue;
                }
                if let Some(distance) = ray.corners(triangle.corners)
                    && distance > super::RAY_EPS_M
                    && distance < max_t
                {
                    hits.push((
                        distance,
                        surface.coverage(triangle.corners, ray.point_at(distance)),
                    ));
                }
            }
        }
        hits.sort_unstable_by(|(a, _), (b, _)| a.total_cmp(b));
        let mut throughput = 1.0;
        let mut previous: Option<(f32, f32)> = None;
        for (distance, coverage) in hits {
            if let Some((last, opacity)) = &mut previous
                && last.to_bits() == distance.to_bits()
            {
                // A shared triangulation edge is one physical crossing.
                *opacity = opacity.max(coverage);
                continue;
            }
            if let Some((_, opacity)) = previous {
                throughput *= 1.0 - opacity;
            }
            previous = Some((distance, coverage));
        }
        if let Some((_, opacity)) = previous {
            throughput *= 1.0 - opacity;
        }
        throughput
    }
}
