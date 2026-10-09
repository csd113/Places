# Integration contracts and verification method

Audited October 8–9, 2026. [Stage 7](README.md) completed with the historical Geometry 7/solver 16 contract below. Current source currency is Geometry 8/solver 17; Full planning max 11 pages (Demo two-group switch budget 10), Medium max 8. [Verification](../../VERIFICATION.md) is authoritative for current guards and commands, and the [separate model-lighting correction](../../model-lighting-root-cause-and-fix.md) explains the later changes. Reader compatibility, currentness, planning capacity and safety ceilings remain distinct.

## Ghost collider root cause and correction

The Stage 3 exact audit established that
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

The focused run executes
`cargo test --lib --all-features geometry_check::tests`, exit 0, 28 passed,
zero failed/ignored, in 89.89 s total. The log
includes the new regressions. Strict Clippy and the workspace gate subsequently passed as recorded in [final Stage 7 acceptance](README.md). The correction changes validation only;
prepared meshes, collision and lighting bytes do not require a new geometry or
solver revision.

## Historical Stage 7 formats and compatibility boundaries

| Contract | Stage 7 publication source and reader behavior |
| --- | --- |
| Authored level / catalogue / package | Level schema 3; catalogue schema 2; package major 1. |
| Geometry / physical solve | Geometry revision 7; solver revision 16. |
| World mesh / collision / navigation | PLMW v 2 / PLCL v 2 / PLNV v 1; older incompatible record revisions are rejected. |
| Props | PLMP v 6 shares exact vertex/frame bytes with unchanged decoded attributes; literal v 3/v 4/v 5 readers retain their documented defaults and caster semantics. Encoded and expanded literal records remain bounded at 512 MiB. |
| Legacy vertex lighting | Lighting record v 2. |
| Lightmaps | Cache/content-key format 13; metadata v 3 with linear mean and signed moment RGBA16F KTX 2 layers; incompatible metadata v 2 is rejected. |
| Irradiance field | PLPF v 3 with aligned local-direct decomposition; v 2 remains a legacy reader path without that decomposition. |
| Reflection images | Positions v 3 and HDR RGBA16F KTX 2 captures/mip chains; positions v 2 and legacy display RGBA8 probes decode once into linear, without recovering already clipped energy; positions v 1 is rejected. |

Source anchors: `src/level.rs`, `src/assets.rs`, `src/package/`,
`src/lighting/bake.rs`, `src/lighting/probes.rs`, `src/lighting/transport.rs`,
`src/render/common/mod.rs` and `src/compiler/probes.rs`.
Reader compatibility is distinct from currentness: an older accepted record
does not establish that a package matches the current source/catalogue/tool.

Stage 4's solver-15 zero-source repair remains intact. Every solved v 3 probe field
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

At Stage 7, Full planning used ten pages and lower profiles eight; current planning caps are stated above. Planning capacities remain distinct from decoder safety ceilings and the 24-million generated vertex limit from art budgets.
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
The completed staging fixture supplies exact installed closure/catalogue; runtime guards remain intact. Final content, recovered-source and new-chair adoption pass [Stage 7 acceptance](README.md).

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

The normal player inspected at this checkpoint embeds the complete demo. Dead stripping from the compiler avoids a presumed circular dependency; the completed normal gate verifies this through an actual C1/C2 rebuild and digest comparison rather than treating byte inspection alone as proof.

## Completed verification order and acceptance method

The normal gate builds release compiler C1, checks assets/generators, migrates the recursive source inventory, then rebuilds ordinary compiler/player C2 after refreshing the embedded demo. C1/C2 compiler identity, required-current verification, dependency closure and unchanged safe reuse must agree. Frozen normal and diagnostic binaries retain separate provenance. Changes to source, catalog or compiler require new compatible inputs.

The complete locked Rust gate and Python discovery follow that build/package order, with no `PLACES_SKIP_SMOKE` or `PLACES_SKIP_PACKAGING` substitutions. The maintained capacity generator preserves its 48-model witness, 5,312 props, 2,760 explicit solid placements, 120 lights and 18 routes under catalog growth; exact serialized-fixture and collision-pressure checks remain required. The optional synthetic sibling archive is not claimed executed when absent.

A normal native inventory replay requires nonzero actual scene submission and matching GPU-ready/scene-presented identities for all 48 supported source paths. Geometry replay additionally exercises the invalid Arc control and requires its exact intended errors. Missing display/binary or platform limitations must be recorded, rather than accepted as native passes. Same-camera normal/diagnostic `final` comparison requires ordinary exposure, grade, bloom, sky, decals and effects. Animated differences require qualified actual payload and pixel-region evidence; [Stage 7's Home conclusion](README.md) retains the missing pose/time qualification.

Live quality checks require requested/applied/resident agreement, selected prepared-package availability and genuine encoded draws/indices. An archive that decodes after losing its previously available atlas/field is rejected as a lighting fallback regression. Capture submission and successful loading remain distinct from physical display cadence and ordinary gameplay FPS. Current reusable commands and inputs are maintained in [Verification](../../VERIFICATION.md).

## Typed atlas-record safety

Demo's historical ten-page, two-group Full atlas produces forty RGBA16F layers: 320 MiB of GPU texel payload and a 335,544,516-byte KTX2 record. Its role-specific allowance is 320 MiB plus 64 KiB; unrelated entries retain their ordinary 256 MiB bound, mesh/prop records 512 MiB and aggregate packages 1 GiB. The exact KTX2 header, layer counts and paired irradiance/moment shapes must validate before allocation. This measured exception is not a generic raising of every record limit or permission to silently omit an atlas.
