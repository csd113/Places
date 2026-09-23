# External texture / material validation

How the external-PNG material architecture is validated, and how to repeat it.
All commands run from the repository root on macOS.

## Automated

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features
cargo build --workspace --all-targets --all-features
cargo test --workspace --all-targets --all-features
python3 tools/assets/validate.py
python3 tools/textures/build.py --check
python3 tools/props/build.py --check
python3 tests/test_package.py
cd level-editor && npm test
```

The Rust tests that carry the contract:

* `materials::tests::decode_png_round_trips_rgba_exactly`
* `materials::tests::decode_png_accepts_rgb_grayscale_palette_and_sixteen_bit_images`
* `materials::tests::malformed_and_truncated_pngs_are_errors_not_panics`
* `materials::tests::oversized_pngs_are_rejected_with_a_clear_message`
* `materials::tests::cache_decodes_each_key_once_and_reuses_the_buffer`
* `materials::tests::shipped_materials_resolve_through_the_catalog`
* `materials::tests::two_materials_sharing_a_texture_share_one_resolved_texture`
* `materials::tests::unknown_material_uses_the_diagnostic_texture_with_a_useful_error`
* `materials::tests::missing_png_falls_back_to_the_diagnostic_and_names_both_ids`
* `materials::tests::pack_materials_parse_both_shapes_and_decode_from_pack_bytes`
* `materials::tests::pack_materials_may_reuse_a_catalog_texture_or_name_a_missing_file`
* `materials::tests::every_shipped_material_resolves_to_a_png_texture`
* `assets::tests::renderer_and_catalog_agree_on_surface_and_decal_ids`
* `assets::tests::level_material_references_cover_floor_regions_too`
* `render::tests::test_shipped_texture_assets_are_opaque_and_within_budget`
* `render::tests::test_shipped_surface_textures_tile`
* `render::tests::test_carpet_png_has_no_metre_checker`
* `render::tests::material_ids_resolve_to_their_own_keys_tiling_and_tint`
* `render::tests::room_material_overrides_pick_the_damaged_sheets_for_that_room_only`
* `render::tests::a_floor_patch_keeps_its_exact_edges_without_a_second_slab`
* `render::tests::wall_material_and_face_overrides_apply_only_to_the_faces_they_name`
* `render::tests::coincident_overlay_walls_become_one_surface_with_material_runs`
* `render::tests::the_shipped_demo_and_the_rendering_fixture_resolve_their_stain_overlays`
* `loader::tests::test_damaged_material_variants_resolve`
* `loader::tests::test_missing_pack_materials_use_the_diagnostic_texture_with_an_error`

## Static frame captures

The `texture_diagnostic` level these commands targeted has been retired; the
commands and spawn coordinates are kept as a historical record of the
validation. The external-texture loading and replacement checks now live in the
Rust and Python suites listed above.

`LIMINAL_CAPTURE` renders one frame and writes a PNG, so every shot needs a
window. Pin the view with `LIMINAL_SPAWN` and, for repeatable shots,
`LIMINAL_BENCH=1 LIMINAL_CAMERA=<yaw>[,<pitch>]`.

```sh
for spec in "a_room_a:1.5,4.0,90" "b_overlay:2.0,7.0,0" "c_alpha_wall:5.0,4.0,90" \
            "d_gable:12.0,5.0,90" "e_recess:9.5,4.0,90" "f_stairs:19.0,4.0,90" \
            "g_elevated:27.0,4.0,180"; do
  name="${spec%%:*}"; spawn="${spec#*:}"
  LIMINAL_LEVEL=texture_diagnostic LIMINAL_SPAWN="$spawn" \
    LIMINAL_CAPTURE="/tmp/places-texture-shots/$name.png" ./target/release/liminal-rust
done

# RGB lighting on external textures (pitch needs the bench camera)
for spec in "rgb_warm:2.0,4.5,0:0,-38" "rgb_blue:6.0,4.5,180:180,-38" \
            "rgb_white:4.0,6.0,0:0,-38"; do
  name="${spec%%:*}"; rest="${spec#*:}"; spawn="${rest%%:*}"; cam="${rest#*:}"
  LIMINAL_LEVEL=texture_diagnostic LIMINAL_BENCH=1 LIMINAL_SPAWN="$spawn" \
    LIMINAL_CAMERA="$cam" LIMINAL_CAPTURE="/tmp/places-texture-shots/$name.png" ./target/release/liminal-rust
done
```

What to look for in the captures:

* the wall arrows point **up** and the corner-marker cluster at a tile corner
  reads red/green/blue/yellow in the authored arrangement (orientation);
* the magenta checker floor, green ring ceiling and blue-striped walls are on
  the surfaces they belong to (assignment);
* the overlay run in `b_overlay` shows the orange/cyan NPOT material over the
  base wall with no coplanar fighting;
* the recess in `e_recess` sits below the floor with real transition faces, the
  gable in `d_gable` is sloped, and the staircase in `f_stairs` reaches the
  elevated room;
* `rgb_warm` / `rgb_blue` show warm or cool casts on the same textures while
  `rgb_white` stays neutral;
* no magenta/black checker anywhere: any diagnostic pattern is a real
  resolution failure and the console names the material.

Cross-checks: `LIMINAL_LEVEL=vertical_diagnostic` shots from
`vertical-geometry-validation.md` must look structurally identical to the
vertical-geometry captures. A matching `LIMINAL_LEVEL=level_1` check was part of
the original validation; `level_1` no longer ships, so it is kept as a
historical note (Level 1 was artistic content and was not redesigned here).

## Interactive walk (state log)

The `texture_diagnostic` level these commands targeted is retired, so the walk
below is kept as a historical record. The `vertical_diagnostic` walk described
under it is the current equivalent.

```sh
# Region staircase into the elevated room: eye 1.93 -> 3.60, x reaches 29+
LIMINAL_LEVEL=texture_diagnostic LIMINAL_SPAWN="21.0,4.0,90" LIMINAL_STATE_LOG=/tmp/walk_stairs.csv \
  ./target/release/liminal-rust &
sleep 4
osascript -e 'tell application "System Events" to set frontmost of process "liminal-rust" to true'
osascript -e 'tell application "System Events" to key down "w"'
sleep 12
osascript -e 'tell application "System Events" to key up "w"'
pkill -f target/release/liminal-rust

# Recess rim: the 1.2 m drop is refused, eye stays 1.60, x stops at 10.0
LIMINAL_LEVEL=texture_diagnostic LIMINAL_SPAWN="1.5,4.0,90" LIMINAL_STATE_LOG=/tmp/walk_recess.csv ...
```

`LIMINAL_STATE_LOG` records `frame,x,y,z,yaw,pitch`; the asserted trajectories
are in the test suite. The vertical-geometry walk itself
(`LIMINAL_LEVEL=vertical_diagnostic`, no spawn) still climbs 1.60 -> 3.60,
passes the shallow recess at 3.25 and stops at x ≈ 24.

## Creator replacement test

Prove that changing a PNG changes the game **without recompiling**. The capture
below targeted the retired `texture_diagnostic` level and is kept as a
historical record; the no-recompile replacement contract it demonstrates is
also covered by the Rust and Python suites listed above.

```sh
shasum target/release/liminal-rust                     # binary fingerprint
cp assets/diagnostic/textures/diagnostic_floor_01.png /tmp/original.png
python3 - <<'PY'
# write any new 128x128 PNG over the file (e.g. solid green with a border)
PY
cp /tmp/new.png assets/diagnostic/textures/diagnostic_floor_01.png
LIMINAL_LEVEL=texture_diagnostic LIMINAL_SPAWN="4.0,6.0,0" LIMINAL_BENCH=1 \
  LIMINAL_CAMERA="0,-38" LIMINAL_CAPTURE=/tmp/after.png ./target/release/liminal-rust
# compare /tmp/after.png with a pre-replacement capture: > 20 % of pixels changed
cp /tmp/original.png assets/diagnostic/textures/diagnostic_floor_01.png   # restore
shasum target/release/liminal-rust                     # must be unchanged
```

The recorded run changed 77.4 % of the captured pixels (62.7 % solid green)
with a byte-identical binary before and after, and restored the original PNG
byte-identically.
