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

## Phase 6 utility kit, duct joins and local HVAC audio

Phase 6 is integrated and accepted on the Razer. The 25 cloud-authored Blender
4.3.2/GIMP 3.0.4 models retain their original GLB and native 256-square atlas bytes.
They total 11,764 triangles, 156–1,464 per model. The catalog adds 25 prop entries
and five operational fixture entries, reaching 674 assets / 417 placeable entries
across the existing ten themes. Existing janitor cart, supply box, two bottles,
mop bucket and mop are reused byte-for-byte, with six native inspection captures.

The final public-GitHub transfer was explicitly authorized after disclosure.
All 34 payloads / 100 ASCII transport parts were verified by length, SHA-256 and
Git blob identity. The repository preserves 31 original runtime/document inputs
and 38 original final editable/master/workflow/text-QA files. The consolidated
receipt is `f75d7f760c3b5b7b90dd60880a1fcb9d1e17d5a0`; editable ZIP SHA-256 is
`7af21156eae3bddc850f1c531641b9fea24993eb56c610a326af1d7bb50b970`.
Original source and per-file receipts live in `tools/props/backrooms_phase6/`.
The complete final contact sheet and hollow-elbow close-up were inspected as
actual pixels before acceptance. They remain outside runtime assets, alongside
native evidence. No models were remade on the Razer; the larger original Library
bundle and early construction history remain in their original Library records.

### Placement and runtime integration

Simple and Advanced modes expose contextual Duct joins for the six HVAC models.
Green A/pink B guides show outward normals. Six direction controls and ordinary
pitch/roll allow either horizontal direction and vertical up/down. Explicit
same-scale joins oppose normals, match rectangular aperture orientation, and
meet actual outer flange planes, including projecting elbow stock. Preview
frames the proposed source/target; Apply is one source pose edit. Cancellation,
undo/redo and save/reopen retain their ordinary document boundaries. Targets are
bounded to 32 nearest visible fixed duct ends; moving/floating/fitted poses are
refused. These decorative 0.6 x 0.4 m ducts are separate from crouch tunnels.
Their collision is the documented conservative transformed box.

The Add to scene sheet now exposes static catalog prop creation. Catalog model
changes capture measured size/solid defaults. The non-release showcase captures
all prop collision sizes explicitly and demonstrates all 25 variants, a joined
six-piece run, five circuit fixtures and reused maintenance stock. Ceiling grids
and passive fixture props are nonsolid. Doors are labeled Static Model; the
existing Add Door tool remains the interactive-leaf workflow. Speaker and clock
are passive/fixed scenery. See the kit's `INTEGRATION.md` for placement details.

Five Add Light variants reuse the delivered fluorescent GLBs through existing
mounting, Circuits, switches and Full lighting. Measured model-top offsets also
drive picking and canonical prepared placement. Luminous planes lie at the actual
lens/tubes; only material slot 2 emits, leaving housings, labels and damaged
diffuser fragments passive. Each owns a distinct 2:1 fallback PNG path containing
exact existing fluorescent-face bytes; normal rendering uses the original GLB
atlas. Passive prop versions are labeled explicitly to avoid overlapping housings.

Automatic `ambience_hvac` is owned by fixed catalog props tagged `hvac-duct`.
One nearest transformed envelope supplies one spatial voice, with 0.75 m near
distance, squared falloff and an 8 m cutoff. Adjacent/duplicate stock cannot stack
voices. Collision-size overrides do not enlarge the acoustic envelope. Anchors
jump between actual ducts instead of sweeping through empty space; gain smooths.
World resets/transitions clear the cache and voices. Old global/authored requests
for this reserved cue are suppressed. The authoring viewport remains silent;
Play uses the runtime owner. New generic Audio placements use positional
`empty_room`, and the chooser excludes the automatically owned HVAC cue.

### Measured budget and acceptance

The new atlas adds **0.25 MiB unique decoded RGBA**, or 6.25 MiB when summed over
25 embedded copies. The pack check reports 89.434 MiB by exact PNG identity,
conservative over decoded-pixel sharing. The authorized desktop cap is 128 MiB;
per-image and geometry safeguards remain. No delivered texture was downscaled.

- Formatting, strict workspace/all-target/all-feature Clippy and the locked
  release workspace/all-feature build pass. All 379 Rust tests pass serially;
  the two small new tests protect all-axis rectangular joins/history and measured
  fixture mounting/persistence without baking a shipped map. Python verification
  passes 369 tests with 18 documented smoke/optional skips. Packaging checks use
  the installed Git shell in the process PATH, without changing host settings.
- Catalog validation reports 674 assets and zero warnings. Prop/texture checks
  pass with existing art advisories retained. All 69 original imported files
  were hash-checked again after integration; reused maintenance GLBs match the
  pre-import branch byte-for-byte.
- Native editor capture covers every new variant. Ordinary input exercises six
  orientations, framed joins, both editor modes, preview/cancel, atomic apply,
  undo/redo, catalog creation/removal, fixture grouping and source save/reopen.
  Checkpoints verify that previews do not mutate source. Build/Play returns to
  editing normally. Full native rendering records all five circuit states on,
  then off, then on, plus material/fixture, hollow stock, joined-run and shadow
  captures. A measured-box control stops the standing player at Z=0.72052 and
  allows walking under overhead stock to Z=-1.82916.
- Native actual-duct audio passes 15 checks: one spatial near voice, lower middle
  gain, stopped far voice, restart on return, one voice for four rotated duplicate
  modules, and silence after removal/map transition. Three obsolete global HVAC
  requests in the no-pipe control produce no voice and PCM RMS 0.0. Locality PCM
  RMS is approximately 0.003713. Mixer/device errors are zero. Startup capture
  queues dropped 45,312/45,824 frames; these are real retained post-mixer samples,
  not a continuous-recording claim. Human listening remains unverified.
- Supported project export/import, package validation/currentness and recovered
  project checks pass with identical source semantics (38 props / 11 lights).
  The recovered project reopens natively with measured collision boxes and
  unchanged maintenance assets. Export archive SHA-256 is
  `5022f7f234dbd0ba42bf5601a10da9504b5e634da22995cd484a96602dac1619`.

The canonical local receipt and retained binary/DLL set, exact source snapshots,
recipes, packages, checkpoints, WAVs, images and hashes are under
`local/verification-0.13.1-utility/`. Earlier independent audio groundwork remains
under `local/verification-0.13.1-hvac-audio/` at commit `210d44b`.
Required evidence is retained outside `target` before cleanup.

For the separately requested responsiveness work, no Rust compiler remained when
the priority instruction arrived. Remaining compiler-only helpers used two
workers and explicitly requested BelowNormal; long-running compiler priorities
were observed as BelowNormal. Native game/editor priority was observed as Normal.
Fast compiler commands sometimes exited before sampling. This is per-process
scope, with no persistent host policy or observed later Rust-child inheritance.

Phase 5 CI run 40681947328 previously passed. This acceptance is native Windows
Razer evidence; new native Mac validation and human audio perception are not
claimed. Fifo captures make no FPS comparison. Production rebaking, later prefab
and runtime/FPS work, main merges and release publication remain outside phase 6.


## Reusable authoring tools (phase 7)

Reuse tools is visible in both Simple and Advanced modes. Its three tabs share
private scene previews, explicit scope, cancellation, ordinary selection, source
validation and one atomic undo/redo transaction. Repeated stamps advance their
destination; preview never selects or saves an uncommitted stamp. Save/reopen and
supported project source export/recovery preserve all authoring metadata.

Furnished rectangular room/corridor sources retain contained walls, openings,
recesses, surrounds, trim, pillars, fixed props, doors, fixtures and local tunnels.
They reuse the real catalog and shared geometry/circuit systems. Each stamp remaps
internal entity identities and native circuit groups. Sources are map-local;
linked updates are explicit three-way merges with field overrides, coherent array
overrides, removed-field/deleted-member tombstones and safe member removal.
Missing sources retain all placed geometry and permit explicit detach. Ordinary
Duplicate drops linked identities. Cross-name source recapture keeps member keys.
Carrier geometry overrides refuse updates until restored or detached, avoiding
child updates in an obsolete carrier frame. Unsupported cross-room, moving,
fitted, script-owned or ambiguous content refuses with a specific explanation.

Office, Worn and Home styles independently cover walls, floor, ceiling, existing
trim and ceiling fixture family. They retain geometry, openings, furnishings,
power and circuits. Only inward wall faces change; shared boundary trim stays.
Resolved fixture locations are pinned before ceiling-grid changes. An implicit
catalog trim topology change refuses with advice to use explicit trim or disable
Walls, preserving the prepared trim geometry.

Controlled prop variation uses an unsigned seed and a saved original pose for yaw,
uniform scale and sinking. A/B/A replay is exact, without accumulated edits;
Restore base retains original field presence, and Keep current base deliberately
adopts a manually changed pose. Active, fitted and circuit props are counted and
skipped. Optional standing-clearance protection uses the shared collision index,
real collision boxes, floor support, ceiling headroom and authored door pose with
the 0.4 m radius / 1.8 m standing hull. A newly blocking choice retains its base.
The bound is 0.2 m witnesses plus exact spawn, at most 50 by 50 m / 65,536 witnesses;
it is not continuous or dynamic route proof. No whole-layout generation was added.

### Final Windows acceptance

- Formatting, strict workspace/all-target/all-feature Clippy in debug and release,
  `cargo test --workspace -- --test-threads=1` (382 passed) and
  `cargo build --locked --release --workspace --all-features` pass. Three tiny CPU
  regressions protect linked identity/merge/history/persistence, geometry-preserving
  room scope and deterministic clearance-aware pose replay. They add no map bake
  to ordinary tests. The production Luau release boundary check also passes.
- Python source verification ran 369 tests with 18 expected smoke/optional skips.
  Asset validation passes 674 assets / 417 placeable entries / ten themes with
  zero warnings. Texture and prop checks pass with existing art/manifest advisories.
  No visual asset bytes or production map sources/packages changed.
- Exact-release native editor acceptance passes 59 checks across 392 actions.
  It covers private previews/cancel, repeated stamping, linked versus independent
  identity, atomic undo/redo, source recapture, per-instance override preservation,
  explicit/idempotent updates, carrier-geometry refusal, missing-source detach,
  save/reopen, locks/hidden scopes, A/B/A seed replay, base restore/adoption and
  protected/unprotected choices. Compact 960x544, tall 1000x900 and wide 1440x900
  windows were inspected at Windows 150% display scale in both editor modes.
- The accepted four-room fixture has 28 real utility props and eight fixtures,
  one source, two linked instances and one independent stamp. Static/Full project
  export/import, validation, currentness and recovered-source checks pass with
  identical source semantics. This opt-in small fixture was the only furnished
  room bake; it did not rebake a shipped map.
- Native Full gameplay passes ten checks: the source and linked switches independently
  gate their own fixture pairs; untouched circuits stay unchanged. The standing
  player walks the varied room at X=4 from Z=4.80000 to 0.60002 and returns to
  5.23580 without crouching or recovery. Runtime script diagnostics are empty.
  Actual Full rendering was inspected for furnishings, changed surfaces, fixtures,
  shadows and walkway. The four-room fixture's door leaves are passive scenery;
  a separate tiny native self-bound door witness passes four checks, opening the
  stamped door to 90 degrees through normal interaction while the source remains
  closed. Its Static/Full package passes current-source verification.

Sources are bounded to 32 names / 512 members each, linked instances to 128 and
variation scopes to 128 fixed props. Prefabs translate X/Z and retain elevation;
unsupported geometry or external bindings refuse rather than leave broken copies.
These implementation bounds and the sampled-clearance limit are documented in
[EDITOR.md](../EDITOR.md#reuse-tools-room-prefabs-room-styles-and-prop-variation-0131)
and the canonical map guide. Existing Safety/Play remains the route review tool.

Compiler-only tasks ran BelowNormal with observed Cargo/Rust child inheritance;
native game/editor processes were Normal. Small map builds used two workers,
without competing builds or persistent host/security/power/GPU changes. Native
runtime used Dx12 on the Razer's RTX 3070 Laptop at 960x600 with resident Full and
Fifo presentation. These are visual/controller checks, with no FPS comparison.
Actual native Mac validation remains unverified for the later cross-platform phase.

The retained acceptance tree is `local/verification-0.13.1-authoring/`:
`validation-receipt.json` indexes the final gates, native assertions and source
round trip; `accepted-bin/`, `binary-identity.json` and `sha256.json` retain exact
binaries, DLLs and hash evidence outside `target`. Earlier exploratory/interrupted
runs are explicitly superseded. `custody.json` records the phase commit, verified
remote SHA, exact-SHA CI observation and final idle/clean/cleanup observations.
Main, v0.13.0, phase 8 runtime fixes and release publication remain outside phase 7.
