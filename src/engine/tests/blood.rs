//! Essential regressions: one triangle catches off-surface clipping and swept
//! tunnelling; a one-triangle metadata-only floor catches bounded queues,
//! expiration and disabled/reset leakage. No assets, GPU or map baking.
use super::*;

#[test]
fn clipped_marks_and_swept_segments_stay_on_the_hit_triangle() -> Result<(), String> {
    let points = [Vec3::ZERO, Vec3::X, Vec3::Z];
    let polygon = clip_mark(points, Vec3::new(0.01, 0.0, 0.01), Vec3::Y, 0.12);
    assert!(
        (3..=9).contains(&polygon.len()),
        "Clipped hexagon must have a bounded triangulatable polygon"
    );
    for bary in polygon {
        assert!(
            bary.min_element() >= -1.0e-6 && (bary.element_sum() - 1.0).abs() < 1.0e-5,
            "Every corner stays inside the original triangle after clipping"
        );
    }
    let hit = triangle_hit(Vec3::new(0.2, 4.0, 0.2), Vec3::new(0.0, -8.0, 0.0), points)
        .ok_or("Fast swept segment missed floor")?;
    assert!(
        (hit - 0.5).abs() < 1.0e-6,
        "Hit is halfway through the swept segment"
    );
    assert!(
        triangle_hit(Vec3::new(2.0, 4.0, 2.0), Vec3::new(0.0, -8.0, 0.0), points).is_none(),
        "A ray outside the triangle must miss"
    );
    Ok(())
}

#[test]
fn blood_queues_caps_fade_disable_and_clear_are_bounded() -> Result<(), String> {
    let triangle = crate::engine::lighting::transport::TransportTriangle::new(
        [-4.0, 0.0, -4.0],
        [0.0, 0.0, 4.0],
        [4.0, 0.0, -4.0],
        [1.0; 3],
    )
    .ok_or("Invalid tiny floor")?;
    let world = TransportScene::new(vec![triangle], vec![]).ok_or("Invalid tiny scene")?;
    let characters = CharacterScene::new();
    let dynamic = DynamicScene::new();
    let mut scene = BloodScene::default();
    let request = BloodEmission {
        origin: [0.0, 1.0, 0.0],
        direction: [0.0, -2.0, 0.0],
        droplets: 12,
    };
    scene.emit(request);
    assert!(scene.pending.is_empty(), "Disabled scene refuses emissions");
    scene.set_enabled(true);
    for _ in 0_u8..20 {
        scene.emit(request);
    }
    assert_eq!(
        scene.pending.len(),
        MAX_REQUESTS,
        "Host calls cannot build an unbounded native queue"
    );
    assert_eq!(scene.rejected, 12, "Excess requests are accounted");
    for _ in 0_u8..40 {
        for _ in 0..MAX_REQUESTS {
            scene.emit(request);
        }
        scene.update(0.016, &characters, &dynamic, Some(&world));
        assert!(
            scene.drops.len() <= MAX_DROPS && scene.marks.len() <= MAX_MARKS,
            "Repeated bursts respect both resource caps"
        );
    }
    assert_eq!(
        scene.marks.len(),
        MAX_MARKS,
        "Stress fills bounded oldest-first mark ring"
    );
    let mut vertices = Vec::with_capacity(MAX_VERTICES);
    scene.vertices(&characters, &dynamic, Vec3::Y, &mut vertices);
    assert!(
        !vertices.is_empty() && vertices.len() <= MAX_VERTICES,
        "GPU triangle upload fits fixed allocation"
    );
    scene.drops.clear();
    for mark in &mut scene.marks {
        mark.age = 28.0;
    }
    scene.vertices(&characters, &dynamic, Vec3::Y, &mut vertices);
    assert!(
        vertices
            .iter()
            .all(|v| v.color[3] > 0.0 && v.color[3] < 0.5),
        "Last five seconds fade instead of a hard pop"
    );
    scene.update(31.0, &characters, &dynamic, Some(&world));
    assert!(!scene.has_geometry(), "Both finite lifetimes expire");
    scene.emit(request);
    scene.update(0.016, &characters, &dynamic, Some(&world));
    scene.set_enabled(false);
    assert!(
        !scene.has_geometry() && scene.pending.is_empty(),
        "Disable clears existing and pending effects"
    );
    scene.set_enabled(true);
    scene.emit(request);
    scene.clear();
    assert!(
        scene.pending.is_empty() && !scene.has_geometry(),
        "Reset/unload seam clears queued effects"
    );
    assert!(
        BloodEmission {
            direction: [f32::NAN, 0.0, 0.0],
            ..request
        }
        .validate()
        .is_err(),
        "NaN cannot enter the native queue"
    );
    Ok(())
}

#[test]
fn static_package_surface_index_needs_no_lighting_field() -> Result<(), String> {
    use crate::engine::render::{
        LevelMesh, LevelMeshBatches, LevelMeshRange, MATERIAL_NONE, SurfaceKey, SurfaceKind, Vertex,
    };
    let level=crate::engine::level::LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"blood_surface","name":"Blood surface","spawn":{"x":0,"z":0},"rooms":[{"width":4,"depth":4}]}"#).map_err(|e|e.to_string())?;
    let points = [[-4.0, 0.0, -4.0], [0.0, 0.0, 4.0], [4.0, 0.0, -4.0]];
    let mesh = LevelMesh {
        ranges: vec![LevelMeshRange {
            key: SurfaceKey::new(SurfaceKind::Floor, MATERIAL_NONE),
            vertices: points.map(|p| Vertex::new(p, [1.0; 4], [0.0; 2])).to_vec(),
            indices: vec![0, 1, 2],
            bounds: Aabb {
                min: [-4.0, 0.0, -4.0],
                max: [4.0, 0.0, 4.0],
            },
        }],
        batches: LevelMeshBatches::default(),
        vertex_count: 3,
        index_count: 3,
    };
    let scene = crate::engine::render::common::light_transport::build_surface_scene(
        &level,
        &mesh,
        &[],
        &crate::engine::materials::MaterialTable::default(),
    )
    .ok_or("Missing Static collision index")?;
    assert_eq!(
        scene.emitter_count(),
        0,
        "Presentation collision must never create light sources"
    );
    assert!(
        scene.intersect([0.0, 1.0, 0.0], [0.0, -2.0, 0.0]).is_some(),
        "Static quality floor is hittable without probes or lighting transport"
    );
    Ok(())
}
