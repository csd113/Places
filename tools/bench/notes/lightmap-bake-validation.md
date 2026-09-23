# Lightmap bake and runtime validation

This note records what Batch 2's baked lightmaps cost and what the numbers were
measured with. Everything here is reproducible with `tools/bench/lightmap_report.py`
(captures + developer log parsing), `tools/bench/visual_check.py` (binary-to-binary
pixel comparison) and the games's own `LIMINAL_BENCH` telemetry.

Machine: Apple Silicon macOS development machine, release build. Absolute times are
not the `PocketCHIP`'s; the proportions and the memory figures are the point.

## What a level load now costs

`[level] ... (lighting A + props B + surfaces C)` plus the new
`[lightmaps] <pages> page(s), <charts> chart(s), <charts texels>, <page texels> (<KiB>),
filled in <ms>` line. Cold runs delete `target/level-cache/lightmaps/` first.

| level | charts | chart texels | pages | page KiB | fill+bake cold | level build cold | level build warm (cache hit) |
|---|---:|---:|---:|---:|---:|---:|---:|
| `places_demo` | 221 | 340 773 | 1 | 3072 | **160 ms** | 179 ms | 9 ms |
| `prop_stress` | 21 | 215 903 | 1 | 3072 | 95 ms | 128 ms | 32 ms |
| `lighting_diagnostic` | 138 | 727 248 | 1 | 3072 | 112 ms | 115 ms | — |
| `lighting_isolation` | 112 | 383 630 | 1 | 3072 | 36 ms | 38 ms | — |
| `prop_showcase` | 25 | 119 783 | 1 | 3072 | 33 ms | 44 ms | — |
| `test_room` | 35 | 112 811 | 1 | 3072 | 5 ms | 17 ms | — |

* Pre-lightmap the same levels build in **6–16 ms**, so the lightmap pass is the
  dominant new load cost. It is a load-time cost only: no per-frame work exists.
* The on-disk cache key covers the level definition, the lightmap config, the
  quality profile, the format version **and the occluder-set fingerprint**, so a
  moved prop, a changed light or an edited prop model re-bakes, while a
  texture-only edit correctly reuses the atlas. A cache hit costs 0 ms.
* Debug builds are ~25x slower here (demo bake ~7.8 s); release is what ships.

## Full vs Low, and why Full is 12 texels per metre

The first implementation used 16 texels/m at Full, which needed **two** 1024²
pages (6 MiB) for the demo because of shelf-packing fragmentation. Measured on
the contact and pool shots, 12 texels/m produced statistically identical output
(luminance mean within 0.0002, local-detail metric identical) while fitting the
whole demo in **one** 1024² page. Both profiles still bake the same patch set:
the chart-span cap is a shared constant (`MAX_CHART_SPAN_M`), not derived from
the density.

| profile | density | page | demo pages | demo lightmap memory |
|---|---:|---:|---:|---:|
| Full | 12 texels/m (8.3 cm) | 1024 | 1 | 3 MiB |
| Low | 8 texels/m (12.5 cm) | 512 | 2 | 1.5 MiB |

Low was also measured against Full on the same shots: contact-shadow detail
falls ~19% (local-detail metric 0.0069 → 0.0056 on the cabinet shot) and mean
luminance moves +0.4%, which is the expected cost of a 1.5x coarser texel grid.
Both are far below the vertex bake's 2.5 m sampling grid.

## Runtime

Same camera, 150 frames each, `LIMINAL_BENCH=1 LIMINAL_VSYNC=off` (the capture
path also skips the swap, so these are CPU submission costs, not presentation):

| run | level | draw calls | visible vertices | batches | render median | VBO bytes |
|---|---|---:|---:|---:|---:|---:|
| Batch 1 baseline | demo | 71 | 10 005 | 83 | 0.033 ms | 284 352 |
| Batch 2, vertex-lit fallback | demo | 72 | 10 305 | 84 | 0.044 ms | 379 136 |
| Batch 2, lightmapped | demo | **64** | 9 918 | 73 | 0.043 ms | 364 352 |
| Batch 1 baseline | prop_stress | 25 | 31 618 | 43 | 0.016 ms | 1 409 232 |
| Batch 2, lightmapped | prop_stress | **20** | 31 408 | 37 | 0.020 ms | 1 870 080 |

* **Draw calls fall** with lightmaps: the merged quads are fewer, so a level
  splits into fewer batches (71 → 64 on the demo, 25 → 20 on the stress level).
* **Frame cost is flat** to within measurement noise (0.01 ms), and the dynamic
  drum path adds one draw call and ~0.05 ms of per-frame update for 400 frames.
* **Vertex memory rises ~33%** because `Vertex` grew from 24 to 32 bytes for the
  lightmap channel — and props pay it too, since they share the vertex type
  while never sampling the atlas (demo 284 KB → 364 KB, stress 1.4 MB →
  1.87 MB). This is the one measured cost of the batch; a prop-only 24-byte
  layout is the obvious Batch 3 follow-up.
* **Lightmap texture memory** is 3 MiB at Full and 1.5 MiB at Low, bound once
  per world draw on texture units 2/3.

## Visual A/B

Lightmaps on vs the exact vertex-lit fallback (`LIMINAL_NO_LIGHTMAPS=1`), same
build, same camera:

| shot | pixels differing | mean delta | worst delta |
|---|---:|---:|---:|
| `demo_desk_contact` | 68.4% | 5.7/255 | 136/255 |
| `demo_cabinet_contact` | 91.5% | 15.0/255 | 37/255 |
| `demo_pool` | 89.9% | 9.5/255 | 39/255 |
| `demo_pool_table` | 81.9% | 2.4/255 | 15/255 |
| `prop_stress_close` (vs Batch 1 binary) | 89.6% | 3.7/255 | 85/255 |

Static prop occlusion also changes the **vertex-lit** path, deliberately: with
occlusion off the render is bit-identical to Batch 1, and with it on 17.2% of
the demo's pixels move (mean 4.7/255, worst 22/255), all of them darkening
around placed props. The largest single connected change on the desk shot is
the contact shadow under the desk.

All 14 benchmark captures were checked with `tools/bench/check_holes.py`
(0.0% near-black each): the lightmap pass introduces no unlit surfaces.
