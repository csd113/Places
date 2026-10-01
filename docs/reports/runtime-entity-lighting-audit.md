# Runtime entity lighting audit

Worktree: `Places-entity-lighting`; branch: `agent/entity-lighting-audit`.
Base: `ba16887`. The primary checkout and the separate baker worktree were
not edited. No baker changes were merged. Diagnostic outputs are under
`target/agent-work/` and are not shipped assets.

## Findings and runtime changes

* Characters previously multiplied environmental light into vertex albedo at
  spawn. Their light stayed at the spawn location even as gameplay moved them.
  They now keep original material albedo and sample the current field each
  frame after transforms advance, independently of animation revision.
* Rigid objects cached lighting behind a 5 cm movement threshold. Stationary
  objects could retain the previous field after a graphics rebuild, and small
  movements lagged. They now sample every frame. GPU uniform comparisons still
  avoid writes for unchanged values.
* The old prepared-field path floored each RGB channel at the authored room
  baseline. This hid dark baked values and could give models much more light
  than the surrounding transport bake. Valid black and low-energy probes now
  remain dark. No ambient increase or model-specific constant was added.
* Sparse grids used renormalized trilinear corners, then a single fallback
  probe selected by Manhattan **lattice offset**, rather than true distance
  from the sample. Crossing missing cells could switch abruptly. The runtime
  now uses normalized compact Shepard weights over a two-cell spherical
  support, with Euclidean distance in all three axes and an exact-centre path.
* The existing height-aware room lookup falls back to X/Z ownership when
  no room contains Y. Probe generation consequently labels some below-floor
  and above-ceiling positions as room-owned. Runtime selection now validates
  both the entity anchor and candidate centres against actual air volumes,
  using the existing indexed containment implementation without that fallback.
  Serialized labels remain untouched for the baker to repair.
* Probe decode called `LightmapTexel::normalized`, silently repairing negative
  energy/other malformed values. Decode now rejects malformed records and
  preserves accepted floats bit for bit. Invalid in-memory candidates are
  skipped; an unresolved sample requests the explicit fallback.
* Runtime entities discarded signed directional moments and used only
  isotropic display light. Their imported geometry also supplied the default
  vertex normal rather than posed face normals. Entities now upload linear
  HDR energy and signed moment and reconstruct diffuse response with the same
  equation as lightmaps, against derivatives of posed world geometry.
* Graphics rebuilds retained placed-character playback but lost runtime actors.
  They now retain those actors, refresh samples against the newly installed
  field, and rebuild their GPU resources. Dynamic samples also refresh before
  installation uploads, rather than waiting for movement.
* Untextured dynamic primitives had no GPU texture binding and were skipped.
  In a mixed model they could also borrow texture zero from another primitive.
  They now use the existing committed `core/textures/white_01.png` fallback.

## Sample position, interpolation and fallback

Both rigid and animated entities sample the transformed model-space bounds
centre, in world metres with Y up. Translation, yaw and scale are applied once.
Character anchors use bind-pose bounds and stay stable as the pose changes;
local centres are cached at spawn. This intentionally gives one spatial
lighting sample per body, with diffuse variation across its surface normals.
Very large bodies spanning separate lighting volumes remain a limitation.

The field's `min` is its lower grid corner, not its first probe centre:
`position = min + (index + 0.5) * cell_m`. Index order is X fastest, then Y,
then Z. Runtime containment uses actual floor/ceiling spans as well as X/Z, with
no footprint-only fallback. Accepted probes must
belong to that room and the existing connected lighting area within it.
The area lookup is reused from `LevelLighting`, without changing or relabelling
compiler room records. Sealed internal partitions and separate stacked rooms
therefore do not borrow another room/area's probes.

For lattice distance `d < 2`, the weight is `(1 - d/2)^2 / d^2`; weights vanish
at the support boundary and are normalized before interpolation. Exact centres
read one valid probe. Energy and signed moments are accumulated in f64, then
converted to f32 as a convex combination. There is no nearest-only fallback
or allocation in the frame sampler. Explicit diagnostics allocate a candidate
list with normalized weights.

When no field exists, no room contains the point, or no valid candidate remains,
the runtime uses the existing `LevelLighting::sample` environment response,
bounded to its display range `[0,1]`. This response includes authored local
lighting and its existing visibility checks. It is not a new brightness floor.
Non-finite sample positions are rejected before spatial lookup and receive a
deterministic environment fallback. The source enum distinguishes `Prepared`,
`NoField`, `Roomless`, `Unresolved` and `InvalidPosition`. Malformed serialized
fields fail map decoding rather than silently turning into white or black.

## Shader/material and resource behavior

For normal `n`, stored energy `I`, moment `g` and `k = sum(I)`, the entity shader
reconstructs `max(0, I + I/k * (2*max(dot(g,n),0) - length(g)))`, with the same
small-energy guard as static lighting. The existing soft knee is applied once,
then material colour/texture and emission follow the established world path.
Prepared energy is not gamma decoded, clamped to display range, or multiplied
into albedo before upload. Existing display-space base-colour texture and final
surface transfer conventions are preserved. Authored face shades remain albedo.
Opaque, cutout and blended dynamic/character primitives share this response;
their existing opacity, alpha and emission routing is preserved.

The environment uniform adds two vec4s at offsets 1248 and 1264, for a total of
1280 bytes. Old offsets are unchanged. The irradiance W flag is 1 for prepared
entity light, -1 for scalar entity fallback and 0 for the static path. Moment W
is reserved zero. Rust layout tests and real wgpu shader validation cover this.
Uniform diagnostics expose the exact CPU mirror last written to the GPU.

The renderer refreshes entity lighting after motion and after installation.
High → Medium → Low → Medium → High recreates/binds resources, restores samples,
retains runtime actors, and renders identical pixels on return to High in the
regression fixture. Three unload/load cycles clear old actors and fields.

## Compiler/runtime contract (shared changes)

No package schema, PLPF record version, solver fingerprint, baking equation,
probe generation, lightmap authoring or map source was changed on this branch.
The only addition to `lighting/bake.rs` is read-only access to its existing
connected-area lookup. The probe reader/validity predicate is a shared type
change that the baker branch must reconcile.

* Package format remains 1; irradiance record magic/version remains PLPF v2.
  V1 dominant-axis data remains unsupported.
* Header: 38 bytes, little endian; lower world corner xyz, cell size, dimensions
  xyz and count. Each 36-byte record is f32 energy RGB, f32 signed moment xyz,
  f32 reserved axis pair and i32 room. Compiler writers normally reserve axis
  as `[0.5,0.5]`; runtime preserves serialized values and ignores that pair.
* Energy is non-negative **linear HDR**, not sRGB. Each channel must be finite
  and at most 65504, consistent with surface lighting's half-float range.
  Signed moment must be finite and have length no greater than summed energy,
  allowing `1e-5` relative plus `1e-6` absolute float error. Reserved floats
  must be finite. These are rejection rules, not runtime clipping rules.
* Room -1 means unavailable and is never sampled. Other serialized room IDs
  retain the existing bound through i16::MAX. Runtime only accepts candidates
  matching actual room containment and existing connected-area metadata.
* Payload length/count/dimensions are checked before allocating the probe
  array. Accepted records round-trip bit for bit, including signed moments,
  HDR values and reserved fields. Outer package dependency/content-address
  validation and compiler/lighting fingerprint checks were left intact.
* Runtime interpolation assumes a uniform lattice with valid coverage extending
  through navigable air. Holes are tolerated within bounded support; coverage
  exhaustion deliberately invokes the environment fallback.

## Compiled-data and rendered comparison

The checked-in demo's Medium and Full fields each contain 19968 records, of
which 5424 have room ownership. Origin is approximately
`[-6.000047,-3,-98.000046]`, spacing 1.8304696 m, dimensions `[52,6,64]`.
Every accepted record re-encodes to the original payload bytes. Full red energy
ranges from 0 to 1.225359; Medium reaches 1.022216. This includes valid darkness.

Full payload SHA-256:
`089ca91504125e8842d7039f77186074b7cd8be9df29326a58664ebcebcdbd78`.
Medium:
`110466051524c721884d6f92a92bc5bf80540cab9ff1ddad42fdc98124dbe0a0`.
Compiler fingerprint:
`d0b5dde4737c03345b060180ac9710edb91e143a2e780d0dd67dad17bc435fd2`.
Lighting fingerprint:
`379a3833293ea9fe0c0c1a1840fa0334c012cc68c6249ea9d512520f6a142dc1`.

`compiled_demo_probe_to_pixel_comparison` reads raw archive records through the
real runtime decoders, checks all probe bytes, samples the same untextured white
triangle at 11 locations for both fields, compares interpolated coefficients
with the actual uploaded uniform mirror, and captures real GPU pixels. Its
JSON includes selected IDs, raw decoded coefficients, world positions,
distances, normalized weights, interpolated energy/moment, GPU payload,
normal-specific reconstruction and framebuffer RGB. It also moves the entity
in 1 cm steps over one metre on X/Y/Z, checks finite samples and stationary
repeatability, and records maximum steps and fallback changes.

Below, “before” reconstructs the historical trilinear/Manhattan sampling and
room floor from the same compiled data (CPU numerical comparison, not an old
framebuffer capture). “After isotropic” shows the new CPU diagnostic value.
The pixel is the new **directional** neutral triangle's centre RGB8. It also
passes through the renderer's existing fog/capture path, so it is not simply
isotropic RGB multiplied by 255.

| Full-field location | Before RGB | After isotropic RGB | New rendered RGB8 |
|---|---|---|---|
| Bright room (3,1.5,3) | 0.891,0.845,0.671 | 0.894,0.849,0.674 | 214,200,159 |
| Moderate room (14,1.5,3.5) | 0.841,0.784,0.639 | 0.842,0.786,0.641 | 211,197,162 |
| Dim hall (45,1,13) | 0.392,0.400,0.393 | 0.319,0.317,0.273 | 85,85,74 |
| Dark hallway (61,.5,-5) | 0.388,0.381,0.370 | 0.327,0.312,0.292 | 87,83,78 |
| Home (59,1,8) | 0.434,0.421,0.397 | 0.438,0.425,0.400 | 86,84,80 |
| Pool (14,0,13) | 0.409,0.557,0.706 | 0.407,0.556,0.706 | 93,127,163 |
| Stairs (21,.5,3) | 0.472,0.308,0.288 | 0.477,0.262,0.223 | 123,67,58 |
| Outdoor (12,1,-30) | 0.354,0.346,0.332 | 0.097,0.094,0.088 | 24,23,23 |
| Doorway (9,1,3) | 0.718,0.672,0.554 | 0.804,0.747,0.601 | 150,139,113 |
| Pumpkin area (13.5,.3,-83) | 0.354,0.346,0.332 | 0.027,0.026,0.023 | 6,6,5 |
| Skeleton area (9.6,.9,-52) | 0.354,0.346,0.332 | 0.165,0.158,0.148 | 39,37,35 |

For paths that stay in actual air and within one room, maximum per-channel
1 cm steps are below .00125, with no fallback changes. The doorway has a
.09284 maximum step when room ownership changes: interpolation is smooth
**within** each room, but the existing discrete room contract does not guarantee
continuity between them. The pumpkin's Y diagnostic deliberately spans -.2 to
.8 m; crossing below its floor invokes the roomless environment fallback and
produces a .32691 step. This is outside usable air, not a valid walking-path
interpolation result. Both transitions are recorded rather than hidden by an
ambient floor. Neutral snapshots use the original demo coefficients, separately
from the full-scene captures described below.

For the controlled directional render, a sloped white face has geometric
normal `(0,1,1.4)` normalized, deliberately different from its default imported
+Z normal. Changing moment +Y to -Y changes diffuse light from approximately
.216 to .1 before the knee. The test checks the predicted bright pixel within
two display levels and requires a framebuffer red difference greater than 20.
A shader that used the default +Z normal would fail this test. The
quality test also asserts restored framebuffer bytes, not just UI state.

Full-scene original/updated binary captures use the same isolated, newly
compiled demo package (SHA-256
`63b91f32179295030d523a67ee20a95d35520dadf8e7dbec8a50dfc8ac4b2ad5`)
with High/Full settings and identical camera overrides. They cover reception,
office, dim hall, dark hall, Home cat, pool cat, stairs, outdoor path, doorway,
pumpkin and skeleton. Files are under `target/agent-work/full-scene/{before,after}`;
`manifest.json` records camera/spawn positions. Their static scenery is shared;
comparison isolates runtime consumption rather than changes to the bake.
The Home cat's mostly black albedo remains black; white feet respond to the
field. The runtime does not make a dark material emissive or brighten it by name.
Native gameplay runs also exercise the live menu-equivalent quality cycle and
capture the restored High scene. The Home actor's restored irradiance
`[.35218227,.3444837,.32943273]` and moment
`[-.124794416,.29396057,-.0009098192]` exactly match its direct High trace.
Pixel identity is asserted by the controlled
GPU test; native gameplay snapshots have advancing animation and UI state.

## Diagnostics and reproduction

Full-scene outdoor models are visibly darker after removing the room floor:
the pumpkin's external shell is almost black while its authored flame/glow
remains visible, and the skeleton's diffuse body follows the field rather than
receiving the old baseline. This is a confirmed remaining bake/environment
mismatch, not evidence that every requested location is visually resolved.
Static vertex-lit surroundings and animated vegetation also expose the
separate compiled lighting paths described below. The branch deliberately
leaves those low-energy results available for the baker audit.

Normal play emits no new logs. Set `PLACES_ENTITY_LIGHT_TRACE=all`, an instance
ID, a dynamic handle such as `DynamicId(0)`, or a model path together with the
existing `PLACES_CAPTURE` path, with `PLACES_VERBOSE=1` to enable developer
telemetry. The capture logs ID/model/material path, map and
graphics settings, transform, sample point, lattice cell, room/area, candidate
IDs/positions/distances/weights/raw coefficients, interpolated sample, fallback
source, uploaded energy/moment/scale, and six normal-specific pre-tonemap values.

Run the GPU checks on a desktop host:

```sh
mkdir -p target/agent-work/gpu-captures
PLACES_ENTITY_TEST_CAPTURES="$PWD/target/agent-work/gpu-captures" \
PLACES_ENTITY_PROBE_REPORT="$PWD/target/agent-work/demo-probe-to-pixel.json" \
cargo test --lib entity_lighting_tests -- --ignored --nocapture
```

Captured artifacts include `entity-facing-light.png`,
`entity-away-from-light.png`, and `demo-{medium,full}-{location}.png`.
`historical-comparison.json` contains the reconstructed old sampler comparison.

## Regression coverage and validation

Added exact-centre/zero-distance/HDR/signed-moment checks; normalized weights;
XYZ continuity, shifted non-integral grid centres and sparse-hole interpolation; out-of-volume/missing/invalid
samples; non-finite/negative/oversized/unphysical records; bit-exact shipped
payload round trips; world bounds transformed once; rigid/animated equivalence;
small movement and stationary field replacement; repeated quality restoration;
real GPU uniforms/pixels; retained runtime actors; repeated unload/load; and
explicit finite environment fallback reasons. Existing tests protect alpha,
material, emission, static shader and package paths.

Commands and results:

* `cargo fmt --all --check`: passed.
* `cargo check --workspace --all-features`: passed.
* `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo`: passed.
* `cargo test --workspace --all-features`: failed with 1829 passed, 5 failed
  and 17 ignored (1420.42 seconds). All five failures are the existing
  demo-loading tests below. The three ignored GPU regressions were also run
  explicitly, as documented next.
* `cargo test --lib entity_lighting_tests -- --ignored --nocapture`: all three
  actual GPU tests passed.
* Targeted probe (11), connected-area and vertical candidate regression tests:
  passed.
* `cargo build --bin places`: passed. Isolated demo compile and
  `places-compile validate target/agent-work/places_demo.placesmap`: passed,
  with off/medium/full variants, 33 entries and 108 dependencies.
* Full-scene original/updated captures and native quality restoration: passed.
* `git diff --check`: passed.

The five existing demo-loading failures are caused by bundled
packages recording outdated asset sizes. For example, `house_02_corner_trim.glb`
is 15536 bytes but the demo records 7640. The original binary also refuses
that package before rendering. No loader bypass, asset replacement or bundled
map rewrite was added to make these checks pass. The failed tests are:

* `loader::tests::test_prepared_demo_aligns_panels_and_gains_office_baseboards`
* `loader::tests::test_prepared_demo_baseboard_geometry_sits_at_floor_level`
* `loader::tests::test_the_default_level_is_the_shipped_demo`
* `loader::tests::test_the_demo_loads_every_fixture_family_with_its_sheet`
* `loader::tests::test_the_embedded_demo_is_always_listed_and_loadable`

## Baker-owned findings and remaining limits

1. Probe labels currently include positions outside actual room air: for
   example a Home probe at Y=-2.084765 is labelled room 6, whose floor is -.9.
   `room_index_at_height` intentionally has a historical X/Z fallback for
   surface lighting; probe placement must use actual volume containment.
   Runtime now rejects those candidates without modifying their bytes.
2. Very low outdoor energy exists in the original payload. The pumpkin area
   samples about .016 red without air filtering, or .027 after rejecting
   below-floor candidates, before directional reconstruction. Compare
   that probe solve with nearby surfaces, sky transfer, wall/floor occupancy
   and switchable lighting before changing calibration. Runtime now exposes it.
3. The transport API solves base probes separately; switchable surface layers
   are solved without probe receivers. PLPF carries no per-switchable probe
   layers. Turning on such a light cannot restore its missing prepared entity
   contribution. The baker must provide consistent all-on energy or additive
   per-fixture probe layers with selection metadata. Runtime must then update
   those coefficients on toggles; it must not guess that contribution.
4. Existing room/connected-area IDs prevent unrelated-room and sealed-area
   blending, but do not encode pairwise visibility around partial walls,
   same-room upper floors, or continuous portal transitions. Prepared
   visibility/region coverage or portal interpolation metadata is needed for
   those guarantees. The existing all-solid visibility diagnostic linearly
   scans floors and prop boxes; it was not moved into every entity/candidate
   frame query. Current room filtering can step at a doorway and fallback can
   step where usable coverage ends.
5. Static placed-prop batches are a separate compiled vertex-lighting path:
   their colour already contains `LevelLighting::sample`, rather than the
   entity's prepared transport field. Matching those props to transport-lit
   architecture/entities requires a compiler-side representation/lighting
   decision. Rigid runtime and animated entities now share coefficients;
   this branch did not reinterpret already-lit compiled prop colour.
6. Checked-in packages need recompilation against their current dependencies.
   An isolated demo recompilation completed in 40.5 minutes, produced a valid
   70 MiB package under `target/agent-work/`, and made full-scene before/after
   captures possible via isolated installed-level directories. Map sources,
   solver code and bundled packages remain unchanged. The GPU comparison
   above remains reproducible with the original compiled payload.

Changed files:

* `docs/reports/runtime-entity-lighting-audit.md`
* `src/lighting/bake.rs`
* `src/lighting/probes.rs`
* `src/lighting/tests.rs`
* `src/render/common/character.rs`
* `src/render/common/dynamic.rs`
* `src/render/common/light_transport.rs`
* `src/render/tests.rs`
* `src/render/wgpu/character.rs`
* `src/render/wgpu/dynamic.rs`
* `src/render/wgpu/entity_lighting_tests.rs`
* `src/render/wgpu/environment.rs`
* `src/render/wgpu/renderer.rs`
* `src/render/wgpu/world.rs`
* `src/render/wgpu/world.wgsl`
