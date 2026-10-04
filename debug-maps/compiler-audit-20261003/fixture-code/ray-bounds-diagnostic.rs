//! Exact-output oracles retained from the profiled compiler baseline.
//! Their arithmetic and traversal order deliberately remain unoptimized.
use super::*;

fn random_coordinate(state: &mut u32) -> f32 {
    *state ^= state.wrapping_shl(13);
    *state ^= state.wrapping_shr(17);
    *state ^= state.wrapping_shl(5);
    f32::from((*state >> 16) as u16) / 4096.0 - 8.0
}

#[test]
fn all_worker_budgets_preserve_lightmaps_switchable_layers_and_probe_bits() {
    let (mut scene, mut charts) = two_room_scene(0.7);
    let mut switchable = point([2.5, 2.5, 6.0], 0.8);
    switchable.switchable = Some(7);
    scene.emitters.push(switchable);
    for (_, chart) in &mut charts {
        chart.width = 16;
        chart.height = 16;
    }
    let serial = scene
        .solve_with_probes(&charts, options(2, 3), None, true)
        .expect("serial");
    for workers in [2, 4, 8, 12, 64] {
        let parallel = scene
            .solve_with_probes(
                &charts,
                SolveOptions {
                    workers,
                    ..options(2, 3)
                },
                None,
                true,
            )
            .expect("parallel");
        assert_eq!(
            parallel, serial,
            "all transport records at {workers} requested workers"
        );
    }
}

#[test]
fn cancellation_and_worker_failure_cannot_return_partial_results() {
    for workers in [1, 2, 12] {
        let cancelled = AtomicBool::new(false);
        let result = parallel_map(256, workers, Some(&cancelled), |index| {
            if index == 7 {
                cancelled.store(true, Ordering::Relaxed);
            }
            index
        });
        assert_eq!(result, Err(LightmapFailure::FillSize));
    }
    let result = parallel_map(256, 4, None, |index| {
        assert_ne!(index, 7, "simulated worker failure");
        index
    });
    assert_eq!(result, Err(LightmapFailure::Worker));
}

#[test]
fn prepared_ray_preserves_watertight_distance_bits() {
    let mut state = 0x29b4_5c73;
    for index in 0..50_000 {
        let magnitude = match index % 4 {
            0 => 0.001,
            1 => 1.0,
            2 => 1000.0,
            _ => 1_000_000.0,
        };
        let mut vector = || std::array::from_fn(|_| random_coordinate(&mut state) * magnitude);
        let Some(triangle) = TransportTriangle::new(vector(), vector(), vector(), [0.5; 3]) else {
            continue;
        };
        let origin = if index % 5 == 0 {
            triangle.p0
        } else {
            vector()
        };
        let direction = vector();
        assert_eq!(
            ray_triangle(origin, direction, &triangle).map(f32::to_bits),
            reference_ray_triangle(origin, direction, &triangle).map(f32::to_bits),
            "ray {index} at scale {magnitude}"
        );
        for direction in [[0.0; 3], [1.0, 1.0, 1.0], [0.0, -1.0, 0.0], [f32::NAN; 3]] {
            assert_eq!(
                ray_triangle(origin, direction, &triangle).map(f32::to_bits),
                reference_ray_triangle(origin, direction, &triangle).map(f32::to_bits)
            );
        }
    }
}

#[test]
fn reused_bvh_entries_preserve_nearest_hits_and_opaque_visibility() {
    let mut triangles = floor(-9.0, -9.0, 9.0, 9.0, [0.7; 3]);
    for index in -8_i16..=8 {
        let x = f32::from(index);
        let mut faces = wall(
            [x, 0.0, -8.0],
            [x, 4.0, -8.0],
            [x, 4.0, 8.0],
            [x, 0.0, 8.0],
            [0.5; 3],
        );
        if index % 3 == 0 {
            for triangle in &mut faces {
                triangle.transmissive = true;
            }
        }
        triangles.extend(faces);
    }
    let scene = TransportScene::new(triangles, Vec::new()).expect("scene");
    let mut state = 0x25bc_964f;
    for _ in 0..10_000 {
        let mut vector = || std::array::from_fn(|_| random_coordinate(&mut state));
        let origin = vector();
        let direction = vector();
        let expected = reference_intersect(&scene, origin, direction);
        assert_eq!(
            scene
                .intersect(origin, direction)
                .map(|(distance, index)| (distance.to_bits(), index)),
            expected.map(|(distance, index)| (distance.to_bits(), index))
        );
        let limit = 2.0;
        let opaque = scene.triangles.iter().any(|triangle| {
            !triangle.transmissive
                && reference_ray_triangle(origin, direction, triangle)
                    .is_some_and(|distance| distance > RAY_EPS_M && distance < limit)
        });
        assert_eq!(scene.any_hit(origin, direction, limit), opaque);
        assert_eq!(scene.linear_any_hit(origin, direction, limit), opaque);
    }
}

#[test]
fn accelerated_tree_preserves_original_identity_at_edges_and_coincident_faces() {
    for magnitude in [0.001, 1.0, 1000.0, 1_000_000.0] {
        let mut triangles = floor(-8.0, -8.0, 8.0, 8.0, [0.5; 3]);
        triangles.extend(triangles.clone());
        for index in -5_i16..=5 {
            let x = f32::from(index);
            triangles.extend(wall(
                [x, 0.0, -8.0],
                [x, 8.0, -8.0],
                [x, 8.0, 8.0],
                [x, 0.0, 8.0],
                [0.7; 3],
            ));
        }
        for triangle in &mut triangles {
            triangle.p0 = scale(triangle.p0, magnitude);
            triangle.p1 = scale(triangle.p1, magnitude);
            triangle.p2 = scale(triangle.p2, magnitude);
        }
        let scene = TransportScene::new(triangles, Vec::new()).expect("scaled seams");
        let mut state = 0x881a_322f;
        for index in 0..20_000 {
            let origin = std::array::from_fn(|_| random_coordinate(&mut state) * magnitude);
            let direction = if index % 2 == 0 {
                [0.0, -1.0, 0.0]
            } else {
                std::array::from_fn(|_| random_coordinate(&mut state))
            };
            assert_eq!(
                scene
                    .intersect(origin, direction)
                    .map(|(t, i)| (t.to_bits(), i)),
                reference_intersect(&scene, origin, direction).map(|(t, i)| (t.to_bits(), i)),
                "sample {index} at magnitude {magnitude}"
            );
        }
    }
}

/// Run separately from wall-clock compiler measurements; there are no timing
/// assertions. The preserved original tree supplies an independent baseline.
#[test]
#[ignore = "release ray-kernel profiling on real representative map geometry"]
fn representative_map_ray_benchmark() {
    let mut reports = Vec::new();
    for path in [
        "assets/levels/places_demo.json",
        "assets/levels/lantern_hollow.json",
    ] {
        let mut level =
            crate::level::LevelDef::from_json(&std::fs::read_to_string(path).expect("map"))
                .expect("representative level");
        let catalog =
            crate::loader::PropCatalog::load_from_path(std::path::Path::new("assets/catalog.json"))
                .expect("shipped model catalogue");
        crate::loader::prepare_level(&mut level, catalog.assets(), None);
        let materials = crate::materials::MaterialTable::logical(&level, catalog.assets(), None);
        let mut assets = crate::props::PropAssets::load_default();
        let prepared = crate::render::prepare_level_geometry_with_lightmaps(
            &level,
            &catalog,
            &mut assets,
            &materials,
            crate::render::LightmapBuildOptions::for_lightmaps(
                crate::quality::LightmapQuality::Medium,
            ),
            None,
        );
        let fill = prepared.fill.expect("representative transport scene");
        let scene = &fill.transport;
        let receivers = scene.receivers(&fill.charts).expect("receivers");
        let stride = (receivers.len() / 4096).max(1);
        let mut rays = Vec::new();
        for receiver in receivers.iter().step_by(stride) {
            let mut state = ray_seed(0, 0);
            for _ in 0..32 {
                let (u1, u2) = next_pair(&mut state);
                rays.push((
                    receiver.ray_origin,
                    hemisphere_sample(receiver.normal, u1, u2),
                ));
            }
        }
        for (origin, direction) in &rays {
            assert_eq!(
                scene
                    .intersect(*origin, *direction)
                    .map(|(t, i)| (t.to_bits(), i)),
                reference_intersect(scene, *origin, *direction).map(|(t, i)| (t.to_bits(), i)),
                "origin={origin:?} direction={direction:?}"
            );
        }
        let mut samples = [Vec::new(), Vec::new()];
        for _ in 0..5 {
            for (channel, samples) in samples.iter_mut().enumerate() {
                let started = std::time::Instant::now();
                for (origin, direction) in &rays {
                    let hit = if channel == 0 {
                        reference_intersect(scene, *origin, *direction)
                    } else {
                        scene.intersect(*origin, *direction)
                    };
                    std::hint::black_box(hit);
                }
                samples.push(started.elapsed().as_secs_f64());
            }
        }
        reports.push(
            serde_json::json!({"map":path, "triangles":scene.triangles.len(),
            "rays":rays.len(), "baseline_seconds":samples[0], "optimized_seconds":samples[1],
            "all_hit_bits_and_identities_equal":true}),
        );
    }
    let result = serde_json::to_string_pretty(&reports).expect("benchmark JSON");
    if let Some(directory) = std::env::var_os("PLACES_COMPILER_TEST_EVIDENCE") {
        let path = std::path::Path::new(&directory).join("ray-kernel-benchmark.json");
        std::fs::create_dir_all(path.parent().expect("evidence directory"))
            .expect("create evidence");
        std::fs::write(path, &result).expect("retain measured ray profile");
    }
    crate::logging::info(format_args!("[transport-ray-profile] {result}"));
}

#[test]
fn distance_pruning_preserves_cache_connectivity_and_tie_order() {
    let mut triangles = floor(-4.0, -4.0, 4.0, 4.0, [0.5; 3]);
    triangles.extend(wall(
        [0.0, 0.0, -4.0],
        [0.0, 2.0, -4.0],
        [0.0, 2.0, 4.0],
        [0.0, 0.0, 4.0],
        [0.5; 3],
    ));
    let scene = TransportScene::new(triangles, Vec::new()).expect("divided floor");
    let mut receivers = Vec::new();
    let mut values = Vec::new();
    for z in -6_i16..=6 {
        for x in -6_i16..=6 {
            let position = [f32::from(x) * 0.5, 0.0, f32::from(z) * 0.5];
            let (_, surface) = scene.surface_sample(position, 0.01).expect("floor");
            receivers.push(TransportReceiver {
                position,
                ray_origin: receiver_position(position, [0.0, 1.0, 0.0]),
                normal: [0.0, 1.0, 0.0],
                albedo: [0.5; 3],
                area: 0.25,
                surface: u32::try_from(surface).expect("surface"),
                attenuation: [1.0; 3],
            });
            values.push(Accumulator {
                surface_light: [f32::from(x + 7), f32::from(z + 7), 0.25],
                irradiance: [f32::from(x + z + 14); 3],
                moment: [[f32::from(x - z); 3]; 3],
            });
        }
    }
    let cache = RadianceCache::build(&receivers);
    for z in -9_i16..=9 {
        for x in -9_i16..=9 {
            let point = [f32::from(x) * 0.4, 0.0, f32::from(z) * 0.4];
            let base = std::array::from_fn(|axis| {
                ((point[axis] - cache.min[axis]) / cache.cell - 0.5).floor()
            });
            for surface in 0..scene.triangles.len() {
                let actual = cache
                    .nearest_visible_surface(&scene, point, surface, base, &receivers, &values);
                let expected = reference_nearest_visible_surface(
                    &cache, &scene, point, surface, base, &receivers, &values,
                );
                assert_eq!(
                    actual.surface_light.map(f32::to_bits),
                    expected.surface_light.map(f32::to_bits)
                );
                assert_eq!(
                    actual.irradiance.map(f32::to_bits),
                    expected.irradiance.map(f32::to_bits)
                );
                assert_eq!(
                    actual.moment.map(|row| row.map(f32::to_bits)),
                    expected.moment.map(|row| row.map(f32::to_bits))
                );
            }
        }
    }
}

fn reference_ray_triangle(
    origin: [f32; 3],
    direction: [f32; 3],
    triangle: &TransportTriangle,
) -> Option<f32> {
    let dominant = if direction[0].abs() > direction[1].abs() {
        if direction[0].abs() > direction[2].abs() {
            0
        } else {
            2
        }
    } else if direction[1].abs() > direction[2].abs() {
        1
    } else {
        2
    };
    let depth = f64::from(direction[dominant]);
    if depth.abs() <= f64::MIN_POSITIVE || !depth.is_finite() {
        return None;
    }
    let horizontal = (dominant + 1) % 3;
    let vertical = (horizontal + 1) % 3;
    let shear_x = f64::from(direction[horizontal]) / depth;
    let shear_y = f64::from(direction[vertical]) / depth;
    let project = |point: [f32; 3]| {
        let translated = [
            f64::from(point[0]) - f64::from(origin[0]),
            f64::from(point[1]) - f64::from(origin[1]),
            f64::from(point[2]) - f64::from(origin[2]),
        ];
        [
            translated[horizontal] - shear_x * translated[dominant],
            translated[vertical] - shear_y * translated[dominant],
            translated[dominant] / depth,
        ]
    };
    let a = project(triangle.p0);
    let b = project(triangle.p1);
    let c = project(triangle.p2);
    let edge_a = c[0] * b[1] - c[1] * b[0];
    let edge_b = a[0] * c[1] - a[1] * c[0];
    let edge_c = b[0] * a[1] - b[1] * a[0];
    if (edge_a < 0.0 || edge_b < 0.0 || edge_c < 0.0)
        && (edge_a > 0.0 || edge_b > 0.0 || edge_c > 0.0)
    {
        return None;
    }
    let determinant = edge_a + edge_b + edge_c;
    if determinant.abs() <= f64::MIN_POSITIVE {
        return None;
    }
    let distance = ((edge_a * a[2] + edge_b * b[2] + edge_c * c[2]) / determinant) as f32;
    if !distance.is_finite() {
        return None;
    }
    // At a shared corner the normal offset can leave the origin exactly on
    // the adjoining face. Its winding resolves the boundary: entering solid
    // blocks immediately, leaving the face is a harmless zero-distance hit.
    // With this projection determinant * depth is -dot(ray, geometric normal).
    if distance.abs() < f32::MIN_POSITIVE && determinant * depth > 0.0 {
        Some(f32::MIN_POSITIVE)
    } else {
        Some(distance)
    }
}

fn reference_intersect(
    scene: &TransportScene,
    origin: [f32; 3],
    direction: [f32; 3],
) -> Option<(f32, usize)> {
    if scene.nodes.is_empty() {
        return None;
    }
    let inv = [
        safe_inverse(direction[0]),
        safe_inverse(direction[1]),
        safe_inverse(direction[2]),
    ];
    let mut best: Option<(f32, usize)> = None;
    let mut stack: [u32; 64] = [0; 64];
    let mut depth = 0_usize;
    let mut node_index = 0_u32;
    loop {
        let Some(node) = scene
            .nodes
            .get(usize::try_from(node_index).unwrap_or(usize::MAX))
        else {
            break;
        };
        let limit = best.map_or(f32::INFINITY, |(distance, _)| distance);
        if !slab_hit(node, origin, inv, limit) {
            // fall through to the pop below
        } else if node.count > 0 {
            let start = usize::try_from(node.first).unwrap_or(usize::MAX);
            let end = start.saturating_add(usize::try_from(node.count).unwrap_or(usize::MAX));
            for entry in start..end {
                let Some(index) = scene.order.get(entry) else {
                    continue;
                };
                let Some(triangle) = scene
                    .triangles
                    .get(usize::try_from(*index).unwrap_or(usize::MAX))
                else {
                    continue;
                };
                if triangle.transmissive {
                    continue;
                }
                if let Some(t) = reference_ray_triangle(origin, direction, triangle)
                    && t > RAY_EPS_M
                    && best.is_none_or(|(distance, _)| t < distance)
                {
                    best = Some((t, usize::try_from(*index).unwrap_or(usize::MAX)));
                }
            }
        } else {
            // Visit the nearer child first so the best hit prunes sooner.
            let left = node.first;
            let right = node.right;
            let left_entry = scene
                .nodes
                .get(usize::try_from(left).unwrap_or(usize::MAX))
                .and_then(|child| slab_entry(child, origin, inv, limit));
            let right_entry = scene
                .nodes
                .get(usize::try_from(right).unwrap_or(usize::MAX))
                .and_then(|child| slab_entry(child, origin, inv, limit));
            match (left_entry, right_entry) {
                (Some(left_t), Some(right_t)) => {
                    let (near, far, far_t) = if left_t <= right_t {
                        (left, right, right_t)
                    } else {
                        (right, left, left_t)
                    };
                    if depth >= stack.len() {
                        break;
                    }
                    if let Some(slot) = stack.get_mut(depth) {
                        *slot = far;
                    }
                    depth = depth.saturating_add(1);
                    let _ = far_t;
                    node_index = near;
                    continue;
                }
                (Some(_), None) => {
                    node_index = left;
                    continue;
                }
                (None, Some(_)) => {
                    node_index = right;
                    continue;
                }
                (None, None) => {}
            }
        }
        if depth == 0 {
            break;
        }
        depth = depth.saturating_sub(1);
        node_index = stack.get(depth).copied().unwrap_or(0);
    }
    best
}

fn reference_nearest_visible_surface(
    cache: &RadianceCache,
    scene: &TransportScene,
    point: [f32; 3],
    surface: usize,
    base: [f32; 3],
    receivers: &[TransportReceiver],
    values: &[Accumulator],
) -> Accumulator {
    // No interpolatable representative: fall back to the nearest
    // representative on the correct side within a bounded
    // neighbourhood, then to zero.
    let mut best: Option<(f32, Accumulator)> = None;
    for dz in -2_isize..=2 {
        for dy in -2_isize..=2 {
            for dx in -2_isize..=2 {
                let cell = [
                    base[0] + dx as f32,
                    base[1] + dy as f32,
                    base[2] + dz as f32,
                ];
                let Some(index) = lattice_cell(cell, cache.dims) else {
                    continue;
                };
                let Some(receiver) =
                    reference_representative(cache, scene, index, surface, point, receivers)
                else {
                    continue;
                };
                let Some(value) = values.get(receiver) else {
                    continue;
                };
                let distance = dx.abs() + dy.abs() + dz.abs();
                let distance = f32::from(u16::try_from(distance).unwrap_or(u16::MAX));
                // Keep the nearest/tie ordering: a farther representative
                // cannot replace an already visible one.
                if best.is_some_and(|(current, _)| distance >= current) {
                    continue;
                }
                best = Some((distance, *value));
            }
        }
    }
    best.map_or_else(Accumulator::default, |(_, value)| value)
}

#[test]
fn shared_stencils_preserve_interpolation_and_fallback_bits() {
    let (scene, charts) = two_room_scene(0.7);
    let receivers = scene.receivers(&charts).expect("receivers");
    let cache = RadianceCache::build(&receivers);
    for sign in [0.3, -0.0] {
        let values: Vec<_> = receivers
            .iter()
            .map(|receiver| Accumulator {
                surface_light: if sign == 0.0 {
                    [sign; 3]
                } else {
                    receiver.position.map(|value| value.abs() + sign)
                },
                ..Accumulator::default()
            })
            .collect();
        for x in -5_i16..=45 {
            for z in -5_i16..=85 {
                let point = [f32::from(x) * 0.1, 0.0, f32::from(z) * 0.1];
                for surface in 0..scene.triangles.len() {
                    let actual = cache
                        .surface_stencil(&scene, point, surface, &receivers)
                        .sample(&values);
                    let expected = cache
                        .sample_surface(&scene, point, surface, &receivers, &values)
                        .surface_light;
                    assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
                }
            }
        }
    }
}

fn reference_layer_solve(
    scene: &TransportScene,
    charts: &[(LightmapPatch, Chart)],
    options: SolveOptions,
) -> TransportSolve {
    let mut direct_rays = 0;
    let mut bounce_rays = 0;
    let mut cache_cells = 0;
    let mut probes = None;
    let base_emitters: Vec<_> = scene
        .emitters
        .iter()
        .enumerate()
        .filter(|(_, emitter)| emitter.switchable.is_none() && emitter.intensity > 0.0)
        .map(|(index, _)| index)
        .collect();
    let mut pass = |emitters: &[usize], apply_fill, probes| {
        scene
            .solve_pass(
                charts,
                emitters,
                apply_fill,
                options.taps_per_axis,
                options.bounces,
                options.gather_samples,
                options.workers,
                None,
                &mut direct_rays,
                &mut bounce_rays,
                &mut cache_cells,
                probes,
            )
            .expect("reference pass")
    };
    let base = pass(&base_emitters, true, Some(&mut probes));
    let switchable = scene
        .emitters
        .iter()
        .enumerate()
        .filter_map(|(index, emitter)| {
            emitter
                .switchable
                .map(|light| (light, pass(&[index], false, None)))
        })
        .collect();
    TransportSolve {
        solution: TransportSolution {
            charts: base,
            switchable,
            direct_rays,
            bounce_rays,
            cache_cells,
        },
        probes,
    }
}

#[test]
fn shared_layers_match_independent_reference_for_sky_water_darkness_and_all_bounce_orders() {
    for configuration in 0..4 {
        let (mut scene, charts) = two_room_scene(0.7);
        let mut switchable = point([2.5, 2.5, 6.0], if configuration == 3 { 0.0 } else { 0.8 });
        switchable.switchable = Some(7);
        if configuration > 0 {
            scene.emitters.clear();
        }
        scene.emitters.push(switchable);
        if configuration == 2 {
            scene.sky_radiance = [0.03, 0.01, 0.1];
        }
        if configuration == 3 {
            scene = scene.with_water(vec![TransportWaterBody {
                x0: 0.0,
                x1: 4.0,
                z0: 0.0,
                z1: 8.0,
                surface_y: 1.0,
                bottom_y: -1.0,
                extinction: [0.1, 0.2, 0.3],
            }]);
        }
        for bounces in 0..=3 {
            for workers in [1, 12] {
                let options = SolveOptions {
                    workers,
                    ..options(bounces, 2)
                };
                let expected = reference_layer_solve(&scene, &charts, options);
                let actual = scene
                    .solve_with_probes(&charts, options, None, true)
                    .expect("shared pass");
                assert_eq!(
                    actual, expected,
                    "configuration {configuration}, bounces {bounces}, workers {workers}"
                );
            }
        }
    }
}

fn reference_representative(
    cache: &RadianceCache,
    scene: &TransportScene,
    cell: usize,
    surface: usize,
    point: [f32; 3],
    receivers: &[TransportReceiver],
) -> Option<usize> {
    let hit = scene.triangles.get(surface)?;
    // A reconstructed ray hit can round just behind its own plane. Trace
    // cache connectivity between air-side origins at both ends, rather
    // than allowing that endpoint error to reject the receiving surface.
    let on_plane = sub(
        point,
        scale(hit.normal, dot(sub(point, hit.p0), hit.normal)),
    );
    let target = receiver_position(on_plane, hit.normal);
    let centre = cell_centre(cell, cache.min, cache.cell, cache.dims);
    let candidates = cache
        .cells
        .get(cell)?
        .iter()
        .map(|entry| entry.receiver)
        .filter_map(|index| {
            let receiver = receivers.get(index)?;
            let candidate = scene
                .triangles
                .get(usize::try_from(receiver.surface).ok()?)?;
            if !same_lighting_plane(hit, candidate) {
                return None;
            }
            let delta = sub(receiver.position, centre);
            Some((dot(delta, delta), index))
        });
    // Usually the nearest compatible sample is visible. Test it once;
    // only search the remaining candidates when a real divider blocks it.
    let compare = |(left, li): &(f32, usize), (right, ri): &(f32, usize)| {
        let lp = receivers.get(*li).map_or([0.0; 3], |r| r.position);
        let rp = receivers.get(*ri).map_or([0.0; 3], |r| r.position);
        cache_sample_order(*left, lp, *right, rp, cache.cell)
    };
    let (_, closest) = candidates.clone().min_by(compare)?;
    if let Some(receiver) = receivers.get(closest)
        && !scene.occluded(receiver.ray_origin, target)
    {
        return Some(closest);
    }
    candidates
        .filter(|(_, index)| {
            *index != closest
                && receivers
                    .get(*index)
                    .is_some_and(|receiver| !scene.occluded(receiver.ray_origin, target))
        })
        .min_by(compare)
        .map(|(_, index)| index)
}

#[test]
#[ignore = "inspect captured representative bake ray, independent of bake timing"]
fn captured_bake_ray_bounds_diagnostic() {
    let mut level = crate::level::LevelDef::from_json(&std::fs::read_to_string("assets/levels/lantern_hollow.json").expect("map")).expect("level");
    let catalog = crate::loader::PropCatalog::load_from_path(std::path::Path::new("assets/catalog.json")).expect("catalogue");
    crate::loader::prepare_level(&mut level, catalog.assets(), None);
    let materials = crate::materials::MaterialTable::logical(&level, catalog.assets(), None);
    let mut assets = crate::props::PropAssets::load_default();
    let prepared = crate::render::prepare_level_geometry_with_lightmaps(&level, &catalog, &mut assets, &materials, crate::render::LightmapBuildOptions::for_lightmaps(crate::quality::LightmapQuality::Medium), None);
    let fill = prepared.fill.expect("scene"); let scene = &fill.transport;
    let origin = [-27.137371, -1.2940107e-5, -16.793753];
    let direction = [-0.9036801, -4.3090824e-7, 0.42820823];
    let max_t = 0.6860311;
    let ray = PreparedRay::new(origin, direction).expect("ray");
    let inv = direction.map(safe_inverse);
    let hits:Vec<_> = scene.triangles.iter().enumerate().filter_map(|(index,t)| if t.transmissive {None} else {ray.triangle(t).filter(|t| *t > 0.0 && *t < max_t).map(|distance|(index,distance))}).collect();
    crate::logging::info(format_args!("[captured-ray] hits={hits:?} origin={origin:?} direction={direction:?} inv={inv:?}"));
    for (index,distance) in hits {
        let t=&scene.triangles[index];
        crate::logging::info(format_args!("[captured-ray] triangle={index} t={distance:?} corners={:?}",[t.p0,t.p1,t.p2]));
        let position=scene.ray_triangles.iter().position(|t| usize::try_from(t.surface).ok()==Some(index)).expect("packed triangle");
        let mut node_index=scene.ray_nodes.iter().position(|n| n.count>0 && position>=n.first as usize && position<(n.first+n.count) as usize).expect("leaf");
        loop {
            let node=&scene.ray_nodes[node_index];
            crate::logging::info(format_args!("[captured-ray] node={node_index} min={:?} max={:?} slab={:?}",node.min,node.max,slab_entry(node,origin,inv,max_t)));
            if node_index==0 {break;}
            node_index=scene.ray_nodes.iter().position(|n|n.count==0 && (n.first as usize == node_index || n.right as usize == node_index)).expect("parent");
        }
    }
    assert!(scene.any_hit(origin,direction,max_t));
}

#[test]
fn tight_ray_bounds_do_not_drop_grazing_corner_blockers() {
    let triangle = TransportTriangle::new([-27.854202, -0.3, -16.5], [-27.7573, -0.3, -16.5], [-27.7573, 0.0, -16.5], [0.5;3]).expect("captured blocker");
    let origin = [-27.137371, -1.2940107e-5, -16.793753];
    let direction = [-0.9036801, -4.3090824e-7, 0.42820823];
    let maximum = 0.6860311;
    let scene = TransportScene::new(vec![triangle], Vec::new()).expect("single tight leaf");
    let distance = ray_triangle(origin, direction, &scene.triangles[0]).expect("watertight corner hit");
    assert!(distance > 0.0 && distance < maximum);
    assert!(scene.any_hit(origin, direction, maximum));
    assert!(!scene.any_hit(origin, direction, distance));
}
