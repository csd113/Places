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
    Ok(())
}
