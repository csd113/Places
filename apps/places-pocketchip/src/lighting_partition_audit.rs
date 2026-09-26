//! Partition-aware baseline lighting: acceptance cases.
//!
//! The bake gives every room a baseline derived from the fixture power it owns.
//! When opaque internal walls split a room's footprint into disconnected areas,
//! each area gets its own baseline ([`LevelLighting::baseline_in_room`]) instead
//! of sharing one room-wide average, while an open room keeps the historical
//! uniform value bit for bit. These tests pin that contract:
//!
//! 1. a solid partition isolates the unlit side completely;
//! 2. a doorway transmits a bounded amount through the aperture;
//! 3. fixtures on both sides keep their own colour and brightness;
//! 4. removing the partition restores the open-room model exactly;
//! 5. a fixture sitting against a partition never teleports its baseline
//!    through the wall;
//! 6. walls that deliberately stop short of the ceiling are not partitions,
//!    and windows do not act as walk-through connections;
//! 7. two doors in one partition stay independent and bounded, edge and corner
//!    walls do not erase the room, and every bake stays deterministic and
//!    finite.

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
    use crate::lighting::{
        AMBIENT_LEVEL, LevelLighting, LightColor, MAX_BRIGHTNESS, OPENING_BLEND_STRENGTH,
    };
    use crate::test_support::assert_exact;

    /// Parses a level or panics with the parser's own message.
    pub(super) fn parse(json: &str) -> LevelDef {
        LevelDef::from_json(json).unwrap_or_else(|error| panic!("test level must parse: {error}"))
    }

    /// Bakes a level.
    pub(super) fn bake(level: &LevelDef) -> LevelLighting {
        LevelLighting::bake(level)
    }

    /// One standard fixture with an optional colour and intensity.
    pub(super) fn fixture(x: f32, z: f32) -> String {
        format!(r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z} }}"#)
    }

    /// One standard fixture with an authored colour.
    pub(super) fn coloured_fixture(x: f32, z: f32, color: [f32; 3]) -> String {
        format!(
            r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "color": [{}, {}, {}] }}"#,
            color[0], color[1], color[2]
        )
    }

    /// A 10 x 10 x 3 m room with a partition at x = 5 (`openings` fills the wall).
    pub(super) fn partitioned_room(openings: &str, lights: &str) -> LevelDef {
        parse(&format!(
            r#"{{
                "format_version": 1,
                "id": "partition",
                "name": "Partition",
                "spawn": {{ "x": 2.0, "z": 5.0 }},
                "rooms": [
                    {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}
                ],
                "walls": [
                    {{ "x": 4.9, "z": 0.0, "width": 0.2, "depth": 10.0, "height": 3.0,
                       "openings": [{openings}] }}
                ],
                "ceiling_lights": [{lights}]
            }}"#
        ))
    }

    /// The same room with no internal wall at all.
    pub(super) fn open_room(lights: &str) -> LevelDef {
        parse(&format!(
            r#"{{
                "format_version": 1,
                "id": "open",
                "name": "Open",
                "spawn": {{ "x": 2.0, "z": 5.0 }},
                "rooms": [
                    {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}
                ],
                "ceiling_lights": [{lights}]
            }}"#
        ))
    }

    /// The single door used by the doorway cases: one metre wide, on the floor.
    pub(super) const DOOR: &str =
        r#"{ "kind": "door", "offset": 4.5, "width": 1.0, "height": 2.1, "sill": 0.0 }"#;

    /// The single window used by the non-connecting cases: raised off the floor.
    pub(super) const WINDOW: &str =
        r#"{ "kind": "window", "offset": 4.5, "width": 1.0, "height": 1.0, "sill": 1.0 }"#;

    /// Two doors in the same partition, one either side of the fixture's
    /// z = 5 line, so each threshold can be probed on its own.
    pub(super) const TWO_DOORS: &str = r#"
        { "kind": "door", "offset": 1.5, "width": 1.0, "height": 2.1, "sill": 0.0 },
        { "kind": "door", "offset": 6.5, "width": 1.0, "height": 2.1, "sill": 0.0 }
    "#;

    /// A 10 x 10 x 3 m room whose only internal wall is authored verbatim.
    ///
    /// `partitioned_room` is the fixed full-height partition; this is the same
    /// room for the walls that deliberately differ (a height, a stub, an edge
    /// or corner footprint).
    pub(super) fn room_with_wall(wall: &str, lights: &str) -> LevelDef {
        parse(&format!(
            r#"{{
                "format_version": 1,
                "id": "wall",
                "name": "Wall",
                "spawn": {{ "x": 2.0, "z": 5.0 }},
                "rooms": [
                    {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }}
                ],
                "walls": [{wall}],
                "ceiling_lights": [{lights}]
            }}"#
        ))
    }

    /// The standard partition wall with an authored height, so the same wall
    /// can deliberately stop short of the 3 m ceiling.
    pub(super) fn partition_wall_of_height(height: f32) -> String {
        format!(r#"{{ "x": 4.9, "z": 0.0, "width": 0.2, "depth": 10.0, "height": {height} }}"#)
    }

    /// The same room and partition position, but the wall is only a 4 m stub:
    /// six metres of the room's depth stay open around its end.
    pub(super) fn stub_partition_room(lights: &str) -> LevelDef {
        room_with_wall(
            r#"{ "x": 4.9, "z": 0.0, "width": 0.2, "depth": 4.0, "height": 3.0 }"#,
            lights,
        )
    }

    /// Asserts two colours are bit-identical on every channel.
    pub(super) fn assert_same_color(actual: LightColor, expected: LightColor) {
        assert_exact(actual.r, expected.r);
        assert_exact(actual.g, expected.g);
        assert_exact(actual.b, expected.b);
    }

    /// True when every channel is finite and inside the legal baked range.
    pub(super) fn is_legal(color: LightColor) -> bool {
        color.is_finite()
            && color
                .to_array()
                .iter()
                .all(|channel| (AMBIENT_LEVEL..=MAX_BRIGHTNESS).contains(channel))
    }

    /// A smoke check that the model is wired up before the detailed cases run.
    #[test]
    fn a_partitioned_room_bakes_deterministically() {
        let level = partitioned_room("", &fixture(2.0, 5.0));
        let first = bake(&level);
        let second = bake(&level);
        assert_eq!(first.summary(), second.summary());
        assert!(
            first.zone_count() >= first.rooms().len(),
            "a partitioned room contributes at least one area per room"
        );
        assert!(first.sample_luminance(2.0, 0.02, 5.0) <= MAX_BRIGHTNESS);
        assert!(first.sample_luminance(2.0, 0.02, 5.0) >= AMBIENT_LEVEL);
    }

    // ------------------------------------------------------------- Case 1

    /// A full-height solid partition splits the room into a lit area and an
    /// unlit one: a fixture's power stays on the side that owns it.
    #[test]
    fn case_1_a_solid_partition_isolates_the_unlit_side() {
        let partitioned = bake(&partitioned_room("", &fixture(2.0, 5.0)));
        let open = bake(&open_room(&fixture(2.0, 5.0)));

        assert_eq!(partitioned.zone_count(), 2, "the wall must split the room");
        assert!(partitioned.is_partitioned(0));

        // Under its own fixture the lit side is clearly above the ambient floor.
        let lit = partitioned.sample_in_room_luminance(0, 2.0, 0.02, 5.0);
        assert!(
            lit > AMBIENT_LEVEL + 0.3,
            "the fixture's own side must be lit: {lit}"
        );

        // The far area owns no fixture, and the fixture cannot see past the
        // solid wall: the far sample is the ambient floor.
        let far = partitioned.sample_in_room_luminance(0, 8.0, 0.02, 5.0);
        assert!(
            far < AMBIENT_LEVEL + 0.05,
            "the partition must not leak its fixture: {far}"
        );

        // The open control reaches that same point with the room-wide
        // baseline, so the isolation above is the wall's doing, not distance.
        let open_far = open.sample_in_room_luminance(0, 8.0, 0.02, 5.0);
        assert!(
            open_far > far + 0.3,
            "removing the wall must restore the far side: open {open_far} vs partitioned {far}"
        );
    }

    // ------------------------------------------------------------- Case 2

    /// A doorway lets a bounded amount of the lit area's light through the
    /// aperture without equalising the two areas.
    #[test]
    fn case_2_a_doorway_transmits_a_bounded_amount() {
        let partitioned = bake(&partitioned_room(DOOR, &fixture(2.0, 5.0)));
        let open = bake(&open_room(&fixture(2.0, 5.0)));

        // The door spans z = 4.5..5.5, so (5.6, 5.0) is just past the threshold.
        let through = partitioned.sample_in_room_luminance(0, 5.6, 0.02, 5.0);
        let open_through = open.sample_in_room_luminance(0, 5.6, 0.02, 5.0);
        assert!(
            through > AMBIENT_LEVEL + 0.05,
            "the doorway must transmit light: {through}"
        );
        assert!(
            through < open_through,
            "the doorway must transmit less than an open room: {through} vs {open_through}"
        );

        // The blend term itself crosses the threshold, isolated from the pool:
        // 0.6 m from the aperture the smooth falloff is ~0.97 of full strength,
        // so a real cross-threshold exchange is far above a token positive.
        let blend = partitioned.opening_blend(0, 5.6, 0.02, 5.0);
        assert!(
            blend.luminance() > 0.05,
            "the blend must cross the door, not just the pool: {blend:?}"
        );

        // A corner beyond the blend's 6 m radius stays at the ambient floor.
        let corner = partitioned.sample_in_room_luminance(0, 9.5, 0.02, 9.5);
        assert!(
            corner < AMBIENT_LEVEL + 0.05,
            "a corner beyond the doorway's reach must stay ambient: {corner}"
        );
    }

    // ------------------------------------------------------------- Case 3

    /// Two fixtures on opposite sides keep their own colour and their own
    /// power: neither side's light crosses the solid partition.
    #[test]
    fn case_3_fixtures_on_both_sides_keep_their_own_colour() {
        let lights = format!(
            "{}, {}",
            coloured_fixture(3.5, 5.0, [1.0, 0.0, 0.0]),
            coloured_fixture(6.5, 5.0, [0.0, 0.0, 1.0])
        );
        let partitioned = bake(&partitioned_room("", &lights));
        let open = bake(&open_room(&lights));
        assert_eq!(partitioned.zone_count(), 2);

        let left = partitioned.sample_in_room(0, 3.5, 0.02, 5.0);
        let right = partitioned.sample_in_room(0, 6.5, 0.02, 5.0);
        assert!(
            left.r > left.b + 0.15,
            "the red side must read red: {left:?}"
        );
        assert!(
            right.b > right.r + 0.15,
            "the blue side must read blue: {right:?}"
        );

        // Each area's baseline is built from its own fixture's power alone.
        let left_baseline = partitioned.baseline_in_room(0, 3.5, 5.0);
        let right_baseline = partitioned.baseline_in_room(0, 6.5, 5.0);
        assert!(
            left_baseline.r > right_baseline.r + 0.3,
            "the red area's baseline must carry the red power: {left_baseline:?} vs {right_baseline:?}"
        );
        assert!(
            right_baseline.b > left_baseline.b + 0.3,
            "the blue area's baseline must carry the blue power: {right_baseline:?} vs {left_baseline:?}"
        );

        // Neither fixture's colour crosses the wall: the red side's blue
        // channel is exactly the ambient floor, and vice versa...
        assert_exact(left.b, AMBIENT_LEVEL);
        assert_exact(right.r, AMBIENT_LEVEL);
        // ...while in the open control the blue fixture does reach the red
        // side. The partitioned value is isolation, not distance.
        let open_left = open.sample_in_room(0, 3.5, 0.02, 5.0);
        assert!(
            open_left.b > left.b + 0.3,
            "the open room must share the blue fixture: open {open_left:?} vs partitioned {left:?}"
        );
    }

    // ------------------------------------------------------------- Case 4

    /// Without internal walls the room keeps the historical uniform baseline,
    /// bit for bit; a wall that does not reach the ceiling changes nothing.
    #[test]
    fn case_4_an_open_room_keeps_one_uniform_baseline() {
        let open = bake(&open_room(&fixture(2.0, 5.0)));
        assert_eq!(open.zone_count(), 1, "an open room is one area");
        assert!(!open.is_partitioned(0));
        let baseline = open.rooms()[0].baseline;
        for (x, z) in [
            (0.5_f32, 0.5_f32),
            (2.0, 5.0),
            (5.0, 5.0),
            (8.0, 1.0),
            (9.5, 9.5),
        ] {
            assert_same_color(open.baseline_in_room(0, x, z), baseline);
        }

        // The same room with a wall that stops a metre short of the ceiling:
        // the connectivity probe passes over it, so the baseline is the same
        // single historical value again.
        let short = bake(&room_with_wall(
            &partition_wall_of_height(2.0),
            &fixture(2.0, 5.0),
        ));
        assert_eq!(short.zone_count(), 1, "a 2 m wall cannot split a 3 m room");
        assert!(!short.is_partitioned(0));
        for (x, z) in [(1.0_f32, 1.0_f32), (8.0, 8.0)] {
            assert_same_color(short.baseline_in_room(0, x, z), baseline);
        }
    }

    // ------------------------------------------------------------- Case 5

    /// A fixture sitting against the partition keeps its baseline on its own
    /// side; the wall does not let the power teleport across.
    #[test]
    fn case_5_a_fixture_against_the_partition_does_not_teleport_its_baseline() {
        let lighting = bake(&partitioned_room("", &fixture(4.6, 5.0)));
        assert!(lighting.is_partitioned(0));

        // Directly around the fixture its own area is bright...
        let near_fixture = lighting.sample_in_room_luminance(0, 4.6, 0.02, 5.0);
        assert!(
            near_fixture > AMBIENT_LEVEL + 0.3,
            "the fixture's own area must be bright: {near_fixture}"
        );

        // ...and the baseline field agrees on both sides of the wall: the
        // fixture's side keeps the power, the far side stays at the ambient
        // floor instead of inheriting a room-wide average.
        let own_side = lighting.baseline_in_room(0, 4.6, 5.0).luminance();
        let behind_wall = lighting.baseline_in_room(0, 5.2, 5.0).luminance();
        assert!(
            own_side > AMBIENT_LEVEL + 0.3,
            "the fixture's own area keeps its power: {own_side}"
        );
        assert!(
            behind_wall < AMBIENT_LEVEL + 0.02,
            "the baseline must not teleport through the wall: {behind_wall}"
        );
    }

    // ------------------------------------------------------------- Case 6

    /// A wall that stops short of the ceiling is not a partition: light passes
    /// over it and the room keeps one baseline.
    #[test]
    fn case_6_a_wall_below_the_ceiling_is_not_a_partition() {
        let lighting = bake(&room_with_wall(
            &partition_wall_of_height(2.0),
            &fixture(2.0, 5.0),
        ));
        assert_eq!(lighting.zone_count(), 1);
        assert!(!lighting.is_partitioned(0));

        // Floor level on the far side is still lit by the room-wide baseline
        // (the 2 m wall shadows it from the pool, but not from the room).
        let far = lighting.sample_in_room_luminance(0, 8.0, 0.02, 5.0);
        assert!(
            far > AMBIENT_LEVEL + 0.3,
            "the far side must stay bright: {far}"
        );

        // The room-wide baseline is exactly the open room's, so the short wall
        // changed no baseline at all.
        let open = bake(&open_room(&fixture(2.0, 5.0)));
        assert_exact(far, open.rooms()[0].baseline.luminance());
    }

    /// A stub that does not span the room is not a partition: the flood fill
    /// walks around its end and the far side keeps the room-wide baseline.
    #[test]
    fn case_6_b_a_stub_partition_does_not_divide_the_room() {
        let lighting = bake(&stub_partition_room(&fixture(2.0, 5.0)));
        assert_eq!(
            lighting.zone_count(),
            1,
            "a 4 m stub cannot divide a 10 m room"
        );
        assert!(!lighting.is_partitioned(0));

        // Behind the stub at floor level the room baseline still applies: the
        // area is connected around the wall's end.
        let behind = lighting.sample_in_room_luminance(0, 8.0, 0.02, 2.0);
        assert!(
            behind > AMBIENT_LEVEL + 0.3,
            "the area behind the stub must keep the room baseline: {behind}"
        );
    }

    /// A window wall partitions the baseline field but does not connect the
    /// two areas: only the pool passes through the aperture, and only at the
    /// window's own height.
    #[test]
    fn case_6_c_a_window_does_not_connect_baselines() {
        let lighting = bake(&partitioned_room(WINDOW, &fixture(2.0, 5.0)));
        assert!(
            lighting.is_partitioned(0),
            "the window wall still divides the room"
        );
        let far_baseline = lighting.baseline_in_room(0, 5.6, 5.0).luminance();
        assert!(
            far_baseline < AMBIENT_LEVEL + 0.02,
            "a window is not a walk-through connection: {far_baseline}"
        );

        // The aperture itself transmits the fixture's pool at sill height...
        let at_sill = lighting.sample_in_room_luminance(0, 5.6, 1.5, 5.0);
        assert!(
            at_sill > AMBIENT_LEVEL + 0.05,
            "the window must transmit at its own height: {at_sill}"
        );
        // ...while the solid wall below the sill keeps blocking.
        let below_sill = lighting.sample_in_room_luminance(0, 5.6, 0.2, 5.0);
        assert!(
            below_sill < AMBIENT_LEVEL + 0.02,
            "the wall below the sill must block: {below_sill}"
        );
    }

    /// Two doors in the same partition each transmit on their own, the dark
    /// area keeps its ambient baseline, and the blend stays bounded.
    #[test]
    fn case_6_d_two_doors_transmit_separately_and_stay_bounded() {
        let lighting = bake(&partitioned_room(TWO_DOORS, &fixture(2.0, 5.0)));
        assert_eq!(lighting.zone_count(), 2);

        // The lit area keeps its own baseline, the dark area keeps ambient.
        let lit_side = lighting.baseline_in_room(0, 2.0, 5.0).luminance();
        assert!(
            lit_side > AMBIENT_LEVEL + 0.3,
            "the lit area must keep its power: {lit_side}"
        );
        for (x, z) in [(5.6_f32, 2.0_f32), (5.6, 7.0)] {
            let dark = lighting.baseline_in_room(0, x, z).luminance();
            assert!(
                dark < AMBIENT_LEVEL + 0.02,
                "the dark area's baseline must stay ambient at ({x}, {z}): {dark}"
            );
        }

        // Both thresholds transmit, half a metre past the wall.
        for (label, z) in [("north", 2.0_f32), ("south", 7.0)] {
            let through = lighting.sample_in_room_luminance(0, 5.6, 0.02, z);
            assert!(
                through > AMBIENT_LEVEL + 0.05,
                "the {label} door must transmit: {through}"
            );
        }

        // Every blend and sample stays finite and inside the legal range; a
        // blend delta is bounded by OPENING_BLEND_STRENGTH x the baseline range.
        let max_delta = OPENING_BLEND_STRENGTH * (MAX_BRIGHTNESS - AMBIENT_LEVEL);
        for (x, z) in [
            (5.6_f32, 2.0_f32),
            (5.6, 7.0),
            (4.0, 5.0),
            (6.0, 5.0),
            (9.5, 9.5),
        ] {
            for channel in lighting.opening_blend(0, x, 0.02, z).to_array() {
                assert!(
                    channel.is_finite(),
                    "blend channel at ({x}, {z}) must be finite"
                );
                assert!(
                    channel.abs() <= max_delta + 1e-6,
                    "blend {channel} at ({x}, {z}) exceeds the model's bound {max_delta}"
                );
            }
            assert!(
                is_legal(lighting.sample_in_room(0, x, 0.02, z)),
                "sample at ({x}, {z}) must stay inside [AMBIENT_LEVEL, MAX_BRIGHTNESS]"
            );
        }
    }

    /// Walls authored on the room boundary or at a corner must not erase the
    /// room or produce degenerate areas.
    #[test]
    fn case_6_e_an_edge_wall_does_not_erase_the_room() {
        // The standard full-depth partition, but authored flush with the east
        // edge instead of through the middle.
        let edge = bake(&room_with_wall(
            r#"{ "x": 9.9, "z": 0.0, "width": 0.2, "depth": 10.0, "height": 3.0 }"#,
            &fixture(2.0, 5.0),
        ));
        // A 0.2 x 0.2 corner stub, the smallest footprint the schema allows.
        let corner = bake(&room_with_wall(
            r#"{ "x": 4.9, "z": 9.8, "width": 0.2, "depth": 0.2, "height": 3.0 }"#,
            &fixture(2.0, 5.0),
        ));
        for lighting in [&edge, &corner] {
            assert_eq!(lighting.zone_count(), 1);
            assert!(!lighting.is_partitioned(0));
        }
        // The room itself is untouched: its floor area and fixture survive.
        assert_exact(edge.rooms()[0].area_m2, 100.0);
        assert_eq!(edge.rooms()[0].fixture_count, 1);

        // Samples anywhere, including inside the boundary wall's own strip,
        // stay finite and inside the legal range.
        for lighting in [&edge, &corner] {
            for (x, y, z) in [
                (0.0_f32, 0.02_f32, 0.0_f32),
                (9.95, 0.02, 5.0),
                (5.0, 1.5, 5.0),
                (9.9, 2.9, 9.9),
                (2.0, 0.02, 5.0),
            ] {
                assert!(
                    is_legal(lighting.sample_in_room(0, x, y, z)),
                    "sample at ({x}, {y}, {z}) must stay legal"
                );
            }
        }
    }

    /// Baking the same partitioned level twice gives bit-identical results.
    #[test]
    fn case_6_f_baking_a_partitioned_level_is_deterministic() {
        let level = partitioned_room(DOOR, &fixture(2.0, 5.0));
        let first = bake(&level);
        let second = bake(&level);

        let (a, b) = (first.summary(), second.summary());
        assert_eq!(a.rooms, b.rooms);
        assert_eq!(a.lights, b.lights);
        assert_eq!(a.blockers, b.blockers);
        assert_eq!(a.walls, b.walls);
        assert_eq!(a.zones, b.zones);
        assert_exact(a.min_baseline, b.min_baseline);
        assert_exact(a.max_baseline, b.max_baseline);
        assert_exact(a.average_baseline, b.average_baseline);

        for (x, y, z) in [
            (2.0_f32, 0.02_f32, 5.0_f32),
            (5.6, 0.02, 5.0),
            (9.5, 0.02, 9.5),
            (5.6, 1.5, 5.0),
        ] {
            assert_same_color(
                first.sample_in_room(0, x, y, z),
                second.sample_in_room(0, x, y, z),
            );
        }
        for (x, z) in [(2.0_f32, 5.0_f32), (5.6, 5.0), (8.0, 5.0)] {
            assert_same_color(
                first.baseline_in_room(0, x, z),
                second.baseline_in_room(0, x, z),
            );
        }
    }

    /// Malformed dimensions stay finite: a negative room height falls back to
    /// the reference height and a wall authored below its own base has no solid
    /// volume, so the bake neither panics nor produces NaN. NaN itself cannot
    /// be authored in JSON, so negative values stand in for the same class of
    /// hand-edited input the loader fuzz already covers.
    #[test]
    fn case_6_g_malformed_dimensions_stay_finite() {
        let level = parse(&format!(
            r#"{{
                "format_version": 1,
                "id": "malformed",
                "name": "Malformed",
                "spawn": {{ "x": 2.0, "z": 5.0 }},
                "rooms": [
                    {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": -3.0 }}
                ],
                "walls": [
                    {{ "x": 4.9, "z": 0.0, "width": 0.2, "depth": 10.0, "height": -1.0 }}
                ],
                "ceiling_lights": [{}]
            }}"#,
            fixture(2.0, 5.0)
        ));
        let lighting = bake(&level);
        assert!(lighting.zone_count() >= 1);
        assert!(lighting.rooms()[0].area_m2.is_finite());
        for (x, y, z) in [
            (2.0_f32, 0.02_f32, 5.0_f32),
            (8.0, 0.02, 5.0),
            (5.0, 1.5, 5.0),
        ] {
            assert!(
                is_legal(lighting.sample_in_room(0, x, y, z)),
                "sample at ({x}, {y}, {z}) must stay legal"
            );
        }
    }
}
