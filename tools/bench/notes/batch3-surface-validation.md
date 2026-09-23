# Batch 3 validation: surface response, transparency and the offscreen scene

How the three Batch 3 subsystems were measured and inspected, on the same
machine, with the same level, assets, camera and frame count in every run.
Everything here lives under `target/agent-work/`, per the repository rule for
temporary files.

## Pixel behaviour: the offscreen path changes nothing

`LIMINAL_NO_OFFSCREEN=1` draws the scene straight into the window; the default
draws it into an offscreen colour+depth target and presents it with one
fullscreen quad. Same binary, same level, same camera, seven fixed views
(`tools/bench/capture_batch3.sh`):

| View | Differing channels > 2 of 255 | Mean channel delta |
|---|---:|---:|
| office, through the windows | 0.0 % (2 pixels > 8) | 0.00 |
| pool side of a window | 0.0 % | 0.00 |
| pool east wall panels | 0.0 % | 0.00 |
| plastic panel | 0.0 % | 0.00 |
| wet deck | 0.0 % | 0.00 |
| corridor sign | 0.0 % | 0.00 |
| linoleum patch | 0.0 % | 0.00 |
| main menu (UI + scene) | 0.0002 % (3 channels) | 0.000 |

Two isolated pixels in the office view (of 522 240) and three channels in the
menu capture differ: sub-pixel rasterisation at a geometry silhouette, the same
class of ±1 ULP difference the Batch 2 renderer note documents for a recompiled
binary. The offscreen target is created with a 24-bit depth renderbuffer (the
log line reports `24-bit depth`), so the decal depth bias behaves as it did.

The UI is unaffected by construction: `render_ui` still draws into the default
framebuffer with the 480×272 reference viewport, after the presentation quad.

## Full vs Low: same scene, less optional work

`LIMINAL_QUALITY=low` on the same capture set differs from Full on 8–28 % of
pixels (mean channel delta 1.1–2.4), concentrated where the response and the
texture budget differ:

* the brushed-metal panel loses its band shading (the normal map) and its
  sheen; it reads as a flat dark panel;
* every texture is downscaled to the Low budget, so tiles and wallpaper soften;
* geometry, ids, materials, emission, alpha and glass stay identical, and the
  glass panes still draw (transparency is correctness, not an optional effect).

That is the intended contract: Low changes how much reaches the GPU, never what
the level authored.

## Frame cost, draw calls and memory

`python3 tools/bench/bench_repeat.py` (macOS, 960×544, `LIMINAL_BENCH_FINISH=1`,
120 frames after 20 warm-up, 9 runs each). `b2_baseline` is a release build of
the Batch 2 checkout with *this* repository's assets, so only the code differs.

| Metric | Batch 2 | Batch 3 (offscreen) | Batch 3 (`NO_OFFSCREEN`) |
|---|---:|---:|---:|
| `render_mean_ms` min / median | 0.348 / 0.377 | 0.473 / 0.491 | 0.458 / 0.483 |
| `frame_median_ms` min / median | 0.431 / 0.476 | 0.576 / 0.602 | 0.522 / 0.558 |
| `draw_calls` | 70 | 75 | 75 |
| `vbo_bytes` | 367 296 | 414 936 | 414 936 |
| `index_bytes` | 33 120 | 33 264 | 33 264 |
| `texture_binds` | not counted | 39 | 38 |
| `material_changes` | not counted | 37 | 37 |

(The Batch 3 numbers include the transfer grille added in the validation commit;
the one-shot runner's earlier numbers, 74 draw calls / 108 binds, are superseded
by these. A single run of this benchmark varies by ±0.1 ms on this machine, which
is why the table reports the minimum as well as the median over nine runs.)

Reading the numbers:

* **Offscreen presentation costs about 0.01 ms here** (0.483 vs 0.491 median,
  3 % of the frame at this size). It is one fullscreen textured quad, and the
  work is proportional to the drawable's pixel count — the PocketCHIP's 480×272
  is well inside budget.
* **Batch 3 costs ~0.11 ms more per frame than Batch 2** on this machine
  (0.377 → 0.491): the presentation pass, five more draw batches (the panes),
  the per-material response/alpha state, and 12.7 % more vertex bytes. On a
  half-millisecond frame this is a *relative* cost, not an absolute one: nothing
  here is per-pixel work on the whole surface (the response is only live on the
  two panel materials), and the device-side numbers need the PocketCHIP bench.
* **Draw calls +5, indices +144 B, vertices +28**: the glazed openings (five
  windows and the transfer grille) plus the three panel walls and two floor
  patches the demo added. Each pane is one quad, lightmapped and split by
  chart/cell like any wall surface, and adds no new per-frame state.
* **Vertex memory +13 %** (32 → 36 bytes per vertex, plus the new geometry). The
  frame costs three signed bytes per vector and one for the sign; that is the
  price of a tangent-space frame on every surface, and it is why the frame is
  packed rather than float.
* **Framebuffer memory**: one RGBA8 colour texture plus a 24-bit depth
  renderbuffer at the scene target's size — 5.2 MiB at 960×544, and 0.9 MiB at
  the PocketCHIP's 480×272 under either profile (Low renders at the reference
  size, so it never allocates more than the device can fill).
* **Texture binds** are now measured, and the surface-state cache keeps them
  down to one per material change for ordinary content: 39 binds for 75 draw
  calls, where the same run bound 108 before the per-sampler comparison was
  added. A run of batches sharing a material costs none at all, and the two
  samplers a material does not use are left alone.
* **Low shows no frame-time win on this machine**, because the macOS driver is
  not fill-bound at this size. Its purpose is the Mali-400: a quarter of the
  scene pixels and the response term are exactly the costs that device pays.

## Scene correctness

* `cargo test --workspace --all-features`: 646 passed, 0 failed, 1 ignored.
  The new coverage is listed in the report; the Batch 2 lighting, lightmap and
  audit suites are untouched and still pass.
* `python3 tools/assets/validate.py`: 87 assets, 0 warnings.
* `python3 tools/textures/build.py --check`: 30 textures, 12 soft size warnings
  (the shipped 1024px sheets, by design); every new sheet passes the tiling seam
  metric.
* `cd level-editor && npm test`: 144 passed (the editor ignores the new optional
  fields, so it loads the shipped demo unchanged).
* Places Demo was inspected in all seven views at Full and Low, and through the
  direct path, before this note was written.

## What Batch 4 could pick up

* Post-processing in the offscreen pass (bloom, exposure, grading, fog): the
  target exists and the presentation pass is one file.
* Refraction/transmission and glass-aware lighting (currently a pane does not
  tint the bake).
* Per-object transparency for GLB props (`alphaMode` is not read yet).
* Realtime specular: the response has no light direction because the bake has
  none; a realtime light would give it one.
