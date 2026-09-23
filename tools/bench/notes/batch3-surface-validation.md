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

`python3 tools/bench/bench_local.py` (macOS, 960×544, `LIMINAL_BENCH_FINISH=1`,
120 frames after 20 warm-up, 3 runs, medians). `batch2` is a release build of the
Batch 2 checkout with *this* repository's assets, so only the code differs.

| Metric | Batch 2 | Batch 3 (offscreen) | Batch 3 (`NO_OFFSCREEN`) | Batch 3 (Low) |
|---|---:|---:|---:|---:|
| `render_mean_ms` | 0.454 | 0.546 | 0.527 | 0.542 |
| `frame_median_ms` | 0.535 | 0.605 | 0.617 | 0.628 |
| `draw_calls` | 70 | 74 | 74 | 74 |
| `visible_batches` | 70 | 74 | 74 | 74 |
| `vbo_bytes` | 367 296 | 413 928 | 413 928 | 413 928 |
| `index_bytes` | 33 120 | 33 180 | 33 180 | 33 180 |
| `texture_binds` | not counted | 108 | 107 | 108 |
| `material_changes` | not counted | 35 | 35 | 35 |

Reading the numbers:

* **Offscreen presentation costs about 0.02 ms here** (0.546 vs 0.527), i.e.
  ~3 % of the frame at this size. It is one fullscreen textured quad; the work
  is proportional to the drawable's pixel count, so the PocketCHIP's 480×272 is
  well inside budget.
* **Draw calls +4, indices +60 B, vertices +20**: the five glass panes and the
  grille pane. Each is one quad, lightmapped and split by chart/cell like any
  wall surface, and they add no new per-frame state.
* **Vertex memory +12.7 %** (32 → 36 bytes per vertex, plus 20 new vertices).
  The frame costs three signed bytes per vector and one for the sign; that is the
  price of a tangent-space frame on every surface, and it is why the frame is
  packed rather than float.
* **Framebuffer memory**: one RGBA8 colour texture plus a 24-bit depth
  renderbuffer at the scene target's size — 5.2 MiB at 960×544, and 0.9 MiB at
  the PocketCHIP's 480×272 under either profile (Low renders at the reference
  size, so it never allocates more than the device can fill).
* **Texture binds** are now measured. 108 binds for 74 draw batches: the three
  units a material change touches (albedo, emission mask, normal map), the decal
  sheet, the lightmap units once per frame and the presentation quad. A run of
  batches sharing a material costs none — that is what the state cache is for.
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
