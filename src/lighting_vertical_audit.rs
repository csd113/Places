//! Vertical lighting isolation: acceptance cases.
//!
//! Floors and ceilings are solid boundaries for light. The bake models every
//! room floor as a stair-step of zero-thickness interfaces at their own heights
//! and every ceiling as a body above the ceiling plane, so a fixture cannot
//! light through a solid slab while a raised platform, a lowered basin and an
//! intentional vertical opening all keep working. These tests pin that
//! contract:
//!
//! 1. a sealed room above an unlit room stays dark when only the lower room is
//!    lit, and the reverse holds too;
//! 2. colour does not contaminate through a floor slab;
//! 3. an intentional vertical opening (a tall space with an upper floor that
//!    covers only part of it) transmits light through the open side;
//! 4. a partial horizontal overlap is occluded exactly where the upper floor
//!    actually is;
//! 5. a lowered basin or raised platform inside one volume is not treated as a
//!    sealed floor;
//! 6. different ceiling heights create neither leaks nor accidental isolation.

#![cfg(test)]

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::missing_const_for_fn,
    clippy::panic
)]
mod tests {
    use crate::level::LevelDef;
    use crate::lighting::{AMBIENT_LEVEL, LevelLighting, MAX_BRIGHTNESS};

    /// Parses a level or panics with the parser's own message.
    pub(super) fn parse(json: &str) -> LevelDef {
        LevelDef::from_json(json).unwrap_or_else(|error| panic!("test level must parse: {error}"))
    }

    /// Bakes a level.
    pub(super) fn bake(level: &LevelDef) -> LevelLighting {
        LevelLighting::bake(level)
    }

    /// One standard fixture mounted at a world height, with an optional colour.
    ///
    /// A stacked building authors `y` on a ceiling fixture so the bake can tell
    /// which storey it belongs to, exactly like a wall fixture.
    pub(super) fn fixture_at(x: f32, y: f32, z: f32, color: Option<[f32; 3]>) -> String {
        let color = color.map_or_else(String::new, |color| {
            format!(r#", "color": [{}, {}, {}]"#, color[0], color[1], color[2])
        });
        format!(
            r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "y": {y}{color} }}"#
        )
    }

    /// Two 6 x 6 x 3 m rooms stacked at the same footprint, with `gap` metres of
    /// slab between the lower room's eave and the upper room's floor.
    pub(super) fn stacked_rooms(gap: f32, lights: &str) -> LevelDef {
        let upper_floor = 3.0 + gap;
        parse(&format!(
            r#"{{
                "format_version": 1,
                "id": "stacked",
                "name": "Stacked",
                "spawn": {{ "x": 3.0, "z": 3.0 }},
                "rooms": [
                    {{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0,
                       "floor_y": 0.0 }},
                    {{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0,
                       "floor_y": {upper_floor} }}
                ],
                "ceiling_lights": [{lights}]
            }}"#
        ))
    }

    /// The level's primary fixture: the first light the bake resolved.
    pub(super) fn first_light(lighting: &LevelLighting) -> &crate::lighting::BakedLight {
        lighting
            .lights()
            .first()
            .unwrap_or_else(|| panic!("the test level places a fixture"))
    }

    /// A tall room T with an upper room U that covers only its eastern half.
    ///
    /// T is `x 0..6, z 0..6, height 6.4` at floor 0; U is `x 3..6, floor 3.2,
    /// height 3.0`, so U's floor slab hangs over T's east side and leaves a
    /// full-height void over its west side. The single fixture's authored world
    /// `y` is the panel height: while it lies inside U's air volume, U owns it
    /// (the two spans overlap, and U's floor area is the smaller one).
    pub(super) fn shaft_level(fixture_y: f32) -> LevelDef {
        parse(&format!(
            r#"{{
                "format_version": 1,
                "id": "shaft",
                "name": "Shaft",
                "spawn": {{ "x": 1.0, "z": 3.0 }},
                "rooms": [
                    {{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 6.4,
                       "floor_y": 0.0 }},
                    {{ "x": 3.0, "z": 0.0, "width": 3.0, "depth": 6.0, "height": 3.0,
                       "floor_y": 3.2 }}
                ],
                "ceiling_lights": [
                    {{ "fixture": "core:fluorescent_panel_01", "x": 3.3, "z": 3.0,
                       "y": {fixture_y}, "brightness": 2.0 }}
                ]
            }}"#
        ))
    }

    /// A smoke check that the model is wired up before the detailed cases run.
    #[test]
    fn stacked_rooms_bake_deterministically() {
        let level = stacked_rooms(0.2, &fixture_at(3.0, 2.99, 3.0, None));
        let first = bake(&level);
        let second = bake(&level);
        assert_eq!(first.summary(), second.summary());
        assert_eq!(first.rooms().len(), 2);
        assert_eq!(
            first.zone_count(),
            2,
            "each storey is its own baseline area"
        );
        assert!(first.sample_luminance(3.0, 0.02, 3.0) <= MAX_BRIGHTNESS);
        assert!(first.sample_luminance(3.0, 0.02, 3.0) >= AMBIENT_LEVEL);
    }

    // ---------------------------------------------- sealed stacks (cases 1-3)

    /// Case 1: a sealed stack whose only fixture is on the lower storey.
    #[test]
    fn a_lit_lower_storey_stays_sealed_from_the_upper_one() {
        let level = stacked_rooms(0.2, &fixture_at(3.0, 2.99, 3.0, None));
        let lighting = bake(&level);

        let lower_floor = lighting.sample_luminance(3.0, 0.02, 3.0);
        let upper_floor = lighting.sample_luminance(3.0, 3.22, 3.0);
        assert!(
            lower_floor > AMBIENT_LEVEL + 0.3,
            "the lower floor must be clearly lit: {lower_floor}"
        );
        assert!(
            upper_floor < AMBIENT_LEVEL + 0.05,
            "the upper floor must sit at ambient: {upper_floor}"
        );

        // The Y-aware lookup separates the storeys; the historical 2D rule,
        // asked about the same column, cannot.
        assert_eq!(lighting.room_index_at_height(3.0, 0.02, 3.0), Some(0));
        assert_eq!(lighting.room_index_at_height(3.0, 3.4, 3.0), Some(1));
        assert_eq!(lighting.room_index_at(3.0, 3.0), Some(0));

        // Both rooms share one footprint exactly (an area tie), so the authored
        // world `y` of the fixture is what gives it to the lower room.
        let light = first_light(&lighting);
        assert_eq!(light.room, Some(0));
        assert!((light.y - 2.99).abs() < 1e-4, "panel y {}", light.y);
    }

    /// Case 2: the mirror image — the only fixture is on the upper storey.
    #[test]
    fn a_lit_upper_storey_stays_sealed_from_the_lower_one() {
        let upper_floor = 3.2;
        let level = stacked_rooms(0.2, &fixture_at(3.0, upper_floor + 2.99, 3.0, None));
        let lighting = bake(&level);

        let lower_floor = lighting.sample_luminance(3.0, 0.02, 3.0);
        let upper_floor_sample = lighting.sample_luminance(3.0, upper_floor + 0.02, 3.0);
        assert!(
            lower_floor < AMBIENT_LEVEL + 0.05,
            "the lower floor must sit at ambient: {lower_floor}"
        );
        assert!(
            upper_floor_sample > AMBIENT_LEVEL + 0.3,
            "the upper floor must be clearly lit: {upper_floor_sample}"
        );
        let light = first_light(&lighting);
        assert_eq!(light.room, Some(1));
        assert_eq!(
            lighting.room_index_at_height(3.0, upper_floor + 0.02, 3.0),
            Some(1)
        );
    }

    /// Case 3: a red fixture below and a blue one above. The slab must refuse
    /// the colour as well as the brightness, not merely dim it.
    #[test]
    fn a_floor_slab_does_not_mix_colours_between_storeys() {
        let level = stacked_rooms(
            0.2,
            &format!(
                "{}, {}",
                fixture_at(3.0, 2.99, 3.0, Some([1.0, 0.0, 0.0])),
                fixture_at(3.0, 6.19, 3.0, Some([0.0, 0.0, 1.0]))
            ),
        );
        let lighting = bake(&level);

        let lower = lighting.sample(3.0, 0.02, 3.0);
        let upper = lighting.sample(3.0, 3.22, 3.0);
        assert!(
            lower.r > lower.b + 0.5,
            "lower sample must be red: {lower:?}"
        );
        assert!(
            upper.b > upper.r + 0.5,
            "upper sample must be blue: {upper:?}"
        );
        assert!(lower.b < AMBIENT_LEVEL + 0.05, "lower blue: {lower:?}");
        assert!(lower.g < AMBIENT_LEVEL + 0.05, "lower green: {lower:?}");
        assert!(upper.r < AMBIENT_LEVEL + 0.05, "upper red: {upper:?}");
        assert!(upper.g < AMBIENT_LEVEL + 0.05, "upper green: {upper:?}");

        // Each fixture is owned by the storey it hangs from.
        assert_eq!(lighting.lights()[0].room, Some(0));
        assert_eq!(lighting.lights()[1].room, Some(1));
    }

    // --------------------------------------------- vertical shaft (cases 4-5)

    /// Case 4: an intentional vertical opening.
    ///
    /// The panel is authored at 3.5 m — inside U's air volume (3.2..6.2), so U
    /// owns it — rather than hanging at U's 6.2 m ceiling. The local pool's
    /// radius is 6 m and T's floor lies more than 6 m below that ceiling, so a
    /// ceiling-mounted panel could not reach T's floor at all; that would
    /// measure the falloff radius, not the slab. At 3.5 m the straight line
    /// from the panel to the probe `(1.0, 0.02, 3.0)` crosses U's floor plane
    /// `y = 3.2` at `x = 2.553 < 3` — through the open side — while the line to
    /// `(5.5, 0.02, 3.0)` crosses it at `x = 4.04`, inside U's footprint.
    #[test]
    fn an_open_side_of_an_upper_floor_transmits_light() {
        let lighting = bake(&shaft_level(3.5));

        let open = lighting.sample_luminance(1.0, 0.02, 3.0);
        let covered = lighting.sample_luminance(5.5, 0.02, 3.0);
        assert!(
            open > AMBIENT_LEVEL + 0.15,
            "light must come down the open side: {open}"
        );
        assert!(open > covered + 0.15, "{open} vs {covered}");
        assert!(
            covered < AMBIENT_LEVEL + 0.05,
            "the floor under U's slab must sit at ambient: {covered}"
        );

        let upper_floor = lighting.sample_luminance(4.5, 3.22, 3.0);
        assert!(
            upper_floor > AMBIENT_LEVEL + 0.3,
            "U's own floor must be lit: {upper_floor}"
        );
        assert_eq!(first_light(&lighting).room, Some(1));
        assert_eq!(lighting.room_index_at_height(4.5, 3.22, 3.0), Some(1));
    }

    /// Case 5: occlusion follows U's floor footprint, not the room column.
    ///
    /// Probes at `y = 2.0`, just below the slab: `(2.5, 2.0)` crosses `y = 3.2`
    /// at `x = 2.660` and `(2.9, 2.0)` at `x = 2.900`, both west of U's edge,
    /// while `(3.05, 2.0)` and `(3.5, 2.0)` cross inside U's footprint and are
    /// blocked by it.
    #[test]
    fn upper_floor_occlusion_follows_its_own_footprint() {
        let lighting = bake(&shaft_level(3.5));

        for (x, lit) in [(2.5_f32, true), (2.9, true), (3.05, false), (3.5, false)] {
            let value = lighting.sample_luminance(x, 2.0, 3.0);
            if lit {
                assert!(
                    value > AMBIENT_LEVEL + 0.2,
                    "({x}, 2.0) must be lit: {value}"
                );
            } else {
                assert!(
                    value < AMBIENT_LEVEL + 0.05,
                    "({x}, 2.0) must be occluded by U's floor: {value}"
                );
            }
        }

        let under_open = lighting.sample_luminance(2.0, 0.02, 3.0);
        let under_covered = lighting.sample_luminance(5.5, 0.02, 3.0);
        assert!(
            under_open > AMBIENT_LEVEL + 0.15,
            "under the void: {under_open}"
        );
        assert!(
            under_covered < AMBIENT_LEVEL + 0.05,
            "under the slab: {under_covered}"
        );
    }

    // ------------------------------- basins and platforms (case 6)

    /// Case 6: one 6 x 6 x 3 m room with a lowered basin (`offset_y: -1.2`) and
    /// a raised platform (`offset_y: 0.6`). Neither region seals the volume: the
    /// ceiling fixture reaches the basin floor and the platform top, while the
    /// platform's own deck blocks the pool from the air beneath it.
    #[test]
    fn a_lowered_basin_and_a_raised_platform_stay_connected_to_their_room() {
        let level = parse(
            r#"{
                "format_version": 1,
                "id": "basin_platform",
                "name": "Basin Platform",
                "spawn": { "x": 3.0, "z": 3.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }
                ],
                "floor_regions": [
                    { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -1.2 },
                    { "x": 4.0, "z": 0.0, "width": 2.0, "depth": 2.0, "offset_y": 0.6 }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 },
                    { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 1.0 }
                ]
            }"#,
        );
        let lighting = bake(&level);
        let baseline = lighting.rooms()[0].baseline.luminance();

        // The pool reaching the basin floor proves the recess is not sealed:
        // its value is above the room's uniform baseline, not merely above
        // ambient.
        let basin = lighting.sample_luminance(3.0, -1.18, 3.0);
        assert!(basin > AMBIENT_LEVEL + 0.3, "basin floor: {basin}");
        assert!(
            basin > baseline + 0.05,
            "the fixture's pool must reach the basin floor: {basin} vs {baseline}"
        );

        let platform_top = lighting.sample_luminance(5.0, 0.62, 1.0);
        let under_platform = lighting.sample_luminance(5.0, 0.30, 1.0);
        assert!(
            platform_top > AMBIENT_LEVEL + 0.3,
            "platform top: {platform_top}"
        );
        assert!(
            platform_top > under_platform + 0.1,
            "the deck must shade the air beneath it: {platform_top} vs {under_platform}"
        );

        // The dimming below the deck is real occlusion, not range or isolation:
        // the same probe in the same level without the platform region is fully
        // lit by the same fixture.
        let mut open = level;
        open.floor_regions.clear();
        let open_control = bake(&open).sample_luminance(5.0, 0.30, 1.0);
        assert!(
            under_platform < open_control - 0.1,
            "the deck must occlude: {under_platform} vs {open_control}"
        );
    }

    // ------------------------------- ceiling heights and walls (case 7)

    /// Case 7: a 6 x 6 x 3 m room beside a raised 6 x 6 x 2.5 m room at floor
    /// 2.0 (like the `vertical_diagnostic` fixture), sharing a wall with a
    /// walk-through door that spans the lower floor's height. The lower room's
    /// fixture is bright red, the raised room's is blue, so a leak between them
    /// would show up as a colour, not just a brightness step.
    #[test]
    fn a_raised_neighbour_keeps_its_own_light_behind_the_shared_wall() {
        let level = parse(
            r#"{
                "format_version": 1,
                "id": "raised_neighbour",
                "name": "Raised Neighbour",
                "spawn": { "x": 3.0, "z": 3.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0, "floor_y": 0.0 },
                    { "x": 6.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 2.5, "floor_y": 2.0 }
                ],
                "walls": [
                    { "x": 5.9, "z": 0.0, "width": 0.2, "depth": 6.0, "y": 0.0, "height": 4.8,
                      "openings": [{ "kind": "door", "offset": 2.5, "width": 1.0, "height": 2.0, "sill": 0.0 }] }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0, "brightness": 4.0,
                      "color": [1.0, 0.0, 0.0] },
                    { "fixture": "core:fluorescent_panel_01", "x": 11.0, "z": 5.0, "y": 4.49, "brightness": 2.0,
                      "color": [0.0, 0.4, 1.0] }
                ]
            }"#,
        );
        let lighting = bake(&level);

        // Each room keeps its own fixture and its own light.
        assert_eq!(lighting.lights()[0].room, Some(0));
        assert_eq!(lighting.lights()[1].room, Some(1));
        let lower = lighting.sample(3.0, 0.02, 3.0);
        let raised = lighting.sample(11.0, 2.02, 5.0);
        assert!(
            lower.r > AMBIENT_LEVEL + 0.3 && lower.r > lower.b + 0.5,
            "the lower floor keeps its red fixture: {lower:?}"
        );
        assert!(
            raised.luminance() > AMBIENT_LEVEL + 0.3 && raised.b > 0.9,
            "the raised floor keeps its blue fixture: {raised:?}"
        );

        // The doorway transmits: red light from the lower room passes through
        // the aperture and reaches the raised room's side of the wall.
        let across_the_door = lighting.sample(6.5, 1.0, 3.0);
        let away_from_the_door = lighting.sample(11.5, 1.0, 0.5);
        assert!(
            across_the_door.r > AMBIENT_LEVEL + 0.3,
            "the doorway must transmit: {across_the_door:?}"
        );
        assert!(
            across_the_door.r > away_from_the_door.r + 0.3,
            "{across_the_door:?} vs {away_from_the_door:?}"
        );
        assert!(
            away_from_the_door.r < AMBIENT_LEVEL + 0.05,
            "only the doorway may carry red across: {away_from_the_door:?}"
        );

        // The solid wall blocks the red fixture: a sample just inside the
        // raised room, away from the door, has no red at all and is dimmer
        // than the raised room's own fixture area.
        let behind_the_wall = lighting.sample(6.4, 2.2, 0.6);
        assert!(
            behind_the_wall.r < AMBIENT_LEVEL + 0.05,
            "the shared wall must block the red fixture: {behind_the_wall:?}"
        );
        assert!(
            raised.luminance() > behind_the_wall.luminance() + 0.1,
            "the raised room's own fixture area must outshine its dark wall: {} vs {}",
            raised.luminance(),
            behind_the_wall.luminance()
        );

        // Every baked value stays inside the model's declared range.
        for (x, y, z) in [
            (3.0_f32, 0.02_f32, 3.0_f32),
            (11.0, 2.02, 5.0),
            (6.5, 1.0, 3.0),
            (11.5, 1.0, 0.5),
            (6.4, 2.2, 0.6),
            (5.5, 1.0, 3.0),
        ] {
            let value = lighting.sample(x, y, z);
            assert!(
                value.min_channel() >= AMBIENT_LEVEL - 1e-6,
                "({x}, {y}, {z}) fell below ambient: {value:?}"
            );
            assert!(
                value.max_channel() <= MAX_BRIGHTNESS + 1e-6,
                "({x}, {y}, {z}) exceeded the maximum: {value:?}"
            );
        }
    }

    /// Case 7's gable check: a pitched ceiling must not shadow its own fixture,
    /// so a sample at the ridge is brighter than ambient.
    #[test]
    fn a_gable_ridge_is_lit_by_its_own_fixture() {
        let level = parse(
            r#"{
                "format_version": 1,
                "id": "gable_vertical",
                "name": "Gable Vertical",
                "spawn": { "x": 4.0, "z": 4.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        );
        let lighting = bake(&level);

        let ridge = lighting.sample_luminance(4.0, 4.98, 4.0);
        assert!(
            ridge > AMBIENT_LEVEL + 0.3,
            "the ridge must be lit: {ridge}"
        );
        assert!(
            lighting.lights()[0].y > 4.5,
            "the panel must hang up at the ridge, not at the eave: {}",
            lighting.lights()[0].y
        );
    }

    // ------------------------------------ lookup, ownership and determinism

    /// Case 8: a height outside every room's vertical span falls back to the
    /// historical 2D footprint rule instead of resolving to nothing.
    #[test]
    fn a_height_above_every_room_falls_back_to_the_flat_lookup() {
        let level = stacked_rooms(0.2, &fixture_at(3.0, 2.99, 3.0, None));
        let lighting = bake(&level);

        assert_eq!(
            lighting.room_index_at_height(3.0, 99.0, 3.0),
            lighting.room_index_at(3.0, 3.0)
        );
        assert_eq!(lighting.room_index_at_height(3.0, 99.0, 3.0), Some(0));
        // The same fallback runs below every floor plane.
        assert_eq!(lighting.room_index_at_height(3.0, -5.0, 3.0), Some(0));
    }

    /// Case 8: adding a room on top of another must not change the lower
    /// room's baseline, power or fixture at all — exactly, not approximately.
    #[test]
    fn a_stacked_room_does_not_change_the_lower_rooms_baseline() {
        let level = stacked_rooms(0.2, &fixture_at(3.0, 2.99, 3.0, None));
        let mut solo = level.clone();
        solo.rooms.truncate(1);

        let stacked = bake(&level);
        let alone = bake(&solo);
        assert_eq!(stacked.rooms()[0], alone.rooms()[0]);
        assert_eq!(stacked.lights()[0], alone.lights()[0]);
        assert_eq!(
            stacked.baseline_in_room(0, 3.0, 3.0),
            alone.baseline_in_room(0, 3.0, 3.0)
        );
        assert_eq!(
            stacked.baseline_in_room(0, 0.5, 5.5),
            alone.baseline_in_room(0, 0.5, 5.5)
        );
    }

    /// Case 8: two bakes of one level are bit-identical, summary and samples.
    #[test]
    fn two_bakes_of_a_stacked_level_are_identical() {
        let level = stacked_rooms(
            0.2,
            &format!(
                "{}, {}",
                fixture_at(3.0, 2.99, 3.0, Some([1.0, 0.0, 0.0])),
                fixture_at(3.0, 6.19, 3.0, Some([0.0, 0.0, 1.0]))
            ),
        );
        let first = bake(&level);
        let second = bake(&level);

        assert_eq!(first.summary(), second.summary());
        assert_eq!(first.rooms(), second.rooms());
        assert_eq!(first.lights(), second.lights());
        for (x, y, z) in [
            (3.0_f32, 0.02_f32, 3.0_f32),
            (3.0, 3.22, 3.0),
            (1.0, 1.5, 1.0),
            (5.5, 4.5, 5.5),
            (0.5, 0.5, 0.5),
        ] {
            assert_eq!(first.sample(x, y, z), second.sample(x, y, z));
            assert_eq!(
                first.sample_in_room(0, x, y, z),
                second.sample_in_room(0, x, y, z)
            );
            assert_eq!(
                first.sample_in_room(1, x, y, z),
                second.sample_in_room(1, x, y, z)
            );
        }
    }
}
