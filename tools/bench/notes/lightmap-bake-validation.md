# Lightmap bake and runtime validation

This note records what the baked lightmaps cost and what the numbers were
measured with. Everything here is reproducible with `tools/bench/lightmap_report.py`
(captures + developer log parsing), `tools/bench/visual_check.py` (binary-to-binary
pixel comparison) and the games's own `LIMINAL_BENCH` telemetry.

Machine: Apple Silicon macOS development machine, release build. Absolute times are
not the `PocketCHIP`'s; the proportions and the memory figures are the point.

## What a level load now costs

`[level] ... (lighting A + props B + surfaces C)` plus the new
`[lightmaps] <pages> page(s), <charts> chart(s), <charts texels>, <page texels> (<KiB>),
filled in <ms>` line. Cold runs delete `cache/lightmaps/` below the run
directory's state root first.

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
|---|---|---|---:|---:|---:|---:|---:|
| pre-lightmap baseline | demo | 71 | 10 005 | 83 | 0.033 ms | 284 352 |
| lightmaps off (vertex-lit fallback) | demo | 72 | 10 305 | 84 | 0.044 ms | 379 136 |
| lightmaps on | demo | **64** | 9 918 | 73 | 0.043 ms | 364 352 |
| pre-lightmap baseline | prop_stress | 25 | 31 618 | 43 | 0.016 ms | 1 409 232 |
| lightmaps on | prop_stress | **20** | 31 408 | 37 | 0.020 ms | 1 870 080 |

* **Draw calls fall** with lightmaps: the merged quads are fewer, so a level
  splits into fewer batches (71 → 64 on the demo, 25 → 20 on the stress level).
* **Frame cost is flat** to within measurement noise (0.01 ms), and the dynamic
  drum path adds one draw call and ~0.05 ms of per-frame update for 400 frames.
* **Vertex memory rises ~33%** because `Vertex` grew from 24 to 32 bytes for the
  lightmap channel — and props pay it too, since they share the vertex type
  while never sampling the atlas (demo 284 KB → 364 KB, stress 1.4 MB →
  1.87 MB). This is the one measured cost of the lightmap channel; a prop-only
  24-byte layout is not implemented.
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
| `prop_stress_close` (vs pre-lightmap binary) | 89.6% | 3.7/255 | 85/255 |

Static prop occlusion also changes the **vertex-lit** path, deliberately: with
occlusion off the render is bit-identical to the pre-lightmap build, and with it
on 17.2% of the demo's pixels move (mean 4.7/255, worst 22/255), all of them
darkening around placed props. The largest single connected change on the desk
shot is the contact shadow under the desk.

All 14 benchmark captures were checked with `tools/bench/check_holes.py`
(0.0% near-black each): the lightmap pass introduces no unlit surfaces.

## Repair: dark rings and material-boundary seams

Visual validation found two artifacts the automated checks had missed.
Both were fixed in the engine, not in the level, and both root causes are
measured in `target/agent-work/lightmap-repair/reports/` (two independent runs)
with A/B captures under `target/agent-work/lightmap-repair/captures/`.

### Dark rings and blotches around fixtures (visual failure A)

**Cause: visibility, not the falloff.** Every ceiling sample sits exactly on the
plane of the room's own ceiling body, and the visibility clip's contract is that
such an endpoint only *touches* the slab and does not cross it. In `f32` the
entry parameter `(low - start) * (1 / delta)` rounds a few ULPs below `1.0`, so a
plain `enter < exit` read the grazing segment as a crossing. The fixture's whole
pool was then deleted for that sample. Because the segment's vertical span
depends only on the horizontal distance to the emitter footprint, the failures
were coherent *rings*: 8.7% of a 0–5 m ceiling sweep, 10–14% of a fixture's
ceiling window, 60–78 RGB8 levels between neighbouring texels. At the pre-lightmap
2.5 m vertex grid the same samples were spread over whole quads; at 8–12
texels/m they became hard rings, and the office/pool ceilings went from smooth
pools to mottled rings in the render.

The clip now requires the clipped overlap to exceed `SEGMENT_CLIP_EPS` of the
segment's own length (1e-5 — 60 µm on a 6 m ray, far below any solid), which
absorbs the rounding without weakening real occlusion. The strict-start nudge and
the exact box sizes are unchanged, so the wall-seam leak the nudge guards against
cannot return. The same fix repairs the vertex-lit fallback; the repaired
lightmap and the vertex-lit control now agree on the pool ceiling to within
0.53/255 (mean 0.17/255), against +6.7/255 rings before.

### Lighting steps at material boundaries (visual failure B)

**Cause: chart-edge sampling.** `fill_chart` evaluated texel *centres*, and a
chart's geometric edge reconstructs its border texel's value (the gutter is a
copy of that texel), i.e. the light half a texel *inside* its own patch. Two
coplanar patches sharing an edge each reconstructed their own inward-shifted
value, and the two shifts point in opposite directions: a first-order step of
`grad * (tA + tB) / 2` at every chart boundary — 2–5 RGB8 levels at the shipped
densities, proportional to 1/density, and present whenever the albedo material
changed even though the lighting was continuous.

Chart texels now *span* their patch: the first and last texel sit exactly on the
patch's geometric edges, so two coplanar charts evaluate the same world point on
a shared edge and store it in the texel that edge reconstructs. The measured
seam step is 0.00–0.01/255 at both profiles (it was 2–5/255); a real 90-degree
corner is unaffected, because those samples are different world points to begin
with and nothing is averaged. The emitter-level `merge_light_runs` span cap was
also off by one segment (the face's final boundary was never checked), which let
a long wall become one over-long chart and silently broke the shared
`MAX_CHART_SPAN_M` invariant; it now stops at the cap.

### Cost of the repair

| measurement | before repair | repaired |
|---|---:|---:|
| demo cold lightmap fill | 162.9 ms | 168.2 ms (+3%) |
| demo warm (cache hit) level build | 8.7 ms | 9.2 ms |
| demo atlas | 1 page, 3072 KiB | 1 page, 3072 KiB |
| demo charts / chart texels | 221 / 340 773 | 221 / 340 773 |
| demo static vertices | 1772 | 1772 |
| demo draw calls (150 frames) | 64 | 64 |
| demo frame median | 0.048 ms | 0.049 ms |
| `prop_stress` frame median | 0.026 ms | 0.024 ms |

The +3% bake cost is the per-texel "is this sample buried in a wall?" check that
lets a floor or ceiling boundary row take the same walked path the vertex bake
uses (without it, a wall authored across a room boundary leaves a dark rim along
its own base). No atlas page, chart, vertex or draw call changed, and no
per-frame work was added. A cached atlas from an older build is rejected by
`LIGHTMAP_FORMAT_VERSION = 3`.
