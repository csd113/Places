# Focused engine cleanup before 0.9

October 12, 2026 · `arcade-env` · version remains 0.8.0.

The focused cleanup reduces duplicated maintenance work and restores complete Full
lighting for the current Model Zoo. Artwork, map sources, controls and gameplay
stay intact. Engine builds, tests, packaging and startup still support zero maps.

## Changes and their value

- Frame and HDR reflection captures now share aligned GPU staging, mapping and
  row compaction in `render/wgpu/readback.rs`. This removes two implementations of
  the same mechanism. Top-down RGBA ordering, half-float probe bytes and native
  rendered pixels are preserved; malformed rows now return an error.
- Exact prepared-prop sharing and compressed archive sizing moved into
  `compiler/storage.rs`, with their existing regression. Its implementation is
  unchanged after visibility and formatting normalization. Compilation,
  validation and publication remain with the compiler.
- Two unused quality predicates and their self-mirroring assertions were removed.
  Real framebuffer sizing and surface-response tests remain. One transport test
  now computes its independent serial reference once per configuration: 16
  reference solves replace 32, retaining all 32 worker comparisons. No elapsed
  speedup is claimed from that consolidation.
- Living guides now use the actual frozen-capture paths, describe package-based
  startup, include the dedicated Office, and document current atlas planning.
  Historical reports, curated images, unique untracked audits and original
  references remain intact. No developer tool was proven obsolete.

## Model Zoo correctness

The old twelve-page plan genuinely cannot hold current content. A manual
prepare-only witness measured **13,773,305 reserved texels** including endpoint
samples and gutters. Thirteen 1024-square pages hold only 13,631,488 texels.
The exact complete plan uses **fourteen pages**, the mathematical minimum,
with 49,773 charts. A fourteen-page run produced the same layout as the larger
20-page diagnostic witness. Planning took about 2.4 seconds on this host;
this excludes the expensive transport solve. The temporary witness was removed
before the source gates. The public dump inspector and its existing boundary
tests also accept the final page and reject the first out-of-range page.

Full's cap is therefore fourteen pages: **224 MiB for one resident group**.
Each 1024² page stores two RGBA16F planes (irradiance and signed direction),
16 bytes per texel per group: 16 MiB per page. Base and switchable groups share
the chart layout; their exact cost bounds total pages to 14/10/6/5/4 for one
through five groups. This is a per-variant limit, not a limit on installed maps.
Zoo's compact lightmap member occupies 136,489,336 bytes before ZIP compression
and 83,911,090 bytes inside ZIP, separate from the decoded 224 MiB GPU allocation.
Two-group maps still permit ten pages; lower profiles retain eight. The 320 MiB
plus 64 KiB typed atlas allowance, aggregate archive limit, group/layer/shape
checks, density, inclusive endpoints, gutters and directional/HDR encoding stay
intact. Existing architecture-hole reuse still starts at page eleven, preserving
previously successful placements.

A separate tiny regression exposed false overflow caused by adjacent disjoint
model-page holes. They are now joined only before final allocation failure.
Overlapping architecture holes retain their existing policy. The regression uses
four rectangles, two orientations and padded/unpadded variants, with no assets,
atlas pixels or transport. It protects prior coordinates, density, gutters,
disjointness, deterministic recovery and sticky genuine overflow.

## Deliberately retained

Static and Full lighting, legacy Medium readers and preparation fixtures,
independent texture/filter controls, material response, HDR, ultraviolet pigment,
probes, direct/indirect illumination, entity animation, weather and atmosphere
remain supported. Current Office, Arcade, Birthday and Spooner content is retained.

Catalogue and executable identities conservatively invalidate prepared packages;
existing tests explicitly enforce that contract. Safe selective catalogue reuse
needs a verified semantic dependency closure. This cleanup retains the contract
and refreshes optional packages once at stable source. Manual map preparation
remains outside default builds, Rust tests, packaging and hosted CI.

## Verification

On Apple M2 Pro / Metal:

| Check | Result |
| --- | --- |
| Formatting and patch whitespace | Passed |
| Strict workspace/all-target/all-feature Clippy, debug and release | Passed |
| `cargo test --workspace` | 2,232 library + 8 integration tests passed; library execution 31.31 s |
| Python source suite | 348 passed, 4 explicit native/export/weather checks skipped (352 total) |
| Asset validator / prop check | 555 assets, 336 placeable; passed, validator has zero warnings |
| Texture generator check | Passed; 96 existing manual-source inventory warnings retained |
| Exact HDR cube upload/readback on GPU | Every face and mip passed |
| Fixed native baseline comparison | Four captures pixel-identical |

The 18 existing opt-in Rust developer/GPU campaigns remain opt-in; no new ignore
markers were introduced. The five lighting-dump tests were rerun successfully
after the final diagnostic boundary update. Native evidence covers the touched
paths; unrelated full-map visual and performance campaigns were not repeated.

The native comparison uses an isolated installation, a 641×360 logical / 1282×720
Metal drawable, and actual BGRA surface readback with padded rows. It covers the
zero-map menu plus Arcade Static, Full and Full with Low textures. Arcade uses
`(x,z,yaw)=(7.3,22.5,322)`, pitch −28°, ready frame 30 and a fixed 1/60-second
simulation step. An arbitrarily named external package is discovered normally.
These are appearance and readback checks, with no new frame-rate claim.

Model Zoo's normal source/archive validation passed after the complete bake.
Both Static and Full loaded through the existing fixed-camera manifest. The Full
native trace records fourteen resident pages / 28 layers / 234,881,024 bytes,
with zero missing world textures. Visual inspection found the expected Full
surface detail and contact lighting; Static retains its cheaper appearance.
The package is 93,752,262 bytes and the compiler reported zero build warnings.
Existing console notices for reviewed large cloth/wrapping textures remain.

## Optional packages and interruption recovery

Eight completed packages passed current source/catalogue/compiler and archive
validation again after the accidental user-run `cargo clean`. The restored
release compiler is byte-identical to the original
(`b77f987ebbb76f104f80cdb18581acc8516066bf6ee7048ef0cd25c80bb43380`).
No completed map needed another bake. The interrupted Arcade process later
faulted in macOS IOSurface/Metal initialization; the native user process also
exited. Healthy processes were not killed. The clean caused an operational
interruption, but its exact causal relationship to that OS fault is unproven.
Arcade's restored-tool retry passed normal Full probe capture, atomic
publication and current-input/archive validation in 548.31 seconds with zero
compiler build warnings; the fault did not recur. Office also passed normal
Full capture and publication in 283.68 seconds, with zero compiler build warnings.

The clean removed temporary target logs and native captures after their completed
comparisons. Their observed results are recorded above; original audit inputs and
all existing packages survived. The matching OS crash report was inspected
read-only and remains untouched.

All ten curated packages passed source/catalogue/tool freshness, bounded archive
validation and the complete Static/Full guard. Final input identities and whole
package hashes were checked again; the native player's headless discovery lists
all ten normally. The packages total 399,006,830 bytes (380.52 MiB)
and stay optional, ignored local outputs under `assets/levels/`.

The final Model Zoo package SHA-256 is
`39193785aab61038d34d9e85b5bbd6e1ad8b38c28d8508915baff385d7e10f6b`.
For the complete set, SHA-256 of sorted newline-separated `ID SHA256` rows
(with no trailing newline) is
`90e4a10b844793522db1df71c28da657fc1a6d865928c547b9f5d2b5cd93898e`.
Existing art-review console notices are retained; all ten compiler build reports
had zero warnings. These checks do not claim ten-map native gameplay acceptance.

## Handoff to the separately queued packing pass

| Map | Full pages / groups | GPU atlas MiB | Reserved texels | Area minimum pages |
| --- | ---: | ---: | ---: | ---: |
| `places_demo` | 10 / 2 | 320 | 8,410,058 | 9 |
| `model_zoo` | 14 / 1 | 224 | 13,773,305 | 14 |
| `pool_showcase` | 2 / 1 | 32 | 1,314,265 | 2 |
| `home_showcase` | 2 / 1 | 32 | 515,433 | 1 |
| `lantern_hollow` | 11 / 1 | 176 | 10,167,576 | 10 |
| `winter` | 7 / 1 | 112 | 5,703,007 | 6 |
| `beach_demo` | 8 / 1 | 128 | 7,177,003 | 7 |
| `frutiger_aero_demo` | 4 / 1 | 64 | 3,434,645 | 4 |
| `retro_arcade_demo` | 4 / 5 | 320 | 3,179,877 | 4 |
| `office_demo` | 4 / 3 | 192 | 2,948,649 | 3 |

GPU values describe only the resident Full atlas. Area minima assume the saved
rectangles and gutters and are necessary rather than sufficient packing bounds.

Zoo allocates 14,680,064 texels. Its 11,926,139 data texels use 81.24% of
that allocation; gutters add 1,847,166 (12.58%); remaining holes account for
906,759 (6.18%). The data rectangles include triangle bounding-rectangle space.
With these fixed rectangles and gutters, even perfect packing cannot reach
thirteen pages. Reducing holes within fourteen pages alone does not reduce GPU
allocation. Any change to chart domains or gutters needs independent quality proof.

Current validated controls already suggest different opportunities: Home reserves
515,433 texels but allocates two pages (area lower bound one), while Demo reserves
8,410,058 over ten pages (area lower bound nine). Those lower bounds are necessary,
not sufficient: unrotated rectangle dimensions and fragmentation can prevent them.
One saved Home page would avoid 16 MiB of atlas allocation; one Demo page avoids
32 MiB because its two illumination groups share the layout. No saving is promised.
The existing separation of architecture MAXRECTS pages and model guillotine pages,
with cross-family holes searched only near the cap, is a concrete investigation
entry point. The current correction preserves existing successful placements.

First inspect existing packages rather than rebaking. `places-compile inspect
assets/levels/model_zoo.placesmap --json` names the Full variant's
`entries.lightmaps_meta` ZIP member. Its JSON stores `page_edge`, `page_count`,
`padding`, `stats.texels`, `charts` and `switchable_lights`. Sum
`(width + 2p) × (height + 2p)` for each saved chart, with `p=1` for props and
`p=2` for architecture, and compare with `page_count × page_edge²`.
`src/lighting/lightmap/atlas.rs` owns packing; `plan.rs` stamps UVs inline;
`LightmapConfig` in `mod.rs` owns density/endpoints/gutters. Preserve the patch's
orientation and both geometry/prop lightmap coordinates. Group-aware record
bounds live in `src/package/lightmaps.rs`; GPU allocation in
`src/render/wgpu/lightmap.rs`. The minimum page count is per prepared variant,
not a shared limit across installed maps.

For a deliberate fresh witness use `PLACES_VERBOSE=1 places-compile build
SOURCE --out NEW_PACKAGE --asset-root assets --workers 8 --force`.
Use `PLACES_LIGHTING_DUMP_DIR=NEW_DIRECTORY` for fresh bake-stage dumps, or the
existing `export-lighting` subcommand for saved package output. The tools
`tools/bench/inspect_lighting_dump.py`, `compare_packages.py` and the native
camera manifests under `tests/fixtures/native/` provide diagnostics/comparison
entry points. Final shader/transport/sample quality must remain unchanged.
The completed source regression gates continue to require no production bakes.

## Future candidates, without extending this cleanup

Zoo's single eight-worker bake took 1,082.87 seconds. Direct visibility used
670.12 seconds, indirect light 348.58 seconds, filtering 35.00 seconds and texel
fill 13.62 seconds. Planning took about two seconds; ZIP publication 1.81 seconds.
The solver recorded 13.78 billion direct rays and 1.53 billion bounce rays.
Peak resident memory was 3.09 GB; macOS also reported a distinct 6.15 GB peak
memory footprint. Those metrics include compiler/capture work and are not
isolated GPU counters. No CPU-heavy build/test/native campaign overlapped this
bake; small read-only reviews and documentation edits occurred.

A focused study of direct visibility is the useful performance candidate.
A hypothetical 20% gain there would save about 2.2 minutes on this Zoo, but
neither that gain nor a safe acceleration has been demonstrated. Verified
selective catalogue identity could avoid minutes of preparation for unaffected
maps during catalogue growth; its correctness proof remains future work.
Current findings do not justify a renderer rewrite or speculative abstractions.

## Notes for the separately queued tasks

These requests were recorded for later owners; none extends this cleanup.

- **Fixture tint and asset warnings:** Start with `render/common/fixtures.rs`,
  `light_transport.rs`, `render/wgpu/material.rs`, `world.wgsl` and emission
  animation. The current contract makes luminous faces neutral and assigns light
  colour to baked illumination. Apply the future authored tint/brightness/flicker
  only to tube/diffuser faces, independently of their housing. Zoo's exact notices
  concern eight reviewed 1024² birthday cloth sheets and six 512² gift sheets,
  against the ordinary 256² texture preference in `src/props.rs`; these are
  art-review notices, not evidence of broken physical bounds. Inspect other exact
  messages before altering metadata or deduplicating; no warning was suppressed.
- **Medium textures:** `quality.rs` and `render/wgpu/texture.rs` retain ordinary
  256² props at High and Medium (Low caps at 128²); 1024² surfaces cap at 512²
  for Medium and 256² for Low. A future midpoint needs matched three-profile
  native evidence, preserving High artwork and independent Static/Full lighting.
- **Blizzard distance:** Weather/fog/sky/emission/transparency were untouched.
  Start with `render/common/atmosphere.rs`, `render/wgpu/world.wgsl`, sky/effects
  and dynamic/character environment bindings. All strengths should converge to
  full distant obscuration, with different onset distance, retaining interiors,
  nearby lights and controls. No particle-count or map-name workaround was added.
- **Arcade bathroom:** The connected stalls/toilets, sink/mirror, tiled floor and
  ceiling vent belong to the separately queued content pass, reusing the old
  bowling kit with collision and native acceptance.
- **Compiler progress:** Existing inputs are `[compiler-progress] preparing/
  capturing`, named `BuildReport.phases` / `[compiler-timing]`, and direct/diffuse
  solve counts in `transport.rs` and `transport/batch.rs`. Verbose lines currently
  precede JSON on stdout with `--json`; the measurement wrapper extracted the
  final object. Preserve machine-readable stdout, quiet runs and trustworthy
  phase denominators in the future terminal progress display.
- **Docs and all Python tools:** No script was proven dead here. Consumer checks
  should retain the vending preview (`tools/props/inspect_arcade_vending.py`,
  referenced by the asset specification), Winter string audit (replayed by its
  final report), Metal trace inspector and historical baseline tools pending
  provenance review. Living frozen-image paths were corrected; dated reports
  need contextual review. Preserve finalized user audits/images and originals.

The requested order is graphics correctness, Arcade bathroom, packing efficiency,
terminal progress, then the broader docs/Python audit before 0.9 integration.
