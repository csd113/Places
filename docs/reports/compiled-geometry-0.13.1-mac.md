# Compiled geometry continuity: Mac validation, 0.13.1

## Result and scope

Extends the existing overlapping-wall union, material runs, grid merging, spatial batching and indexed deduplication. It does not replace those systems or rewrite authoring JSON.

- Full atlas preparation can connect touching collinear sections with matching explicit height/base, thickness, resolved face/body materials, parent shade/UV frame and stable room ownership. Real gaps are never filled. Existing overlap groups, intersecting cap contacts, differing frames/materials, pitched or partial room contacts and local floor-profile contacts remain separate.
- Full atlas preparation combines contiguous recessed/raised-floor edge runs only with identical plane, normal, low/high, material and original room owner. Static/vertex-lit interpolation remains unchanged.
- Vertically chart-split skirts interpolate the original full-face, already-clamped material gradient instead of restarting it on each tile. This is a real latent/preview bug: the current Full chart span is 63.6875 m, preview span 2 m; scanned active authored offsets are at most 4.595 m. This is not presented as a confirmed shipped compiled-map doorway fix.
- Geometry revision 7 → 8 invalidates explicit preparation caches. Package record layouts and runtime loading contracts are unchanged. A current-player/old-package native control is pixel-exact.

Editable repository sources were preserved byte-for-byte: 66 project/fixture inputs checked. All three measured packages retain byte-identical semantic, collision, navigation and lighting records. Static world meshes and reconstructed Static prop records, including Pool's switched-off fixture state, are byte-identical. Shared prop recipe bytes can differ because cross-quality factoring follows the changed Full UV payload; reconstructed Static bytes were checked, not inferred from recipe names. Full atlases, irradiance, prop atlas UVs and captured probes change with retessellation.

## Measured package and geometry costs

Base source: `aac1c97a19d2307107e9d70c9fb21a01e23983c8`. Candidate is this commit. Same Rust 1.99.1 production all-feature executables, assets, source bytes, 8 workers, `--variants off,full --force`, isolated fresh output paths. Pool was compiled through its owning `projects/environments --level pool`, not as a standalone scripted JSON. The mixed private fixture contains matching and differing material boundaries, a doorway, glazed window, six-step stairs and a split recessed region. Existing `architecture_after.json` and the actual Pool source provide additional stepped/recessed coverage.

Counts below are indexed **world** geometry, not prop/entity geometry. Package sizes include both variants.

| Map | .placesmap bytes, before → after | Full vertices | Full triangles | Full charts | Active chart texels |
|---|---:|---:|---:|---:|---:|
| compiled_sections | 882,240 → 876,367 | 544 → 488 | 272 → 244 | 384 → 370 | 157,341 → 156,541 |
| architecture_after | 3,659,543 → 3,658,278 | 562 → 522 | 280 → 260 | 4,731 → 4,721 | 470,706 → 470,542 |
| pool | 19,355,058 → 19,275,844 | 4,546 → 4,030 | 2,248 → 1,990 | 30,145 → 30,016 | 1,282,597 → 1,277,788 |

Full mesh ranges stay 10 / 19 / 34; atlas pages stay 1 / 1 / 3. Static vertices stay 561 / 657 / 5,842 and triangles 398 / 456 / 5,254. Prop batch counts, collision boxes and navigation cells/regions are unchanged. These are three measured cases, not an all-catalog size or optimization guarantee.

| Map | Total compile wall seconds | Full prepare/encode seconds | Reflection capture seconds | Peak process RSS bytes |
|---|---:|---:|---:|---:|
| compiled_sections | 3.066 → 3.112 | 2.870 → 2.867 | none | 183,992,320 → 182,779,904 |
| architecture_after | 30.748 → 28.402 | 19.742 → 19.830 | 10.522 → 8.156 | 396,935,168 → 400,146,432 |
| pool | 125.256 → 124.680 | 119.465 → 118.961 | 2.726 → 2.704 | 1,275,052,032 → 1,489,354,752 |

The table retains the first clean-force sample per side, not a statistically established bake-speed improvement. A reverse-order Pool repeat reproduced each side's complete package byte-for-byte: candidate 124.320 s / 1,249,443,840 peak RSS bytes; baseline 124.505 s / 1,262,026,752 bytes. Thus the first Pool RSS increase is not a repeatable fixed cost: sampled ranges overlap, the candidate has the higher observed maximum, and no memory reduction is claimed. The two Pool wall-time pairs remain effectively flat. Architecture's lower total mainly reflects its capture phase, not faster Full preparation. Fixture and Pool preparation are effectively flat. Fewer world triangles did not reduce atlas page memory or draw-call count.

## Native appearance and runtime

Apple M2 Pro / Metal, 960×540 logical, verified 1920×1080 drawable, High textures/filtering/lighting, Full lightmaps, bloom and Full reflections. Sixteen accepted baseline/candidate capture pairs cover door/window openings, material boundaries, stairs, drop edges, architecture and Pool. Eight pairs are pixel-exact, including both Static controls and all six separate doorway-control views. The ordinary Full mixed views differ by at most 1/255 except the drop view's few contact pixels (max 9); architecture max 3. Pool average absolute channel delta is 0.033837/255, 99th percentile 1; 92 pixels exceed 10 and two edge-intersection pixels exceed 30 (max 140). Full images and worst-edge crops were inspected: no lost holes, material boundaries, steps or continuous edge, and no appearance degradation accepted as an optimization. Images are not claimed pixel-exact where retessellation changes samples/rasterization.

Three alternating baseline/candidate timing pairs: 300 warmup plus 1,200 measured fixed-delta frames each, V-sync off, no benchmark pacer/frame limiter, actual Immediate presentation, normal presented rendering, no refresh-rate plateau. Per-run logs retain the startup Fifo line and subsequent actual Immediate switch. Mean frame interval 3.233262 → 3.222166 ms; ranges overlap (baseline 3.225–3.245, candidate 3.211–3.242). Effectively flat, **no FPS improvement claimed**. Submitted scene triangles fall 44,722 → 44,464; draws remain 118. Timings are one Pool camera, not a broad renderer benchmark.

Pool's measured isolated member correctly refuses menu loading because it contains project transitions. Native validation instead used private complete-project wrappers: the preserved project's other members and all exact sources stayed unchanged; only each measured Pool member's records were substituted byte-for-byte. All 21 archived sources also match current authoring. Both wrappers validate, and every native receipt verifies the selected Pool member-manifest identity. Wrapper sizes and unrelated transition/environment behavior are not used for geometry claims. See the complete external handoff for archive paths, hashes and launch recipe.

## Low-light doorway finding and limits

No exact user map/camera was supplied. A controlled low-brightness scene reproduces a visible header band with separate side/header walls. Side `y=0,height=3` has material shade 1.006667 at world y=2; header `y=2,height=1` starts at 0.92, about 8.6% lower, before lighting. One parent wall with an opening has a continuous frame. The two controls have pixel-exact base-PNG diagnostics; combined baked-light diagnostics differ by at most 1/255, while final mode differs by up to 8/255 over the header region. The PNG-only diagnostic deliberately excludes tint; it is not evidence that authored shade frames match.

Both forms are individually baseline/candidate pixel-exact. The optimizer deliberately leaves differently framed sections alone rather than silently changing approved appearance. This investigation distinguishes a reproduced material-frame band from lightmap sample continuity; it does not prove the unspecified user seam has been located or fixed. Existing inclusive endpoint-to-texel-center mapping, physical receiver footprints, hole support and guarded indirect cross-chart filtering were inspected and retained. No broad seam blur or direct-shadow filtering was introduced. Stair side per-step UV origins/gradients remain unchanged because broad union would reframe them.

## Regression and production checks

Five small CPU-only regressions protect actual door/window hole coverage, winding/normals, material shade/UV phase and untouched authoring; gap/frame/material/room/overlay boundaries; interleaved crossing-wall cap material ownership; vertical skirt chart-edge gradients/UVs; and exact compatible skirt runs. They neither bake shipped maps nor allocate GPU resources. Entire 528-test Rust suite passes in 2.27 s after compilation. A bounded readonly review caught the cap-ownership counterexample before candidate acceptance; the final guard and emitted-mesh test protect it.

- fmt, locked all-target/all-feature check, strict debug and release Clippy, locked all-feature release workspace build: pass.
- Python: 373 discovered, 16 skipped, no failures (`PLACES_SKIP_SMOKE=1`; direct native checks above are separate).
- Asset validator, texture `--check`, prop `--check`: pass with existing catalog/art warnings; no asset changed.
- Explicit production Lua error-boundary check: 11 bounded refusals and owning-project source initialization pass. The aggregate wrapper's Lua substep assumes `target/release`; its missing-default-path failure is preserved, then the same check passed against the actual private-target compiler.
- All three candidate source/package `verify --require-current` checks pass. Mixed/architecture standalone validation and both complete Pool wrappers pass. Pool member-alone `validate` correctly refuses its project transitions, as does the excluded native attempt. No rejection was relabelled a pass.

No broad renderer audit, prop/entity inventory, Windows phase, gameplay work, canceled Drift Lung, deferred cat, production map authoring or final demo construction is included. GitHub CI is not awaited.

## Evidence

Complete preserved evidence and handoff: `2026-11-07/compiler-geometry/IMPLEMENTATION_HANDOFF.md (local evidence bundle)`.

Machine-readable measurements: `evidence/package-metrics.json`, `compile-cost-comparison.json`, `pool-cost-repeat.json`, `decoded-props.json`, `native-image-comparison.json`, `timing-results.json`, `pool-native-wrapper.json`, `package-validation.json`, `source-preservation.json`. Full commands, raw logs, source fixtures, packages, screenshots and receipts remain outside reproducible build targets. All 14 production executables and 735 exact build inputs are preserved under `candidate/` with SHA256 manifests before target cleanup.
