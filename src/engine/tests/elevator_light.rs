//! Protect the gameplay-to-shader boundary that previously published an
//! unrestricted point light through closed stock. This tiny CPU check covers
//! rotated/elevated cars, their real eased seal and the packed GPU ABI; it has
//! no assets, device, scene preparation or map bake.
use crate::engine::{
    elevator::{Elevator, ElevatorDef, Phase, Stop},
    render::{
        common::elevators::cabin_light,
        wgpu::world::{DynamicLightsUniform, GpuDynamicLight},
    },
};
use glam::Vec3;

#[test]
fn cabin_light_keeps_the_interior_on_and_tracks_the_actual_seal() -> Result<(), String> {
    let definition = ElevatorDef {
        id: "dark".into(),
        x: 7.0,
        z: -3.0,
        rotation_degrees: 90.0,
        stops: vec![Stop {
            label: "G".into(),
            y: 4.0,
            level: None,
            arrival_elevator: None,
        }],
        initial_stop: 0,
        speed_mps: 1.5,
    };
    let mut elevator = Elevator::new(&definition);
    for (opening, gap) in [(0.0, -0.004), (0.5, 0.3365), (1.0, 0.677)] {
        elevator.opening = opening;
        let light = cabin_light(&elevator);
        assert!(light.is_well_formed());
        assert_eq!(
            light.intensity, 2.0,
            "Closing must not extinguish the interior"
        );
        assert!(light.position.distance(definition.point(0.0, 6.3, 1.2)) < 0.0001);
        let enclosure = light.enclosure.ok_or("Missing cabin visibility")?;
        assert!((enclosure.half_gap - gap).abs() < 0.00001);
        assert_eq!(enclosure.origin, Vec3::new(7.0, 4.0, -3.0));
        let gpu = GpuDynamicLight::new(&light);
        assert_eq!(
            gpu.enclosure_origin_gap,
            [7.0, 4.0, -3.0, enclosure.half_gap]
        );
        assert!(gpu.enclosure_right_enabled[0].abs() < 0.0001);
        assert_eq!(gpu.enclosure_right_enabled[1], -1.0);
        assert_eq!(gpu.enclosure_right_enabled[2], 1.0);
        assert!(gpu.enclosure_doors[2] < gpu.enclosure_doors[0]);
    }
    elevator.phase = Phase::Moving;
    elevator.opening = 0.0;
    elevator.y = 5.0;
    let light = cabin_light(&elevator);
    let gpu = GpuDynamicLight::new(&light);
    assert_eq!(gpu.enclosure_origin_gap[1], 5.0);
    assert_eq!(gpu.enclosure_doors[0], gpu.enclosure_doors[2]);
    assert_eq!(gpu.enclosure_doors[1], gpu.enclosure_doors[3]);
    let mut ordinary_glow = light;
    ordinary_glow.enclosure = None;
    assert_eq!(
        GpuDynamicLight::new(&ordinary_glow).enclosure_right_enabled,
        [0.0; 4]
    );
    assert_eq!(std::mem::size_of::<GpuDynamicLight>(), 112);
    assert_eq!(std::mem::offset_of!(DynamicLightsUniform, lights), 16);
    assert_eq!(std::mem::offset_of!(DynamicLightsUniform, flashlight), 912);
    let receiver = crate::engine::render::common::elevators::landing_light_probe(
        &elevator,
        crate::engine::render::common::elevators::Part::Cabin,
        [7.0, 5.0, -3.0],
    );
    assert!(
        receiver.distance(definition.point(0.0, 5.1, -0.35)) < 0.0001,
        "Moving through unsupported shaft air must retain the departure landing receiver"
    );
    Ok(())
}

/// The native dark capture exposed an unearned 0.1 roomless fallback on doors.
/// One room, one black probe and two untextured triangles protect the receiver
/// override, dark spatial admission and unrelated-object behavior. No assets,
/// atlas solve, native device or shipped map is used.
#[test]
fn landing_receiver_uses_dark_prepared_energy_without_dimming_other_objects() -> Result<(), String>
{
    use crate::engine::{
        level::LevelDef,
        lighting::{
            LevelLighting,
            probes::{ProbeDirectField, ProbeField, ProbeSample},
        },
        render::{DynamicScene, common::dynamic::SpawnOrientation},
    };
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"receiver","name":"Receiver","spawn":{"x":0,"z":-1},"rooms":[{"x":-2,"z":-3,"width":4,"depth":3,"height":3}]}"#)
        .map_err(|error| error.to_string())?;
    let lighting = LevelLighting::bake(&level);
    let field = ProbeField {
        min: [-0.5, 0.6, -0.85],
        cell_m: 1.0,
        dims: [1; 3],
        probes: vec![ProbeSample {
            irradiance: [0.0; 3],
            direction: [0.0; 3],
            axis: [0.5; 2],
            room: 0,
        }],
        local_direct: Some(ProbeDirectField {
            light_indices: Vec::new(),
            probes: vec![crate::engine::lighting::lightmap::LightmapTexel {
                irradiance: [0.0; 3],
                direction: [0.0; 3],
                axis: [0.5; 2],
            }],
        }),
        switchable_indirect: Vec::new(),
    };
    let model = receiver_triangle();
    let mut scene = DynamicScene::new();
    let mesh = scene
        .register_model("receiver", &model, Vec::new(), &[])
        .ok_or("model")?;
    let boundary = scene
        .spawn_registered(mesh, [0.0, 0.0, 0.15], SpawnOrientation::YAW_ONLY, 0.0, 1.0)
        .ok_or("boundary")?;
    let other = scene
        .spawn_registered(mesh, [1.0, 0.0, 0.15], SpawnOrientation::YAW_ONLY, 0.0, 1.0)
        .ok_or("other")?;
    let _before = scene.update_with_field(0.0, Some(&lighting), Some(&field));
    let unchanged_bounds = scene.get(boundary).ok_or("boundary")?.world_bounds();
    assert_eq!(
        scene.get(boundary).ok_or("boundary")?.light_scale(),
        [0.1; 3]
    );
    assert!(scene.set_lighting_receiver(
        boundary,
        Some([0.0, 1.1, -0.35]),
        Some(crate::engine::spatial::Aabb {
            min: [-0.29, 0.03, 0.0],
            max: [0.29, 2.17, 0.0]
        })
    ));
    let _after = scene.update_with_field(0.0, Some(&lighting), Some(&field));
    let receiver = scene.get(boundary).ok_or("boundary")?;
    assert_eq!(
        receiver.light_scale(),
        [0.0; 3],
        "Valid darkness must replace the ambient fallback"
    );
    assert_eq!(
        receiver.world_bounds(),
        unchanged_bounds,
        "Receiver selection never moves geometry"
    );
    assert!(
        receiver.spatial_lighting().is_some(),
        "Unmapped stock must use real direct-light visibility with zero unsupported bounce"
    );
    assert_eq!(
        scene.get(other).ok_or("other")?.light_scale(),
        [0.1; 3],
        "Unrelated objects retain their existing lighting policy"
    );
    assert!(scene.set_lighting_receiver(boundary, None, None));
    let _restored = scene.update_with_field(0.0, Some(&lighting), Some(&field));
    assert_eq!(
        scene.get(boundary).ok_or("boundary")?.light_scale(),
        [0.1; 3]
    );
    Ok(())
}

fn receiver_triangle() -> crate::engine::gltf::PropModel {
    use crate::engine::{
        gltf::{PropModel, PropSubmesh, PropVertex},
        materials::{MaterialAlpha, MaterialEmission, MaterialResponse},
    };
    PropModel {
        vertices: [[-0.3, 0.0, 0.0], [0.3, 0.0, 0.0], [0.3, 2.2, 0.0]]
            .map(|pos| PropVertex {
                pos,
                normal: Some([0.0, 0.0, -1.0]),
                color: [1.0; 4],
                uv: [0.0; 2],
            })
            .to_vec(),
        indices: vec![0, 1, 2],
        submeshes: vec![PropSubmesh {
            double_sided: false,
            response: MaterialResponse::default(),
            material: 0,
            texture: None,
            emission: MaterialEmission::default(),
            alpha: MaterialAlpha::OPAQUE,
            first_index: 0,
            index_count: 3,
        }],
        triangles: 1,
        materials: 1,
        ..PropModel::default()
    }
}
