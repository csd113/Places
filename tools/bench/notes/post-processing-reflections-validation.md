# Post-processing, reflections and visual-repair validation

How the four change areas were measured and inspected, on the same
machine, with the same level, assets and cameras in every run. Everything here
lives under `target/agent-work/`, per the repository rule for temporary files.

The before/after pairs were captured with the same script against two binaries:

```sh
sh tools/bench/capture_views.sh                                  # this build
LIMINAL_BIN=target/agent-work/baseline/target/release/liminal-rust \
    sh tools/bench/capture_views.sh                              # baseline, same assets
```

The baseline binary lives in the worktree this validation created; it can be
rebuilt from the branch point with:

```sh
git worktree add target/agent-work/baseline 708c177
cd target/agent-work/baseline && CARGO_TARGET_DIR=$PWD/target cargo build --release
```

(`708c177` is the previous build's verification commit. `target/agent-work/` is ignored by
git, so the worktree never shows up as repository content.)

`LIMINAL_BENCH_NOSWAP=1` is set by the script: a desktop whose display has gone
to sleep blocks inside `SDL_GL_SwapWindow`, and the switch is what lets a capture
run finish. It does not change a pixel.

## The four repairs

**Pause-menu colour.** `render_ui` bound the world program and then *asserted*
that the surface-state cache held a plain state without uploading one, so the
uniforms still held whatever the last world material left in them. In the pool
the last translucent surface is a glass pane, whose `u_opacity` (0.88) and
emission then applied to the whole HUD. `before_pause_pool.png` shows the pause
panel as a washed-out grey you can see the pool ladder and guardrail through;
`after_pause_pool.png` shows it opaque and dark, exactly as authored
(`[0.06, 0.06, 0.05]` frame, `[0.12, 0.11, 0.10]` panel). The fix applies
`SurfaceState::plain` through the ordinary `apply_surface_state` path, so the UI
cannot inherit a material by construction; `the_hud_surface_state_disables_every_world_term`
pins the state it applies.

**The white region beside the pool curtains.** Not a bake problem: the vertex
frame was never wired. `set_vertex_attributes` pointed five attributes and left
`a_normal`, `a_tangent` and `a_handedness` at the generic attribute default
`(0, 0, 0, 1)`, so `v_normal` was the zero vector on every fragment and the
sheen's grazing lobe (`pow(1 - 0, ...) == 1`) was pinned on everywhere. A surface
in the curtains' shadow therefore received a constant additive
`specular × 0.55 × light` term: the brushed-metal screen read (137, 157, 186)
instead of (69, 80, 98), and the wet deck read as a flat white rectangle.
`after_curtains.png` shows both surfaces dark and correctly shaded, and the
brushed normal map on the screen is visible again. The exact layout's lightmap
offsets were wrong in the same function (36/40, pointing into the normal bytes
instead of 64/68); both are now derived from one pointer table and pinned by
`the_vertex_attribute_table_wires_every_scene_attribute_in_both_layouts`,
`the_vertex_shader_declares_exactly_the_attributes_the_table_wires` and
`the_exact_vertex_offsets_match_the_vertex_struct`.

**Window corners.** `emit_wall_slice_cap` built its quad in the wall unit's local
length space and never added the wall's length origin, so every sill and header
on a wall whose min corner is not zero was shifted along the wall by that origin:
a 0.15 m gap at one jamb and an equally sized overhang buried in the wall at the
other, on all six glazed openings of the demo. `before_jamb_left.png` shows the
office side of the pool window with a yellow sliver of wallpaper visible *above*
the header line; `before_jamb_right.png` shows the header underside stopping
short of the right jamb. Both are closed in the `after_` pair.
`every_window_cap_spans_its_opening_in_world_space` is the regression test: it
builds a wall offset from the origin and fails with
`the cap at y = 1 spans 2.5..4.5, expected (6.5, 8.5)` when the translation is
removed (verified by reverting the fix).

**The two thin white boxes.** They are the surface-response
demonstrations: a 0.06 × 2.6 × 1.7 m brushed-metal slab at (25.78, 7.3) and a
0.06 × 2.4 × 1.6 m moulded-plastic slab at (5.2, 7.16), both authored as thin
`walls` in `places_demo.json`. Bare, they read as unexplained white panels. Each
now sits inside a four-rail brushed-metal frame and carries a backlit sign face,
so they read as the pool's two illuminated notice boards; the north wall's sign
is the one on the failing ballast. `after_panels_east.png` and
`after_plastic_panel.png` show them, and `surface_audit`'s
`the_shipped_demo_has_no_coincident_architecture_surfaces` still passes: the
frames and signs are offset by 2–10 mm so no two architecture surfaces are
coplanar.

## Post-processing

`LIMINAL_NO_BLOOM=1` reproduces the frame without the emissive pass and the blur
on the same build, so the stage can be measured in isolation. Bloom is visible
only where something emits: `b4_pool_wide.png` has soft halos on the round pool
lights and the corridor light seen through the window, and
`nobloom_curtains.png` is pixel-identical to the same frame with bloom on because
no emissive surface is in that view.

**Occlusion is part of the stage.** The emissive image shares the scene's depth
buffer, so an emitter behind a wall cannot glow through it. The first
implementation drew the emissive image at a quarter resolution, which reads only
the bottom-left corner of a full-resolution depth buffer: the corridor capture
showed two fluorescent panels from the rooms behind it glowing through the
wallpaper. `captures/crop_corridor.png` is that frame and
`captures/crop_corridor3.png` is the fixed one.

`Limit`: the emissive image is drawn at a quarter of the scene target's edge and
blurred with two separable 5-tap passes, so the glow is a low-frequency pool
rather than a halo with structure.

Tone and grade are a single fullscreen resolve pass; fog is in the world
fragment stage and therefore free of any extra pass. `Low`'s resolve settings are
the identity, so the renderer presents the scene with the plain copy quad:

| Profile | resolve stage | bloom | planar reflections | probes |
| --- | --- | --- | --- | --- |
| `Full` | exposure, shoulder, grade | yes | yes, one plane per frame | 64 texels/face |
| `Low` | skipped (plain copy quad) | no | no | 32 texels/face |

## Reflections

`LIMINAL_NO_REFLECTIONS=1` measures the same build with every reflection source
off. On the curtains view (the wet deck on screen) the two frames differ on 1 482
significant pixels, all on the deck patch: the planar pass is a real, localized
effect rather than a global tint.

The plane is derived from the emitted geometry, not from the level file, so the
regression test is in the geometry path:

* `the_demo_routes_its_reflective_materials_to_a_plane_and_a_probe` builds the
  shipped demo, asserts exactly one mirror plane exists, that its normal is +Y and
  its offset is `1.5` (the pool deck at `y = -1.5`), that
  `core:pool_deck_wet_01` routes to it, that `core:linoleum_polished_01` routes to
  a probe instead, and that `core:carpet_beige_01` — which authors no reflection —
  stays exactly `MaterialReflection::NONE`.
* `the_mirror_matrix_reflects_points_and_fixes_the_plane` pins the mirror maths:
  a point on the plane is a fixed point, which is what makes the projected lookup
  line up with the surface.

Memory the reflections add at 960×544 `Full`: one 480×272 RGBA8 planar target
(0.5 MiB) plus its 16-bit depth (0.25 MiB), and two 64-texel cubemaps
(0.19 MiB total). `Low` drops the planar target and halves the probes.

## Dynamic content

The rotating washer drum is the earlier dynamic-content demonstration and is unchanged
(`b4_drum2.png`). Animated emissions are verified by capturing the same
camera at several frame numbers and reading the sign's own pixels:

| frame | sign pixel | note |
| --- | --- | --- |
| 20 | (172, 209, 208) | at rest |
| 140 | (150, 200, 198) | mid-stutter (the 7.5 Hz flicker) |
| 180–300 | (172, 209, 208) | at rest again |

The animation is deterministic in *time* (`EmissionAnimation::factor` is a pure
function of elapsed seconds and both shapes start at full brightness), advanced
by the simulation's own delta and bounded by `depth`. A frame number therefore
does not pin the phase — the table above is one run's sample of the stutter, not
a reproducible frame reference — while frame 1 always shows the authored
brightness. The unit tests pin the bounds, the resting share of a flicker
(> 60 %) and the first-frame identity.

## Frame cost

`python3 tools/bench/bench_local.py --repeat 9` (macOS, 960×544, `LIMINAL_BENCH_FINISH=1`,
120 frames after 20 warm-up, 7–9 runs each, camera `74,0`). `baseline` is the
previous checkout's own binary; every other row is this build.

| Run | `render` min / median | draws | binds | material changes |
| --- | --- | --- | --- | --- |
| `baseline` | 0.430 / 0.445 | 75 | 48 | 37 |
| `full` | 0.894 / 0.954 | 77 | 97 | 77 |
| `full`, `LIMINAL_NO_BLOOM=1` | 0.707 / 0.718 | 77 | 91 | 77 |
| `full`, `LIMINAL_NO_REFLECTIONS=1` | 0.666 / 0.678 | 77 | 58 | 43 |
| `low` | 0.467 / 0.474 | 77 | 48 | 38 |
| `direct` (`NO_OFFSCREEN`) | 0.452 / 0.466 | 77 | 48 | 38 |

Reading the numbers:

* **Each stage is measurable on its own build.** Bloom costs about 0.19 ms
  (0.894 → 0.707) and the planar reflection about 0.23 ms (0.894 → 0.666); the
  rest of the difference against the baseline — about 0.24 ms — is the resolve pass
  itself, the fog term in the world shader and the two notice boards' extra
  geometry. `reflection_passes: 1` in the same run's summary confirms the plane
  was on screen for these frames.
* **Bloom costs what it costs because it is occlusion-correct.** The emissive
  image is drawn at the *scene target's* resolution and shares the scene's depth
  buffer; a quarter-resolution image would read the wrong corner of that buffer
  and let an emitter hidden behind a wall glow through it (which is exactly what
  the first implementation did, and what
  `captures/crop_corridor.png`/`crop_corridor3.png` show). Only the emissive
  batches are submitted, so the extra pixels are the few the emitters cover, and
  a view with no emissive surface on screen skips the stage entirely.
* **`Low` costs about the same as `direct`** (0.467 vs 0.452, inside the
  run-to-run spread) and has the baseline's bind and material-change shape: its
  resolve settings are the identity, so it presents the scene with the plain copy
  quad and never allocates a planar or bloom target. What is left is the fog and
  the new content.
* **Draw calls +2** (75 → 77): the two notice boards' extra wall ranges. The
  reflection and emissive passes submit the *same* batches in their own passes,
  so they raise binds and material changes rather than the main pass's draw-call
  count — 97 binds and 77 material changes for 77 draws against the baseline's 48
  and 37.
* **Memory**: at 960×544 `Full` adds one 480×272 RGBA8 planar target (0.5 MiB)
  with a 16-bit depth (0.25 MiB), a scene-sized emissive target (2.0 MiB,
  RGBA8), two 240×136 blur targets (0.26 MiB) and two 64-texel cubemaps
  (0.19 MiB) — about 3.2 MiB. `Low` adds two 32-texel cubemaps (0.05 MiB) and
  nothing else.
* **Level load grows by the probe bake**: 3.6 ms for two 64-texel probes (2.3 ms
  at 32 texels on `Low`), once per level load, reported as
  `[reflections] baked N probe(s) at 64 texels/face in 3.6 ms`.
* **The resolve pass, the emissive pass and the planar pass are proportional to
  the drawable**, so on the PocketCHIP's 480×272 they are a quarter of the pixels
  measured here (and the planar pass is half-resolution on top). The device
  numbers still need the PocketCHIP run; this machine's driver is not a proxy
  for it.

## What was inspected visually

Every capture below is in `target/agent-work/captures/` and was viewed at
full size, not only diffed numerically:

* the pause menu over the office and over the pool (before/after);
* all six glazed openings from both sides, straight on, at grazing angles and at
  close range, plus the transfer grille;
* the pool curtains, the wet deck from directly above and at a grazing angle, and
  the two notice boards;
* the corridor sign, the linoleum patch, the spawn view, the washer drum;
* `Full`, `Low`, `LIMINAL_NO_OFFSCREEN=1`, `LIMINAL_NO_BLOOM=1` and
  `LIMINAL_NO_REFLECTIONS=1` on the same views.

The `Low` captures keep the scene's geometry, materials, emission, alpha and fog
identical and differ only in texture resolution, the surface response and the
post-processing stage, which is the profile's contract.
