# Vertical geometry validation

How the vertical-geometry architecture was validated, and how to repeat it. All
commands run from the repository root on macOS.

## Automated

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features
cargo build --workspace --all-targets --all-features
cargo test --workspace --all-targets --all-features
cargo test --release -- --nocapture lighting_benchmark_report   # bake/build budget table
cargo test -- --ignored generate_lighting_parity_vectors --nocapture  # only after a bake change
python3 tests/test_package.py
python3 tools/assets/validate.py
python3 tools/props/build.py --check
cd level-editor && npm test
```

The Rust tests that carry the contract:

* `level::tests::test_legacy_room_gets_zero_elevation_flat_ceiling_and_the_new_default_height`
* `level::tests::test_gable_ceiling_interpolates_eave_to_ridge_on_both_axes`
* `level::tests::test_floor_region_offsets_resolve_inside_outside_and_last_wins`
* `level::tests::test_floor_grid_cuts_at_region_edges_and_carries_offsets`
* `level::tests::test_floor_region_rims_are_solid_only_for_unwalkable_steps`
* `level::tests::test_walkable_floor_matches_the_surface_queries`
* `level::tests::test_estimate_geometry_accounts_for_regions_and_gables`
* `collision::tests::test_player_band_follows_the_foot_height`
* `collision::tests::test_recess_rims_block_from_below_but_not_from_above`
* `game::tests::test_controller_steps_down_into_a_shallow_recess_and_back_out`
* `game::tests::test_controller_climbs_a_staircase_of_floor_regions`
* `game::tests::test_controller_refuses_a_drop_larger_than_a_step`
* `game::tests::test_controller_cannot_walk_off_the_last_floor_into_the_void`
* `render::tests::test_elevated_room_shifts_floor_and_ceiling_together`
* `render::tests::test_recessed_region_emits_a_lowered_slab_and_real_transition_faces`
* `render::tests::test_gable_ceiling_is_real_sloped_geometry`
* `render::tests::test_gable_end_wall_follows_the_sloped_ceiling`
* `render::tests::test_walls_follow_the_local_ceiling_when_their_origin_is_not_at_zero`
* `lighting::tests::fixture_panel_follows_a_gable_eave_and_ridge`
* `loader::tests::test_vertical_diagnostic_level_exercises_the_new_geometry`

## Static frame captures

`LIMINAL_CAPTURE` renders one frame of a running level and writes a PNG, so
every shot needs a window (no headless mode). Pin the view with `LIMINAL_SPAWN`
and, for repeatable shots, `LIMINAL_BENCH=1 LIMINAL_CAMERA=<yaw>[,<pitch>]`
(`LIMINAL_CAMERA` is ignored unless the bench harness is enabled).

```sh
for spec in "a_spawn:1.5,5.0,90" "b_stairs:6.0,5.0,90" "c_elevated:12.5,5.0,90" \
            "d_shallow:20.4,5.0,90" "e_deep:22.5,4.2,95" "f_gable_north:33.0,1.2,180" \
            "g_gable_west:36.5,5.0,290" "h_gable_ridge:29.0,4.5,30" \
            "i_colored:6.0,6.0,250" "j_looking_back:20.0,5.0,270"; do
  name="${spec%%:*}"; spawn="${spec#*:}"
  LIMINAL_LEVEL=vertical_diagnostic LIMINAL_SPAWN="$spawn" \
    LIMINAL_CAPTURE="/tmp/vertical-captures/$name.png" ./target/release/liminal-rust
done
```

Inspect the PNGs for holes, seams, inverted faces, floating fixtures and
surfaces at the wrong height. A quick automated pre-filter: count pixels that
are exactly the clear colour (`r,g,b < 40`) — a grid of captures should have
almost none, and any block of them is a hole. A one-off Python check over
`/tmp/vertical-captures` did this for the captures.

## Interactive walk (state log)

`LIMINAL_STATE_LOG=file.csv` records `frame,x,y,z,yaw,pitch` every few frames
while the game runs, so a real input walk can be asserted afterwards. Drive the
keys with macOS System Events (W/A/S/D work; synthetic arrow keys were not
delivered by System Events on the validation machine, so the look controls are
covered by unit tests instead):

```sh
LIMINAL_LEVEL=vertical_diagnostic LIMINAL_STATE_LOG=/tmp/walk1.csv ./target/release/liminal-rust &
sleep 4
osascript -e 'tell application "System Events" to set frontmost of process "liminal-rust" to true'
osascript -e 'tell application "System Events" to key down "w"'
sleep 14
osascript -e 'tell application "System Events" to key up "w"'
pkill -f target/release/liminal-rust
```

Expected in the log for that walk: the eye starts at `1.6` (floor `0`), steps
through `1.93 … 3.27` while climbing the region staircase, holds `3.6` on the
elevated floor (`floor_y 2.0`), drops to `3.25` inside the shallow recess
(`-0.35`), returns to `3.6`, and stops just before the deep recess (`x ≈ 24.0`)
because a 1.5 m drop is refused.

For a flat-floor regression walk, `LIMINAL_LEVEL=level_1` had to keep the eye
height exactly `1.6` for the whole walk; `level_1` no longer ships, so that
expectation is kept as a historical record.
