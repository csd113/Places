//! Protects live direct RGB recomposition and initially dark source activation.
//! Unlike the metadata roundtrip tests, these inspect the actual receiver field.
//! Tiny source-only worlds prepare at most a few thousand cell records, no atlas,
//! assets, native window or GPU resources; no shipped map participates.

use super::{ReceiverField, SourceKind};
use crate::engine::level::{LevelDef, WaterVolumes};
use crate::engine::lighting::LevelLighting;

fn field(json: &str) -> Result<ReceiverField, String> {
    let level = LevelDef::from_json(json).map_err(|error| error.to_string())?;
    Ok(field_from_level(&level))
}

fn field_from_level(level: &LevelDef) -> ReceiverField {
    let lighting = LevelLighting::bake(level);
    let volumes = WaterVolumes::from_level(level);
    let mut result = ReceiverField::build(level, &lighting, volumes.volumes());
    result.compose();
    assert!(
        result
            .sources
            .iter()
            .flat_map(|source| &source.factors)
            .all(|factor| {
                usize::try_from(factor.sample).is_ok_and(|sample| {
                    result
                        .cells
                        .iter()
                        .any(|cell| !cell.bounds.is_empty() && cell.corners.contains(&sample))
                })
            }),
        "every positive source node belongs to a full nonempty certificate cell; separate dynamic point rays are redundant"
    );
    result
}

#[test]
fn mixed_direct_sources_recompose_dark_activation_dim_colour_and_off() -> Result<(), String> {
    let mut field = field(
        r#"{"schema": "places-level", "format_version":1,"id":"caustics_field","name":"Caustics field",
        "spawn":{"x":0,"z":0},"rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":-1,"height":4}],
        "water":[{"x":-0.8,"z":-0.8,"width":1.6,"depth":1.6,"surface_y":0,"bottom_y":-1}],
        "ceiling_lights":[
            {"id":"focused","fixture":"core:pool_light_round","x":-0.2,"z":0,"y":2,"enabled":false,"brightness":0,"color":[0,0,0]},
            {"id":"blue","fixture":"core:pool_light_round","x":0.2,"z":0,"y":2,"brightness":1,"color":[0,0,1]},
            {"id":"excluded","fixture":"core:fluorescent_panel_01","x":0,"z":0,"y":2,"brightness":8,"color":[0,1,0]}
        ]}"#,
    )?;
    let dark = field
        .sources
        .iter()
        .find(|source| source.id == "focused")
        .ok_or("dark source missing")?;
    assert!(
        field.has_active_sources(),
        "active direct bases request movable visibility"
    );
    assert!(
        !dark.factors.is_empty(),
        "an initially off, zero-intensity black source retains its spatial basis"
    );
    assert!(
        field.samples.iter().any(|sample| sample[2] > 0.0),
        "the active blue source reaches receivers"
    );
    assert!(
        field
            .samples
            .iter()
            .all(|sample| sample[0].abs() < f32::EPSILON && sample[1].abs() < f32::EPSILON),
        "ambient, baseline and excluded fluorescent green never enter caustic RGB"
    );
    let factors = field.factor_count();
    let blue: Vec<_> = field.samples.iter().map(|sample| sample[2]).collect();
    assert_eq!(
        field.set_source("focused", Some(true), Some(2.0), Some([1.0, 0.0, 0.0])),
        Some(true),
        "activation changes source energy"
    );
    field.compose();
    let bright_red: Vec<_> = field.samples.iter().map(|sample| sample[0]).collect();
    assert!(
        bright_red.iter().any(|channel| *channel > 0.0),
        "activation uses the pre-existing direct basis"
    );
    assert_eq!(
        field.set_source("focused", None, Some(0.5), None),
        Some(true),
        "dimming changes energy"
    );
    field.compose();
    assert!(
        field
            .samples
            .iter()
            .zip(&bright_red)
            .all(|(sample, before)| (sample[0] - before * 0.25).abs() < 1.0e-6),
        "source intensity scales only that source's direct RGB"
    );
    assert_eq!(
        field.set_source("focused", None, None, Some([0.0, 1.0, 0.0])),
        Some(true),
        "recolour changes energy"
    );
    field.compose();
    assert!(
        field
            .samples
            .iter()
            .zip(&blue)
            .all(|(sample, before)| sample[0].abs() < f32::EPSILON
                && (sample[2] - before).abs() < f32::EPSILON),
        "recolour removes red without changing the unrelated blue source"
    );
    assert!(
        field.samples.iter().any(|sample| sample[1] > 0.0),
        "eligible source now contributes green"
    );
    check_turn_off_and_validation(&mut field, factors);
    Ok(())
}

fn check_turn_off_and_validation(field: &mut ReceiverField, factors: usize) {
    assert_eq!(
        field.set_source("focused", Some(false), None, None),
        Some(true),
        "focused source turns off"
    );
    assert_eq!(
        field.set_source("blue", Some(false), None, None),
        Some(true),
        "blue source turns off"
    );
    field.compose();
    assert!(
        field.samples.iter().all(|sample| sample
            .iter()
            .take(3)
            .all(|channel| channel.abs() < f32::EPSILON)),
        "all eligible sources off leaves exactly zero modulated energy"
    );
    assert!(
        !field.has_active_sources(),
        "excluded sources cannot keep movable visibility updates active"
    );
    assert_eq!(
        field.factor_count(),
        factors,
        "live controls never rebuild geometry or visibility"
    );
    assert_eq!(
        field.set_source("blue", Some(false), None, None),
        Some(false),
        "valid no-op is recognized without an upload"
    );
    assert_eq!(
        field.set_source("excluded", Some(false), None, None),
        Some(false),
        "excluded known sources remain addressable without changing composed energy"
    );
    assert_eq!(
        field.set_source("focused", None, Some(f32::NAN), None),
        None,
        "nonfinite controls are rejected"
    );
}

#[test]
fn dark_sun_retains_visible_basis_while_moon_stays_excluded() -> Result<(), String> {
    let mut level = LevelDef::from_json(
        r#"{"schema": "places-level", "format_version":1,"id":"caustics_sun","name":"Caustics sun",
        "spawn":{"x":0,"z":0},
        "water":[{"shape":"circle","x":-0.5,"z":-0.5,"radius":0.5,"surface_y":0,"bottom_y":-1}],
        "global_illuminators":[
            {"id":"sun","kind":"sun","direction":[0,-1,0],"enabled":false,"intensity":0,"color":[0,0,0]},
            {"id":"moon","kind":"moon","direction":[0,-1,0],"intensity":8,"color":[0,1,0]}
        ]}"#,
    ).map_err(|error| error.to_string())?;
    for radius in [0.5_f32, 0.1] {
        let water = level.water.first_mut().ok_or("missing circle fixture")?;
        water.radius = Some(radius);
        water.x = -radius;
        water.z = -radius;
        let mut field = field_from_level(&level);
        assert!(
            field.sources.iter().any(|source| source.id == "sun"
                && matches!(source.kind, SourceKind::Directional(_))
                && !source.factors.is_empty()),
            "initially dark sun has an above-water visible entry basis"
        );
        assert!(
            field
                .sources
                .iter()
                .any(|source| source.id == "moon" && !source.eligible && source.factors.is_empty()),
            "moon retains its identity but cannot contribute a caustic basis"
        );
        assert!(
            field.samples.iter().all(|sample| sample
                .iter()
                .take(3)
                .all(|channel| channel.abs() < f32::EPSILON)),
            "moon and initially dark sun contribute zero field energy"
        );
        assert_eq!(
            field.set_source("sun", Some(true), Some(0.5), Some([1.0, 0.0, 0.0])),
            Some(true),
            "sun activation recognized"
        );
        field.compose();
        assert!(
            field
                .samples
                .iter()
                .zip(&field.receivers)
                .all(
                    |(sample, receiver)| (sample[0] - if receiver.is_some() { 0.5 } else { 0.0 })
                        .abs()
                        < f32::EPSILON
                        && sample[1].abs() < f32::EPSILON
                        && sample[2].abs() < f32::EPSILON
                ),
            "a small circular pool keeps valid interior support while excluding dry corners"
        );
    }
    Ok(())
}

#[test]
fn wall_fixture_intensity_preserves_shared_aperture_power() -> Result<(), String> {
    let mut field = field(
        r#"{"schema": "places-level", "format_version":1,"id":"caustics_wall","name":"Caustics wall",
        "spawn":{"x":0,"z":0},"rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":-1,"height":4}],
        "water":[{"x":-0.8,"z":-0.8,"width":1.6,"depth":1.6,"surface_y":0,"bottom_y":-1}],
        "ceiling_lights":[{"id":"wall","fixture":"core:pool_light_wall","mount":"wall","x":0,"z":0,"y":2,"brightness":1}]}"#,
    )?;
    assert_eq!(
        field.sources.len(),
        2,
        "a pool wall fixture has two separated emitting apertures"
    );
    assert!(
        field.factor_count() > 0,
        "both fixture apertures prepare receiver factors"
    );
    let initial = field.samples.clone();
    assert_eq!(
        field.set_source("wall", None, Some(1.0), None),
        Some(false),
        "setting authored fixture brightness to its current value cannot double aperture power"
    );
    field.compose();
    assert!(
        field
            .samples
            .iter()
            .zip(&initial)
            .all(|(sample, before)| sample.map(f32::to_bits) == before.map(f32::to_bits)),
        "unchanged fixture intensity preserves exactly the composed receiver field"
    );
    assert_eq!(
        field.set_source("wall", None, Some(2.0), None),
        Some(true),
        "fixture brightness can still scale both apertures"
    );
    field.compose();
    assert!(
        field
            .samples
            .iter()
            .zip(&initial)
            .all(|(sample, before)| sample
                .iter()
                .take(3)
                .zip(before)
                .all(|(after, channel)| (after - channel * 2.0).abs() < 1.0e-6)),
        "each aperture retains its authored share while total fixture energy doubles"
    );
    Ok(())
}

/// Protects the exact between-column leak that point visibility cannot detect.
/// One 2 cm blocker and fewer than 3000 bounded records add no assets or transport solve.
#[test]
fn thin_sun_blocker_zeros_entire_cell_between_visible_point_samples() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema": "places-level", "format_version":1,"id":"caustics_thin","name":"Caustics thin",
        "spawn":{"x":0,"z":0},
        "rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":-1,"height":4,"ceiling":{"kind":"open"}}],
        "walls":[{"x":0.17,"z":-0.7,"width":0.02,"depth":1.4,"y":0.5,"height":0.05}],
        "water":[{"x":-0.8,"z":-0.8,"width":1.6,"depth":1.6,"surface_y":0,"bottom_y":-1}],
        "global_illuminators":[{"id":"sun","kind":"sun","direction":[0,-1,0]}]}"#)
        .map_err(|error| error.to_string())?;
    let lighting = LevelLighting::bake(&level);
    let volumes = WaterVolumes::from_level(&level);
    let mut field = ReceiverField::build(&level, &lighting, volumes.volumes());
    field.compose();
    assert!(
        field
            .receivers
            .iter()
            .flatten()
            .all(|point| lighting.runtime_direction_visible(*point, [0.0, 1.0, 0.0])),
        "every sampled point misses the intentionally between-column thin blocker"
    );
    assert!(
        !lighting.runtime_direction_visible([0.18, -0.988, 0.0], [0.0, 1.0, 0.0]),
        "the actual receiver below the thin blocker is shadowed"
    );
    let cell = field
        .cells
        .iter()
        .find(|cell| {
            !cell.bounds.is_empty()
                && cell.bounds.min[0] <= 0.18
                && cell.bounds.max[0] >= 0.18
                && cell.bounds.min[2] <= 0.0
                && cell.bounds.max[2] >= 0.0
        })
        .ok_or("thin-blocker receiver cell missing")?;
    assert!(
        cell.corners.iter().all(
            |corner| field.samples.get(*corner).is_some_and(|sample| sample
                .iter()
                .take(3)
                .all(|channel| channel.abs() < f32::EPSILON))
        ),
        "a failed full-cell certificate zeros all eight interpolation corners"
    );
    assert!(
        field.samples.iter().any(|sample| sample[0] > 0.0),
        "conservative shadow contraction preserves unrelated clear receiver regions"
    );
    Ok(())
}

/// A submerged model plate must receive on its top instead of querying inside
/// its solid bottom. This tiny disc also checks the shader's floor-bound data;
/// it adds no assets and fewer than 1200 bounded cell records.
#[test]
fn circular_receiver_queries_above_submerged_static_floor_plate() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema": "places-level", "format_version":1,"id":"caustics_bottom","name":"Caustics bottom",
        "spawn":{"x":0,"z":0},
        "rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":-1,"height":4,"ceiling":{"kind":"open"}}],
        "walls":[{"x":-0.5,"z":-0.5,"width":1,"depth":1,"y":-1,"height":0.13}],
        "water":[{"shape":"circle","x":-0.5,"z":-0.5,"radius":0.5,"surface_y":0,"bottom_y":-1}],
        "global_illuminators":[{"id":"sun","kind":"sun","direction":[0,-1,0]}]}"#)
        .map_err(|error| error.to_string())?;
    let field = field_from_level(&level);
    assert!(
        field
            .samples
            .iter()
            .zip(&field.receivers)
            .any(|(sample, receiver)| {
                sample[0] > 0.0
                    && (sample[3] + 0.87).abs() < 1.0e-5
                    && receiver.is_some_and(|point| (point[1] + 0.858).abs() < 1.0e-5)
            }),
        "the plate's airside top receives direct energy in the lowest layer"
    );
    assert!(
        field
            .receivers
            .iter()
            .flatten()
            .all(|point| point[1] > -0.87),
        "no receiver sample remains inside the submerged solid plate"
    );
    Ok(())
}

/// The shipped stair treads are about .333 m deep. Two such steps distinguish
/// cell-local certificates from shared-point erosion: the crossing cell must
/// stay dark while its adjacent clear cell keeps the same spatial corner lit.
/// This source-only fixture has fewer than 500 records and no GPU or assets.
#[test]
fn narrow_step_treads_keep_clear_cells_beside_a_blocked_riser() -> Result<(), String> {
    let field = field(
        r#"{"schema": "places-level", "format_version":1,"id":"caustics_steps","name":"Caustics steps",
        "spawn":{"x":0,"z":0},
        "rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":-1,"height":4,"ceiling":{"kind":"open"}}],
        "walls":[
            {"x":-0.3,"z":0,"width":0.6,"depth":0.33333334,"y":-1,"height":0.2},
            {"x":-0.3,"z":0.33333334,"width":0.6,"depth":0.33333334,"y":-1,"height":0.4}
        ],
        "water":[{"x":-0.3,"z":0,"width":0.6,"depth":0.6666667,"surface_y":0,"bottom_y":-1}],
        "global_illuminators":[{"id":"sun","kind":"sun","direction":[0,-1,0]}]}"#,
    )?;
    for top in [-0.8_f32, -0.6] {
        assert!(
            field
                .samples
                .iter()
                .any(|sample| sample[0] > 0.0 && (sample[3] - top).abs() < 1.0e-5),
            "each narrow tread has a lit airside receiver cell"
        );
    }
    let blocked = field
        .cells
        .iter()
        .find(|cell| {
            !cell.bounds.is_empty()
                && cell.bounds.min[2] < 0.333_333_34
                && cell.bounds.max[2] > 0.333_333_34
                && cell.bounds.min[1] < -0.7
        })
        .ok_or("riser-crossing cell missing")?;
    assert!(
        blocked.corners.iter().all(|corner| field
            .samples
            .get(*corner)
            .is_some_and(|sample| sample[0].abs() < f32::EPSILON)),
        "the complete riser-crossing cell remains dark"
    );
    assert!(
        field.cells.iter().any(|cell| {
            cell.points
                .iter()
                .any(|point| blocked.points.contains(point))
                && cell.corners.iter().any(|corner| {
                    field
                        .samples
                        .get(*corner)
                        .is_some_and(|sample| sample[0] > 0.0)
                })
        }),
        "a clear neighbour retains independently owned radiance at shared spatial points"
    );
    Ok(())
}

/// Guards the moving-receiver override path independently of shared-field W.
/// A submerged slab above a body raises evaluation anchors, but every anchor
/// carries that bound so the shader rejects actual fragments below the slab.
/// One tiny open room and eight anchors require no GPU or assets.
#[test]
fn moving_receiver_below_static_platform_carries_rejection_floor() -> Result<(), String> {
    use crate::engine::render::common::dynamic_visibility::DynamicVisibility;
    use crate::engine::spatial::Aabb;
    let level = LevelDef::from_json(r#"{"schema": "places-level", "format_version":1,"id":"caustics_buried","name":"Caustics buried",
        "spawn":{"x":0,"z":0},
        "rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":-1,"height":4,"ceiling":{"kind":"open"}}],
        "walls":[{"x":-0.5,"z":-0.5,"width":1,"depth":1,"y":-0.4,"height":0.1}],
        "water":[{"x":-0.6,"z":-0.6,"width":1.2,"depth":1.2,"surface_y":0,"bottom_y":-1}],
        "global_illuminators":[{"id":"sun","kind":"sun","direction":[0,-1,0]}]}"#)
        .map_err(|error| error.to_string())?;
    let lighting = LevelLighting::bake(&level);
    let volumes = WaterVolumes::from_level(&level);
    let volume = volumes.volumes().first().ok_or("water missing")?;
    let field = ReceiverField::build(&level, &lighting, volumes.volumes());
    let receiver = super::receiver::sample_receiver(
        &field,
        &lighting,
        (0, volume),
        Aabb {
            min: [-0.1, -0.8, -0.1],
            max: [0.1, -0.6, 0.1],
        },
        glam::Mat4::IDENTITY,
        None,
        &DynamicVisibility::new(),
    );
    assert!(
        receiver.bounds_min[3] > 0.0,
        "buried receiver keeps an active override to prevent global fallback"
    );
    assert!(
        receiver
            .radiance
            .iter()
            .all(|sample| (sample[3] + 0.3).abs() < 1.0e-5 && -0.6 < sample[3] - 0.02),
        "every raised anchor carries the platform bound that rejects the actual buried top"
    );
    Ok(())
}
