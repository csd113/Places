//! Explicit real-scene microkernels separate traversal from gather lookup.
//! This diagnostic never substitutes its samples for the production solve.
use super::super::{
    AtomicBool, Chart, LightmapPatch, RadianceCache, TransportReceiver, TransportScene, add,
    hemisphere_axes, hemisphere_from_local, hemisphere_sequence, length, parallel_map, scale, sub,
};
use std::time::Instant;

pub(in super::super) fn profile_cpu(
    scene: &TransportScene,
    charts: &[(LightmapPatch, Chart)],
    cancel: Option<&AtomicBool>,
    worker_budget: usize,
) -> Result<(), String> {
    let Some(directory) =
        std::env::var_os("PLACES_SOLVER_CPU_PROFILE").map(std::path::PathBuf::from)
    else {
        return Ok(());
    };
    let started = Instant::now();
    let receivers = scene.receivers(charts).map_err(|e| e.name().to_string())?;
    let receivers_ms = ms(started);
    let cache_started = Instant::now();
    let cache = RadianceCache::build(&receivers);
    let cache_ms = ms(cache_started);
    let sampling_started = Instant::now();
    let sequence = hemisphere_sequence(16, 0);
    let count = receivers.len().min(2048);
    let queries: Vec<_> = (0..count)
        .filter_map(|index| {
            receivers.get(
                index
                    .saturating_mul(receivers.len())
                    .checked_div(count.max(1))?,
            )
        })
        .flat_map(|receiver| {
            let (tangent, bitangent) = hemisphere_axes(receiver.normal);
            sequence.iter().map(move |local| {
                (
                    receiver.ray_origin,
                    hemisphere_from_local(receiver.normal, tangent, bitangent, *local),
                )
            })
        })
        .collect();
    let sampling_ms = ms(sampling_started);
    let mut rows = Vec::new();
    for workers in std::iter::once(1).chain((worker_budget > 1).then_some(worker_budget)) {
        rows.push(measure(
            scene, &receivers, &cache, &queries, workers, cancel,
        )?);
    }
    let scene_bytes = scene
        .triangles
        .capacity()
        .saturating_mul(std::mem::size_of::<super::super::TransportTriangle>())
        .saturating_add(
            scene
                .ray_triangles
                .capacity()
                .saturating_mul(std::mem::size_of::<super::super::RayTriangle>()),
        )
        .saturating_add(
            scene
                .legacy_ray_triangles
                .capacity()
                .saturating_mul(std::mem::size_of::<super::super::RayTriangle>()),
        )
        .saturating_add(
            scene
                .ray_nodes
                .capacity()
                .saturating_add(scene.nodes.capacity())
                .saturating_mul(std::mem::size_of::<super::super::BvhNode>()),
        );
    super::write_json(
        &directory,
        "cpu-queries.json",
        &serde_json::json!({
            "diagnostic_only":true,"triangles":scene.triangle_count(),"receivers":receivers.len(),"queries":queries.len(),
            "receiver_build_ms":receivers_ms,"spatial_cache_build_ms":cache_ms,"hemisphere_transform_ms":sampling_ms,
            "primary_triangle_hierarchy_capacity_bytes":scene_bytes,
            "receiver_capacity_bytes":receivers.capacity().saturating_mul(std::mem::size_of::<TransportReceiver>()),
            "one_accumulator_layer_bytes":receivers.len().saturating_mul(std::mem::size_of::<super::super::Accumulator>()),
            "rows":rows,"limits":"timings include kernel output allocation, scheduling and collection; cache lookup includes its own connectivity rays; these are bounded microkernels, not full-bake speedups or exact live heap attribution"
        }),
    )
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn measure(
    scene: &TransportScene,
    receivers: &[TransportReceiver],
    cache: &RadianceCache,
    queries: &[([f32; 3], [f32; 3])],
    workers: usize,
    cancel: Option<&AtomicBool>,
) -> Result<serde_json::Value, String> {
    let started = Instant::now();
    let nearest = parallel_map(queries.len(), workers, cancel, |index| {
        queries
            .get(index)
            .map_or((None, 1.0), |(origin, direction)| {
                scene.intersect_transport(*origin, *direction)
            })
    })
    .map_err(|e| e.name().to_string())?;
    let nearest_ms = ms(started);
    let visibility_started = Instant::now();
    let visibility = parallel_map(queries.len(), workers, cancel, |index| {
        queries.get(index).is_some_and(|(origin, _)| {
            scene
                .emitters
                .get(
                    index
                        .checked_rem(scene.emitters.len())
                        .unwrap_or(usize::MAX),
                )
                .is_some_and(|light| scene.occluded(*origin, light.position))
        })
    })
    .map_err(|e| e.name().to_string())?;
    let visibility_ms = ms(visibility_started);
    let values = vec![[0.2_f32, 0.3, 0.4]; receivers.len()];
    let lookup_started = Instant::now();
    let samples = parallel_map(queries.len(), workers, cancel, |index| {
        let Some((origin, direction)) = queries.get(index) else {
            return [0.0; 3];
        };
        let Some((Some((distance, surface)), _)) = nearest.get(index) else {
            return [0.0; 3];
        };
        cache
            .surface_stencil(
                scene,
                add(*origin, scale(*direction, *distance)),
                *surface,
                receivers,
            )
            .sample(&values)
    })
    .map_err(|e| e.name().to_string())?;
    let lookup_ms = ms(lookup_started);
    let collect_started = Instant::now();
    let checksum: [f64; 3] = samples.iter().fold([0.0_f64; 3], |mut total, value| {
        for (slot, channel) in total.iter_mut().zip(value) {
            *slot += f64::from(*channel);
        }
        total
    });
    let accumulation_ms = ms(collect_started);
    // Preserve query results against optimization and retain their scale.
    let hits = nearest.iter().filter(|(hit, _)| hit.is_some()).count();
    let blockers = visibility.iter().filter(|blocked| **blocked).count();
    let emitter_distance = scene.emitters.first().and_then(|light| {
        queries
            .first()
            .map(|(origin, _)| length(sub(light.position, *origin)))
    });
    Ok(
        serde_json::json!({"workers":workers,"nearest_alpha_transport_ms":nearest_ms,"finite_opaque_mask_visibility_ms":visibility_ms,
        "gather_stencil_connectivity_and_sample_ms":lookup_ms,"output_accumulation_ms":accumulation_ms,"nearest_hits":hits,"finite_blockers":blockers,"checksum":checksum,"example_emitter_distance":emitter_distance}),
    )
}
