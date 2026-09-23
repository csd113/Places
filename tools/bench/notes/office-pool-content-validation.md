# Office and Pool content validation

How the first two complete environment families were validated, and how to
repeat it. All commands run from the repository root on macOS, with a window
(the capture path needs a real GL context; there is no headless mode).

## Automated

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features
cargo build --workspace --all-targets --all-features
cargo test --workspace --all-targets --all-features
python3 tests/test_package.py
python3 tools/assets/validate.py
python3 tools/textures/build.py --check
python3 tools/props/build.py --check
cd level-editor && npm test
```

The Rust tests that carry the contract:

* `lighting::tests::fixture_families_own_their_footprint_and_mount`
* `lighting::tests::a_wall_fixture_needs_a_height_and_validation_says_so`
* `assets::tests::renderer_and_catalog_agree_on_surface_and_decal_ids`
* `render::tests::a_catalogued_png_decal_draws_from_its_own_sheet`
* `render::tests::unknown_decal_materials_are_skipped_without_failing_the_build`
* `render::tests::test_carpet_png_has_no_metre_checker`
* `render::tests::test_shipped_surface_textures_tile`
* `loader::tests::test_rendering_diagnostic_level_shows_every_decal_sheet`
* `loader::tests::test_vertical_diagnostic_level_exercises_the_new_geometry`
* `render::tests::the_showcase_level_renders_every_core_prop_with_real_geometry`
* `props::tests::shipped_prop_assets_match_the_catalogue_and_budgets`

and the Python checks in `tests/test_package.py::PoolContentTests`:

* `test_pool_content_is_classified_and_organized`
* `test_the_no_diving_sign_is_external_cut_out_artwork`
* `test_the_pool_showcase_is_real_lowered_floor_geometry`
* `test_the_pool_showcase_places_every_pool_prop`
* `test_guardrails_and_the_ladder_have_collision`

## Static frame captures

The Office commands below target `level_1` and `office_showcase`, two levels
that no longer ship; the spawn coordinates are kept as a historical record of
the validation. The Pool fixture survives as
`tests/fixtures/levels/pool_showcase.json`.

```sh
mkdir -p target/agent-work/captures/office target/agent-work/captures/pool

# Office: Level 1 (the maintained set in place) and the showcase.
for spec in "level1_spawn:0,0,0" "level1_corner:-60,-60,45" \
            "showcase_wall:3.2,2.5,0" "showcase_carpet:3.2,2.5,180" \
            "showcase_ceiling:3.2,2.5,0" "showcase_desk:3.2,2.5,270"; do
  name="${spec%%:*}"; spawn="${spec#*:}"
  LIMINAL_LEVEL=$([ "${name#level1}" != "$name" ] && echo level_1 || echo office_showcase) \
    LIMINAL_SPAWN="$spawn" \
    LIMINAL_CAPTURE="target/agent-work/captures/office/$name.png" ./target/debug/liminal-rust
done

# Pool: the showcase from the deck, the basin, the props and the sign.
for spec in "wide:8,11,0" "deck_edge:8,8,0" "basin:8,5,180" \
            "ladder:11.5,4.5,200" "furniture:6,9.5,150" \
            "curtains:2,8,0" "guardrails:8,6.5,180" \
            "sign:8,5.2,0" "lights:4,2.6,0"; do
  name="${spec%%:*}"; spawn="${spec#*:}"
  LIMINAL_LEVEL=pool_showcase LIMINAL_SPAWN="$spawn" \
    LIMINAL_CAPTURE="target/agent-work/captures/pool/$name.png" ./target/debug/liminal-rust
done
```

Look-down/up shots need the bench camera:

```sh
LIMINAL_BENCH=1 LIMINAL_BENCH_FRAMES=3 LIMINAL_BENCH_WARMUP=1 \
  LIMINAL_BENCH_OUT=/tmp/bench.csv LIMINAL_LEVEL=pool_showcase \
  LIMINAL_SPAWN="4,2.6,0" LIMINAL_CAMERA=0,75 \
  LIMINAL_CAPTURE=target/agent-work/captures/pool/round_light.png ./target/debug/liminal-rust
```

Inspect each PNG for holes (clear-colour blocks), seams, wrong tile scale,
stretched UVs, inverted or black faces, floating/buried props, z-fighting and
lighting that is too flat or too dark. An automated pre-filter counts pixels
that are exactly the clear colour:

```sh
python3 tools/bench/check_holes.py target/agent-work/captures/office target/agent-work/captures/pool
python3 tools/bench/check_holes.py --json /tmp/holes.json target/agent-work/captures
```

A shelled room should be ~0%; a block of clear-colour pixels is a hole — that
check caught a centre-authored wall in the Pool level during this validation. The
tool's default threshold is tiny on purpose: the clear colour is exactly black
while an unlit room still renders its ambient-lit surfaces around 23, so the
many legitimately dim captures stay unflagged.

## Moddability (on the final content)

The final Office and Pool artwork must be replaceable without Rust changes:

```sh
cp assets/environment/office/textures/floors/carpet_beige_01.png /tmp/carpet.png
cp assets/environment/pool/textures/floors/pool_tile_deck_01.png /tmp/deck.png
python3 - <<'PY'
# Write a loud temporary sheet over each final PNG (any pixels will do).
import struct, zlib
def write(path, rgba, w=8, h=8):
    raw = b"".join(b"\x00" + rgba for _ in range(h))
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))
write("assets/environment/office/textures/floors/carpet_beige_01.png", bytes([255, 0, 255, 255]))
write("assets/environment/pool/textures/floors/pool_tile_deck_01.png", bytes([0, 255, 0, 255]))
PY
# Relaunch with no rebuild; the captures must show the temporary colours.
LIMINAL_LEVEL=level_1 LIMINAL_CAPTURE=/tmp/moddable_office.png ./target/debug/liminal-rust
LIMINAL_LEVEL=pool_showcase LIMINAL_CAPTURE=/tmp/moddable_pool.png ./target/debug/liminal-rust
cp /tmp/carpet.png assets/environment/office/textures/floors/carpet_beige_01.png
cp /tmp/deck.png assets/environment/pool/textures/floors/pool_tile_deck_01.png
```

## Interactive walk

`LIMINAL_STATE_LOG` records `frame,x,y,z,yaw,pitch` every few frames so a real
input walk can be asserted afterwards; drive the keys with macOS System Events
(the same approach the `vertical-geometry-validation.md` note documents).

```sh
LIMINAL_LEVEL=pool_showcase LIMINAL_STATE_LOG=/tmp/pool_walk.csv ./target/debug/liminal-rust &
sleep 4
osascript -e 'tell application "System Events" to set frontmost of process "liminal-rust" to true'
osascript -e 'tell application "System Events" to key down "w"'
sleep 3 && osascript -e 'tell application "System Events" to key up "w"'
```

Walk the deck, push into the guardrails and the ladder (the position must stop),
walk onto the `-0.35 m` entry step (the foot height may change) and push at the
deep basin edge (the player must not fall in). Repeat in `office_showcase` and
`level_1` for movement, prop collision and brightness; both of those levels no
longer ship, so those walks are kept as a historical record.
