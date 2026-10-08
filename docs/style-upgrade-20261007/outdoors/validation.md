# Outdoors validation and cost record — October 7–8, 2026

The exact publication SHA, remote verification and CI run are recorded in the task handoff; the [standalone reconstruction commit history](https://github.com/csd113/Places/commits/Winter-expansion/docs/style-upgrade-20261007/outdoors) and [Rust verification workflow](https://github.com/csd113/Places/actions/workflows/rust.yml?query=branch%3AWinter-expansion) provide browseable publication links. The final source is built on the completed Home revision `7ecacdfbe9f9c11df2c00932248a73634e78904e`; Pool and Home artwork, sources, journals and earlier passing CI remain intact.

## Asset and scope checks

| Check | Result |
| --- | --- |
| Catalog | PASS: 340 assets, 197 placeables, five themes, zero warnings. |
| Focused Outdoors route, Showcase and Zoo Python tests | PASS: 21 tests, including stable order after a later generator appends decals. |
| Full asset integrity audit | PASS: 197 models, 199 embedded images, 287 standalone PNGs, 37 maps, 65 materials, zero errors. |
| Prop `--check` | PASS: 50,984 triangles, 39,908 KiB decoded RGBA, below the existing 64 MiB pack budget. No budget raised. |
| Texture `--check` | PASS: 78 sheets; 69 existing soft-budget/manifest warnings remain. No corrupt or oversized PNG. |
| Outdoors master/native derivatives | PASS: all 26 pairs exactly match the fitted Lanczos/periodic authoring contract. |
| Ground seams | PASS: all three sheets pass raw and smoothed periodic seam checks. |
| New/rebuilt geometry | Closed outward shells, finite positions/UVs, correct catalog bounds, no degenerate/nonmanifold/inconsistent edges. Rebuilt trees: 768/720/708 triangles. Static seating fire: 744. |
| Generator checks | Fixture and route outputs current; no unrelated source or architecture changes. |
| Scope | All demo fields except owned static night props exactly equal, including ordering. Non-night props, entity placements and interaction collections exact. Non-Outdoors catalog entries, artwork and source maps byte-identical. Zoo adds eight Outdoors apron exhibits and updates only the slender-tree envelope among older exhibits. |
| Rust engine scope | No production renderer, solver, serializer, global sky, lighting setting, entity behavior or budget change. One Outdoors rock test reference is corrected to native RGBA16F precision. |

## Reviewable failures and corrections

The first complete local library run after asset iteration finished with 2,020 passing tests, 23 ignored and two failures. Earlier showcase failure evidence is retained separately. The Night generator's strip/append behavior moved its decals after the completed Pool/Home entries; replacement now preserves the original insertion position. The existing showcase assertion stays intact, and all non-prop structural arrays are exact against the start copy.

The seating fire was initially loaded from the existing animated campfire asset, making a seventeenth character despite having no authored actions. The new `campfire_static` builds the same stone/log/flame meshes and removes animation clips. The sixteen-owner assertion is unchanged and passes; the original animated model remains byte-identical.

The reconstructed boulder exposed a mismatch in the package round-trip test's reference. Direct native uploads already convert coefficients to RGBA16F, while the test sampled the original FP32 solve. Grazing-face cancellation amplified legitimate coefficient rounding (`0.009072715` versus `0.006941355` in the first diagnostic). The scoped Outdoors fixture now independently computes binary16 rounding using powers of two and ties-to-even before shader sampling. Its original `0.002` relative / `0.0001` absolute tolerance and exact normals/albedo/UV/index/submesh/chart assertions remain. New assertions compare every shader texel and the encoded half-float upload planes exactly. Production lighting and package conversion are untouched. Intermediate geometric diagnostics and failed campaigns are retained in the local archive.

The boulder itself now has grounded homothetic rings and planar shoulders, with a wide stable foot rather than an undercut lower ring. This is an actual low-poly construction refinement; test tolerance is not used to hide a model or change a limit.

## Known compatibility limitations

The inherited optional untracked drop-in packages under `levels/` remain in place, including `geometry_intentional.placesmap` and `level0_pit.placesmap`. They are stale against earlier Pool/Home and current Outdoors dependencies. Quiet-discovery integration tests can therefore report stale-dependency warnings locally. They are absent from clean CI checkouts. No archive is deleted or unrelated historical map rebaked; assertions and discovery warnings remain intact. This preserves the [Pool](../pool/validation.md) and [Home](../home/validation.md) evidence.

Winter's unchanged `tree_snow_01`, `tree_snow_02`, `boulder_snow` and `rock_face_snow` still contain the preceding canonical bare kit. The original canonical comparisons currently disagree with rebuilt bare geometry/textures; the bounded [comparison evidence](winter-deferred-canonical.json) records all six original predicates. Snow railings and the fence post still pass identity. A direct Python canonical test was stopped because its huge failing mesh diff was costly; the assertion was not edited. The original bare/snow artwork and packages remain in the before copy and starting commit. `tree_03_snow_base.png` is the exact preceding evergreen image, preserving genuine embedded/source PNG identity. The later authorized Winter pass owns reconciliation; Outdoors does not redesign Winter.

The Full-lightmap night scene remains nearly black before and after. Native Low views demonstrate asset silhouettes/materials, but do not establish a solved High atmosphere. The reference neighbourhood's full street layout, diffuse blue ambient night and small `ROAD CLOSED` gate plate remain gaps. No fabricated exposure, custom fill scene or image correction is used.

## Final native evidence and publication gate

The normal release build, formatting, strict debug and release Clippy all pass. Final focused Rust verification passes **20 static-model tests, one ignored**, including the repaired boulder round-trip and actual Demo/Zoo package budget tests; the unchanged sixteen-character ownership test also passes. The earlier complete `cargo test --workspace` run is retained with its two failures above. Its repaired cases are green; a second full local campaign was deliberately avoided under the authorized proportionate-check instruction. Publication requires the exact pushed revision's clean-checkout Rust workflow to pass the complete workspace and archive gates, and its final result is recorded in the task handoff. See [final static-model tests](checks/static-model-final-tests.log), [ownership test](checks/static-fire-ownership-test.log), [debug Clippy](checks/clippy-final-debug.log), [release Clippy](checks/clippy-final-release.log) and [normal release build](checks/release-build-final.log).

All five affected bundled packages pass `verify --require-current` and ZIP integrity checks. The Demo and Zoo were normally compiled after the last fitted facade finish; Hollow, Winter and Movement were refreshed once for actual shared Outdoors dependencies. Their source JSON and non-Outdoors art are unchanged. Exact identities, variant counts and durations are in [package verification](packages-final.json) and [compile costs](costs.json).

| Package | Final bytes | Final compile seconds | Warnings |
| --- | ---: | ---: | --- |
| Demo | 74,520,351 | 362.33 | Four room-probe warnings: rooms 7/10 at 2.030 m spacing in Medium and Full. |
| Zoo | 87,015,388 | 407.11 | None. |
| Hollow | 110,288,475 | 649.83 | None. |
| Winter | 47,509,519 | 269.26 | None. |
| Movement | 42,370,588 | 96.55 | None. |

The compiler also reports small sub-texel quads using vertex lighting: Demo 15, Zoo 32, Hollow 64 and Winter 45. Demo's room-probe warnings are reported honestly; this pass does not change unrelated room geometry or inflate probe/page limits. The final Demo geometry check has **zero errors**, one existing Office wall `m47` overlap sliver of 0.00029 m², and the original intent annotations suppress ten missing-wall and three room-leak reports. The porch dressing overlap is removed. See [geometry](geometry-final.json), [scope proof](scope-final.json) and [archive check](checks/archives-final.log).

The CPU fallback drops from **99,974 to 64,489 vertices** (212,376 to 159,216 indices), with its existing 100,000-vertex limit unchanged. The Demo package drops from 100,086,061 to 74,520,351 bytes. Across the ten actual matched captures, High loaded vertices fall from 517,285 to 393,481 and vertex buffers from 33,106,240 to 25,182,784 bytes; Low falls from 375,862 to 305,745 vertices. Full still uses eight lightmap pages. The added construction costs draw/material work: High median draws 113 → 123 and material changes 67 → 87.5; Low draws 115 → 122.5 and material changes 67 → 87. Short native loop medians rise High 2.398 → 2.7795 ms and Low 2.497 → 2.9425 ms. These startup captures and differently loaded compilation runs are not a controlled performance experiment; no FPS improvement is claimed. Exact camera results are in [native performance](performance.json) and [CPU mesh evidence](cpu-budget.log).

All forty actual before/after High/Low PNGs, raw logs, CSVs and matching playable review assets/binaries live outside `target` in the [retained archive](../../../debug-maps/outdoors-style-20261007/README.md). Earlier before/candidate/visual/order/static iterations remain available. The detached inherited sleep assertion is verified and transferred separately; persistent power/security settings stay unchanged.
