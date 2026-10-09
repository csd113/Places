//! Visibility-footprint integration in a fixed physical surface frame.
//! A triangulation or chart cut cannot rotate or resize that footprint.

use super::{
    AtomicBool, Chart, DirectLightingSample, LightmapFailure, LightmapPatch, PatchKind,
    SURFACE_OFFSET_M, TransportReceiver, TransportScene, TransportTriangle, add, add_scaled,
    cross3, dot, length, parallel_map, patch_normal, receiver_position, same_lighting_plane, scale,
    sub, texel_axis,
};

type DirectSample = DirectLightingSample;

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
    scene.direct_sample(receiver, emitters, global, taps)
}

/// Canonical orthonormal axes depend only on the oriented geometric plane.
/// UV winding, triangle diagonals and atlas placement never enter this frame.
pub(super) fn surface_axes(normal: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let reference = if normal[0].abs() <= normal[1].abs() && normal[0].abs() <= normal[2].abs() {
        [1.0, 0.0, 0.0]
    } else if normal[1].abs() <= normal[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let projected = cross3(reference, normal);
    let tangent = scale(projected, 1.0 / length(projected).max(f32::MIN_POSITIVE));
    (tangent, cross3(normal, tangent))
}

/// Production receives the plan's explicit physical density. Hand-built
/// analytical charts retain their actual endpoint spacing as a fallback.
pub(super) fn sample_pitch(
    scene: &TransportScene,
    chart_index: usize,
    patch: &LightmapPatch,
    chart: &Chart,
) -> f32 {
    scene.chart_sample_pitch(chart_index).unwrap_or_else(|| {
        let (width, height) = patch.extent_m();
        let du = width
            / f32::from(u16::try_from(chart.width.saturating_sub(1).max(1)).unwrap_or(u16::MAX));
        let dv = height
            / f32::from(u16::try_from(chart.height.saturating_sub(1).max(1)).unwrap_or(u16::MAX));
        du.min(dv).max(SURFACE_OFFSET_M)
    })
}

/// Intersect a planar centre-to-tap segment with the triangle's true three
/// half-planes. Expanding these planes can leave a diagonal tap outside the
/// surface even after a metric inset, allowing it to escape a touching wall.
fn triangle_interval(
    triangle: &TransportTriangle,
    from: [f32; 3],
    to: [f32; 3],
) -> Option<(f32, f32)> {
    let mut start = 0.0_f32;
    let mut end = 1.0_f32;
    for (a, b) in [
        (triangle.p0, triangle.p1),
        (triangle.p1, triangle.p2),
        (triangle.p2, triangle.p0),
    ] {
        let edge = sub(b, a);
        let first = dot(cross3(edge, sub(from, a)), triangle.normal);
        let last = dot(cross3(edge, sub(to, a)), triangle.normal);
        if first < 0.0 && last < 0.0 {
            return None;
        }
        if first < 0.0 {
            start = start.max(-first / (last - first));
        } else if last < 0.0 {
            end = end.min(-first / (last - first));
        }
        if start > end {
            return None;
        }
    }
    Some((start, end))
}

fn compatible_surface(
    scene: &TransportScene,
    reference: &TransportTriangle,
    candidate: &TransportTriangle,
    candidate_surface: u32,
    receiver: &TransportReceiver,
    point: [f32; 3],
) -> bool {
    same_lighting_plane(reference, candidate)
        && reference.transmissive == candidate.transmissive
        && scene.same_surface_material(
            receiver.surface,
            candidate_surface,
            receiver.albedo,
            candidate.albedo,
        )
        && dot(receiver.normal, candidate.shading_normal_at(point)) >= 1.0 - 8.0 * f32::EPSILON
}

/// Clip the footprint to contiguous compatible geometric support, including
/// coplanar triangles across an internal diagonal. A BVH-local interval union
/// prevents taps from jumping over a real opening to another piece of wall.
/// Every tap remains in the integral; unsupported extension clips to the first
/// physical boundary rather than discarding the sample or folding a chart UV.
pub(super) fn supported_point(
    scene: &TransportScene,
    receiver: &TransportReceiver,
    from: [f32; 3],
    to: [f32; 3],
    intervals: &mut Vec<(f32, f32)>,
) -> [f32; 3] {
    let Some(reference) = scene
        .triangles
        .get(usize::try_from(receiver.surface).unwrap_or(usize::MAX))
    else {
        return from;
    };
    let magnitude = from
        .iter()
        .chain(&to)
        .fold(1.0_f32, |current, coordinate| current.max(coordinate.abs()));
    let tolerance = 8.0 * SURFACE_OFFSET_M * magnitude;
    let minimum = std::array::from_fn::<_, 3, _>(|axis| from[axis].min(to[axis]) - tolerance);
    let maximum = std::array::from_fn::<_, 3, _>(|axis| from[axis].max(to[axis]) + tolerance);
    intervals.clear();
    let mut stack = [0_u32; 64];
    let mut depth = 1_usize;
    while depth > 0 {
        depth = depth.saturating_sub(1);
        let Some(node) = stack
            .get(depth)
            .and_then(|index| scene.nodes.get(usize::try_from(*index).ok()?))
        else {
            continue;
        };
        if (0..3).any(|axis| node.max[axis] < minimum[axis] || node.min[axis] > maximum[axis]) {
            continue;
        }
        if node.count == 0 {
            let Some(next) = stack.get_mut(depth..depth.saturating_add(2)) else {
                continue;
            };
            next.copy_from_slice(&[node.first, node.right]);
            depth = depth.saturating_add(2);
            continue;
        }
        let first = usize::try_from(node.first).unwrap_or(usize::MAX);
        let end = first.saturating_add(usize::try_from(node.count).unwrap_or(0));
        for index in scene.order.get(first..end).unwrap_or_default() {
            let Some(candidate) = scene
                .triangles
                .get(usize::try_from(*index).unwrap_or(usize::MAX))
            else {
                continue;
            };
            if !compatible_surface(scene, reference, candidate, *index, receiver, from) {
                continue;
            }
            if let Some(interval) = triangle_interval(candidate, from, to) {
                intervals.push(interval);
            }
        }
    }
    intervals.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.total_cmp(&right.1)));
    let delta = sub(to, from);
    let distance = length(delta);
    let numerical_span = tolerance / distance.max(tolerance);
    let mut end = 0.0_f32;
    for &(start, finish) in intervals.iter() {
        if start > end + numerical_span {
            break;
        }
        end = end.max(finish);
    }
    if end >= 1.0 {
        return to;
    }
    // Land just inside a real boundary so normal separation cannot choose
    // the opposite side of its touching opaque face.
    add(from, scale(delta, (end - numerical_span).max(0.0)))
}

#[expect(
    clippy::too_many_arguments,
    reason = "one pass's immutable scene, chart domain and bounded solve budget"
)]
pub(super) fn direct_pass(
    scene: &TransportScene,
    charts: &[(LightmapPatch, Chart)],
    receivers: &[TransportReceiver],
    emitters: &[usize],
    global: bool,
    taps: u8,
    workers: usize,
    cancel: Option<&AtomicBool>,
) -> Result<(Vec<super::Accumulator>, usize), LightmapFailure> {
    if taps <= 1 {
        let centres = parallel_map(receivers.len(), workers, cancel, |index| {
            receivers
                .get(index)
                .map_or_else(DirectSample::default, |receiver| {
                    evaluate(scene, receiver, emitters, global, taps)
                })
        })?;
        let rays = centres
            .iter()
            .fold(0_usize, |sum, sample| sum.saturating_add(sample.rays));
        return Ok((
            centres.into_iter().map(|sample| sample.light).collect(),
            rays,
        ));
    }
    let mut offset = 0_usize;
    let domains = charts
        .iter()
        .enumerate()
        .map(|(index, (patch, chart))| {
            let first = offset;
            offset = offset.saturating_add(
                usize::try_from(chart.width.saturating_mul(chart.height)).unwrap_or(0),
            );
            Domain {
                first,
                patch,
                chart,
                axes: surface_axes(patch_normal(patch)),
                pitch: sample_pitch(scene, index, patch, chart),
            }
        })
        .collect::<Vec<_>>();
    let pass = FootprintPass {
        scene,
        domains,
        receivers,
        emitters,
        global,
        taps,
    };
    let refined = parallel_map(receivers.len(), workers, cancel, |index| pass.refine(index))?;
    let rays = refined
        .iter()
        .fold(0_usize, |sum, sample| sum.saturating_add(sample.rays));
    Ok((
        refined.into_iter().map(|sample| sample.light).collect(),
        rays,
    ))
}

struct Domain<'a> {
    first: usize,
    patch: &'a LightmapPatch,
    chart: &'a Chart,
    axes: ([f32; 3], [f32; 3]),
    pitch: f32,
}

struct FootprintPass<'a> {
    scene: &'a TransportScene,
    domains: Vec<Domain<'a>>,
    receivers: &'a [TransportReceiver],
    emitters: &'a [usize],
    global: bool,
    taps: u8,
}

impl FootprintPass<'_> {
    fn sample(
        &self,
        domain: &Domain<'_>,
        receiver: &TransportReceiver,
        point: [f32; 3],
        du: f32,
        dv: f32,
        intervals: &mut Vec<(f32, f32)>,
    ) -> DirectSample {
        let target = add(
            point,
            add(scale(domain.axes.0, du), scale(domain.axes.1, dv)),
        );
        let supported = supported_point(self.scene, receiver, point, target, intervals);
        let geometric_normal = patch_normal(domain.patch);
        let position = receiver_position(supported, geometric_normal);
        let same_point = supported
            .iter()
            .zip(point)
            .all(|(left, right)| left.to_bits() == right.to_bits());
        let ray_origin = if same_point {
            receiver.ray_origin
        } else {
            position
        };
        let tolerance = 8.0
            * SURFACE_OFFSET_M
            * supported
                .iter()
                .fold(1.0_f32, |current, coordinate| current.max(coordinate.abs()));
        let normal = if domain.patch.kind == PatchKind::Prop {
            self.scene
                .near_surface(supported, tolerance, false, Some(geometric_normal))
                .and_then(|(_, index)| self.scene.triangles.get(index))
                .map_or(receiver.normal, |triangle| {
                    triangle.shading_normal_at(supported)
                })
        } else {
            geometric_normal
        };
        let sample = TransportReceiver {
            position,
            ray_origin,
            normal,
            attenuation: self.scene.attenuation_at(position),
            ..*receiver
        };
        evaluate(self.scene, &sample, self.emitters, self.global, self.taps)
    }

    fn refine(&self, index: usize) -> DirectSample {
        let Some(receiver) = self.receivers.get(index) else {
            return DirectSample::default();
        };
        let domain_index = self
            .domains
            .partition_point(|domain| domain.first <= index)
            .saturating_sub(1);
        let Some(domain) = self.domains.get(domain_index) else {
            return evaluate(self.scene, receiver, self.emitters, self.global, self.taps);
        };
        let width = usize::try_from(domain.chart.width).unwrap_or(1).max(1);
        let height = usize::try_from(domain.chart.height).unwrap_or(1).max(1);
        let local = index.saturating_sub(domain.first);
        #[expect(
            clippy::arithmetic_side_effects,
            reason = "width is normalized with max(1), so row and column decomposition cannot divide by zero"
        )]
        let (column, row) = (local % width, local / width);
        let point = domain
            .patch
            .point_at(texel_axis(column, width), texel_axis(row, height));
        let axis = if self.taps >= 3 { 4_u16 } else { 2_u16 };
        let mut intervals = Vec::with_capacity(8);
        // Equal centre/corner silhouettes do not prove uniform coverage:
        // thin openings and model details can leave lit interior samples.
        // Integrate the complete bounded physical stencil. No separate centre
        // pass or repeated corner rays are needed for these quality levels.
        let mut result = DirectSample::default();
        let weight = 1.0 / f32::from(axis.pow(2));
        for sample_v in 0..axis {
            for sample_u in 0..axis {
                let du = ((f32::from(sample_u) + 0.5) / f32::from(axis) - 0.5) * domain.pitch;
                let dv = ((f32::from(sample_v) + 0.5) / f32::from(axis) - 0.5) * domain.pitch;
                let sample = self.sample(domain, receiver, point, du, dv, &mut intervals);
                result.rays = result.rays.saturating_add(sample.rays);
                add_scaled(&mut result.light, &sample.light, weight);
            }
        }
        result
    }
}

/// Legacy chart-local extension retained for analytical callers. Production
/// footprints use the physical surface frame instead.
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

#[cfg(test)]
#[path = "tests/continuity.rs"]
mod continuity;
