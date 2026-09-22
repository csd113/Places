# Wall-boundary lighting isolation and validation (Goal 5.5)

How the wall/corner lighting defects were reproduced, what actually caused them,
how the fix works, and how to repeat every check. All commands run from the
repository root on macOS, with a window (the capture path needs a real GL
context; there is no headless mode).

## The model

Baked lighting is three terms, all resolved once per level load
(`src/lighting/`):

| term | what it is | where |
| --- | --- | --- |
| room baseline | one colour per room from its fixtures' density, compressed and saturating | `lighting/math.rs`, `lighting/bake.rs` |
| local fixture pool | one fixture's own colour, falling off smoothly to zero at 6 m, measured to the fixture's panel | `lighting/bake.rs` |
| doorway blend | a bounded exchange of the two rooms' baselines near a walk-through opening | `lighting/bake.rs` |

Three rules keep the terms from leaking past an opaque wall:

1. **A pool only reaches what the fixture can see.** Every solid patch of every
   wall becomes a world-space box (`lighting/visibility.rs`), built from the
   same `wall_solid_slices_profiled` geometry the mesh and collision use, so a
   door, window, passage or vent removes exactly the box it cuts. A segment
   from the fixture's panel to a sample that crosses a box is blocked; one that
   passes through an opening's own footprint and height is not.
2. **The doorway blend follows the aperture.** The blend is only applied where
   the sample has line of sight through the opening, so an opening joins the two
   rooms through the hole it cuts rather than through the wall around it.
3. **A wall face is lit by the room it opens into.** The emitter resolves each
   face's room once, from an unambiguous point in the middle of the face, and
   the bake uses that room for every sample on the face. A sample that is
   *strictly* inside another room (a face that genuinely spans two rooms)
   overrides the hint; a sample that is merely touching a boundary, or that
   falls inside the perpendicular wall a face ends against, is evaluated inside
   the face's own room instead of dropping to the outside fill.

Nothing in this runs per frame: the boxes, the per-fixture blocker lists and the
per-opening lists are built during the bake, and the render loop still only
reads vertex colours.

## What was actually wrong

The artifacts were three separate causes, all in the *bake*, not in the
renderer:

* **Dark corners / dark wedges.** Wall faces sampled the bake at their own
  endpoints. A wall authored across a room boundary (the construction both Goal
  5 showcase levels use) ends inside the perpendicular wall, so that endpoint's
  probe resolved to no room at all and the face interpolated down to the
  ambient fill over its first 2.5 m segment. A wall face running along a *shared*
  room boundary had the mirror problem: the containment tie-break picked the
  neighbouring room, so a red room's wall carried a blue-grey wedge.
* **White and RGB bleed.** Local fixture pools were pure distance: a fixture
  lit anything within 6 m, including surfaces behind an opaque wall and around a
  closed corner.
* **Darkness bleed.** The same distance-only pool, seen from the other side: a
  dark neighbour could not darken a lit room, but a wall face whose probe landed
  in a solid was left at the ambient fill, which read as darkness on the wrong
  side of a join.

## The diagnostic level

`assets/levels/lighting_isolation.json` is a deliberately plain row of thirteen
cells, one per case. Boot it with:

```sh
LIMINAL_LEVEL=lighting_isolation LIMINAL_SPAWN="3,1.6,3,180" ./target/debug/liminal-rust
```

| cell | case |
| --- | --- |
| `corner_white` | a 90° corner with one white fixture |
| `dark_neighbour` | an empty cell next to a lit one |
| `red_source` / `blocked_from_red` | a red fixture behind a solid wall |
| `door_source` / `through_door` | a doorway that must transmit |
| `window_source` / `through_window` | a window that must transmit only over its sill |
| `red_room` / `blue_room` | two coloured rooms sharing a solid divider |
| `ambient_only` | a genuinely unlit cell, next to a lit one |
| `stub_room` | an interior partition inside one room |
| `corner_rgb` | a red and a blue fixture near the same corner |

It is a test fixture, not a showcase: keep it small, keep the cells separate and
do not dress it up.

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

The tests that carry the phase:

* `lighting_isolation::*` — the acceptance suite over the diagnostic level:
  blocked white light, blocked colour (with the unoccluded contribution
  measured so the test proves the wall is doing real work), doorway
  transmission, window sill and header, spectral isolation, dark-room
  adjacency, an interior stub, corner continuity, per-fixture independence and
  the exact-ambient control.
* `lighting_isolation::emitted_wall_faces_are_lit_by_the_room_they_open_into` —
  the same rules asserted on *emitted vertices*, so a regression in the geometry
  emitter cannot hide behind a correct bake.
* `lighting::tests::openings_blend_between_differently_lit_rooms`,
  `lighting_audit_cases::group_h_*` — the doorway exchange is bounded,
  symmetric, smooth and blind to opaque walls.
* `lighting_audit_cases::group_l_*`, `render::tests::merged_*` — material
  modulation and wall-strip merging are unchanged.
* `lighting/visibility.rs` unit tests — the box set, the opening span, the
  header above a door and an unregistered query site.

## Captures

```sh
mkdir -p target/goal55/captures
LIMINAL_LEVEL=lighting_isolation LIMINAL_SPAWN="34,1.5,2.9,270" \
  LIMINAL_CAPTURE=target/goal55/captures/iso_door.png ./target/debug/liminal-rust
```

`LIMINAL_LEVEL`, `LIMINAL_SPAWN=x,y,z,yaw` and `LIMINAL_CAPTURE` render one
frame of a specific level from a specific position; the process exits after
writing the PNG. Two runs of the same command are byte-identical, which is what
makes a before/after comparison meaningful.

Views used for the phase (all reproduced in the Goal 5.5 changelog entry):

| view | command |
| --- | --- |
| Office corner | `LIMINAL_LEVEL=office_showcase LIMINAL_SPAWN="1.2,1.4,1.2,315"` |
| Office doorway | `LIMINAL_LEVEL=office_showcase LIMINAL_SPAWN="5.5,1.4,0.6,270"` |
| Office floor edge | `LIMINAL_LEVEL=office_showcase LIMINAL_SPAWN="3.0,1.4,3.6,180"` |
| Pool deck | `LIMINAL_LEVEL=pool_showcase LIMINAL_SPAWN="8.0,1.5,1.0,180"` |
| Level 1 shell corner | `LIMINAL_LEVEL=level_1 LIMINAL_SPAWN="-128,1.6,-129,270"` |
| Diagnostic red/blue wall | `LIMINAL_LEVEL=lighting_diagnostic LIMINAL_SPAWN="49.5,1.4,1.5,315"` |
| Diagnostic dark-to-lit door | `LIMINAL_LEVEL=lighting_diagnostic LIMINAL_SPAWN="9.0,1.4,6,90"` |
| Isolation door / red-blue wall | `LIMINAL_LEVEL=lighting_isolation LIMINAL_SPAWN="34,1.5,2.9,270"` / `"59.5,1.5,4.5,180"` |

Walk each of these with the camera moved toward, away from and sideways along
the surface; a dark seam or a coloured edge that changes with the angle is a
regression.

## Performance

The visibility test is static and prefiltered: one flat pool of boxes per
fixture, ordered by distance, with a reach cut-off, so the common query tests
one or two boxes. Measured with
`cargo test --release --bin liminal-rust lighting_benchmark_report -- --nocapture`:

| level | bake before | bake after | build before | build after |
| --- | --- | --- | --- | --- |
| Level 1 (225 fixtures, 208 walls) | 0.02 ms | 2.9 ms | 3.6 ms | 6.5 ms |
| The Residence (44 fixtures, 65 walls, 140 props) | 0.04 ms | 0.3 ms | 5.5 ms | 17–22 ms |
| After the Leak (39 fixtures, 61 walls, 144 props) | 0.03 ms | 0.25 ms | 4.6 ms | 15–18 ms |

The remaining cost is prop vertex lighting (one occluded sample per transformed
prop vertex); the static bake stays under 3 ms for the largest shipped level.

## Known limitations

* The room baseline is a room-wide term by design, so a fixture's *pool* is
  occlusion-tested but its contribution to the room baseline is not. A partition
  inside one room therefore shadows a pool without dimming the room baseline;
  author two rooms to get two baselines.
* Only walls block light. Floors and ceilings are not blockers, so two stacked
  rooms are not separated vertically yet; the blocker set is a flat list of
  boxes, so adding a slab is a small change when a level needs it.
* A surface sample that lies inside a wall (the outermost row of a room's floor
  where a wall straddles the boundary) is walked into the room before it is
  measured. A sample that cannot leave the solid falls back to the room centre.
* Long walls sample at most eight times along their length and large floors at
  most twelve times per axis. On a 264 m shell wall that is one sample per 33 m
  while fixture pools reach 6 m, so a pool can fall between samples. Long-room
  lighting is therefore broad, not exact; it is a sampling-resolution limit, not
  a visibility one.
