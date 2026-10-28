# 0.13.1 editor acceptance (unreleased)

The first phase added rectangular/circular pillars and fitted automatic trim.
Its accepted source is `fa9a8b93cbfba0b0e8a1b1ee064c01cbbe1be9e1`; its receipt,
native captures and binaries remain under `local/verification-0.13.1-trim/`.
The [changelog](../../CHANGELOG.md) retains its exact acceptance scope.

The second phase exposes New Room, Add Wall, Add Door, Add Light and Play in Simple
mode. Make Doorway cuts an aperture; Add Door creates or fits a functional leaf.
Wall placement has a private preview, optional chaining, snapping and cancellation.
Body dragging and endpoint handles preserve openings, doors, wall lights/decals
and automatic trim in one history transaction. Floor decals stay fixed. Existing
room stretching retains attachment, shared-boundary and elevated-prop guards.
Four visibility groups, room/wall isolation and persistent object locks support
focused editing. Material browsing has 24 compact tiles in four rows or 15 larger
tiles in three, while retaining labels, selection, search, theme and paging.
Eleven absent material previews are committed opaque 160x120 PNGs; existing
previews and source textures are unchanged.

## Razer acceptance, 2026-10-28

Formatting, strict workspace/all-target/all-feature Clippy, the locked release
workspace/all-feature build, 367 Rust tests and 369 Python source tests pass
(18 expected source skips). Two small CPU regressions protect atomic wall
attachments/source/history/focus identity and DPI control/hit correspondence;
ordinary tests never bake production maps. The asset validator checks 644 assets
with zero warnings. Texture/prop checks pass with their existing manifest and
preferred-budget advisories.

Native SDL verification uses ordinary editor handlers, real windows/GPU rendering,
framebuffer captures and source checkpoints. The accepted recipes cover adjacent
room creation; four chained walls; doorway versus leaf; light and materials;
repeat/cancel; locks, isolation and hidden layers; wall body movement, both
endpoints, aperture-conflict refusal, atomic undo/redo; and room stretching with a
door and trim. Save/reopen retains source. Simple Play saves/exports/builds,
launches the game and returns cleanly; native captures show the new door closed
and opened, and the player trace crosses its opening. Failed exploratory recipes
are retained separately and excluded from acceptance assertions.

Visual layout checks cover 960x544, 1280x720/800, 1000x900 and 1440x900 on the Razer
at Windows 150% display scale. CPU coordinate checks also cover 100/125/200% and
1x/2x drawable density. Actual Mac Retina validation is deferred. Fifo presentation
was negotiated; these checks make no uncapped FPS claim.

## Construction integrity (phase 3)

Construction/Safety now shares wall-plane/thickness classification and offline
alignment planning, opening slicing, collision, wall/mount attachment edits and
the emitter's covered-face subtraction. It adds short corner/endpoint gap/tail
review, floor/ceiling seams, sampled player/route/landing clearance, authored door
pose intrusion and measured ceiling/wall UV phase checks. Compatible chosen
corrections preview privately and commit in one undo step; stale, conflicting,
locked/hidden/isolated or protected contexts refuse. Lock changes invalidate cached
review, allowing immediate unlock/rescan. Cancellation discards partial
results. Supported L/T contacts, solid-supported seams and angled geometry remain.

Final Razer gates pass: format, strict workspace/all-target/all-feature Clippy,
locked release workspace build, 371 Rust tests, 369 Python source tests (18 skips),
644 assets with zero warnings and the unchanged texture/prop checks. Four tiny CPU
regressions protect source/history, original-index grouping, corner intent and
solid-versus-open floor gaps. Native Windows acceptance has 144 assertions across
392 actions, including preview/apply/undo/redo/save/reopen, repeat/idempotence,
mixed conflicts, locks/hidden layers, cancellation/restart and compact/tall/wide
layouts. Shared CLI/editor findings and marker positions agree. The deliberately
bad fixture retains its one unrelated original error after local corrections.

Three old/new runs retain all 10 small-scene and 89 Arcade legacy findings with
deterministic results. Small scans finish before the next frame poll; Arcade's
median report completion is 758 / 776 ms before/after; cancellation returns in
194 ms. These frame-polled observations justify retaining existing spatial
traversal and full scans; no new chunk/dirty-region cache, bake or FPS claim.

Automatic endpoint edits stay within 5 cm with one full-thickness solid carrier.
Near gaps up to 35 cm, partial/ambiguous corners, destructive aperture/headroom/
landing changes and unsupported UV wrapping remain review-only. Overlapping
record edits refuse as a group. Existing model/interactive budgets still report
incomplete coverage; this is no watertightness or complete-traversability proof.
Actual Mac native validation and full production/release closing remain deferred.
The prior phase CI passed; the phase commit's observed CI state is in the receipt.

Accepted binaries, source fixtures, recipes, captures, profiles, hashes and the
final receipt are under `local/verification-0.13.1-integrity/final-lock/`. Earlier
root-level integrity evidence is superseded, retained for audit. Main/v0.13.0,
production sources/packages and prior phase evidence are unchanged.

## Construction generators (phase 4)

Four generators now share validated editable source, private preview, ordinary
history and deterministic regeneration. Wall recesses retain real solid backing,
separate side/back materials and optional fitted trim. Through openings retain
their existing semantics. Door/window surrounds follow the aperture and provide
frame, sill and outer trim parameters. Their geometry clips existing wall trim
before lightmap construction, avoiding overlapping jamb/baseboard faces.
The niche back material affects only its cavity face; the exterior wall material
is retained. Surround fit includes neighboring apertures and other surrounds.

Railing paths collect corners in the existing direct-placement tool and commit
all sections/posts atomically. They reuse guardrail materials, spacing and height,
split at stair/ramp boundaries, follow the real surface profile, share corner
posts and meet at mitered rail faces. Short swept collision boxes preserve usable
space beside angled runs. Reversed and excessively tight turns refuse.

Point-to-point tunnels generate an enclosed passage on the owning room floor.
Clearance is bounded between the real crouched and standing hulls, with full
standing landings at each mouth. Collision, props, fixtures, authored door poses,
other passages and unsupported floor changes receive actionable refusal. Shared
Construction/Safety uses the same generator validation and collision; it reports
crouch clearance without destructive repair. No generated child geometry is saved.

Simple mode exposes these tools through More geometry and a selected wall's
Build button. Basic dimensions appear first; materials and explicit elevation
overrides remain on contextual form pages. Wall/door/light placement, 24-tile
material browsing and compact/tall/wide layouts remain available.

Native acceptance exercises private preview/cancel, repeated Apply, dimension
adjustments, undo/redo, source movement/resizing, generator/carrier deletion and
save/reopen. A blocked route, excessive recess depth, standing tunnel height,
truncated landing and colliding surround receive specific refusals. Captures
include baked niche lighting and backing-mounted light, the opposite exterior,
functional door/window frames, the passage interior and stair/landing corners.
Actual game input tests both tunnel directions, release-Crouch refusal inside,
standing refusal at each mouth, both exit transitions and stair ascent/descent
beside the generated railing. These are native visual/controller checks, not
just script completion or package preparation.

Final Razer source gates pass: format, strict workspace/all-target/all-feature
Clippy, the locked release workspace/all-feature build, 373 Rust tests and 369
Python source tests (18 expected skips). Two small CPU regressions protect
backing/shell collision, source/history/save determinism, bounded refusals and
stair-path regeneration; ordinary tests never bake maps. The unchanged asset
checks retain 644 assets with zero warnings and existing texture/prop advisories.

Final native acceptance passes 80 assertions across 576 editor actions. The
reopened combined fixture has no Safety errors or coplanar trim slivers; 56
measured wall-corner UV phase items remain review-only. Standing stops at X 1.60
and 7.40 at the two tunnel mouths, the 0.9 m crouched hull traverses both ways,
and both landings restore the 1.8 m standing hull without recovery resets. The
stair witness climbs to its landing, stops at the corner rail and returns down
the flight. Both small Static/Full packages pass integrity and current-source
verification. The game loads resident Full lightmaps on Dx12 at 960x600 with
Fifo presentation; these runs make no uncapped FPS claim.

Accepted executables/DLLs, fixtures, recipes, native captures, controller traces,
source/build identities and gate logs are sealed under
`local/verification-0.13.1-generators/final-lock/`; `sha256.json` hashes the
inventory. The adjacent `validation-receipt.json` records the exact commit,
remote verification, CI observation and cleanup. Earlier unsuffixed,
accepted/miter/exploratory evidence remains retained but is superseded.

Tunnels support straight, level X/Z routes 1-30 m long within one flat room.
Bent, angled, vertical, automatic through-wall and stair/ramp/region connections
are explicitly unsupported. Rail paths require supported floors and safe fit;
unusual crossings need manual review and Play validation. The existing Mount
Surface command fits niche backing and wall movement carries the fitted light.
Changing niche depth/aperture requires explicit remount/review; no persistent
niche-specific fixture anchor is introduced. Dynamic obstacles still need Play.

Actual Mac validation, production rebaking and release closing remain deferred.
Lighting circuits/compiler performance, HVAC/assets, presets/prefabs, runtime
repairs, packaging and FPS attribution remain outside this phase.

## Boundaries and retained evidence

Walls remain axis-aligned and must fit one room floor. Connected perimeter
changes use room edge stretching; edits that sever junctions, cut through openings,
cross shared-room boundaries or ambiguously move elevated fixed props are refused
with feedback. Visibility groups and isolation are session state; object locks
persist. Custom layer authoring is outside this bounded implementation. Advanced
numeric/build tools remain available, while Simple Play builds automatically.

`local/verification-0.13.1-editor/validation-receipt.json` records the accepted
commit, hashes, cleanup and remote verification. `accepted-bin/` retains the
production executables and DLLs; `accepted-project-v3/` and
`accepted-project-release/` retain small sources/packages. Recipes, logs,
`assertion-receipt.json` and native captures provide the supporting evidence.
No production map rebake, main merge or release is part of this phase. Published
main/v0.13.0 remain `b24361755e15c1a96d5b56d115499b9d4a9ef07f`. Full cross-platform
CI/version closing remains later; the local receipt records the observed branch
CI state when custody is released.


## Phase 5: circuits, compiler investigation and local lighting preview

The contextual Light circuits sheet selects one or many fixtures, supports
viewport painting, chooses an existing fixed prop or creates the existing wall
switch asset, and assigns/unlinks in one source undo step. Fixture IDs and group
names are managed internally. Reassignment, member/switch deletion, undo/redo and
save/reopen use the existing validated document/history paths. Duplicate member
light actions are adopted while sibling event actions survive. Existing generated
flicker ownership is retargeted atomically; conflicting/custom ownership receives
the existing explicit refusals. There is one generated controller per pattern.

The native switch gates power ahead of existing fixture/circuit updates. Flicker
can update its requested pattern while power is off without relighting the room.
Full uses the existing switched direct/indirect layers, fixture emission and
prepared irradiance field. The simple authoring default requires Full, retains
Static's authored state, and preserves the eight switched-source ceiling. Exact
advanced Static switching keeps its existing limits. No face-only runtime
substitute, compatibility layer or separate circuit baker was introduced.
The quick editor off/on view is private source preview and visibly distinguished
from game behavior and final transport.

The room/global-region preview reuses the preparation worker, compiler geometry,
transport, visibility rules and atlas fill. It keeps all emitters/occluders and
limits receiver charts to the footprint plus a 4 m halo, with at most 4 texels/m,
one bounce, 16 gather samples and 262,144 texels. Preview architectural spans cap
at 2 m before allocation; moving-object probes are omitted. Indirect energy from
beyond the halo remains approximate. The persistent NON-FINAL footer exposes
real stage/count progress, cancellation and clearing; edits/undo invalidate and
stale generations cannot publish. No package/cache writer is used. Large source
geometry may still exceed the shared preparation budget and report refusal.

### Native acceptance

The exact frozen production editor/compiler/game passed 95 assertions across
the native circuit lifecycle and room/region preview flows. Captures were visually
inspected at compact, normal and wide sizes. Ordinary E input targeted the actual
second switch for three off/on cycles in both one-light and two-light cases.
Off holds span flicker periods, outside/unlinked fixtures stay on, and no script
diagnostics occurred. Full lightmaps were resident. The crate/floor illumination
and real shadow changed with the prepared contribution, beyond the fixture face.
The many-light floor/crate capture mean changed from 63.23 to 0.19 display-code
units; the single-light off view retained 40.39 from the unaffected fixture.
These are functional visual witnesses at 960×600 with Fifo, not FPS measurements.

The room preview solved 17,089 texels in
901.5 ms and became ready in
1499.6 ms. The region solved
17,057 texels in 897.5 ms.
Actual progress included a partially completed direct receiver pass. Start/cancel,
repeat, edit invalidation, undo, clear and reopen passed. Nine source/package/cache
files remained byte-identical across the entire preview sequence. A subsequent
source edit and normal Play performed a final export and admitted it successfully.
The full-scene direct-shadow CPU regression also requires bit-identical selected
receivers after scope restriction, with all scene emitters and triangles retained.

### Matched compiler measurements

Before is the retained phase-4 compiler at `12018cad8a52ad19002c5807281894b2f16dcbdc`;
after is the frozen phase-5 compiler. On the same Ryzen 9 5900HX / RTX 3070 Laptop
Razer, Balanced power plan, each bounded fixture has three alternating fresh-process
forced Static/Full runs at 12 workers, with independent state/config/cache roots.
No build, editor, game or other campaign ran concurrently. Diagnostics were excluded
from these timed cohorts. Values below are medians; CPU is process user+kernel
time and memory is peak working set. Earlier build-contended exploratory worker
sweeps are retained separately and do not establish gains or an optimal worker count.

| Bounded source | Wall seconds before → after | CPU seconds before → after | Peak MiB before → after |
|---|---:|---:|---:|
| `material_transport` | 15.047 → 15.146 | 98.234 → 98.016 | 239.9 → 239.8 |
| `architecture_after` | 47.596 → 47.947 | 463.766 → 464.594 | 285.6 → 285.2 |
| `circuit_range` | 20.133 → 18.823 | 198.453 → 184.156 | 217.8 → 217.8 |

The circuit fixture derives from the native two-room scene: two near members and
one independently prepared outside circuit, no unswitched emitters, and bounded
fixture ranges. It targets real empty-channel and out-of-range work. The material
and architecture fixtures retain accepted general lighting workloads. Do not
extrapolate a circuit-specific gain into a universal bake speedup.

All nine pairs have exact physical payload equality: geometry, collision,
navigation, lighting metadata, HDR lightmaps, irradiance and reflection-probe mips.
Only the checked compiler code/executable identity, corresponding manifest input
record hash and compiler/lighting provenance fingerprints differ. No density,
taps, bounces, gather budget, floating precision, occlusion predicate or solver
quality revision was relaxed. Each cohort also remains deterministic across repeats.

The landed CPU changes skip an exactly empty direct channel, reject footprint
refinement only when a conservative spatial bound proves no emitter reaches it,
and omit visibility tests whose unoccluded support cannot improve the current max.
The implementation retains the exact accepted CPU visibility and shadow path.

| Source | Direct visibility seconds before → after | Indirect seconds before → after | After reflection capture s | After compression/publication s |
|---|---:|---:|---:|---:|
| `material_transport` | 4.003 → 4.030 | 4.512 → 4.581 | 5.338 | 0.051 |
| `architecture_after` | 17.005 → 17.190 | 21.639 → 21.521 | 5.952 | 0.121 |
| `circuit_range` | 4.730 → 3.572 | 13.013 → 12.903 | 0.000 | 0.063 |

Material transport has 1,498 triangles but 201,642 receivers, approximately
73.7 million counted direct rays and 25.8 million nominal bounce slots. Architecture
has 8,286 triangles, 470,706 receivers, approximately 178.8 million direct rays and
60.3 million nominal bounce slots. Footprint/support, alpha transport and gather
connectivity add work beyond these nominal counters. Low polygon count does not
bound receiver/sample cost. Stage timers separate geometry/receiver setup,
visibility, gather/filter/fill, GPU reflection capture and final archive costs.

A separate final-compiler diagnostic transforms 32,768 deterministic
queries from the actual material scene, without replacing its bake samples.
Receiver creation took 217.673 ms, spatial-cache construction
7.580 ms and direction transforms
0.787 ms. Timed kernels include allocation,
scheduling and result collection; gather includes its connectivity rays, so these
microkernel times are not additive whole-bake percentages or isolated hardware
instruction costs. Worker active-wall logs expose distribution/stragglers rather
than falsely labeling wall time as worker CPU.

| Workers | Nearest alpha-aware ms | Finite opaque/MASK ms | Gather stencil/connectivity/sample ms | Accumulation ms |
|---:|---:|---:|---:|---:|
| 1 | 9.763 | 8.261 | 25.852 | 0.035 |
| 12 | 2.985 | 1.958 | 5.650 | 0.024 |

The finite diagnostic segment goes from a real receiver to an emitter centre,
not to the bake's offset area-light samples; all 32,768
segments were blocked in this set. The nearest/gather set had
17,371 hits. Both worker budgets produced identical
hit counts and accumulated checksums. These bounded query distributions are
reported explicitly and do not represent the entire direct-light workload.

The primary retained triangle/hierarchy capacity estimate is
617,120 bytes, receiver capacity
14,361,600 bytes, and one accumulator layer
12,098,520 bytes. These exclude many textures, atlas,
driver and transient allocations; the process peak above is the actual whole
process observation. Compression/publication is a small cost on these fixtures.
Sampling/traversal/gather and, on the small source, reflection capture deserve
attention before archive work. No production map was blanket-rebaked.

### GPU feasibility

The retained wgpu 30.0.1 DX12 opaque f32 BVH microprototype was rerun against
16,384 fresh real-scene rays (1,496 opaque triangles,
2 nonopaque triangles excluded). Device setup
took 467.175 ms, packing/upload/pipeline
32.741 ms; dispatch/copy/synchronize/readback
times were 11.511, 0.918, 0.371, 0.345, 0.329 ms.
The CPU reference took 1.735 ms at 12 workers.
There were 0 differing classifications in this bounded replay.
GPU inputs used 642,800 bytes; the reported adapter storage-binding
limit was 2,147,483,644 bytes, not a portable device
allocation guarantee. The snapshot, exact prototype
source/lock/binary hashes and adapter report are retained outside target.

This experiment covers opaque any-hit queries only. The accepted baker also
needs alpha/MASK/BLEND/water transport, f64 watertight triangle decisions/canonical
ties, footprint support, nearest hits, radiance-cache connectivity, gather,
per-bounce updates, probes and deterministic output. Earlier Pool replay produced
151 f32/CPU classifications differing among 16,384 queries, mostly finite
endpoint/precision cases; that historical rejection still matters even if a new
small fixture agrees. It did not demonstrate 151 final-image errors.

In the pinned API, [SHADER_F64](https://docs.rs/wgpu/30.0.1/wgpu/struct.Features.html#associatedconstant.SHADER_F64)
and [EXPERIMENTAL_RAY_QUERY](https://docs.rs/wgpu/30.0.1/wgpu/struct.Features.html#associatedconstant.EXPERIMENTAL_RAY_QUERY)
are Vulkan-only capabilities, so neither supplies a portable DX12/Metal drop-in
for the current f64 CPU contract. The compiler currently uploads capture data
for reflections but owns no persistent GPU transport/radiance-cache backend.
Initialization could be amortized or share that device in a future design; it is
not an unavoidable cost of every GPU architecture. A useful future prototype
would keep the solve resident and use conservative GPU candidates plus exact CPU
fallback for ambiguous boundaries, with cancellation, memory limits and whole-map
quality/occlusion parity on each backend. Production remains the measured CPU
path. No fragile GPU backend, GPU acceleration promise or iGPU campaign landed.

### Gates, evidence and limits

Final format, strict debug and release workspace/all-target/all-feature Clippy,
locked release workspace/all-feature build, 376 Rust tests and the production
Luau exception-boundary verifier pass. The 369 Python source checks pass with
18 expected skips; 644 authored assets validate with zero warnings and unchanged
prop advisories. Three narrow deterministic regressions protect native circuit
power/flicker, atomic membership/source history and full-scene scoped shadows;
ordinary tests bake no production map. The prior phase
[CI run](https://github.com/csd113/Places/actions/runs/40668479262) completed
successfully. Phase-5 remote/CI/cleanup observations are in its custody receipt.

`local/verification-0.13.1-lighting/validation-receipt.json` is the concise index;
`final-bin/`, `final-binary-identity.json`, native source projects/packages,
recipes/captures/traces, `native-assertions.json`, `preview-isolation.json`,
`paired-results.json`, pair quality reports and `performance-analysis.json`
retain the exact evidence. Accepted artifacts and supporting helpers are sealed
by a SHA-256 inventory outside target before Cargo cleanup. Actual Mac validation,
full production rebaking/cross-platform closing and final FPS attribution remain
later. No HVAC/assets, prefab, runtime water/stair/draw-distance work was absorbed.

## Phase 6 audio groundwork (assets blocked)

The independent audio portion is implemented. Fixed props whose catalog entry
explicitly carries the existing `hvac-duct` tag supply transformed acoustic
envelopes. The player selects one nearest envelope for the entire family, using
a 0.75 m near distance and squared falloff to an 8 m cutoff. Duplicate modules
cannot stack voices. Catalog dimensions follow the rendered scale and
yaw/pitch/roll, independently of collision-size overrides. A changed nearest
duct moves the anchor directly to the real envelope; gain fades smoothly.
World reset and map changes clear all derived duct anchors and voices.

The player reserves `ambience_hvac` for this owner, so existing standalone and
prop-attached room/vending requests cannot produce global or duplicate hum.
The standalone sound picker excludes that automatic bed; new Audio placements
default to positional `empty_room`. Other audio controls and cue files retain
their existing contracts. No production catalog entry is tagged yet: existing
maps therefore have quiet HVAC until the actual duct assets are adopted.

### Independent Razer acceptance

Formatting, strict debug/release workspace/all-target/all-feature Clippy and the
locked release workspace/all-feature build pass with Rust/Cargo 1.99.1.
All 377 Rust tests pass serially. The first parallel run had one existing bowling
Luau wall-time-budget failure; that exact case passes in isolation and the full
serialized rerun passes without changing budgets or scripts. The new tiny CPU
control protects six horizontal/vertical rotations, elevated floors, collision
override independence, duplicate candidates, untagged props and empty rebuilding.

The source Python campaign ran 369 checks with 18 expected skips. Its three
packaging cases initially reported six subtest/process errors because this
process lacked `sh` on PATH. Their focused six-test module passes (three expected
skips) using the already-installed Git shell on process-local PATH; no host
setting or test changed. All other source cases passed. Asset validation reports
644 assets, zero warnings; audio, texture and prop checks pass with unchanged
texture/prop preferred-budget advisories. No imagery, model bytes or budgets
were changed.

Two real native Windows/Dx12 runs pass 15 assertions using the unchanged crate as
an explicitly labelled stand-in and a private catalog tag. All three tiny
Static control packages pass integrity/current-source verification. Native
observations establish a single spatial voice near the stock, reduced gain at
distance, no far voice, restart on return, one voice for four overlapping
90-degree stock placements, and clearing after loading the source with no stock.
Three obsolete global emitters stay silent without stock. Captured no-stock
post-mixer PCM has exactly zero RMS; the mixed locality run has RMS 0.0036864.

Both runs report zero mixer/device errors. The developer WAV queue discarded
startup frames before the first observation (58,112 / 61,696); the locality
counter stays unchanged across every measured step. Captures prove actual stored
post-mixer samples, not an uninterrupted recording or human listening perception.
The retained native screenshot was inspected and shows the existing crate.
These 960x600, Fifo observations make no FPS or delivered-HVAC visual claim.
The accepted executable/DLL set, source snapshots, control sources/packages,
recipes, WAVs, traces, initial failed runs and hashes remain under
`local/verification-0.13.1-hvac-audio/`. The receipt records commit/remote/custody.

### Bundle transfer blocker and remaining work

The approved Library bundle
`libfile_df3ab6e82cb081918279130cc6c4cb55`,
`Places_0.13.1_Utility_HVAC_Cloud_Assets.zip` v0 (45,317,941 bytes), has expected
SHA-256 `97f78cef00a4a234fd1b89a3c3613238ce5f19b30f624a6665f22044754119ae`.
The current supported Library helper downloaded on retry but failed before
atomic installation at `os.setxattr`, which Windows Python lacks. There is no
installed WSL distribution or Docker/Podman runtime. No installation, security
change, patched/no-op metadata, raw URL download or further transfer retry was
attempted. This is an unavailable supported transfer route, not an approval
denial. No consumer-local archive exists and its hash has NOT been verified here.

The smallest unblock is a supported Windows-compatible Library materialization
preserving identity/metadata, or an available supported executor-local Linux
route. The cloud-authored models must not be remade on the Razer.

Actual bundle QA pixels, import/catalog/material/collision integration,
connector/snapping/joins, functional doors, fixture/circuit anchors, complete
editor placement/preview/Play/save/reopen validation and source/export acceptance
remain undone. Existing `janitor:janitor_cart` and supplies remain registered
without duplicates; their phase-6 native visual confirmation is still pending.
No new accepted-model ledger entry or editable Blender/GIMP master was adopted.
Measure the installed plus new unique decoded images before choosing the
authorized pack-budget increase. Phase 6 remains incomplete.

Phase 5 GitHub CI run 40681947328 completed successfully. Production maps,
protected main/v0.13.0, host settings and later queued work remain outside this
independent audio change.
