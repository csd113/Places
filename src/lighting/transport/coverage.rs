//! Adaptive receiver-footprint integration. Only visibility edges are refined;
//! the smooth direct field is never denoised or blurred.
use super::{
    Accumulator, AtomicBool, Chart, EmitterShape, LightmapFailure, LightmapPatch, PatchKind,
    SURFACE_OFFSET_M, TransportReceiver, TransportScene, accumulate_surface_lobe, add, add_scaled,
    attenuate, parallel_map, patch_normal, receiver_position, receiver_ray_origin, scale,
    texel_axis,
};

#[derive(Clone, Copy, Default)]
struct DirectSample {
    light: Accumulator,
    visibility: u64,
    rays: usize,
}

pub(super) fn visibility_key(key: u64, index: usize, visible: u32) -> u64 {
    (key ^ u64::try_from(index).unwrap_or(u64::MAX) ^ u64::from(visible).rotate_left(32))
        .wrapping_mul(0x100_0000_01b3)
}

fn evaluate(
    scene: &TransportScene,
    receiver: &TransportReceiver,
    emitters: &[usize],
    global: bool,
    taps: u8,
) -> DirectSample {
    let mut result = DirectSample::default();
    for index in emitters {
        let Some(emitter) = scene.emitters.get(*index) else {
            continue;
        };
        let (weight, direction, visibility) =
            emitter.direct_with_visibility(scene, receiver.position, receiver.ray_origin, taps);
        result.visibility = visibility_key(result.visibility, *index, visibility.to_bits());
        if emitter.intensity > 0.0 && emitter.reaches(receiver.position) {
            let axis = usize::from(taps.clamp(1, 3));
            let count = match emitter.shape {
                EmitterShape::Point => 1,
                EmitterShape::Line { .. } => axis,
                EmitterShape::Rect { .. } => axis.saturating_mul(axis),
            };
            result.rays = result.rays.saturating_add(count);
        }
        accumulate_surface_lobe(
            &mut result.light,
            attenuate(weight, receiver.attenuation),
            direction,
            receiver.normal,
        );
    }
    if global {
        let (visibility, rays) = scene.accumulate_global(
            &mut result.light,
            receiver.ray_origin,
            Some(receiver.normal),
            receiver.attenuation,
            taps,
        );
        result.visibility ^= visibility.rotate_left(17);
        result.rays = result.rays.saturating_add(rays);
    }
    result
}

/// Centre visibility classifies edges. Medium integrates 2×2 subtexels at
/// edges; Full integrates 4×4. The one-tap vertex/diagnostic path stays exact.
#[allow(clippy::too_many_arguments)] // one pass's scene, chart domain and bounded solve budget
pub(super) fn direct_pass(
    scene: &TransportScene,
    charts: &[(LightmapPatch, Chart)],
    receivers: &[TransportReceiver],
    emitters: &[usize],
    global: bool,
    taps: u8,
    workers: usize,
    cancel: Option<&AtomicBool>,
) -> Result<(Vec<Accumulator>, usize), LightmapFailure> {
    let centres = parallel_map(receivers.len(), workers, cancel, |index| {
        receivers
            .get(index)
            .map_or_else(DirectSample::default, |receiver| {
                evaluate(scene, receiver, emitters, global, taps)
            })
    })?;
    let mut rays = centres
        .iter()
        .fold(0_usize, |sum, sample| sum.saturating_add(sample.rays));
    if taps <= 1 {
        return Ok((
            centres.into_iter().map(|sample| sample.light).collect(),
            rays,
        ));
    }
    let mut offset = 0_usize;
    let domains = charts
        .iter()
        .map(|(patch, chart)| {
            let first = offset;
            offset = offset.saturating_add(
                usize::try_from(chart.width.saturating_mul(chart.height)).unwrap_or(0),
            );
            (first, patch, chart)
        })
        .collect::<Vec<_>>();
    let pass = FootprintPass {
        scene,
        domains,
        receivers,
        centres: &centres,
        emitters,
        global,
        taps,
    };
    let refined = parallel_map(receivers.len(), workers, cancel, |index| pass.refine(index))?;
    rays = rays.saturating_add(
        refined
            .iter()
            .fold(0_usize, |sum, sample| sum.saturating_add(sample.rays)),
    );
    Ok((
        refined.into_iter().map(|sample| sample.light).collect(),
        rays,
    ))
}

struct FootprintPass<'a> {
    scene: &'a TransportScene,
    domains: Vec<(usize, &'a LightmapPatch, &'a Chart)>,
    receivers: &'a [TransportReceiver],
    centres: &'a [DirectSample],
    emitters: &'a [usize],
    global: bool,
    taps: u8,
}

impl FootprintPass<'_> {
    fn border_edge(&self, index: usize, domain: (usize, &LightmapPatch, &Chart)) -> (bool, usize) {
        let (Some(centre), Some(receiver)) = (self.centres.get(index), self.receivers.get(index))
        else {
            return (false, 0);
        };
        let (first, patch, chart) = domain;
        let width = usize::try_from(chart.width).unwrap_or(1).max(1);
        let height = usize::try_from(chart.height).unwrap_or(1).max(1);
        let local = index.saturating_sub(first);
        let column = local % width;
        let row = local / width;
        let u = texel_axis(column, width);
        let v = texel_axis(row, height);
        let du = 1.0 / f32::from(u16::try_from(width.saturating_sub(1).max(1)).unwrap_or(u16::MAX));
        let dv =
            1.0 / f32::from(u16::try_from(height.saturating_sub(1).max(1)).unwrap_or(u16::MAX));
        let normal = patch_normal(patch);
        let mut edge = false;
        let mut rays = 0_usize;
        for (outside, u, v) in [
            (column == 0, u - du, v),
            (column.saturating_add(1) == width, u + du, v),
            (row == 0, u, v - dv),
            (row.saturating_add(1) == height, u, v + dv),
        ] {
            if !outside {
                continue;
            }
            let point = footprint_point(patch, u, v);
            let magnitude = point.iter().fold(1.0_f32, |scale, v| scale.max(v.abs()));
            if self
                .scene
                .near_surface(
                    point,
                    8.0 * SURFACE_OFFSET_M * magnitude,
                    false,
                    Some(normal),
                )
                .is_none()
            {
                continue;
            }
            let position = receiver_position(point, normal);
            let neighbour = TransportReceiver {
                position,
                ray_origin: position,
                attenuation: self.scene.attenuation_at(position),
                ..*receiver
            };
            let sample = evaluate(
                self.scene,
                &neighbour,
                self.emitters,
                self.global,
                self.taps,
            );
            rays = rays.saturating_add(sample.rays);
            edge |= sample.visibility != centre.visibility;
        }
        (edge, rays)
    }

    fn footprint_receiver(
        &self,
        receiver: &TransportReceiver,
        patch: &LightmapPatch,
        u: f32,
        v: f32,
    ) -> TransportReceiver {
        let normal = patch_normal(patch);
        let point = footprint_point(patch, u, v);
        let magnitude = point.iter().fold(1.0_f32, |scale, v| scale.max(v.abs()));
        let supported = self
            .scene
            .near_surface(
                point,
                8.0 * SURFACE_OFFSET_M * magnitude,
                false,
                Some(normal),
            )
            .is_some();
        let (position, origin) = if supported {
            let position = receiver_position(point, normal);
            (position, position)
        } else {
            let u = u.clamp(0.0, 1.0);
            let v = v.clamp(0.0, 1.0);
            (
                receiver_position(patch.point_at(u, v), normal),
                receiver_ray_origin(patch, u, v, normal),
            )
        };
        TransportReceiver {
            position,
            ray_origin: origin,
            attenuation: self.scene.attenuation_at(position),
            ..*receiver
        }
    }

    fn refine(&self, index: usize) -> DirectSample {
        let Some(centre) = self.centres.get(index) else {
            return DirectSample::default();
        };
        let domain = self
            .domains
            .partition_point(|(first, _, _)| *first <= index)
            .saturating_sub(1);
        let Some((first, patch, chart)) = self.domains.get(domain) else {
            return DirectSample { rays: 0, ..*centre };
        };
        let width = usize::try_from(chart.width).unwrap_or(0).max(1);
        let height = usize::try_from(chart.height).unwrap_or(0).max(1);
        let local = index.saturating_sub(*first);
        let column = local % width;
        let row = local / width;
        let edge = column == 0
            || row == 0
            || column.saturating_add(1) == width
            || row.saturating_add(1) == height;
        let neighbours = [
            column.checked_sub(1).map(|_| index.saturating_sub(1)),
            (column.saturating_add(1) < width).then_some(index.saturating_add(1)),
            row.checked_sub(1).map(|_| index.saturating_sub(width)),
            (row.saturating_add(1) < height).then_some(index.saturating_add(width)),
        ];
        let visibility_edge = neighbours.into_iter().flatten().any(|neighbour| {
            self.centres
                .get(neighbour)
                .is_some_and(|sample| sample.visibility != centre.visibility)
        });
        // A chart border is not itself a visibility edge. Compare supported
        // world-space neighbours across it, rather than averaging a smooth
        // gradient merely because authoring split the surface into strips.
        let (border_edge, border_rays) = if edge && patch.kind != PatchKind::Prop {
            self.border_edge(index, (*first, patch, chart))
        } else {
            (false, 0)
        };
        if !(visibility_edge || border_edge) {
            return DirectSample {
                rays: border_rays,
                ..*centre
            };
        }
        let Some(receiver) = self.receivers.get(index) else {
            return DirectSample { rays: 0, ..*centre };
        };
        let centre_u = texel_axis(column, width);
        let centre_v = texel_axis(row, height);
        let step_u =
            1.0 / f32::from(u16::try_from(width.saturating_sub(1).max(1)).unwrap_or(u16::MAX));
        let step_v =
            1.0 / f32::from(u16::try_from(height.saturating_sub(1).max(1)).unwrap_or(u16::MAX));
        let axis = if self.taps >= 3 { 4_u16 } else { 2_u16 };
        let mut result = DirectSample {
            rays: border_rays,
            ..DirectSample::default()
        };
        for sample_v in 0..axis {
            for sample_u in 0..axis {
                let du = (f32::from(sample_u) + 0.5) / f32::from(axis) - 0.5;
                let dv = (f32::from(sample_v) + 0.5) / f32::from(axis) - 0.5;
                let u = du.mul_add(step_u, centre_u);
                let v = dv.mul_add(step_v, centre_v);
                let sample_receiver = self.footprint_receiver(receiver, patch, u, v);
                let sample = evaluate(
                    self.scene,
                    &sample_receiver,
                    self.emitters,
                    self.global,
                    self.taps,
                );
                add_scaled(
                    &mut result.light,
                    &sample.light,
                    1.0 / f32::from(axis.pow(2)),
                );
                result.rays = result.rays.saturating_add(sample.rays);
            }
        }
        result
    }
}

/// Extend a planar quad's affine frame across a shared chart edge. Triangles
/// retain their folded chart domain; unrelated geometry is never welded.
pub(super) fn footprint_point(patch: &LightmapPatch, u: f32, v: f32) -> [f32; 3] {
    if patch.triangle {
        return patch.point_at(u.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    }
    add(
        patch.origin,
        add(
            scale(patch.u_axis, u),
            add(
                scale(patch.v_axis, v),
                scale(patch.diagonal_correction, u.min(v)),
            ),
        ),
    )
}
