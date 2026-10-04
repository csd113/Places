//! Denoise diffuse gather energy over physical surfaces, including chart joins.
//! Direct light keeps its independently integrated visibility footprint.

use std::collections::BTreeMap;

use super::{
    Accumulator, Chart, LightmapFailure, LightmapPatch, LightmapTexel, PatchKind, SURFACE_OFFSET_M,
    TransportReceiver, TransportScene, add_scaled, channel_luminance, compress_surface, coverage,
    dot, patch_normal, receiver_position, sub, surface_energy_matches,
};

struct SurfaceFilter<'a> {
    scene: &'a TransportScene,
    charts: &'a [(LightmapPatch, Chart)],
    receivers: &'a [TransportReceiver],
    values: &'a [Accumulator],
    starts: Vec<usize>,
    surfaces: BTreeMap<u32, Vec<usize>>,
}

impl<'a> SurfaceFilter<'a> {
    fn new(
        scene: &'a TransportScene,
        charts: &'a [(LightmapPatch, Chart)],
        receivers: &'a [TransportReceiver],
        values: &'a [Accumulator],
    ) -> Self {
        let mut starts = Vec::with_capacity(charts.len());
        let mut surfaces = BTreeMap::<u32, Vec<usize>>::new();
        let mut first = 0_usize;
        for (index, (patch, chart)) in charts.iter().enumerate() {
            starts.push(first);
            let end = first.saturating_add(
                usize::try_from(chart.width.saturating_mul(chart.height)).unwrap_or(0),
            );
            if patch.kind != PatchKind::Prop {
                for receiver in receivers.get(first..end).unwrap_or_default() {
                    if receiver.surface == u32::MAX {
                        continue;
                    }
                    let entry = surfaces.entry(receiver.surface).or_default();
                    if entry.last() != Some(&index) {
                        entry.push(index);
                    }
                }
            }
            first = end;
        }
        Self {
            scene,
            charts,
            receivers,
            values,
            starts,
            surfaces,
        }
    }

    fn across_edge(
        &self,
        chart_index: usize,
        u: f32,
        v: f32,
        centre: &TransportReceiver,
    ) -> Option<(Accumulator, [f32; 3])> {
        let (patch, _) = self.charts.get(chart_index)?;
        if patch.kind == PatchKind::Prop {
            return None;
        }
        let normal = patch_normal(patch);
        let point = coverage::footprint_point(patch, u, v);
        let magnitude = point
            .iter()
            .fold(1.0_f32, |scale, coordinate| scale.max(coordinate.abs()));
        let (_, surface) = self.scene.near_surface(
            point,
            8.0 * SURFACE_OFFSET_M * magnitude,
            false,
            Some(normal),
        )?;
        let surface_id = u32::try_from(surface).ok()?;
        for candidate in self.surfaces.get(&surface_id)? {
            let (other, chart) = self.charts.get(*candidate)?;
            if other.kind != patch.kind
                || dot(normal, patch_normal(other)) < 1.0 - 8.0 * f32::EPSILON
            {
                continue;
            }
            let (neighbor_u, neighbor_v) = other.local_of(point);
            if !(-1.0e-5..=1.0 + 1.0e-5).contains(&neighbor_u)
                || !(-1.0e-5..=1.0 + 1.0e-5).contains(&neighbor_v)
            {
                continue;
            }
            let plane_distance = dot(
                sub(
                    other.point_at(neighbor_u.clamp(0.0, 1.0), neighbor_v.clamp(0.0, 1.0)),
                    point,
                ),
                normal,
            )
            .abs();
            if plane_distance > 8.0 * SURFACE_OFFSET_M * magnitude {
                continue;
            }
            let first = *self.starts.get(*candidate)?;
            let (value, receiver) = self.interpolate(chart, first, neighbor_u, neighbor_v)?;
            if centre
                .albedo
                .iter()
                .zip(receiver.albedo)
                .any(|(left, right)| (*left - right).abs() > 1.0e-5)
                || dot(centre.normal, receiver.normal) < 1.0 - 8.0 * f32::EPSILON
            {
                continue;
            }
            return Some((value, receiver_position(point, normal)));
        }
        None
    }

    // UVs are clamped and chart axes are validated/bounded before filtering.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "UVs are clamped and chart axes are checked to fit u16 before flooring; saturating native casts and checked slice access keep invalid coordinates within safe fallback paths."
    )]
    fn interpolate(
        &self,
        chart: &Chart,
        first: usize,
        u: f32,
        v: f32,
    ) -> Option<(Accumulator, &TransportReceiver)> {
        let width = usize::try_from(chart.width).ok()?;
        let height = usize::try_from(chart.height).ok()?;
        let x = u.clamp(0.0, 1.0) * f32::from(u16::try_from(width.saturating_sub(1)).ok()?);
        let y = v.clamp(0.0, 1.0) * f32::from(u16::try_from(height.saturating_sub(1)).ok()?);
        let ix = x.floor() as usize;
        let iy = y.floor() as usize;
        let mut total = Accumulator::default();
        for (dx, dy, weight) in [
            (0, 0, (1.0 - x.fract()) * (1.0 - y.fract())),
            (1, 0, x.fract() * (1.0 - y.fract())),
            (0, 1, (1.0 - x.fract()) * y.fract()),
            (1, 1, x.fract() * y.fract()),
        ] {
            let index = first
                .saturating_add(
                    iy.saturating_add(dy)
                        .min(height.saturating_sub(1))
                        .saturating_mul(width),
                )
                .saturating_add(ix.saturating_add(dx).min(width.saturating_sub(1)));
            add_scaled(&mut total, self.values.get(index)?, weight);
        }
        let index = first
            .saturating_add(iy.saturating_mul(width))
            .saturating_add(ix);
        Some((total, self.receivers.get(index)?))
    }

    fn neighbour(
        &self,
        chart_index: usize,
        column: usize,
        row: usize,
        di: i32,
        dj: i32,
    ) -> Option<(Accumulator, [f32; 3])> {
        let (_, chart) = self.charts.get(chart_index)?;
        let width = usize::try_from(chart.width).ok()?;
        let height = usize::try_from(chart.height).ok()?;
        let first = *self.starts.get(chart_index)?;
        if let (Some(x), Some(y)) = (
            column.checked_add_signed(isize::try_from(di).ok()?),
            row.checked_add_signed(isize::try_from(dj).ok()?),
        ) && x < width
            && y < height
        {
            let index = first
                .saturating_add(y.saturating_mul(width))
                .saturating_add(x);
            return Some((
                *self.values.get(index)?,
                self.receivers.get(index)?.ray_origin,
            ));
        }
        let index = first
            .saturating_add(row.saturating_mul(width))
            .saturating_add(column);
        let u = (f32::from(u16::try_from(column).ok()?) + f32::from(i16::try_from(di).ok()?))
            / f32::from(u16::try_from(width.saturating_sub(1).max(1)).ok()?);
        let v = (f32::from(u16::try_from(row).ok()?) + f32::from(i16::try_from(dj).ok()?))
            / f32::from(u16::try_from(height.saturating_sub(1).max(1)).ok()?);
        self.across_edge(chart_index, u, v, self.receivers.get(index)?)
    }
}

pub(super) fn filter_accumulators(
    scene: &TransportScene,
    charts: &[(LightmapPatch, Chart)],
    receivers: &[TransportReceiver],
    values: &[Accumulator],
    direct: &[Accumulator],
) -> Result<Vec<LightmapTexel>, LightmapFailure> {
    let field = SurfaceFilter::new(scene, charts, receivers, values);
    let mut out = Vec::with_capacity(values.len());
    for (chart_index, (_, chart)) in charts.iter().enumerate() {
        let width = usize::try_from(chart.width).unwrap_or(0);
        let height = usize::try_from(chart.height).unwrap_or(0);
        let first = *field
            .starts
            .get(chart_index)
            .ok_or(LightmapFailure::FillSize)?;
        for row in 0..height {
            for column in 0..width {
                let index = first
                    .saturating_add(row.saturating_mul(width))
                    .saturating_add(column);
                let centre = values.get(index).ok_or(LightmapFailure::FillSize)?;
                let receiver = receivers.get(index).ok_or(LightmapFailure::FillSize)?;
                let mut weight_sum = 1.0;
                let mut total = *centre;
                for (di, dj) in [
                    (-1_i32, 0_i32),
                    (1_i32, 0_i32),
                    (0_i32, -1_i32),
                    (0_i32, 1_i32),
                ] {
                    let Some((source, origin)) = field.neighbour(chart_index, column, row, di, dj)
                    else {
                        continue;
                    };
                    if scene.occluded(receiver.ray_origin, origin) {
                        continue;
                    }
                    let diff = (channel_luminance(source.irradiance)
                        - channel_luminance(centre.irradiance))
                    .abs();
                    let weight = 1.0 / (1.0 + 8.0 * diff);
                    weight_sum += weight;
                    add_scaled(&mut total, &source, weight);
                }
                let mut averaged = Accumulator::default();
                add_scaled(&mut averaged, &total, 1.0 / weight_sum);
                add_scaled(
                    &mut averaged,
                    direct.get(index).ok_or(LightmapFailure::FillSize)?,
                    1.0,
                );
                let texel = compress_surface(&averaged, receiver.normal);
                if !surface_energy_matches(texel, receiver.normal, averaged.surface_light) {
                    return Err(LightmapFailure::TransportEnergy);
                }
                out.push(texel);
            }
        }
    }
    if out.len() != values.len() {
        return Err(LightmapFailure::FillSize);
    }
    Ok(out)
}
