# Winter validation and cost record — October 8, 2026

The exact pushed SHA and clean-checkout CI result are recorded in the final task handoff. [Commit history](https://github.com/csd113/Places/commits/Winter-expansion/docs/style-upgrade-20261007/winter) and the [Rust verification workflow](https://github.com/csd113/Places/actions/workflows/rust.yml?query=branch%3AWinter-expansion) provide publication links. This revision builds on the completed Outdoors SHA `026eedb1995669e3c8951c1da447d88edc0fe316`.

## Asset and native checks

| Check | Result / evidence |
| --- | --- |
| Catalog | PASS: 352 assets, 204 placeables, five themes, zero warnings; twelve new Winter-local entries, all preceding entries exact. |
| Focused Winter Python tests | PASS: 12 tests including canonical bases/materials/UVs/PNGs, supported tree loads, complete old/new reproducible exports, closure/winding/budgets, exposed versus sheltered snow, original collision and real raised-frame alignment. Final asset-only regression: 8 tests. |
| Winter + Zoo Python campaign | PASS: 22 tests before the added frame regression; final frame/asset checks and Zoo deterministic `--check` pass. Existing registration/growth/removal and serial/parallel inspection assertions retained. |
| Full integrity audit | PASS: 204 GLBs, 206 embedded PNGs, 296 standalone PNGs, 37 maps, 68 materials; zero errors. Full JSON retained outside target. |
| PNG / prop checks | PASS: 80 registered surface sheets; 73 soft resolution/manifest warnings. Prop pack 52,090 triangles / 42,084 KiB decoded RGBA, below the unchanged 64 MiB pack budget. All embedded images have equivalent real source PNGs. |
| Master/runtime derivatives | PASS: `build_winter_textures.py --check` reproduces all repaired surface, 256² snow/ice and 1024²/256² village atlas files exactly from committed masters. |
| Periodic surfaces | PASS: all four snow/ice/stone/timber sheets meet raw and smoothed seam metrics on both axes. |
| Geometry | PASS: Winter checker reports zero errors/warnings. The existing 159 open-ground / 24 exterior leak annotations remain exactly scoped. Added shells have no degenerate, boundary, nonmanifold, inconsistent, flipped or contradictory geometry. |
| Normal compile | Final Winter: 50,056,698 bytes, 105.946 s; severe: 50,056,737 bytes, 92.965 s; Zoo: 87,454,894 bytes, 334.660 s. All Off/Medium/Full variants baked normally. 45 Winter / 32 Zoo sub-texel sliver quads retain the existing vertex-lit fallback; no compile warnings in JSON. |
| Packages | PASS: Winter, severe and Zoo `verify --require-current`; all three ZIP integrity checks pass. Other bundled maps do not depend on Winter assets and remain untouched. |
| Native comparison | Twelve High + four matched Low + four severe views on each side: **40 matched actual PNGs**, plus one supplementary final Low square. Apple M2 Pro, wgpu Metal, complete matching camera/settings/build hashes. First native candidate and its defect preserved separately. |
| Native controls / weather | PASS: six established routes in each calm and severe mode (**12 runs**): lodge, stairs, pond, forest, ice coasting and snow stopping. Controller bounds satisfy original tests; zero missed ready presentations, weather rejection accounting valid, particle capacity growth zero. |

Links: [focused tests](checks/winter-focused-final.log), [prop budget](checks/winter-prop-check.log), [seams](checks/winter-seams-final.log), [derivatives](checks/winter-derivatives.log), [geometry](checks/winter-final-geometry.log), [archive integrity](checks/winter-zip-check.log), [native traversal](traversal.json), [scope evidence](scope.json).

## Rust gate and retained local compatibility

Repository formatting, locked all-target/all-feature `cargo check`, strict debug Clippy and strict release Clippy pass. No lint level, allowlist or triangle budget is raised. The only Rust edit updates the existing tree-budget explanatory comment to the current 708/1,284/1,380 counts; production renderer, weather, controller, compiler and entity code are untouched. The optional `sccache` wrapper could not start (`Operation not permitted`) even through the supported native execution controls. Verification therefore uses `RUSTC_WRAPPER=` for the normal Rust compiler, preserving Cargo build artifacts and caches; persistent configuration is unchanged.

The first all-feature library run began before the new Zoo bake finished. Discovery consequently omitted its stale preceding package, and `test_the_default_level_is_the_shipped_demo` failed while 2,021 other library cases passed. The completed Zoo package is now current. The final `cargo test --workspace` run passes **all 2,022 library tests**, 23 ignored, the game/compiler targets and the non-Unicode command-line target. Its subsequent three `list_levels` integration cases fail on inherited optional drop-ins:

* `levels/home_showcase.placesmap`: preceding Home cabinet dependency (recorded 97,224 bytes; current 59,144).
* `levels/geometry_intentional.placesmap`: preceding Pool tile dependency (recorded 1,390,031 bytes; current 30,873).
* `levels/level0_pit.placesmap`: preceding Pool rail dependency (recorded 65,196 bytes; current 31,800).

These optional local packages are absent from clean CI checkouts. All three quiet-discovery assertions remain unchanged; warnings remain honest. No historical package is deleted, moved out of discovery, disguised or rebaked for this Winter task. The unaffected Mach host-port target and workspace documentation tests are completed separately. Original and final local test logs are preserved in the review archive; [final workspace output](checks/winter-workspace-final.log) explicitly records this local compatibility limit. The exact pushed revision's clean-checkout CI must pass the complete workflow, including these integration cases and archive verification, before handoff is considered publication-complete.

## Technical fixes and scope

Connected snow coats initially had reversed rim ordering; the actual outward closed winding was corrected and original topology assertions pass. The timber periodic seam initially crossed a high-contrast board joint; a measured adjacent-column offset of 370 makes the production sheet periodic without blurring the interior detail. Both failures are retained in local validation evidence. The first native candidate showed doubled raised-deck offsets on new opening frames; final floor-relative anchors align with the actual sill/doorway, covered by a targeted regression. The old overlaid hood is replaced by genuine braced construction while its exporter, topology and Zoo display remain validated.

All pre-existing catalog entries and all non-Winter environment/entity files are byte-identical to the start copy. Original Winter rooms, floor regions/patches, ramp, sky/moon/weather, door controls/geometry, stairs and guardrail geometry remain exact; material changes are Winter-local. Tree/rock base identity and original trunk/mineral collider tests pass against the completed Outdoors kit. New stone bodies and timber posts have deliberately narrow explicit colliders outside the clear walking lines. All old Zoo exhibits and every non-prop Zoo field are exact; seven new small exhibits occupy previously unused apron space.

## Resource cost and remaining visual limits

The whole static pack grows from 50,984 to **52,090 triangles** (+1,106), and decoded RGBA from 39,908 to **42,084 KiB** (+2,176). The new family models are 24–252 triangles; drift families drop 72 → 60. Evergreen variants change 1,238/1,442 → 1,284/1,380 while replacing their obsolete bases with the exact current bare model. No budget is relaxed.

Winter package grows **47,509,519 → 50,056,698 bytes** (+5.36%); Zoo grows 87,015,388 → 87,454,894 (+0.51%). High static surface vertices drop 3,655 → 2,743 by replacing primitive frame dressing, while loaded prop vertices rise 328,785 → 341,331. High total loaded vertices are 335,800 → **347,434** (+3.46%) and VBO bytes 21,491,200 → 22,235,776. Low total loaded vertices rise 243,500 → **282,043** (+15.83%). Full Winter lightmap pages grow four → five. Unique loaded models rise 52 → 53; prop draws 347 → 354.

Across matched short capture cameras, High median visible draws are 82 → 74 and material changes 50 → 51; Low draws 86.5 → 86.5 and material changes 61 → 65.5. Short loop medians are High 1.959 → 1.662 ms and Low 1.611 → 2.626 ms. Native startup timing and concurrent compilation/testing differ; these are descriptive measurements, **not a controlled speed comparison or an FPS improvement claim**. The first-frame overview has no measured CSV rows and is excluded from timing aggregates. Per-camera counts, source hashes, compile phases and package costs are in [performance.json](performance.json).

The full distant village/church, softer illustrated evergreen contours, less regular roof draping and brighter isolated ice vignette remain visual gaps. The elevated native overview still reveals arena-like containment. Actual severe weather remains nearly opaque outside; it preserves the existing blizzard rather than claiming an art comparison through whiteout. Existing quiet-discovery warnings are a final integration compatibility item. The [archive](../../../debug-maps/winter-style-20261007/README.md), earlier queue evidence and reusable target artifacts remain available; inherited sleep protection stays alive and is transferred to the parent.
