//! Two independent light layers share only their immutable geometric queries.
//! No transfer matrix survives a receiver: memory stays linear in texel count,
//! and each channel retains the reference sample/accumulation order.
use super::{
    Accumulator, AtomicBool, BOUNCE_GAIN, Chart, GatherAudit, GatherEvent, LightmapFailure,
    LightmapPatch, MAX_BOUNCE_RAYS, RadianceCache, SolveOptions, StageAudit, SurfaceCacheTarget,
    TransportReceiver, TransportScene, TransportSolution, TransportSolve, accumulate_surface_lobe,
    add, add_scaled, append_switchable_probe_field, attenuate, coverage, dot, hemisphere_axes,
    hemisphere_from_local, lattice_cell, parallel_map, record_gather, scale,
};

impl TransportScene {
    pub(super) fn solve_pair(
        &self,
        charts: &[(LightmapPatch, Chart)],
        base_emitters: &[usize],
        light_index: usize,
        emitter: &[usize],
        options: SolveOptions,
        cancel: Option<&AtomicBool>,
        bake_probes: bool,
    ) -> Result<TransportSolve, LightmapFailure> {
        let started = std::time::Instant::now();
        crate::engine::progress::stage("building shared base and switched receivers");
        let receivers = self.receivers(charts)?;
        let receiver_ms = started.elapsed().as_secs_f64() * 1_000.0_f64;
        if !self.receiver_target.is_empty() && self.receiver_target.len() != receivers.len() {
            return Err(LightmapFailure::FillSize);
        }
        let mut direct_rays = 0usize;
        let mut direct_ms = [0.0_f64; 2];
        let mut layers = Vec::with_capacity(2);
        for (channel, emitters) in [base_emitters, emitter].into_iter().enumerate() {
            let direct_started = std::time::Instant::now();
            let (direct, rays) = coverage::direct_pass(
                self,
                charts,
                &receivers,
                emitters,
                channel == 0,
                options.taps_per_axis,
                options.workers,
                cancel,
            )?;
            direct_ms[channel] = direct_started.elapsed().as_secs_f64() * 1_000.0_f64;
            if direct.iter().any(|value| !value.is_finite()) {
                return Err(LightmapFailure::FillNonFinite);
            }
            StageAudit {
                charts,
                receivers: &receivers,
                dump: channel == 0,
            }
            .accumulators("direct", &direct);
            direct_rays = direct_rays.saturating_add(rays);
            layers.push(direct);
        }
        let [direct_base, direct_switch] = layers
            .try_into()
            .map_err(|_incomplete_layers: Vec<Vec<Accumulator>>| LightmapFailure::FillSize)?;
        let active = [
            self.paired_base_bounce_enabled(base_emitters, options.bounces),
            options.bounces > 0,
        ];
        let bounce_started = std::time::Instant::now();
        let (combined, bounce_rays, cache_cells) = self.bounce_pair_orders(
            &receivers,
            [direct_base.clone(), direct_switch.clone()],
            options,
            active,
            cancel,
        )?;
        let bounce_ms = bounce_started.elapsed().as_secs_f64() * 1_000.0_f64;
        let [base_values, switch_values] = combined;
        if active[0] {
            StageAudit {
                charts,
                receivers: &receivers,
                dump: true,
            }
            .accumulators("bounced", &base_values);
        }
        let mut probes = None;
        let base = self.finish_pass(
            charts,
            &receivers,
            &direct_base,
            base_values,
            true,
            options,
            cancel,
            bake_probes.then_some(&mut probes),
            [receiver_ms, direct_ms[0], bounce_ms],
        )?;
        let mut switched_probes = None;
        let switchable = self.finish_pass(
            charts,
            &receivers,
            &direct_switch,
            switch_values,
            false,
            options,
            cancel,
            bake_probes.then_some(&mut switched_probes),
            [0.0_f64, direct_ms[1], 0.0_f64],
        )?;
        append_switchable_probe_field(&mut probes, light_index, switched_probes)?;
        Ok(TransportSolve {
            solution: TransportSolution {
                charts: base,
                switchable: vec![(light_index, switchable)],
                direct_rays,
                bounce_rays,
                cache_cells,
            },
            probes,
        })
    }

    fn paired_base_bounce_enabled(&self, emitters: &[usize], bounces: u8) -> bool {
        bounces > 0
            && (!emitters.is_empty()
                || !self.global_lights.is_empty()
                || self.sky_radiance.iter().any(|channel| *channel > 0.0))
    }

    fn bounce_pair_orders(
        &self,
        receivers: &[TransportReceiver],
        mut combined: [Vec<Accumulator>; 2],
        options: SolveOptions,
        active: [bool; 2],
        cancel: Option<&AtomicBool>,
    ) -> Result<([Vec<Accumulator>; 2], usize, usize), LightmapFailure> {
        if !active.iter().any(|channel| *channel) {
            return Ok((combined, 0, 0));
        }
        crate::engine::progress::stage("building shared diffuse radiance cache");
        let cache_started = std::time::Instant::now();
        let cache = RadianceCache::build(receivers);
        let mut previous = combined.clone();
        crate::engine::logging::info(format_args!(
            "[transport-cache] receivers={} build_ms={:.3}",
            receivers.len(),
            cache_started.elapsed().as_secs_f64() * 1_000.0_f64
        ));
        for order in 0..options.bounces {
            let order_started = std::time::Instant::now();
            crate::engine::logging::info(format_args!(
                "[transport-progress] shared_layers=2 diffuse_order={}/{} receivers={} samples={} workers={}",
                order.saturating_add(1),
                options.bounces,
                receivers.len(),
                options.gather_samples,
                options.workers
            ));
            let gained = self.bounce_pair_pass(
                receivers,
                [&previous[0], &previous[1]],
                &cache,
                options,
                order,
                active,
                cancel,
            )?;
            let gather_ms = order_started.elapsed().as_secs_f64() * 1_000.0_f64;
            let accumulation_started = std::time::Instant::now();
            for channel in 0..2 {
                if !active[channel] {
                    continue;
                }
                for ((total, prior_gain), gain) in combined[channel]
                    .iter_mut()
                    .zip(&mut previous[channel])
                    .zip(&gained)
                {
                    add_scaled(total, &gain[channel], 1.0);
                    *prior_gain = gain[channel];
                }
                if combined[channel]
                    .iter()
                    .chain(&previous[channel])
                    .any(|value| !value.is_finite())
                {
                    return Err(LightmapFailure::FillNonFinite);
                }
            }
            crate::engine::logging::info(format_args!(
                "[transport-order] shared_layers=2 order={order} gather_ms={gather_ms:.3} accumulation_ms={:.3}",
                accumulation_started.elapsed().as_secs_f64() * 1_000.0_f64
            ));
        }
        let geometric_samples = receivers
            .len()
            .saturating_mul(options.gather_samples)
            .saturating_mul(usize::from(options.bounces));
        let logical_samples = geometric_samples.saturating_mul(
            active
                .into_iter()
                .filter(|channel_active| *channel_active)
                .count(),
        );
        crate::engine::logging::info(format_args!(
            "[transport-work] shared_layers=2 logical_gather_samples={logical_samples} geometric_gather_samples={geometric_samples}"
        ));
        Ok((combined, logical_samples, cache.occupied_cells()))
    }

    fn bounce_pair_pass(
        &self,
        receivers: &[TransportReceiver],
        values: [&[Accumulator]; 2],
        cache: &RadianceCache,
        options: SolveOptions,
        order: u8,
        active: [bool; 2],
        cancel: Option<&AtomicBool>,
    ) -> Result<Vec<[Accumulator; 2]>, LightmapFailure> {
        let count = options.gather_samples.clamp(1, MAX_BOUNCE_RAYS);
        let inverse_count = 1.0 / f32::from(u16::try_from(count).unwrap_or(u16::MAX));
        let audit = super::diagnostics::enabled().then(GatherAudit::default);
        crate::engine::progress::stage(format!(
            "shared bounce {}/{}",
            order.saturating_add(1),
            options.bounces
        ));
        let directions = super::hemisphere_sequence(count, order);
        let sampled = parallel_map(receivers.len(), options.workers, cancel, |index| {
            let receiver = &receivers[index];
            let geometric_normal = self
                .triangles
                .get(usize::try_from(receiver.surface).unwrap_or(usize::MAX))
                .map_or(receiver.normal, |triangle| triangle.normal);
            let mut result = [Accumulator::default(); 2];
            let (tangent, bitangent) = hemisphere_axes(receiver.normal);
            for local in &directions {
                let direction = hemisphere_from_local(receiver.normal, tangent, bitangent, *local);
                if dot(direction, geometric_normal) <= 0.0 {
                    record_gather(audit.as_ref(), GatherEvent::Horizon);
                    continue;
                }
                record_gather(audit.as_ref(), GatherEvent::Traced);
                let (nearest_hit, throughput) =
                    self.intersect_transport(receiver.ray_origin, direction);
                let Some((distance, surface)) = nearest_hit else {
                    record_gather(audit.as_ref(), GatherEvent::Escaped);
                    if active[0]
                        && order == 0
                        && self.sky_radiance.iter().any(|channel| *channel > 0.0)
                    {
                        let weight = self
                            .sky_radiance
                            .map(|radiance| radiance * 2.0 * inverse_count * BOUNCE_GAIN);
                        accumulate_surface_lobe(
                            &mut result[0],
                            attenuate(scale(weight, throughput), receiver.attenuation),
                            direction,
                            receiver.normal,
                        );
                    }
                    continue;
                };
                let triangle = &self.triangles[surface];
                record_gather(audit.as_ref(), GatherEvent::Hit);
                if dot(direction, triangle.normal) >= 0.0 {
                    record_gather(audit.as_ref(), GatherEvent::Backface);
                    continue;
                }
                let hit_position = add(receiver.ray_origin, scale(direction, distance));
                let stencil = cache.surface_stencil(self, hit_position, surface, receivers);
                if stencil.is_empty() {
                    record_gather(audit.as_ref(), GatherEvent::CacheEmpty);
                } else if stencil.is_fallback() {
                    record_gather(audit.as_ref(), GatherEvent::CacheFallback);
                }
                let mut any_radiance = false;
                for channel in 0..2 {
                    if !active[channel] {
                        continue;
                    }
                    let radiance = stencil.sample(values[channel]);
                    any_radiance |= radiance.iter().any(|value| *value > 0.0);
                    let weight = [
                        triangle.albedo[0] * radiance[0] * 2.0 * inverse_count * BOUNCE_GAIN,
                        triangle.albedo[1] * radiance[1] * 2.0 * inverse_count * BOUNCE_GAIN,
                        triangle.albedo[2] * radiance[2] * 2.0 * inverse_count * BOUNCE_GAIN,
                    ];
                    accumulate_surface_lobe(
                        &mut result[channel],
                        attenuate(scale(weight, throughput), receiver.attenuation),
                        direction,
                        receiver.normal,
                    );
                }
                if !any_radiance {
                    record_gather(audit.as_ref(), GatherEvent::ZeroRadiance);
                }
            }
            result
        })?;
        if let Some(counters) = audit {
            counters.report(
                order,
                active.into_iter().filter(|enabled| *enabled).count(),
                receivers.len().saturating_mul(count),
            );
        }
        Ok(sampled)
    }
}

/// At most eight trilinear representatives, or one nearest visible fallback.
/// Only indices/weights are shared; radiance stays in independent layer buffers.
#[derive(Default)]
pub(super) struct SurfaceStencil {
    entries: [(usize, f32); 8],
    len: usize,
    inverse: f32,
    fallback: bool,
}

impl SurfaceStencil {
    pub(super) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) const fn is_fallback(&self) -> bool {
        self.fallback
    }

    pub(super) fn sample(&self, values: &[Accumulator]) -> [f32; 3] {
        if self.fallback {
            return values
                .get(self.entries[0].0)
                .map_or([0.0; 3], |value| value.surface_light);
        }
        let mut light = [0.0; 3];
        for (index, weight) in &self.entries[..self.len] {
            let Some(value) = values.get(*index) else {
                continue;
            };
            for channel in 0..3 {
                light[channel] += value.surface_light[channel] * weight;
            }
        }
        for channel in &mut light {
            *channel *= self.inverse;
        }
        light
    }
}

impl RadianceCache {
    pub(super) fn surface_stencil(
        &self,
        scene: &TransportScene,
        point: [f32; 3],
        surface: usize,
        receivers: &[TransportReceiver],
    ) -> SurfaceStencil {
        let mut stencil = SurfaceStencil::default();
        if self.cells.is_empty() || !point.iter().all(|value| value.is_finite()) {
            return stencil;
        }
        let Some(target) = SurfaceCacheTarget::new(scene, surface, point) else {
            return stencil;
        };
        let coords =
            std::array::from_fn::<_, 3, _>(|axis| (point[axis] - self.min[axis]) / self.cell - 0.5);
        let base = coords.map(f32::floor);
        let frac =
            std::array::from_fn::<_, 3, _>(|axis| (coords[axis] - base[axis]).clamp(0.0, 1.0));
        let mut weight_sum = 0.0;
        for dz in 0_i16..2_i16 {
            for dy in 0_i16..2_i16 {
                for dx in 0_i16..2_i16 {
                    let weight = (if dx == 0_i16 { 1.0 - frac[0] } else { frac[0] })
                        * (if dy == 0_i16 { 1.0 - frac[1] } else { frac[1] })
                        * (if dz == 0_i16 { 1.0 - frac[2] } else { frac[2] });
                    if weight <= 0.0 {
                        continue;
                    }
                    let Some(cell) = lattice_cell(
                        [
                            base[0] + f32::from(dx),
                            base[1] + f32::from(dy),
                            base[2] + f32::from(dz),
                        ],
                        self.dims,
                    ) else {
                        continue;
                    };
                    let Some(receiver) = self.representative_at(scene, cell, target, receivers)
                    else {
                        continue;
                    };
                    weight_sum += weight;
                    // The 2x2x2 loop inserts at most eight entries; len is
                    // therefore 0..7 before each write to this eight-slot array.
                    stencil.entries[stencil.len] = (receiver, weight);
                    stencil.len = stencil.len.saturating_add(1);
                }
            }
        }
        if weight_sum > 0.0 {
            stencil.inverse = 1.0 / weight_sum;
        } else if let Some(receiver) = self.nearest_stencil_receiver(scene, target, base, receivers)
        {
            stencil.entries[0] = (receiver, 1.0);
            stencil.len = 1;
            stencil.inverse = 1.0;
            stencil.fallback = true;
        }
        stencil
    }

    fn nearest_stencil_receiver(
        &self,
        scene: &TransportScene,
        target: SurfaceCacheTarget<'_>,
        base: [f32; 3],
        receivers: &[TransportReceiver],
    ) -> Option<usize> {
        let mut best: Option<(i16, usize)> = None;
        for dz in -2_i16..=2 {
            for dy in -2_i16..=2 {
                for dx in -2_i16..=2 {
                    let distance = dx.abs().saturating_add(dy.abs()).saturating_add(dz.abs());
                    if best.is_some_and(|(current, _)| distance >= current) {
                        continue;
                    }
                    let Some(cell) = lattice_cell(
                        [
                            base[0] + f32::from(dx),
                            base[1] + f32::from(dy),
                            base[2] + f32::from(dz),
                        ],
                        self.dims,
                    ) else {
                        continue;
                    };
                    if let Some(receiver) = self.representative_at(scene, cell, target, receivers) {
                        best = Some((distance, receiver));
                    }
                }
            }
        }
        best.map(|(_, receiver)| receiver)
    }
}
