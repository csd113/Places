# Animated Pool caustics — issue #4

Underwater light bands now move across Pool's basin, shallow shelf, steps and
eligible movable surfaces. The shared world shader redistributes existing
diffuse illumination using two advected samples of a committed numeric PNG.
Tile detail, source colour, baked shadows, sheen, emission, transparency and
water attenuation retain their existing paths. There is no additional draw,
render pass, fluid simulation, map field or package format.

![Native Pool shallow shelf and basin](../images/pool-caustics/shelf.png)

Exact rectangle/circle footprints, bottom and free-surface bounds exclude dry
decks, walls, undersides and the water sheet. A threaded spatial tree supports
every resolved volume without a camera-dependent cap; priority-first traversal
preserves authored overlap precedence and prunes coincident volumes. Only a
16-byte clock header changes each wet frame. Dry levels write nothing.

Static caustics require a visible, above-water finite practical light. Ambient,
sun and sky alone do not enable this indoor-pool approximation. Water spanning
rooms, partitions or intervening room floors conservatively disables static
caustics. Existing receiver illumination/shadows supply the local brightness;
this is not a refractive transport solve. A flashlight contributes only its
existing shadowed beam, with an above-water origin and a ray entering through
the water footprint. Submerged flashlights keep their ordinary beam.

## Native acceptance

Final release: `a25b646999d5f5132e158ce3796ca858f4930a951bb64aa2b3916b609dd32fd3`.
Baseline at `535f401`: `ae6d6c9686653481f15b984f813a7092174a173ffca2b23b88b3bdd1890af9cc`.
Apple M2 Pro / Metal, genuine 1280×720 drawable, identical Pool package
`14c3ba2fa44e252a655b2edc28ffe2789e1492c4f7b80e8e2d454d4ff4deda22`.

- Pool live campaign: **39 checks passed**. Basin, shelf/step edge, circular
  hot tub, moving bodies, above/submerged flashlight, Static/Full switching,
  High/Low textures and restoration all captured and visually inspected.
  Renderer state confirms Static (`off` internally), Full and texture quality
  commit independently. Mask residency remains identical across lighting modes.
- Float/sink campaign: **32 checks passed**. Physical duck floats and dense
  drum sinks without recovery. Player campaign: **32 checks passed**, including
  upright standing, explicit crouch, water entry, shelf traversal and dry exit.
- Three tiny 4×4 m native controls in both lighting modes: lit floor changes
  across time; source-disabled and partition-crossing scenes are pixel-identical
  (**zero changed pixels**). Flashlight-only dark controls change above water
  (104,550 / 93,328 pixels, Full / Static) and remain identical submerged.
- Video: **28 checks passed**, 8 seconds, 240 genuine native frames, source ready
  frames 1…479 at stride 2, H.264 1280×720 30 fps BT.709. No interpolation or
  synthesized frames. Video SHA-256:
  `07ec4d8198b67d10f998617f617383a85dd6bbe39f24215b87bf2723b60b88b4`.

![Native Static lighting](../images/pool-caustics/static.png)

## Matched performance

Three alternating baseline/candidate pairs per lighting mode; 600 warm-up and
1,800 measured frames per run, active authored physical water demo, fixed west
camera, High textures/filtering, Full reflections, bloom on, Immediate
presentation. No builds, captures or video encoding ran during timing. Values
below are medians of the three per-run statistics, in milliseconds. GPU samples
are deduplicated hardware elapsed spans; overlapping pass spans are not summed.
CPU values are frame wall time, including renderer/presentation backpressure.

| Measurement | Baseline median / p95 / p99 | Caustics median / p95 / p99 |
| --- | --- | --- |
| Full GPU | 0.817 / 1.644 / 1.922 | 0.957 / 1.832 / 2.589 |
| Static GPU | 0.758 / 1.483 / 1.999 | 0.880 / 1.910 / 2.477 |
| Full CPU frame | 1.359 / 7.544 / 8.760 | 1.427 / 7.636 / 8.589 |
| Static CPU frame | 1.272 / 7.641 / 8.777 | 1.328 / 7.558 / 8.807 |

Median GPU increase: **0.141 ms Full, 0.122 ms Static**. GPU tails also increase,
as shown; these are shared-host observations, not an isolated hardware cost.
Both builds submit **44 scene draws and the same six GPU passes** (scene,
emission, bloom H/V, resolve, presented). No added reflection pass or geometry.

Same-build disabled-effect control, three alternating pairs: GPU median
0.782→0.785 ms; p95 1.672→1.595; p99 2.003→2.457. Actual dry Office, two
alternating pairs: GPU median 0.843→0.922 ms; p95 1.193→1.320; p99
1.268→1.348. Dry CPU median 1.444→1.449 ms; 30 draws unchanged. Dry cost is
bounded by the shared shader's early exit: no mask residency or clock upload.

Pool adds **1,398,100 GPU mask bytes + 256 tree/header bytes** at High;
Low mask storage is 87,380 bytes. An empty level uses a 64-byte zero buffer and
the existing white fallback. A separate fresh-process diagnostic pair observed
peak native RSS 287,113,216→288,555,008 bytes (+1.38 MiB); RSS varies between
processes and is not an exact allocation counter.

Source PNG: 533,201 bytes; production executable: 9,709,344→9,742,384 bytes
(+33,040). No new dependency or compiler stage. Rust build-time delta was not
isolated from the local LTO/verification queue. All eight installed packages
and the catalog remain unchanged; only the three tiny controls were compiled.
No archived Demo/Zoo bake and no tracked compiled package.

## Source gates and reproduction

`sh tools/check-rust.sh` passed format, all-target/all-feature check, strict
debug and release Clippy, and workspace tests: **2,340 passed, 19 explicit GPU /
measurement tests ignored**. `cargo test --workspace`: **2,333 passed, 19 ignored**.
Python source suite: **356 tests, 4 expected skips**. Catalog validation: zero
warnings; texture check: 123 textures, 96 existing advisories; prop check and
new mask seam check pass.

The three new bounded Rust regressions protect numeric/opaque PNG contracts
(one 4 MiB decode), exact water/overlap boundaries and coincident-tree pruning
(32 leaves), and source/partition isolation (one 4 m room). They require no
shipped map, atlas bake or native window.

```sh
python3 tools/bench/water_physics.py --mode acceptance --case caustics \
  --binary local/caustics/final/places --package assets/levels/pool_showcase.placesmap \
  --out local/pool-caustics-review
python3 tools/bench/water_physics.py --mode caustics-video --video-frames 240 \
  --binary local/caustics/final/places --package assets/levels/pool_showcase.placesmap \
  --out local/pool-caustics-video-review
```

Curated local delivery lives in `local/caustics/final/`: playable release tools,
matching Pool package, video, screenshots, concise receipts, performance
summaries and tiny control sources. Imagegen prompt/provenance and the fixed
renderer-resource contract are in `docs/ASSET_SPECIFICATION.md` §7.1b. Linux /
Windows native GPU performance and sun/sky-only caustics were not established.
