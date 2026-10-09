//! Production-coordinate controls for model-lighting investigation.

use super::*;

fn corridor_charts() -> Vec<(LightmapPatch, Chart)> {
    // Untouched Demo Full charts 1006, 1285 and 1287. Keep their f32 geometry
    // and endpoint grids: a rounded, origin-centred substitute hides the bug.
    [
        (
            PatchKind::Ceiling,
            [60.3, 1.6, -12.3],
            [1.400_001_5, 0.0, 0.0],
            [0.0, 0.0, 15.15],
            24,
            244,
        ),
        (
            PatchKind::Wall,
            [60.3, 1.6, -13.7],
            [0.0, 0.0, 16.55],
            [0.0, -2.5, 0.0],
            266,
            41,
        ),
        (
            PatchKind::Wall,
            [61.7, 1.6, 2.849_999_4],
            [0.0, 0.0, -15.15],
            [0.0, -2.5, 0.0],
            244,
            41,
        ),
    ]
    .into_iter()
    .map(|(kind, origin, u_axis, v_axis, width, height)| {
        (
            LightmapPatch {
                origin,
                u_axis,
                v_axis,
                diagonal_correction: [0.0; 3],
                triangle: false,
                room: None,
                kind,
            },
            Chart {
                page: 0,
                x: 0,
                y: 0,
                width,
                height,
            },
        )
    })
    .collect()
}

fn corridor_scene(sealed: bool) -> TransportScene {
    let mut triangles = Vec::new();
    for (patch, _) in corridor_charts() {
        let [a, b, c, d] = [
            patch.point_at(0.0, 0.0),
            patch.point_at(1.0, 0.0),
            patch.point_at(1.0, 1.0),
            patch.point_at(0.0, 1.0),
        ];
        triangles.extend(wall(a, b, c, d, [0.6; 3]));
    }
    if sealed {
        triangles.extend(wall(
            [60.3, -0.9, -6.0],
            [61.7, -0.9, -6.0],
            [61.7, 1.6, -6.0],
            [60.3, 1.6, -6.0],
            [0.6; 3],
        ));
    }
    TransportScene::new(triangles, vec![point([61.0, 1.59, -5.0], 0.1)])
        .expect("production-scale corridor")
}

#[test]
fn production_corner_witness_exposes_unsafe_support_ray() {
    let triangle = TransportTriangle::new(
        [60.3, 1.6, -13.7],
        [60.3, 1.6, 2.849_999_4],
        [60.3, -0.9, 2.849_999_4],
        [0.6; 3],
    )
    .expect("actual west wall triangle");
    let position = [60.3, 1.599_971_3, -12.237_655];
    let direction = [0.096_267_23, -0.001_371_292_4, 0.995_354_65];
    assert_eq!(
        position[0], triangle.p0[0],
        "witness is on the exact wall plane"
    );
    assert!(direction[0] > 0.0, "ray departs the wall into room air");
    // The supporting-plane intersection is exactly zero. The projected
    // barycentric sum leaves a positive cancellation residual in f64, which
    // the zero-dead-zone kernel correctly cannot distinguish from real t>0.
    let unsafe_hit = ray_triangle(position, direction, &triangle).expect("edge witness");
    assert!(
        unsafe_hit > 0.0 && unsafe_hit < 1.0e-12,
        "a departing zero-distance plane hit acquired cancellation residual {unsafe_hit}"
    );
    let safe = [60.300_056, position[1], position[2]];
    let safe_hit = ray_triangle(safe, direction, &triangle).expect("supporting plane behind ray");
    assert!(
        safe_hit < 0.0,
        "safe-origin supporting plane is behind the ray"
    );
}

#[test]
fn production_corridor_boundary_fill_matches_clear_air_support() {
    let charts = corridor_charts();
    let geometry_scene = corridor_scene(false);
    let receivers = geometry_scene
        .receivers(&charts)
        .expect("corridor receivers");
    let scene = geometry_scene.with_receiver_target(vec![[0.4; 3]; receivers.len()]);
    let filled = scene
        .apply_chart_fill(
            &charts,
            &receivers,
            vec![LightmapTexel::ZERO; receivers.len()],
            1,
            None,
        )
        .expect("boundary recovery fill");
    let mut first = 0;
    for (chart_index, (_, chart)) in charts.iter().enumerate() {
        let width = usize::try_from(chart.width).expect("width");
        let height = usize::try_from(chart.height).expect("height");
        let count = if chart_index == 0 { height } else { width };
        let mut previous: Option<f32> = None;
        for edge_index in 1..count.saturating_sub(1) {
            let index = first
                + if chart_index == 0 {
                    edge_index * width
                } else {
                    edge_index
                };
            let receiver = receivers[index];
            // Every source ray leaves both touching faces into air. The
            // independent oracle is the authored falloff with visibility=1.
            let emitter = &scene.emitters[0];
            let expected = 0.4
                * emitter
                    .falloff
                    .factor(length(sub(emitter.position, receiver.position)) / emitter.range);
            let actual = filled[index].light_at(receiver.normal)[0];
            assert!(
                (actual - expected).abs() < 1.0e-5,
                "chart {chart_index} edge {edge_index}: recovery {actual}, clear-air {expected}; position {:?}, safe origin {:?}",
                receiver.position,
                receiver.ray_origin,
            );
            if let Some(last) = previous {
                assert!(
                    (actual - last).abs() < 0.01,
                    "smooth source support developed a boundary spike: {last} -> {actual}"
                );
            }
            previous = Some(actual);
        }
        first += width * height;
    }
}

#[test]
fn production_corridor_boundary_fill_preserves_a_sealed_blocker() {
    let charts = corridor_charts();
    let geometry_scene = corridor_scene(true);
    let receivers = geometry_scene
        .receivers(&charts)
        .expect("sealed corridor receivers");
    let scene = geometry_scene.with_receiver_target(vec![[0.4; 3]; receivers.len()]);
    let filled = scene
        .apply_chart_fill(
            &charts,
            &receivers,
            vec![LightmapTexel::ZERO; receivers.len()],
            1,
            None,
        )
        .expect("sealed corridor fill");
    let width = usize::try_from(charts[0].1.width).expect("width");
    let height = usize::try_from(charts[0].1.height).expect("height");
    let mut blocked = 0_usize;
    let mut clear = 0_usize;
    for row in 1..height.saturating_sub(1) {
        let index = row * width;
        let receiver = receivers[index];
        let light = filled[index].light_at(receiver.normal)[0];
        if receiver.position[2] < -6.1 {
            assert_eq!(light, 0.0, "recovery fill crossed the sealed divider");
            blocked += 1;
        } else if receiver.position[2] > -5.9 {
            let emitter = &scene.emitters[0];
            let expected = 0.4
                * emitter
                    .falloff
                    .factor(length(sub(emitter.position, receiver.position)) / emitter.range);
            assert!(
                (light - expected).abs() < 1.0e-5,
                "clear side lost recovery fill: {light} versus {expected}"
            );
            clear += 1;
        }
    }
    assert!(
        blocked > 20 && clear > 20,
        "both sides must have many control samples"
    );
}

#[test]
fn visible_footprint_interior_is_detected_when_centre_and_corners_are_blocked() {
    // The actual fridge corner has identical black centre/corner signatures,
    // but its [1,3] 4x4 sample sees the ceiling lights. This independent
    // geometric oracle isolates that trigger failure with a small real opening.
    let [a, b, c] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    let patch =
        LightmapPatch::from_quad(PatchKind::Prop, [a, b, c, c], None).expect("triangular receiver");
    let chart = Chart {
        page: 0,
        x: 0,
        y: 0,
        width: 9,
        height: 13,
    };
    let mut triangles = vec![TransportTriangle::new(a, b, c, [0.6; 3]).expect("receiver triangle")];
    // Four opaque rectangles leave a 15x18mm slot at the one interior
    // footprint sample. All four corner samples and the centre miss the slot.
    for [x0, y0, x1, y1] in [
        [-1.0, -1.0, 2.0, 0.006],
        [-1.0, 0.024, 2.0, 2.0],
        [-1.0, 0.006, 0.039, 0.024],
        [0.054, 0.006, 2.0, 0.024],
    ] {
        triangles.extend(wall(
            [x0, y0, 0.4],
            [x1, y0, 0.4],
            [x1, y1, 0.4],
            [x0, y1, 0.4],
            [0.6; 3],
        ));
    }
    let scene = TransportScene::new(triangles, Vec::new())
        .expect("real slot scene")
        .with_global_lights(vec![crate::lighting::directional::DirectionalLight {
            incoming: [0.0, 0.0, 1.0],
            color: [1.0; 3],
            intensity: 1.0,
            cast_shadows: true,
            angular_radius: 0.0,
        }])
        .with_chart_sample_density(vec![8.0])
        .expect("actual model pitch");
    let solved = scene
        .solve(&[(patch, chart)], options(0, 3), None)
        .expect("slot coverage solve");
    let light = solved.charts[0].texels[0].light_at([0.0, 0.0, 1.0])[0];
    assert!(
        (light - 1.0 / 16.0).abs() < 1.0e-6,
        "one of the 16 physical samples sees the real slot: {light} versus 1/16"
    );
}

fn diagnostic_receiver(
    scene: &TransportScene,
    patch: &LightmapPatch,
    centre: TransportReceiver,
    point: [f32; 3],
    offset: [f32; 2],
) -> TransportReceiver {
    let geometric_normal = patch_normal(patch);
    let (tangent, bitangent) = coverage::surface_axes(geometric_normal);
    let target = add(
        point,
        add(scale(tangent, offset[0]), scale(bitangent, offset[1])),
    );
    let supported = coverage::supported_point(scene, &centre, point, target, &mut Vec::new());
    let position = receiver_position(supported, geometric_normal);
    let same_point = supported
        .iter()
        .zip(point)
        .all(|(left, right)| left.to_bits() == right.to_bits());
    let magnitude = supported
        .iter()
        .fold(1.0_f32, |m, value| m.max(value.abs()));
    let normal = if patch.kind == PatchKind::Prop {
        scene
            .near_surface(
                supported,
                8.0 * SURFACE_OFFSET_M * magnitude,
                false,
                Some(geometric_normal),
            )
            .and_then(|(_, index)| scene.triangles.get(index))
            .map_or(centre.normal, |triangle| {
                triangle.shading_normal_at(supported)
            })
    } else {
        geometric_normal
    };
    TransportReceiver {
        position,
        ray_origin: if same_point {
            centre.ray_origin
        } else {
            position
        },
        normal,
        attenuation: scene.attenuation_at(position),
        ..centre
    }
}

fn diagnostic_taps(scene: &TransportScene, receiver: &TransportReceiver) -> Vec<serde_json::Value> {
    scene
        .emitters
        .iter()
        .enumerate()
        .filter(|(_, emitter)| {
            emitter.switchable.is_none()
                && emitter.intensity > 0.0
                && emitter.reaches(receiver.position)
        })
        .map(|(source, emitter)| {
            let mut taps = Vec::new();
            emitter.shape.for_each_sample(3, |offset| {
                let target = add(emitter.position, offset);
                let delta = sub(target, receiver.ray_origin);
                let distance = length(delta);
                let direction = scale(delta, 1.0 / distance);
                let hit = scene.intersect(receiver.ray_origin, direction);
                let first_hit = hit.map(|(blocker_distance, surface)| {
                    let triangle = scene.triangles[surface];
                    serde_json::json!({
                        "distance_m": blocker_distance,
                        "surface": surface,
                        "corners": [triangle.p0, triangle.p1, triangle.p2],
                        "geometric_normal": triangle.normal,
                    })
                });
                taps.push(serde_json::json!({
                    "target": target,
                    "direction": direction,
                    "target_distance_m": distance,
                    "transmittance": scene.transmittance(receiver.ray_origin, target),
                    "first_opaque_hit": first_hit,
                }));
            });
            let sample = scene.direct_sample(receiver, &[source], false, 3);
            serde_json::json!({
                "source": source,
                "position": emitter.position,
                "surface_rgb": sample.light.surface_light,
                "visibility_signature": sample.visibility,
                "taps": taps,
            })
        })
        .collect()
}

fn production_demo_scene() -> (TransportScene, Vec<(LightmapPatch, Chart)>) {
    production_scene(
        "assets/levels/places_demo.json",
        crate::quality::LightmapQuality::Full,
    )
}

fn production_scene(
    source: &str,
    quality: crate::quality::LightmapQuality,
) -> (TransportScene, Vec<(LightmapPatch, Chart)>) {
    let (scene, charts, _) = production_scene_and_batches(source, quality);
    (scene, charts)
}

fn production_scene_and_batches(
    source: &str,
    quality: crate::quality::LightmapQuality,
) -> (
    TransportScene,
    Vec<(LightmapPatch, Chart)>,
    Vec<crate::render::PropMeshBatch>,
) {
    let mut level = crate::level::LevelDef::from_json(
        &std::fs::read_to_string(source).expect("production semantics"),
    )
    .expect("Demo level");
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
        crate::render::LightmapBuildOptions::for_lightmaps(quality),
        None,
    );
    let fill = prepared.fill.expect("actual prepared production scene");
    let scene = std::sync::Arc::try_unwrap(fill.transport).expect("exclusive diagnostic scene");
    (scene, fill.charts, prepared.build.batches)
}

#[test]
#[ignore = "export exact production chart densities without a bake or GPU"]
fn production_static_chart_density_audit() {
    let output =
        std::env::var_os("PLACES_CHART_AUDIT_OUT").expect("new chart audit output required");
    let root = std::path::PathBuf::from(output);
    assert!(!root.exists(), "keep earlier chart evidence untouched");
    for (label, source) in [
        ("demo", "assets/levels/places_demo.json"),
        ("hallows", "assets/levels/lantern_hollow.json"),
    ] {
        for quality in [
            crate::quality::LightmapQuality::Medium,
            crate::quality::LightmapQuality::Full,
        ] {
            let (scene, charts, batches) = production_scene_and_batches(source, quality);
            assert_eq!(charts.len(), scene.chart_sample_density.len());
            let directory = root.join(label).join(quality.name());
            std::fs::create_dir_all(&directory).expect("audit directory");
            let pitches = scene
                .chart_sample_density
                .iter()
                .map(|density| density.recip())
                .collect::<Vec<_>>();
            let pitch_file =
                std::fs::File::create(directory.join("sample-pitches.json")).expect("pitch file");
            let mut pitch_writer = std::io::BufWriter::new(pitch_file);
            serde_json::to_writer(&mut pitch_writer, &pitches).expect("finite source pitches");
            std::io::Write::flush(&mut pitch_writer).expect("complete pitch file");
            let record = serde_json::json!({"source":source,"quality":quality.name(),"charts":charts,"solver_revision":SOLVER_REVISION});
            let chart_file =
                std::fs::File::create(directory.join("chart-plan.json")).expect("chart file");
            let mut chart_writer = std::io::BufWriter::new(chart_file);
            serde_json::to_writer(&mut chart_writer, &record).expect("finite prepared chart plan");
            std::io::Write::flush(&mut chart_writer).expect("complete chart file");
            std::fs::write(
                directory.join("props.record"),
                crate::package::props::write_props(&batches).expect("prepared prop records"),
            )
            .expect("prop audit file");
        }
    }
}

/// Exact physical panels from the preserved original receipts. Original
/// triangle frames remain independent controls; current atlas IDs are outputs.
#[derive(Clone, Copy)]
struct ProductionPanel {
    label: &'static str,
    corners: [[f32; 3]; 4],
    original_dimensions: [[u32; 2]; 2],
    visible_inset: f32,
}

fn production_panels() -> [ProductionPanel; 3] {
    [
        ProductionPanel {
            label: "cabinet",
            corners: [
                [53.314, -0.789_999_96, 3.707],
                [53.592, -0.789_999_96, 3.707],
                [53.592, -0.209_999_98, 3.707],
                [53.314, -0.209_999_98, 3.707],
            ],
            original_dimensions: [[4, 7], [7, 6]],
            visible_inset: 0.031,
        },
        ProductionPanel {
            label: "sink",
            corners: [
                [54.514, -0.789_999_96, 3.707],
                [54.792, -0.789_999_96, 3.707],
                [54.792, -0.209_999_98, 3.707],
                [54.514, -0.209_999_98, 3.707],
            ],
            original_dimensions: [[4, 7], [7, 6]],
            visible_inset: 0.031,
        },
        ProductionPanel {
            label: "fridge",
            corners: [
                [56.92, 0.300_000_07, 3.83],
                [57.58, 0.300_000_07, 3.83],
                [57.58, 0.89, 3.83],
                [56.92, 0.89, 3.83],
            ],
            original_dimensions: [[7, 9], [9, 6]],
            visible_inset: 0.003,
        },
    ]
}

impl ProductionPanel {
    fn original_domains(self) -> [(LightmapPatch, Chart); 2] {
        let [a, b, c, d] = self.corners;
        let faces = [[a, b, c, c], [a, c, d, d]];
        std::array::from_fn(|side| {
            let [width, height] = self.original_dimensions[side];
            (
                LightmapPatch::from_quad(PatchKind::Prop, faces[side], None)
                    .expect("preserved physical triangle frame"),
                Chart {
                    page: 0,
                    x: 0,
                    y: 0,
                    width,
                    height,
                },
            )
        })
    }

    fn contains(self, point: [f32; 3]) -> bool {
        (point[2] - self.corners[0][2]).abs() < 1.0e-4
            && point[0] >= self.corners[0][0] - 1.0e-4
            && point[0] <= self.corners[2][0] + 1.0e-4
            && point[1] >= self.corners[0][1] - 1.0e-4
            && point[1] <= self.corners[2][1] + 1.0e-4
    }

    fn discovers(self, charts: &[(LightmapPatch, Chart)]) -> Vec<(usize, (LightmapPatch, Chart))> {
        let domains = charts
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, (patch, _))| {
                patch.kind == PatchKind::Prop
                    && dot(patch_normal(patch), [0.0, 0.0, 1.0]) >= 1.0 - 8.0 * f32::EPSILON
                    && [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
                        .into_iter()
                        .all(|(u, v)| self.contains(patch.point_at(u, v)))
            })
            .collect::<Vec<_>>();
        assert!(
            matches!(domains.len(), 1 | 2),
            "{} must resolve its original two triangles or shared quad, found {}",
            self.label,
            domains.len()
        );
        let area = domains
            .iter()
            .map(|(_, (patch, _))| patch.area_m2())
            .sum::<f32>();
        let expected_area =
            (self.corners[2][0] - self.corners[0][0]) * (self.corners[2][1] - self.corners[0][1]);
        assert!(
            (area - expected_area).abs() < 1.0e-5,
            "{} chart discovery did not cover the exact physical panel: {area} vs {expected_area}",
            self.label
        );
        domains
    }

    fn original_grid_point(self, column: u16, row: u16) -> (usize, LightmapPatch, f32, f32) {
        let domains = self.original_domains();
        let width = domains[0].0.u_axis[0];
        let height = domains[0].0.v_axis[1];
        let x = self.visible_inset + (width - 2.0 * self.visible_inset) * f32::from(column) / 11.0;
        let y = self.visible_inset + (height - 2.0 * self.visible_inset) * f32::from(row) / 23.0;
        let nx = x / width;
        let ny = y / height;
        if nx >= ny {
            (0, domains[0].0, nx - ny, ny)
        } else {
            (1, domains[1].0, nx, ny - nx)
        }
    }
}

fn assert_actual_panel_shapes(domains: &[(usize, (LightmapPatch, Chart))]) {
    for (_, (patch, _)) in domains {
        assert_eq!(
            patch.is_triangular(),
            patch.triangle,
            "a prepared shared prop quad must not retain the legacy triangular-family interpretation"
        );
    }
}

fn domain_coordinates(patch: &LightmapPatch, point: [f32; 3]) -> Option<(f32, f32)> {
    let (u, v) = patch.local_of(point);
    if !(-1.0e-5..=1.0 + 1.0e-5).contains(&u)
        || !(-1.0e-5..=1.0 + 1.0e-5).contains(&v)
        || (patch.is_triangular() && u + v > 1.0 + 1.0e-5)
        || length(sub(patch.point_at(u, v), point)) > 1.0e-4
    {
        return None;
    }
    Some((u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)))
}

fn locate_panel_domain(
    domains: &[(usize, (LightmapPatch, Chart))],
    point: [f32; 3],
) -> (usize, LightmapPatch, Chart, f32, f32) {
    domains
        .iter()
        .find_map(|(index, (patch, chart))| {
            let (u, v) = domain_coordinates(patch, point)?;
            Some((*index, *patch, *chart, u, v))
        })
        .expect("the exact physical point must belong to the current panel chart plan")
}

#[test]
fn production_panel_locator_keeps_physical_points_across_shared_and_reordered_charts() {
    for panel in production_panels() {
        let original = panel.original_domains();
        let shared = (
            LightmapPatch::from_quad(PatchKind::Prop, panel.corners, None)
                .expect("same physical quad"),
            original[0].1,
        );
        let unrelated = floor_patch(0.0, 0.0, 1.0, 1.0, 3, 3);
        for charts in [
            vec![unrelated, original[1], original[0]],
            vec![unrelated, shared],
        ] {
            let indexed = charts.iter().copied().enumerate().collect::<Vec<_>>();
            assert_actual_panel_shapes(&indexed);
            let domains = panel.discovers(&charts);
            for (column, row) in [(0, 0), (5, 12), (11, 23)] {
                let (_, reference, u, v) = panel.original_grid_point(column, row);
                let point = reference.point_at(u, v);
                let (_, actual, _, current_u, current_v) = locate_panel_domain(&domains, point);
                assert!(
                    length(sub(actual.point_at(current_u, current_v), point)) < 1.0e-4,
                    "{} world grid changed under chart sharing/reordering",
                    panel.label
                );
            }
        }
    }
}

#[test]
#[ignore = "actual production material-continuity regression; run explicitly"]
fn production_fridge_footprint_crosses_its_textured_material_diagonal() {
    let (scene, charts) = production_demo_scene();
    let panel = production_panels()[2];
    let domains = panel.discovers(&charts);
    assert_actual_panel_shapes(&domains);
    let reference = panel.original_domains()[0];
    let receiver = incident_receiver(&scene, &reference.0, 0.0, 0.0);
    let point = reference.0.point_at(0.0, 0.0);
    let target = add(point, [0.046_875, 0.046_875, 0.0]);
    let _corner_domain = locate_panel_domain(&domains, point);
    let _target_domain = locate_panel_domain(&domains, target);
    let source_triangles = scene
        .triangles
        .iter()
        .filter(|triangle| {
            dot(triangle.normal, [0.0, 0.0, 1.0]) >= 1.0 - 8.0 * f32::EPSILON
                && [triangle.p0, triangle.p1, triangle.p2]
                    .into_iter()
                    .all(|corner| panel.contains(corner))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        source_triangles.len(),
        2,
        "the original physical fridge front triangles must remain present"
    );
    assert!(
        source_triangles[0]
            .albedo
            .iter()
            .zip(source_triangles[1].albedo)
            .any(|(left, right)| (*left - right).abs() > 1.0e-5),
        "the regression must retain the actual textured centroid-colour disagreement"
    );
    let supported = coverage::supported_point(&scene, &receiver, point, target, &mut Vec::new());
    assert!(
        length(sub(supported, target)) < 1.0e-5,
        "one textured primitive is continuous across its geometric diagonal: {supported:?} versus {target:?}"
    );
}

#[test]
fn authored_material_identity_crosses_texture_variation_but_keeps_real_boundaries() {
    for (identities, gap, other_albedo, transmissive, other_normal, crosses) in [
        ([1, 1, 1, 1], 0.0, [0.9; 3], false, [0.0, 1.0, 0.0], true),
        ([1, 1, 2, 2], 0.0, [0.6; 3], false, [0.0, 1.0, 0.0], false),
        ([1, 1, 1, 1], 0.02, [0.6; 3], false, [0.0, 1.0, 0.0], false),
        ([1, 1, 1, 1], 0.0, [0.6; 3], true, [0.0, 1.0, 0.0], false),
        ([1, 1, 1, 1], 0.0, [0.6; 3], false, [0.6, 0.8, 0.0], false),
        ([0, 0, 1, 1], 0.0, [0.6; 3], false, [0.0, 1.0, 0.0], false),
    ] {
        let mut geometry = floor(0.0, 0.0, 1.0, 1.0, [0.6; 3]);
        geometry.extend(
            floor(1.0 + gap, 0.0, 2.0, 1.0, other_albedo)
                .into_iter()
                .map(|triangle| {
                    triangle
                        .with_transmissive(transmissive)
                        .with_shading_normals([other_normal; 3])
                }),
        );
        let scene = TransportScene::new(geometry, Vec::new())
            .expect("material boundary scene")
            .with_surface_materials(identities.to_vec())
            .expect("aligned material identities");
        let point = [0.9, 0.0, 0.5];
        let (albedo, surface) = scene.surface_sample(point, 1.0e-5).expect("owning surface");
        let receiver = TransportReceiver {
            position: receiver_position(point, [0.0, 1.0, 0.0]),
            ray_origin: receiver_position(point, [0.0, 1.0, 0.0]),
            normal: [0.0, 1.0, 0.0],
            albedo,
            surface: u32::try_from(surface).expect("surface ID"),
            area: 1.0,
            attenuation: [1.0; 3],
        };
        let target = [1.1, 0.0, 0.5];
        let supported =
            coverage::supported_point(&scene, &receiver, point, target, &mut Vec::new());
        if crosses {
            assert!(
                length(sub(supported, target)) < 1.0e-6,
                "authored texture variation lost contiguous support: {supported:?}"
            );
        } else {
            assert!(
                (supported[0] - 1.0).abs() < 1.0e-4,
                "tap crossed a real material, gap, transmissive or normal boundary: {supported:?}"
            );
        }
    }
}

#[test]
fn diffuse_filter_uses_authored_material_identity_without_crossing_distinct_materials() {
    let charts = [
        floor_patch(0.0, 0.0, 1.0, 1.0, 3, 3),
        floor_patch(1.0, 0.0, 2.0, 1.0, 3, 3),
    ];
    for (identities, other_albedo, crosses) in [
        (vec![1, 1, 1, 1], [0.9; 3], true),
        (vec![1, 1, 2, 2], [0.6; 3], false),
        (Vec::new(), [0.9; 3], false),
    ] {
        let mut geometry = floor(0.0, 0.0, 1.0, 1.0, [0.6; 3]);
        geometry.extend(floor(1.0, 0.0, 2.0, 1.0, other_albedo));
        let geometry_scene =
            TransportScene::new(geometry, Vec::new()).expect("diffuse boundary scene");
        let scene = if identities.is_empty() {
            geometry_scene
        } else {
            geometry_scene
                .with_surface_materials(identities)
                .expect("aligned material identities")
        };
        let receivers = scene
            .receivers(&charts)
            .expect("diffuse boundary receivers");
        let values = (0_usize..18_usize)
            .map(|index| {
                let energy = if index < 9_usize { 0.0 } else { 1.0 };
                Accumulator {
                    irradiance: [energy; 3],
                    surface_light: [energy; 3],
                    ..Accumulator::default()
                }
            })
            .collect::<Vec<_>>();
        let filtered = filter::filter_accumulators(
            &scene,
            &charts,
            &receivers,
            &values,
            &vec![Accumulator::default(); 18],
        )
        .expect("diffuse material-aware filter");
        let edge_light = filtered[5].light_at([0.0, 1.0, 0.0])[0];
        if crosses {
            assert!(
                edge_light > 0.0,
                "incident diffuse light did not cross authored texture variation"
            );
        } else {
            assert_eq!(
                edge_light, 0.0,
                "diffuse light crossed a real or untagged material boundary"
            );
        }
    }
}

/// Rebuilds the real scene and chart plan, but performs no lightmap bake or
/// GPU capture. Redirect the single JSON report from the test's stderr.
#[test]
#[ignore = "focused production kitchen visibility attribution; run explicitly with --nocapture"]
#[expect(
    clippy::print_stderr,
    reason = "The explicitly requested ignored diagnostic emits one machine-readable report."
)]
fn production_kitchen_direct_visibility_attribution() {
    let (mut scene, charts) = production_demo_scene();
    let densities = scene.chart_sample_density.clone();
    let sources = scene
        .emitters
        .iter()
        .enumerate()
        .filter(|(_, emitter)| emitter.switchable.is_none())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let panels = production_panels();
    let named_anchors: [(&str, usize, usize, usize, usize); 11] = [
        ("cabinet-right-hidden-edge-original12573-2-2", 0, 0, 2, 2),
        ("cabinet-bright-interior-original12573-1-2", 0, 0, 1, 2),
        ("cabinet-right-rail-shadow-original12573-1-3", 0, 0, 1, 3),
        ("cabinet-hidden-top-rail-original12574-1-4", 0, 1, 1, 4),
        ("cabinet-left-bright-interior-original12574-2-2", 0, 1, 2, 2),
        ("cabinet-top-rail-shadow-original12574-3-2", 0, 1, 3, 2),
        ("sink-right-rail-shadow-original13365-1-3", 1, 0, 1, 3),
        ("sink-top-rail-shadow-original13366-3-2", 1, 1, 3, 2),
        (
            "fridge-original-material-diagonal-corner15529-0-0",
            2,
            0,
            0,
            0,
        ),
        ("fridge-right-bright-interior-original15529-2-2", 2, 0, 2, 2),
        ("fridge-left-bright-interior-original15530-2-2", 2, 1, 2, 2),
    ];
    let mut reports = Vec::new();
    for (name, panel_index, side, column, row) in named_anchors {
        let panel = panels[panel_index];
        let actual_domains = panel.discovers(&charts);
        assert_actual_panel_shapes(&actual_domains);
        let reference = panel.original_domains()[side];
        let (patch, chart) = reference;
        let width = usize::try_from(chart.width).expect("original width");
        let height = usize::try_from(chart.height).expect("original height");
        let point = patch.point_at(texel_axis(column, width), texel_axis(row, height));
        let (chart_index, current_patch, current_chart, current_u, current_v) =
            locate_panel_domain(&actual_domains, point);
        let current_density = densities[chart_index];
        // The named physical sample, original safe origin and 125mm stencil
        // remain exact controls even when current atlas endpoints change.
        scene.chart_sample_density = vec![8.0];
        let receivers = scene
            .receivers(&[reference])
            .expect("preserved reference receivers");
        let index = row * width + column;
        let centre = receivers[index];
        let centre_sample = scene.direct_sample(&centre, &sources, true, 3);
        let (refined, _) =
            coverage::direct_pass(&scene, &[reference], &receivers, &sources, true, 3, 1, None)
                .expect("original-frame physical footprint control");
        let (current_refined, _) = footprint_oracle(
            &scene,
            &current_patch,
            centre,
            point,
            current_density.recip(),
            &sources,
        );
        let mut footprint = Vec::new();
        for sample_v in 0_u16..4 {
            for sample_u in 0_u16..4 {
                let offset = [
                    ((f32::from(sample_u) + 0.5) / 4.0 - 0.5) * 0.125,
                    ((f32::from(sample_v) + 0.5) / 4.0 - 0.5) * 0.125,
                ];
                let receiver = diagnostic_receiver(&scene, &patch, centre, point, offset);
                let sample = scene.direct_sample(&receiver, &sources, true, 3);
                footprint.push(serde_json::json!({
                    "grid": [sample_u, sample_v], "offset_m": offset,
                    "position": receiver.position, "ray_origin": receiver.ray_origin,
                    "normal": receiver.normal, "surface_rgb": sample.light.surface_light,
                    "visibility_signature": sample.visibility,
                    "sources": diagnostic_taps(&scene, &receiver),
                }));
            }
        }
        reports.push(serde_json::json!({
            "physical_anchor": name, "panel": panel.label, "chart": chart_index,
            "reference_frame_side": side, "column": column, "row": row,
            "original_endpoint_dimensions": [chart.width, chart.height],
            "rectangle": [current_chart.x, current_chart.y, current_chart.width, current_chart.height],
            "actual_uv": [current_u, current_v], "density_per_m": current_density,
            "reference_density_per_m": 8.0_f32, "position": centre.position,
            "ray_origin": centre.ray_origin, "normal": centre.normal, "surface": centre.surface,
            "centre_surface_rgb": centre_sample.light.surface_light,
            "refined_surface_rgb": refined[index].surface_light,
            "actual_pitch_refined_surface_rgb": current_refined,
            "centre_visibility_signature": centre_sample.visibility,
            "centre_sources": diagnostic_taps(&scene, &centre), "footprint": footprint,
        }));
    }
    eprintln!(
        "[model-lighting-attribution] {}",
        serde_json::json!({
            "map": "assets/levels/places_demo.json", "quality": "full", "samples": reports,
            "first_hit_contract": "nearest opaque hit may be beyond emitter; transmittance is the actual finite-segment visibility",
            "solver_revision": SOLVER_REVISION,
            "footprint_contract": "named original physical anchors and complete125mm 4x4 control; actual plan IDs/pitch are separately discovered outputs",
        })
    );
}

fn incident_receiver(
    scene: &TransportScene,
    patch: &LightmapPatch,
    u: f32,
    v: f32,
) -> TransportReceiver {
    let point = patch.point_at(u, v);
    let geometric_normal = patch_normal(patch);
    let (_, surface) = scene
        .near_surface(point, 0.2, false, Some(geometric_normal))
        .expect("actual point surface");
    let triangle = scene.triangles[surface];
    let position = receiver_position(point, geometric_normal);
    TransportReceiver {
        position,
        ray_origin: receiver_ray_origin(patch, u, v, geometric_normal),
        normal: triangle.shading_normal_at(point),
        albedo: triangle.albedo,
        surface: u32::try_from(surface).expect("surface ID"),
        attenuation: scene.attenuation_at(position),
        area: 1.0,
    }
}

fn interpolated_incident(
    texels: &[LightmapTexel],
    chart: Chart,
    u: f32,
    v: f32,
    normal: [f32; 3],
) -> [f32; 3] {
    let width = usize::try_from(chart.width).expect("width");
    let height = usize::try_from(chart.height).expect("height");
    let x0 = (0..width)
        .rfind(|x| texel_axis(*x, width) <= u)
        .unwrap_or(0);
    let y0 = (0..height)
        .rfind(|y| texel_axis(*y, height) <= v)
        .unwrap_or(0);
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = if x0 == x1 {
        0.0
    } else {
        (u - texel_axis(x0, width)) / (texel_axis(x1, width) - texel_axis(x0, width))
    };
    let ty = if y0 == y1 {
        0.0
    } else {
        (v - texel_axis(y0, height)) / (texel_axis(y1, height) - texel_axis(y0, height))
    };
    let mut combined = LightmapTexel::ZERO;
    for (x, y, weight) in [
        (x0, y0, (1.0 - tx) * (1.0 - ty)),
        (x1, y0, tx * (1.0 - ty)),
        (x0, y1, (1.0 - tx) * ty),
        (x1, y1, tx * ty),
    ] {
        let texel = texels[y * width + x];
        for channel in 0..3 {
            combined.irradiance[channel] += texel.irradiance[channel] * weight;
            combined.direction[channel] += texel.direction[channel] * weight;
        }
    }
    combined.light_at(normal)
}

fn footprint_oracle(
    scene: &TransportScene,
    patch: &LightmapPatch,
    receiver: TransportReceiver,
    point: [f32; 3],
    sample_spacing: f32,
    sources: &[usize],
) -> ([f32; 3], bool) {
    let centre = scene.direct_sample(&receiver, sources, true, 3);
    let mut integral = [0.0; 3];
    let mut corners_match = true;
    for sample_v in 0_u16..4 {
        for sample_u in 0_u16..4 {
            let offset = [
                ((f32::from(sample_u) + 0.5) / 4.0 - 0.5) * sample_spacing,
                ((f32::from(sample_v) + 0.5) / 4.0 - 0.5) * sample_spacing,
            ];
            let sample_receiver = diagnostic_receiver(scene, patch, receiver, point, offset);
            let sample = scene.direct_sample(&sample_receiver, sources, true, 3);
            for (value, contribution) in integral.iter_mut().zip(sample.light.surface_light) {
                *value += contribution / 16.0;
            }
            if [0, 3].contains(&sample_u) && [0, 3].contains(&sample_v) {
                corners_match &= sample.visibility == centre.visibility;
            }
        }
    }
    (integral, corners_match)
}

struct PanelReconstruction {
    plan_index: usize,
    patch: LightmapPatch,
    chart: Chart,
    density: f32,
    texels: Vec<LightmapTexel>,
}

fn solve_panel_reconstruction(
    scene: &mut TransportScene,
    domains: &[(usize, (LightmapPatch, Chart))],
    sources: &[usize],
    production_densities: &[f32],
    diagnostic_density: Option<f32>,
) -> Vec<PanelReconstruction> {
    domains
        .iter()
        .map(|(plan_index, (patch, chart))| {
            let density = diagnostic_density.unwrap_or(production_densities[*plan_index]);
            let solved_chart = if diagnostic_density.is_some() {
                let config = crate::lighting::lightmap::LightmapConfig {
                    texels_per_metre: density,
                    ..crate::lighting::lightmap::LightmapConfig::for_profile(
                        crate::quality::QualityProfile::Full,
                    )
                };
                let (width, height) = config.chart_texels(patch);
                Chart {
                    width,
                    height,
                    ..*chart
                }
            } else {
                *chart
            };
            let domain = (*patch, solved_chart);
            scene.chart_sample_density = vec![density];
            let receivers = scene
                .receivers(&[domain])
                .expect("physical diagnostic endpoint receivers");
            let (direct, _) =
                coverage::direct_pass(scene, &[domain], &receivers, sources, true, 3, 1, None)
                    .expect("same geometry and sources at diagnostic endpoints");
            let texels = direct
                .iter()
                .zip(&receivers)
                .map(|(sample, receiver)| compress_surface(sample, receiver.normal))
                .collect::<Vec<_>>();
            PanelReconstruction {
                plan_index: *plan_index,
                patch: *patch,
                chart: solved_chart,
                density,
                texels,
            }
        })
        .collect()
}

fn locate_reconstruction(
    reconstructions: &[PanelReconstruction],
    point: [f32; 3],
) -> (&PanelReconstruction, f32, f32) {
    reconstructions
        .iter()
        .find_map(|reconstruction| {
            let (u, v) = domain_coordinates(&reconstruction.patch, point)?;
            Some((reconstruction, u, v))
        })
        .expect("current physical panel reconstruction must cover the unchanged world point")
}

/// Compare exact original 12x24 world grids with original triangle controls,
/// the current actual atlas plan and diagnostic 8/16/32 endpoint charts.
/// Discovery supports split triangles or shared quads; geometry/lights stay fixed.
#[test]
#[ignore = "measure kitchen visible-surface interpolation and physical footprint aliasing"]
#[expect(
    clippy::print_stderr,
    reason = "The explicitly requested ignored diagnostic emits one machine-readable report."
)]
fn production_kitchen_visible_surface_interpolation_control() {
    let (mut scene, charts) = production_demo_scene();
    let densities = scene.chart_sample_density.clone();
    let sources = scene
        .emitters
        .iter()
        .enumerate()
        .filter(|(_, source)| source.switchable.is_none())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut panels = Vec::new();
    for panel in production_panels() {
        let current_domains = panel.discovers(&charts);
        assert_actual_panel_shapes(&current_domains);
        let original_domains = panel
            .original_domains()
            .into_iter()
            .enumerate()
            .collect::<Vec<_>>();
        let original_control =
            solve_panel_reconstruction(&mut scene, &original_domains, &sources, &[8.0, 8.0], None);
        let actual_plan =
            solve_panel_reconstruction(&mut scene, &current_domains, &sources, &densities, None);
        let mut reconstructions = Vec::new();
        for density in [8.0_f32, 16.0, 32.0] {
            reconstructions.push((
                density,
                solve_panel_reconstruction(
                    &mut scene,
                    &current_domains,
                    &sources,
                    &densities,
                    Some(density),
                ),
            ));
        }
        let mut samples = Vec::new();
        for row in 0_u16..24 {
            for column in 0_u16..12 {
                let (side, patch, u, v) = panel.original_grid_point(column, row);
                let receiver = incident_receiver(&scene, &patch, u, v);
                let point = patch.point_at(u, v);
                let exact = scene
                    .direct_sample(&receiver, &sources, true, 3)
                    .light
                    .surface_light;
                let reference = &original_control[side];
                let original_interpolated = interpolated_incident(
                    &reference.texels,
                    reference.chart,
                    u,
                    v,
                    receiver.normal,
                );
                let (actual, actual_u, actual_v) = locate_reconstruction(&actual_plan, point);
                let actual_interpolated = interpolated_incident(
                    &actual.texels,
                    actual.chart,
                    actual_u,
                    actual_v,
                    receiver.normal,
                );
                let density_reconstructions = reconstructions.iter().map(|(density, domains)| {
                    let (candidate, local_u, local_v) = locate_reconstruction(domains, point);
                    serde_json::json!({
                        "density_per_m": density,
                        "endpoint_dimensions": [candidate.chart.width, candidate.chart.height],
                        "physical_pitch_m": density.recip(),
                        "interpolated_rgb": interpolated_incident(
                            &candidate.texels, candidate.chart, local_u, local_v, receiver.normal,
                        ),
                    })
                }).collect::<Vec<_>>();
                let (narrow, _) =
                    footprint_oracle(&scene, &patch, receiver, point, 0.004, &sources);
                let (wide, stable) =
                    footprint_oracle(&scene, &patch, receiver, point, 0.125, &sources);
                let adaptive = if stable { exact } else { wide };
                let normal_target = add(receiver.ray_origin, scale(patch_normal(&patch), 0.2));
                let visible_along_normal =
                    scene.transmittance(receiver.ray_origin, normal_target) > 0.0;
                samples.push(serde_json::json!({
                    "grid": [column, row], "position": receiver.position,
                    "chart": actual.plan_index, "uv": [u, v], "actual_uv": [actual_u, actual_v],
                    "reference_frame_side": side, "surface": receiver.surface,
                    "surface_albedo": receiver.albedo,
                    "visible_along_geometric_normal": visible_along_normal,
                    "centre_rgb": exact, "narrow_4mm_rgb": narrow,
                    "wide_forced_4x4_rgb": wide, "wide_corners_match_centre": stable,
                    "wide_adaptive_rgb": adaptive, "coarse_interpolated_rgb": original_interpolated,
                    "actual_plan_interpolated_rgb": actual_interpolated,
                    "actual_density_per_m": actual.density,
                    "density_reconstructions": density_reconstructions,
                }));
            }
        }
        let coplanar = scene
            .triangles
            .iter()
            .enumerate()
            .filter(|(_, triangle)| {
                dot(triangle.normal, [0.0, 0.0, 1.0]) >= 1.0 - 8.0 * f32::EPSILON
                    && [triangle.p0, triangle.p1, triangle.p2]
                        .into_iter()
                        .all(|point| panel.contains(point))
            })
            .map(|(surface, triangle)| {
                serde_json::json!({
                    "surface": surface, "albedo": triangle.albedo,
                    "corners": [triangle.p0, triangle.p1, triangle.p2],
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            coplanar.len(),
            2,
            "{} source panel geometry changed",
            panel.label
        );
        let actual_domains = actual_plan
            .iter()
            .map(|domain| {
                serde_json::json!({
                    "chart": domain.plan_index, "patch": domain.patch,
                    "endpoint_dimensions": [domain.chart.width, domain.chart.height],
                    "density_per_m": domain.density,
                })
            })
            .collect::<Vec<_>>();
        panels.push(serde_json::json!({
            "label": panel.label, "visible_inset_m": panel.visible_inset,
            "original_corners": panel.corners, "actual_domains": actual_domains,
            "coplanar_surface_albedos": coplanar, "samples": samples,
        }));
    }
    eprintln!(
        "[model-lighting-fine-grid] {}",
        serde_json::json!({
            "map": "assets/levels/places_demo.json", "quality": "full", "solver_revision": SOLVER_REVISION,
            "oracle_contract": "exact original world grid/triangle8m controls and125mm footprint; separate discovered actual plan and current-topology8/16/32m diagnostic reconstructions; geometry and source strengths fixed",
            "panels": panels,
        })
    );
}
