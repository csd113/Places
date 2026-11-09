//! Sample previous-order irradiance at the hit's actual surface coordinates.
//! Coarse cell-centre representatives lose the steep gradients at contacts.

use super::{
    Chart, LightmapPatch, SURFACE_OFFSET_M, TransportReceiver, TransportScene,
    batch::SurfaceStencil, dot, patch_normal, receiver_position, same_lighting_plane, sub,
};

pub(super) struct SurfaceCache {
    charts: Vec<(LightmapPatch, Chart, usize)>,
    by_surface: Vec<Vec<usize>>,
}

#[cfg(test)]
#[path = "../../tests/transport_surface_cache.rs"]
mod tests;

impl SurfaceCache {
    pub(super) fn capacity_bytes(&self) -> usize {
        self.charts
            .capacity()
            .saturating_mul(std::mem::size_of::<(LightmapPatch, Chart, usize)>())
            .saturating_add(
                self.by_surface
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Vec<usize>>()),
            )
            .saturating_add(self.by_surface.iter().fold(0_usize, |bytes, entries| {
                bytes.saturating_add(
                    entries
                        .capacity()
                        .saturating_mul(std::mem::size_of::<usize>()),
                )
            }))
    }

    pub(super) fn new(
        charts: &[(LightmapPatch, Chart)],
        receivers: &[TransportReceiver],
        surfaces: usize,
    ) -> Self {
        let mut stored = Vec::with_capacity(charts.len());
        let mut by_surface = vec![Vec::new(); surfaces];
        let mut first = 0_usize;
        for (index, &(patch, chart)) in charts.iter().enumerate() {
            stored.push((patch, chart, first));
            let end = first.saturating_add(
                usize::try_from(chart.width.saturating_mul(chart.height)).unwrap_or(0),
            );
            for receiver in receivers.get(first..end).unwrap_or_default() {
                let Some(entries) =
                    by_surface.get_mut(usize::try_from(receiver.surface).unwrap_or(usize::MAX))
                else {
                    continue;
                };
                if entries.last() != Some(&index) {
                    entries.push(index);
                }
            }
            first = end;
        }
        Self {
            charts: stored,
            by_surface,
        }
    }

    pub(super) fn stencil(
        &self,
        scene: &TransportScene,
        point: [f32; 3],
        surface: usize,
        receivers: &[TransportReceiver],
    ) -> Option<SurfaceStencil> {
        let hit = scene.triangles.get(surface)?;
        let magnitude = point
            .iter()
            .fold(1.0_f32, |scale, value| scale.max(value.abs()));
        let tolerance = 8.0 * SURFACE_OFFSET_M * magnitude;
        let target_origin = receiver_position(point, hit.normal);
        for &index in self.by_surface.get(surface)? {
            let (patch, chart, first) = self.charts.get(index)?;
            if dot(patch_normal(patch), hit.normal) < 1.0 - 8.0 * f32::EPSILON {
                continue;
            }
            let (u, v) = patch.local_of(point);
            let (width, height) = patch.extent_m();
            let du = tolerance / width.max(tolerance);
            let dv = tolerance / height.max(tolerance);
            if !(-du..=1.0 + du).contains(&u)
                || !(-dv..=1.0 + dv).contains(&v)
                || (patch.is_triangular() && u + v > 1.0 + du + dv)
                || dot(sub(point, patch.origin), hit.normal).abs() > tolerance
            {
                continue;
            }
            let mut stencil = Self::chart_stencil(chart, *first, u, v)?;
            // The hit's chart bounds are insufficient when a separate solid
            // crosses a large floor/ceiling. Keep the same visibility barrier
            // used by the coarse cache, at each actual supporting receiver.
            stencil.retain(|receiver_index| {
                receivers.get(receiver_index).is_some_and(|receiver| {
                    scene
                        .triangles
                        .get(usize::try_from(receiver.surface).unwrap_or(usize::MAX))
                        .is_some_and(|triangle| same_lighting_plane(hit, triangle))
                        && !scene.occluded(receiver.ray_origin, target_origin)
                })
            });
            // A containing chart with no visible support is dark. Falling
            // back to another cell would cross the rejected barrier again.
            return Some(stencil);
        }
        None
    }

    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Clamped UVs scale by checked u16 chart extents before flooring; all receiver access remains checked."
    )]
    fn chart_stencil(chart: &Chart, first: usize, u: f32, v: f32) -> Option<SurfaceStencil> {
        let width = usize::try_from(chart.width).ok()?;
        let height = usize::try_from(chart.height).ok()?;
        let x = u.clamp(0.0, 1.0) * f32::from(u16::try_from(width.saturating_sub(1)).ok()?);
        let y = v.clamp(0.0, 1.0) * f32::from(u16::try_from(height.saturating_sub(1)).ok()?);
        let ix = x.floor() as usize;
        let iy = y.floor() as usize;
        let mut stencil = SurfaceStencil::default();
        for (dx, dy, weight) in [
            (0, 0, (1.0 - x.fract()) * (1.0 - y.fract())),
            (1, 0, x.fract() * (1.0 - y.fract())),
            (0, 1, (1.0 - x.fract()) * y.fract()),
            (1, 1, x.fract() * y.fract()),
        ] {
            if weight <= 0.0 {
                continue;
            }
            let receiver = first
                .saturating_add(
                    iy.saturating_add(dy)
                        .min(height.saturating_sub(1))
                        .saturating_mul(width),
                )
                .saturating_add(ix.saturating_add(dx).min(width.saturating_sub(1)));
            stencil.push(receiver, weight);
        }
        Some(stencil)
    }
}
