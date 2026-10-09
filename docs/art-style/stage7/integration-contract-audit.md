# Stage 7 renderer/compiler integration audit

2026-10-08 UTC. Entry: `Art-style` at
`26486b8538424f013c243ae6edea8720ac07d7f2`.
This report records source review and the bounded validator, roof and probe-placement corrections. It does
not substitute for the primary's final package, native, Rust or publication gate.
[Integration evidence plan](integration-evidence-plan.md) identifies those checks.

## Scope and custody

The renderer/compiler owner read `AGENTS.md`, the canonical authoring and asset
contracts, Stage 1–6 reports/contracts/performance, the deferred ledger and
[Stage 6 completion](../../../debug-maps/art-style-hero/evidence/stage6-completion.json).
Stage 6 released checkout/index/target with no active owned jobs. The primary
retains sole ownership of Git, Cargo builds, package preparation and native
processes. This specialist ran no build, bake, native process or Git mutation.
Source writes were frozen before the primary's focused test checkpoint.

Initial owned changes are `src/geometry_check.rs`, `docs/MAP_AUTHORING_GUIDE.md`,
`docs/ASSET_SPECIFICATION.md`, `docs/VERIFICATION.md` and these two integration
reports. No runtime shader, camera, level ID, lighting intensity, asset, collision
volume, navigation policy, package format or safety limit is changed by the
ghost correction. The subsequent recovered-review check below found a real
roof-bound issue and received a separate bounded source allocation.

## Ghost collider root cause and correction

The Stage 3 [exact audit](../stage3/ghost-collider-audit.json) established that
wall 2's small collision partition has real face coverage by a larger return-wall
triangle, with unchanged occupied collision union. `triangle_on_face` previously
required the supporting triangle's centroid to lie inside the box's face
rectangle. Independent mesh triangulation and collision decomposition do not
share that partition: a triangle may contain the whole rectangle while its
centroid lies outside. The centroid test therefore rejected legitimate support.

The new predicate requires all triangle vertices to lie within the existing
3 mm face-plane tolerance, projects the triangle into the face plane, translates
to a local origin, and clips against the actual rectangle. It requires positive
clipped area. The caller still requires the actual face centre to lie inside an
individual supporting triangle. Fragment bounds cannot bridge a real centre hole,
and a slanted crossing or nearby parallel face cannot impersonate coplanar support.
The existing spatial candidate index and six face-centre search are retained.

One supported face centre remains sufficient, as before. This conservatively
handles roof-following walls whose AABBs span differing closed-roof heights;
their sides may provide real support even without a horizontal cap at the box's
maximum height. The checker neither invents roof geometry nor equates this
heuristic with complete watertight coverage. A small centre patch still counts
as support; full-face holes and microscopic isolation require separate checks.

Five deterministic embedded regressions cover a containing triangle with an
outside centroid at both origin and 250 m offset; empty and unsupported partial
faces; disconnected coplanar fragments around a true centre hole; zero-area,
offset and slanted contacts; and the unchanged hero. The hero test requires the
ghost warning to disappear while its intentionally open decorative garden
retains `room-leak`. Existing roof, collision and checker regressions remain.

The primary's [focused receipt](execution/geometry-focused.json) records
`cargo test --lib --all-features geometry_check::tests`, exit 0, 28 passed,
zero failed/ignored, in 89.89 s total. The [log](execution/geometry-focused.log)
includes the new regressions. Strict Clippy and the full workspace gate remain
part of the primary's final campaign. The correction changes validation only;
prepared meshes, collision and lighting bytes do not require a new geometry or
solver revision.

## Current formats and compatibility boundaries

| Contract | Current source and reader behavior |
| --- | --- |
| Authored level / catalogue / package | Level schema 3; catalogue schema 2; package major 1. |
| Geometry / physical solve | Geometry revision 7; solver revision 16. |
| World mesh / collision / navigation | PLMW v2 / PLCL v2 / PLNV v1; older incompatible record revisions are rejected. |
| Props | PLMP v6 shares exact vertex/frame bytes with unchanged decoded attributes; literal v3/v4/v5 readers retain their documented defaults and caster semantics. Encoded and expanded literal records remain bounded at 512 MiB. |
| Legacy vertex lighting | Lighting record v2. |
| Lightmaps | Cache/content-key format 13; metadata v3 with linear mean and signed moment RGBA16F KTX2 layers; incompatible metadata v2 is rejected. |
| Irradiance field | PLPF v3 with aligned local-direct decomposition; v2 remains a legacy reader path without that decomposition. |
| Reflection images | Positions v3 and HDR RGBA16F KTX2 captures/mip chains; positions v2 and legacy display RGBA8 probes decode once into linear, without recovering already clipped energy; positions v1 is rejected. |

Source anchors: `src/level.rs`, `src/assets.rs`, `src/package/`,
`src/lighting/bake.rs`, `src/lighting/probes.rs`, `src/lighting/transport.rs`,
`src/render/common/mod.rs` and `src/compiler/probes.rs`.
Reader compatibility is distinct from currentness: an older accepted record
does not establish that a package matches the current source/catalogue/tool.

Stage 4's solver-15 zero-source repair remains intact. Every solved v3 probe field
carries the local-direct vector even when there are no always-on local sources,
with coefficients aligned to slots and correctly zero. Writer and reader reject
nonzero energy with zero source count, malformed lengths and inconsistent
labels/visibility. Empty always-on terms cannot erase the residual field or
switchable live contribution. No compatibility shortcut is added in Stage 7.

## Shared rendering and resource contracts

Colour PNGs decode from sRGB; numeric normal/mask/alpha data remains UNORM.
Authored tint and lighting coefficients are linear. The stored atlas is incident
illumination, excluding the receiver's albedo. Architectural PNG mean reflectance
and tint enter diffuse gathers, without receiver face shading. Scene, emission,
bloom, probe and planar resources use linear HDR RGBA16F. Shared fixed defaults
remain exposure 1, knee 0.75, saturation 1.03 and contrast 1.02; quality changes
resource budgets rather than silently changing authored exposure.

Static, rigid dynamic and skinned glTF routes retain OPAQUE/MASK/BLEND. Shared
stable back-to-front centre ordering covers world/prop/dynamic/character draws
in scene and emission. This is per object/range ordering, with limitations for
intersecting triangles and multi-primitive objects. Straight transmission,
coverage and emission stay consistent; coloured refraction, screen AA and
general tiny cutout mip preservation are not claimed implemented.

Runtime Low/Medium/High defaults select Off/Medium/Full lightmaps and reflections.
These advanced controls are independent of overall quality, texture filtering
and lighting profile. Medium/Full atlases use 1024 edges, densities 12/16 and
eight/ten pages per contribution group. Each produced page contributes two RGBA16F
layers per group; unused maximum-capacity pages are not allocated. Inclusive
endpoints plus gutters permit a 63.6875 m Full chart span. The legacy two-profile
API's 512-edge/10-density Low plan is not the ordinary Low runtime preset.
Reflection Medium/Full probes are 48/64 pixels and both allow the selected
half-drawable planar pass. Off has neither probe sampling nor planar capture.

Medium/Full panes and water use prepared charts; Off retains vertex lighting.
Dynamic receivers sample eight spatial anchors plus selected finite direct
sources, cached rigid triangle/alpha visibility (64 casters) and bounded skinned
proxy/contact terms. Static atlases do not react to moving actors/doors; fully
general entity bounce and selective global caster invalidation remain limits.
The Stage 6 isolated moving-receiver kernel measurement is not ordinary FPS.

Stage 5 optional environment defaults preserve the historical fog behavior and
shared presentation. Optional water attenuation is finite, 0..16, default zero;
straight throughput is `(1 - opacity) * exp(-attenuation * depth)`. This is
distinct from prepared receiver-depth RGB absorption and gameplay collision.
Weather movement/shelter, ice traction and water geometry stay unchanged. Existing
maps need no per-name rendering exception or obligatory artistic retuning.

The guide now distinguishes Full's ten-page and lower profiles' eight-page planning capacities from decoder
safety ceilings and the 24-million generated vertex limit from art budgets.
The 100,000 count is a prop limit, not a vertex budget. Historical platform/test
summaries keep their dates; macOS CI and Metal hero captures do not certify
unexecuted Linux/Windows native hardware or a gameplay frame-rate budget.

## Stage 6 currency, closure and cache audit

The final read-only review also records a pre-existing metadata sanity limitation:
`src/package/lightmaps.rs::validate` accumulates chart width plus height for its
coarse coverage ceiling. That guard does not prove rectangle area or disjointness.
The expression originates in pre-overhaul commit
[`713b93d` (2026-09-27)](https://github.com/csd113/Places/commit/713b93de70abfecff352632fa28de5cc9d817ed7)
and the file remains byte-identical throughout the preserved Art-style source
receipts. Individual rectangle edges, record/count/group/shape and aggregate
allocation guards remain enforced; no current packing failure was demonstrated.
Current planning's separate padded-reservation bitmap verifies actual disjointness.
The metadata guard alone is never cited as that proof. This existing robustness
limit does not justify changing the frozen production compiler during map integration.

Compiler collection includes every placed prop and every authored spawn template,
including unused model overrides; it resolves architectural albedo, normal and
emissive resources plus fixtures and sky through the catalogue. Embedded model
PNG bytes are covered by the containing GLB digest. A standalone prop source
image edit must regenerate the GLB before it changes the runtime dependency.

Revision-one `build-inputs.json` binds source and catalogue SHA, exact compiler
executable SHA and capture mode. Its declared role, revision, size and contents
must validate. Changed source/catalogue/tool, corrupt or missing declared
provenance and same-size physical dependency changes cannot qualify for exact
reuse. `--force` bypasses both reuse stages. Supported metadata/navigation/AI/
final-presentation changes can retain prepared products while refreshing the
semantic world, navigation, atlas metadata and provenance to equal clean output;
physical changes conservatively prepare anew.

Package open streams and checks declared installed dependency hashes, including
same-size PNG/GLB edits, and verifies catalogue provenance for new packages.
Legacy packages lacking the optional provenance retain their documented immutable
bundle path. Runtime does not require the source JSON or the compiler executable;
offline required-current verification does. The ordinary loader key is canonical
manifest identity plus prepared quality. No atlas-consuming mutable runtime
cache is introduced: transient `LightmapCache` ownership remains immutable inputs
or explicit caller clearing, and compiler builds start fresh resolved inputs.
There is no new per-frame file hashing.

The integration audit identified that `tests/list_levels.rs` package-layout
staging originally omitted installed external dependency closure and the
catalogue. An existing `assets` directory is selected by asset-root resolution,
so an incomplete staged root cannot rely on fallback to the repository catalogue.
This was routed to the primary/validation owner for an honest fixture repair;
runtime guards must remain intact. Content migration, recovered local sources,
new-chair Zoo adoption and reference receipts remain the content/primary owners'
work. No file in those write sets is modified by this specialist.

## Compiler identity and embedded demo dependency

`src/loader.rs` includes the demo archive for the player's fallback. A naïve
executable-hash dependency could be circular if merely rebaking that archive
changed the compiler tool used to prove it current. Read-only byte inspection
found the release compiler does not contain either the current entire archive
or its initial 64-byte prefix, while the player contains the full archive. This
supports release dead stripping; it does not replace a final rebuild experiment.

| Read-only inspected file | Bytes | SHA-256 | Demo occurrence |
| --- | ---: | --- | --- |
| `target/release/places-compile` at audit | 6,809,648 | `751c5153d138534232deb60d8e710b87f34f227c7bbbd6644133b04136ee3935` | Absent |
| Stage 6 retained `places-compile` | 6,809,648 | Same digest | Absent |
| `assets/levels/places_demo.placesmap` at entry | 74,520,351 | `01595d413f58f298d178d0b97be2e2dde8513a9537b10ce2006ba14b2615047a` | Input archive |

The normal player inspected at audit held the full demo at byte 6,250,846;
the retained Stage 6 player at byte 6,218,214. No speculative loader refactor
or weaker tool provenance was justified. Freeze Rust/assets/catalogue, build
ordinary compiler C1, refresh the demo with C1, rebuild ordinary player/compiler
C2, compare C1/C2 SHA, then require currentness and byte-preserving unchanged reuse.
If the digest changes, distinguish linker/tool changes from embedded archive
retention before considering a root-cause repair. Final normal and diagnostic
executables keep their own provenance; an old compiler is no exception to the
normal full-build workflow.

## Release to primary

The subsequent [recovered review audit](integration-recovered-review-audit.md)
found two Blizzard wall tops one f32 ULP below their actual ridge after interior
profile extrapolation. Exact occupied union and an exact body/support predicate
changed, so no tolerance waiver was accepted. A separately authorized repair
resolves each cut span's midpoint roof owner, evaluates both endpoints against
that fixed owner, and shares the resulting maximum across collision, geometry,
coalescing and validation. Unsupported generic profiles retain their existing
callback API/fallback. Geometry revision 7 invalidates prepared products.

The additional changed source files are `src/level.rs`,
`src/level/tests/wall_ceiling.rs`, `src/loader.rs`,
`src/render/common/mod.rs` and `src/render/common/architecture.rs`.
The existing geometry emitter already evaluates exact endpoints against the
span's owner and needs no separate `geometry.rs` change. Four new embedded
tests require exact ridge bounds, adjacent-roof maxima without borrowing,
constant/explicit bounds and the previous body/support predicate witnesses.
Their serialized Rust check and final package comparison remain the primary's
next gate. The earlier C1 campaign is preserved as superseded evidence.

The [Demo probe coverage audit](integration-demo-probe-coverage-audit.md) then
confirmed missing prepared support on the playable Home loop in preserved C1.
Room 7's saved walkable witness is 9.4766 m from its nearest valid probe, beyond
the unchanged 4.06094 m runtime support radius; room 8 also has unsupported
walkable centres. The source remained unchanged. A separately allocated bounded
phase repair moves the exact compiler air predicate into a shared helper and
checks at most 64 existing-grid phases. Already covered fields retain the
original phase; a changed phase must increase covered-room count and preserve
every originally covered room. Missing-room searches count every strict valid
probe in all 63 additional phases and compare covered-room count, then sorted
ascending room populations; an early boundary singleton cannot end the search.
Target preparation and bake share one chosen
layout, with exact position alignment checked before baking. Solver revision 16
invalidates earlier physical caches. PLPF v3, caps, radius, visibility and the
solver-15 aligned zero-source contract remain unchanged. Six new focused tests
cover exact/legacy placement, blocked and infeasible cases, the actual Demo
source witness and zero-source serialization. The test geometry uses real
architecture/materials with asset-less prop placeholders; fully resolved-asset
final field and native coherence still require separate acceptance evidence.
The retained first-strategy preflight-v7 failed the unchanged Demo runtime
witness and an incorrectly bounded synthetic fixture. The corrected strategy
and fixture passed the primary [focused-v2 gate](execution/probe-focused-v2.json),
21/21 tests in 15.00 s (44.54 s with build), including actual Demo source support.
Selected Demo room counts 7/10 are 16/4; all original room coverage is retained.
The primary [strict preflight-v8](execution/rust-preflight-v8.json) subsequently
passed format/check/strict debug+release Clippy and compiler5/roof16/geometry28
focused suites in 78.37 s. A [normal release checkpoint](execution/release-compiler-c1-v3.json)
passed in 96.05 s. The allocated obsolete Demo assertion cleanup and final source
freeze remain primary work; full workspace and final resolved-field/native gates
remain separate.

The separate [Pit lower-floor audit](integration-pit-probe-coverage-audit.md)
checks all 1,236 actual saved lower-floor walkable humanoid cell centres. Despite
no labels 16–19, smaller overlapping shaft rooms 20–23 own their actual air; every
centre has at least 12 clear, label-valid probes against saved PLLT2 solids.
No source migration or label/radius exception is justified. Final new-package
Rust sampler/native checks remain separate.

The additional probe source files are `src/lighting.rs`,
`src/lighting/probe_placement.rs`, `src/compiler/probes.rs`,
`src/lighting/transport.rs`, `src/lighting/transport/tests/probes.rs` and
`src/render/common/light_transport.rs`. They are frozen and released; the
primary owns formatting and serialized tests/builds. The primary also updates
current canonical solver/package documentation. Both prior C1 campaigns remain
superseded, with byte-verified preservation receipts.

Source and canonical documentation writes are released after the bounded repair.
The primary can format/check/build the shared target serially. The focused ghost
checker gate above passed; roof tests, remaining full checks, directed live quality/
map changes, all supported current packages, final identity/reuse and native review
belong to the primary's integration campaign. Historical receipts and seven
concepts remain immutable. There are no active specialist jobs or permission changes.
