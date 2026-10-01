# Offline probe baker audit — solver revision 8

This audit owns compiler preparation, transport, probe placement, baked data and
PLPF serialization. It does not change runtime spatial sampling, object lighting,
shaders, exposure, quality transitions, gameplay, source maps or visual assets.
The work is isolated on `agent/probe-baker-audit` in `Places-probe-baker`, from
`ba16887`; no runtime-agent work is included.

## Confirmed defects and fixes

| Defect | Reproduction and resulting behavior |
| --- | --- |
| Escaping probe rays discarded the authored sky | An empty scene with sky `[2, 0.125, 0.001]` returned zero. Probes now gather that radiance with the existing sphere-mean convention and preserve its HDR/dim channels. |
| Every diffuse order, including switchable layers, injected sky again | A black surface became brighter with additional orders despite reflecting nothing. Sky enters only the base first order; subsequent orders transport the preceding order. |
| Surface-cache identity did not isolate an architectural triangle spanning a divider | A sealed dark compartment received about `0.010410` per channel from the bright side. Cache interpolation and nearest fallback now require visibility from the representative receiver to the hit. The dark side is exactly zero in the regression. |
| Chart denoising moved light through a divider within one chart | A 3×3 neighbor could sit across a sealed wall. Filtering now checks visibility between safe receiver origins. |
| Authored recovery fill used fixture falloff without fixture visibility | A blocked sample received permanent fill from an unseen fixture. The existing support curve is now weighted by the visible source fraction; the authored fill formula and constants are retained. |
| Grid dimensions used the preferred spacing even after the cell size increased | Large worlds retained unnecessary vertical/X cells outside the useful bounds. Choose the common cell size first, recompute all dimensions, and center the lattice in its bounds. |
| Room labeling used an XZ wall check and forgiving height lookup | Probes above ceilings, below floors, inside raised regions and inside props could be marked usable; air above a half wall could be excluded. Compiler labels now require actual air heights, 3D wall bounds and opaque-surface clearance. |
| Codec and numeric paths repaired or concealed invalid energy | The writer accepted nonfinite values; some reads normalized/clamped records, and source-strength overflow could disappear as zero. Validate inputs, accumulated passes, final values and both codec directions; physical-validation failures abort compilation. |
| Per-probe random rays added unrelated neighbor noise | Independent 24-ray patterns had directional bias even in constant sky. Use 64 shared stratified antipodal directions; a constant environment has zero net moment and identical neighboring probes. |
| Alpha-tested prop cards were entirely opaque in transport | Native rays at the outdoor actor sample hit grass cards only 3–7 cm above the floor, blacking out static ground while the 1 m air probe saw fixtures. Prop MASK/cutout primitives now transmit using the architecture cutout convention. Opaque and static BLEND-fallback primitives remain solid. |

## Pipeline and calculation audit

Authored rooms, fixtures, surfaces and transformed props feed geometry preparation.
Static geometry, lights and probes operate in world-space metres, +Y up. Model
transforms are applied by preparation before triangles enter transport; neither
the baker nor the PLPF reader reapplies them. Nonfinite positions, albedo, emitter
parameters, sky/water parameters and target arrays fail validation. Degenerate
triangles are rejected by triangle construction; zero-length emitter-to-receiver
directions use the existing finite fallback. Receiver-target length mismatches
fail instead of disabling fill.

Direct light samples the authored point/rectangle/line source, traces the same
two-sided static BVH and applies color, intensity, height factor, calibrated
falloff and visible fraction once. Ceiling-fixture falloff intentionally uses
horizontal reach; this is the game's authored pool convention, not inverse-square
radiometry. There is no spotlight-cone source type. Fixture/material glow is not
an independent diffuse emitter without an authored light. Switchable sources
remain excluded from permanent probes and retain separate static atlas layers.

Watertight ray/triangle tests, bounded BVH traversal, endpoint allowance and
receiver offsets remain in use. Existing edge/T-junction, thin wall, acute-corner,
long-distance, area source and curved-surface tests exercise those paths.
Opaque blockers count from either side. Glass/water triangles marked transmissive
are skipped by shadow and nearest-hit rays; water depth attenuates/tints receiver
light with the existing per-channel extinction. This is a transparent/opaque
approximation, not refractive transport or partial glass absorption.
Prop cutout cards use the same transmission approximation as architecture cutout
grilles: transparent card regions cannot turn the entire card into a blocker.
The solve does not trace individual alpha texels, so it also omits blade-level
cutout shadows. Static prop BLEND remains the renderer's opaque fallback; routed
dynamic transparency/fade state belongs to runtime.

Diffuse surface orders gather outgoing irradiance times hit-surface albedo;
receiver albedo is applied later by shading, not twice by the solve. Each order
reads only the previous order, with the existing bounded count and gain of 1.
Sky enters once. The cache stores the exact cosine integral at the geometric
normal, not the nonlinear reconstruction of compressed moments. The probe then
gathers these physical solved receivers before chart filtering/artistic fill,
plus direct fixtures and escaping sky. It applies the same visibility-supported
continuous authored recovery response as surfaces. No room mean, entity exception
or new ambient/brightness constant enters this calculation.

Transport indices are interleaved across at most 12 scoped workers to distribute
spatially clustered expensive work. Visibility-supported chart fill also uses
that pool; each index retains its serial arithmetic order and shares the immutable
scene. Buffers remain bounded by the existing receiver/probe budgets. Regression
tests compare filled atlas values and probes bit-for-bit at 1 and 12 workers.
The nearest cache fallback also avoids visibility traces for candidates that
cannot improve its already-visible nearest result, preserving original tie order.
Variants with no reflection positions avoid a redundant capture-preparation
bake when the logical and resolved capture reflection metadata agree. Custom
asset roots with different reflection state retain the capture path.

Arithmetic is linear in the engine's calibrated light units, not normalized RGB.
There is no probe gamma conversion, exposure, tone map or early HDR clamp.
The existing material albedo convention uses raw display-authored values loaded
as `Rgba8Unorm`; it is not a full sRGB-to-radiometric-linear material pipeline.
A bake-only gamma change would disagree with current renderer/material semantics
and is deliberately outside this fix. Static atlases quantize to RGBA16F;
PLPF coefficients retain their original f32 bits.

## Placement behavior

Preferred spacing is 1.5 m; all axes share a cell edge, enlarged as needed for
the 64-cell axis cap (at most 262144 probes). Dimensions are recomputed at that
edge and bounds centered, so a large horizontal extent does not leave vertical
centers outside a thin air volume. Sub-cell input bounds are padded to one
preferred cell; a floor-only scene consequently has air samples above the floor.

Final compiler validity requires the room footprint, the actual floor plus
walkable region/stair/ramp offset, the ceiling, 3D wall exclusion and at least
5 cm opaque-surface clearance. Doorways without a wall solid and air above a
half wall remain eligible. Six axis rays conservatively reject a closed,
outward-wound solid when every nearest hit is an exiting backface; placed prop
triangles participate. Open/non-manifold or incorrectly wound models cannot be
classified perfectly by that test. Curved surfaces use their actual triangles.
The compiler reports every room without a usable sample.

The bounded uniform lattice can still miss narrow passages, and a large world
can reduce vertical resolution in tall spaces. It does not adaptively insert
extra probes. Uncharted prop surfaces block rays but have no solved receiver
radiance to contribute; a zero cache hit can be legitimate darkness or missing
surface coverage. Neither case is replaced with invented ambient light.

## Runtime contract

The complete binary contract is in
[PACKAGE_FORMAT.md §6.1](PACKAGE_FORMAT.md#61-irradiance-field-for-moving-objects).
PLPF remains v2: 38-byte header, 36-byte samples, little-endian, no padding.
Positions are implicit centers `origin + (index + 0.5) * cell_m`, X fastest,
then Y then Z. The lower boundary is not the first center.

Samples contain nonnegative finite RGB means `I`, signed world XYZ aggregate
first moment `g`, reserved axis `[0.5, 0.5]`, and signed room label. In valid
records `|g| <= sum(I)` within rounding tolerance. Finite HDR values above one
are allowed; there is no universal display-space minimum or upper brightness
cap. Values are calibrated white-surface light, not physical lux. For normal
`n`, the shared compact reconstruction is:

```text
k = sum(I)
light_c(n) = max(0, I_c + I_c/k * (2*max(0, dot(g,n)) - length(g)))
```

At effectively zero `k`, return `I`. This is a compact directional fit, not
full SH and not exact for arbitrary multi-direction illumination. Interpolate
means and signed moments before reconstruction; never normalize stored `g`.
Label `-1` means unavailable. A valid zero sample means deliberate darkness.
Missing samples return `None` for runtime fallback. Invalid records return an
error, never a repaired bright/dark value. Solver revision 8 and fingerprinted
probe parameters invalidate old bakes; package version and runtime APIs are
unchanged. Existing packages must be rebuilt.

## Deterministic diagnostics and inspection

Diagnostics are opt-in and do not add normal-build logs:

```sh
PLACES_PROBE_DUMP_DIR="$PWD/target/probe-audit/demo" \
  target/release/places-compile build assets/levels/places_demo.json \
  --asset-root "$PWD/assets" --workers 12 --force --out target/probe-audit/demo.placesmap
python3 tools/bench/probe_lighting_report.py target/probe-audit/demo.placesmap \
  --locations target/probe-audit/locations.json --variant full
```

Use a separate dump directory for each source/build. `probes-bN.json` records
world positions, target rooms, direct/indirect RGB and moments, combined values,
ray counts, surface/escaping/zero-radiance hits and contributing emitter indices.
`labels-medium.json`/`labels-full.json` supply final compiler validity.
`surfaces-bN.json` records at most 65536 deterministic physical receiver samples
and a continuous-fill estimate; these are prefilter diagnostics, not the final
atlas. Capture preparation may repeat the solve and overwrite the same dump.
JSON files are published through exclusive temporary files and rename.

The report independently reads the **packaged PLPF and packaged RGBA16F atlas**,
checks entry hashes/sizes and reconstructs unit-albedo static illumination.
It samples at most 65536 distributed atlas positions plus the closest projected
texel at each requested location and each selected probe center (at most 64
requested locations). It chooses the nearest valid
same-room probe and upward static sample, records their actual positions/distances and
flags 100× mismatches or adjacent same-room jumps. These are investigation
candidates, not automatic errors: normals, heights, occlusion and sparsity
matter. This is not the runtime interpolated entity result or a visual claim.
Both the surface beneath the requested point and the surface beneath the selected
probe are retained, so a sparse lattice does not conceal an offset or shadow.

## Representative map measurements

Measurements use the unchanged authored `places_demo` source on both sides.
Its night sky ambient is **zero**. Locations cover office room 0 at
`[4.5,1,3.5]`, moderate room 1 `[14,1,3.5]`, corridor room 5 `[43,0.1,13]`,
dark home hallway room 7 `[61,0.1,-8]`, living area room 6 `[62.5,0.1,8]`,
pool room 3 `[5.6,-0.5,8.4]`, stair room 2 `[21.5,-0.5,3.5]`, home stairs
room 6 `[56.6,0.5,13.6]`, outdoor actor room 12 `[1.4,1,-18]`, doorway
approaches room 6 `[58,0.1,13]` and room 5 `[52,0.1,13]`, and the outdoor
night-guard room 13 `[13.5,1,-95.7]`. Pool/stairs/outdoor locations include
authored model placements.

The table reports raw mean RGB and upward reconstructed probe luminance,
compared with the final packaged unit-albedo upward static atlas beneath the
selected probe. These are calibrated linear working-light values, before exposure.
The centered lattice changes nearest probe positions; these are representative
location comparisons, not identical sample-center comparisons.

| Location | Mean RGB before → after | Upward probe L before → after | Static L beneath final probe |
| --- | --- | --- | --- |
| bright office | `[1.079, 0.995, 0.777]` → `[1.022, 0.946, 0.774]` | 1.079 → 1.132 | 0.977 |
| moderate office | `[0.841, 0.783, 0.648]` → `[0.849, 0.788, 0.652]` | 0.806 → 0.912 | 0.778 |
| dim corridor | `[0.447, 0.448, 0.405]` → `[0.415, 0.417, 0.375]` | 0.478 → 0.407 | 0.445 |
| dark Home hallway | `[0.232, 0.222, 0.208]` → `[0.238, 0.227, 0.212]` | 0.233 → 0.205 | 0.220 |
| Home living area | `[0.294, 0.288, 0.277]` → `[0.323, 0.316, 0.303]` | 0.394 → 0.368 | 0.006 |
| pool | `[0.484, 0.639, 0.797]` → `[0.488, 0.643, 0.800]` | 0.820 → 0.746 | 0.591 |
| stairs | `[0.500, 0.251, 0.215]` → `[0.524, 0.255, 0.215]` | 0.321 → 0.338 | 0.300 |
| Home staircase | `[0.350, 0.343, 0.326]` → `[0.306, 0.301, 0.289]` | 0.396 → 0.341 | 0.260 |
| outdoor route | `[0.353, 0.323, 0.285]` → `[0.555, 0.488, 0.409]` | 0.156 → 0.507 | 0.431 |
| bright doorway side | `[0.350, 0.343, 0.326]` → `[0.306, 0.301, 0.289]` | 0.396 → 0.341 | 0.260 |
| dim doorway side | `[0.153, 0.193, 0.140]` → `[0.276, 0.349, 0.273]` | 0.216 → 0.351 | 0.096 |
| night guard | `[0.543, 0.516, 0.484]` → `[0.484, 0.438, 0.386]` | 0.585 → 0.495 | 0.416 |

No requested location triggers the report's 100× probe/static mismatch threshold.
The living-area floor beneath the chosen air probe is in a furniture shadow
(static L 0.005964 versus probe L 0.367801); this remains a substantial local
contrast. A sparse 1.83047 m lattice and different heights cannot resolve every
shadow at an entity's exact position. The actual requested living-area floor
is L 0.091830. Pool requested-floor L is 0.166315 versus 0.590733 beneath the
selected probe; the report retains both measurements rather than hiding the offset.

The intermediate visibility-corrected bake exposed a stronger outdoor defect:
ground was exactly zero while the air probe saw two fixtures. Native rays hit
grass card triangles just above the floor. With the MASK fix, physical receiver
RGB near `[2.3625,0,-18.6036]` is `[0.449155,0.380149,0.299301]`. Final outdoor
upward probe L is 0.506678 and static L beneath it is 0.431217; static L beneath
the requested point is 0.230140. No owner-prop exemption or brightness tuning
was used.

The full grid changes from `[52,6,64]` / 19968 records / 5424 valid records to
`[42,5,64]` / 13440 records / 2470 valid records. All 14 rooms have
coverage (minimum four valid probes in room 10); stricter air/solid rejection
accounts for most of the valid-count reduction. Valid mean luminance spans
0–1.472524 after versus 0–1.667446 before. Genuine zero values remain valid.

Adjacent same-room 100× candidates fall from 653 to 46. Only three final
candidates have their brighter value above 0.05; all are in outdoor room 12
and have different visible emitter sets. No indoor 100× pair remains. Very dim
outdoor transport tails and authored fixture reach boundaries account for the
remaining candidate classes; the metric does not imply every contrast is a defect.

Full-quality PLPF SHA-256:

- Before: `82dddb016f4d9bd4735b0c39aa48ba051734082ef63e91eb1cb1bbd2e5e004c4`
- After: `a5043d093a9cea89b765a84f85f3d86bc2d7e605383e809e4bb508e57879fd4c`

The demo package is 74,576,126 bytes, with no compiler warnings. Its forced
release build took 1477.288 s (24 m 37 s), including reflection capture and the
independent capture-preparation solves. The baseline debug build took 1265.100 s
(21 m 5 s); an intermediate release build before work balancing took 3071.704 s
(51 m 12 s). These runs shared a heavily contended host and differ in build
profile, so they are observations, not a controlled performance benchmark.
Probe rays increase from 24 to 64 and visibility checks add work; the lattice
contains 32.7% fewer records. Quality was not reduced for speed.

Model Zoo's final forced build took 464.438 s (7 m 44 s), compared with
2103.653 s (35 m 4 s) for the intermediate release build before balancing and
empty-capture elimination. The final package is 58,788,712 bytes, has 6912
records / 5182 valid, no warnings and no adjacent 100× candidates. At spawn
`[15.6,1,21.1]`, mean RGB is `[0.782369,0.754161,0.698763]`; upward probe L
0.871599 compares with static L 0.738654 beneath the chosen probe. Full PLPF
SHA-256 is `f744ba98069e7808c6fe914c4c74f9b59933eceaab9999225f7ceaba8c618a9f`.

Every RGB/moment coefficient and room label in the medium/full records matches
the pre-serialization diagnostics exactly: 13440 records per demo variant,
6912 per Zoo variant, zero mismatches. Demo reflection capture independently
repeated both numeric solves; those repeat coefficients match the package's
original solve bit for bit. Zoo's proven empty reflection capture is skipped.
Unit tests separately establish repeated and 1/12-worker equality. Archive
compression/manifest timestamps need not produce identical whole-ZIP bytes;
the baked numerical records are the determinism contract.


## Regression coverage and validation

Eighteen new Rust tests and five Python tests cover dark/lit enclosed compartments, sealed-wall
probe leakage, fill/filter leakage, coherent neighbors, repeat/1-vs-12-worker
bit identity, bounded interleaved work assignment, HDR sky with zero moment,
sky once per order and excluded from switchable layers, solid/surface clearance,
capped world-coordinate grids, real floor/ceiling/region heights, half-wall
openings, neutral probe/static consistency, source/accumulation overflow and
codec corruption/NaN/Inf/negative energy/invalid moments/exact HDR round-trip.
Prop-scene tests cover MASK transmission and clearance while proving OPAQUE and
static BLEND-fallback geometry still blocks.
Python oracles independently check record offsets, world centers, signed
moments, HDR/dim channels, malformed records, normal reconstruction, single-texel
centering and exact location sampling between coarse atlas taps.

No dependencies or lint suppressions were added. The existing numeric-kernel
lint exceptions remain.

| Validation | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed. |
| `cargo check --workspace --all-targets --all-features` | Passed. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` | Passed. |
| `CARGO_BUILD_JOBS=2 CARGO_PROFILE_TEST_OPT_LEVEL=3 cargo test --workspace --all-features -- --test-threads=2` | Passed: 1841 unit tests + 3 CLI tests; 14 existing ignored tests; zero failures. Unit suite took 1321.88 s. |
| Focused transport / offline scene / compiler placement tests | 54 / 6 / 2 passed. |
| `python3 -m unittest tests.test_probe_lighting_report tests.test_tool_execution tests.test_lightmap_harness tests.test_package tests.test_packaging` | 76 run, passed, one existing packaging skip (no release game binary). Expected negative-fixture error output is intentional. |
| `python3 tools/assets/validate.py` | Passed; zero warnings. |
| `python3 tools/textures/build.py --check` / `python3 tools/props/build.py --check` | Passed. |
| Forced 12-worker native demo / Zoo builds, all off/medium/full variants | Passed; no warnings. |
| Native `places-compile validate <package> --json`, both final packages | Passed; dependencies intact. |
| Native `places-compile verify <source> --package <package> --asset-root "$PWD/assets" --require-current --json`, both final packages | Passed; current, no fingerprint differences. |
| Independent PLPF diagnostic/package comparison, both qualities and maps | Every RGB/moment coefficient and room label exact; zero mismatches. |
| Independent native repeated demo bake and 1/12-worker tests | Bit-identical probe output. |
| `git diff --check` | Passed. |

The full Rust run uses optimized numeric code with debug assertions and all
features enabled. Only concurrent test count is limited; no tests, bake quality
or resolution are removed. Timings above are native representative builds,
not synthetic estimates. Native validation reads the rebuilt payload through
the normal compiled-map decoder, including the strict PLPF reader.


## Files changed

| File | Purpose |
| --- | --- |
| `src/compiler.rs` | Validate final probe air/coverage and abort invalid physical bakes. |
| `src/compiler/probes.rs` | Compiler-only 3D room/wall/solid labeling and placement tests. |
| `src/lighting/transport.rs` | Visibility-isolated transport/fill/filter, sky accounting, corrected lattice, numeric validation, coherent gathers, bounded worker distribution and revision/fingerprint. |
| `src/lighting/transport/probe_audit.rs` | Opt-in atomic deterministic diagnostic dumps. |
| `src/lighting/probes.rs` | Exact, validated f32 PLPF codec and corruption/HDR tests; contract comments. |
| `src/lighting/transport/tests.rs` | Updated revision pin and visibility-aware existing regression expectations. |
| `src/lighting/transport/tests/fill.rs` | Serial/parallel partition-independent authored fill validation. |
| `src/lighting/transport/tests/probes.rs` | New independent transport, placement, determinism and consistency oracles. |
| `src/render/common/light_transport.rs` | Offline prop-submesh MASK transmission and regression; runtime object-light logic is untouched. |
| `tools/bench/probe_lighting_report.py` | Read-only native-payload probe/static comparison and outlier candidates. |
| `tests/test_probe_lighting_report.py` | Independent binary/HDR/atlas/location oracles for the report. |
| `docs/PACKAGE_FORMAT.md` | Exact PLPF coordinate, energy, interpolation and validity contract. |
| `docs/MAP_AUTHORING_GUIDE.md` | Revision-8 behavior and diagnostic workflow. |
| `docs/PROBE_BAKER_AUDIT.md` | Findings, measurements, limitations and validation evidence. |
| `assets/levels/places_demo.placesmap` | Rebuilt baked data with current dependencies and revision-8 lighting. |
| `assets/levels/model_zoo.placesmap` | Rebuilt second representative package with current dependencies and revision-8 lighting. |

Only compiled map content changes. Authored JSON, geometry, texture files,
catalog/model assets and gameplay are unchanged. Against the rebuilt solver-7
baseline, every off-variant payload and dependency declaration is unchanged.
Medium/full payload changes are restricted to mesh lighting, lightmap metadata/atlas,
irradiance and captured reflection data; collision, navigation and authored lighting
records are unchanged. The existing bundles also
recorded older corner-trim model byte sizes than the assets at `ba16887`; their
regeneration repairs those dependency declarations as part of the required bake.

## Runtime handoff and remaining limits

There is no shared runtime API or binary-layout migration to integrate.
Consumers need revision-8 packages and must respect the documented mean/moment
and missing/zero distinction. The reader now rejects malformed records that
previously could be repaired; valid v2 data remains compatible.

The runtime room-only interpolation can still mix opposite sides of an
internal divider carrying the **same room ID**. The ray-tested bake does not
encode per-edge visibility in v2. Runtime object-light baseline floors,
orientation-free `sample_display`, fallback behavior, material application and
quality transitions must be audited on the runtime side. In particular, the
existing moving-object baseline floor can lift a valid dark probe, and the
stored mean alone differs from a normal-aware static reconstruction. No
compiler multiplier was introduced to compensate for either behavior.
