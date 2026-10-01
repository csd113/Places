# Lantern Hollow showcase acceptance

Status: Lantern Hollow is baked, current, decoded and visually checked in the native app.
All three bundled packages are baked, current and decoded. Full Rust, strict lint,
release, source/package and GPU resource checks pass. Final presented-window
acceptance and steady-state frame timing are blocked by the locked Mac desktop.

## Workspace and launch

Persistent isolated checkout: `/Users/connordawkins/Documents/GitHub/Places-night-showcase`,
branch `agent/night-showcase`, based on integrated lighting commit
`50e5f758f8843f324927010bb04ef6a646ca5a83`. All commits are local; no push.
The original checkout, unrelated local edits and stash
`fa23032a1ce9324cf4657c9a8efffa5de9ffb1bf` are preserved.

Launch the current playable package:

```sh
cd /Users/connordawkins/Documents/GitHub/Places-night-showcase
PLACES_ASSET_ROOT="$PWD" PLACES_LEVEL=lantern_hollow ./target/release/places
```

Use the normal Level Select menu to choose **Lantern Hollow**. WASD moves,
mouse looks, E operates a door, Space jumps, and crouch enters the shallow pond.
The new default-off **Use Low-quality lighting** Graphics setting retains the
selected texture quality/filtering and restores lighting when switched off.

## Authored content

The actual attached 1536x1024 reference was inspected locally. Exactly four
walkable furnished cottages face the road with connected sidewalk/porches,
fenced gardens and pumpkins. Existing blue, cream, white and ochre home
modules/furniture provide modest variation. Five forest trails converge on a
rocky clearing with seven skeletons playing the existing seated pose around
an animated campfire. The left pond has stepped shores, water and safe exits.
There are 54 disjoint rooms, 1,076 props, 247 trees and 459 grass placements.
Visible rock faces and closed road gates contain walking and jumping. No cars.
The existing human sheet ghost and three repaired sheet-cat placements retain
the original characters; no rejected spectral-cat model is included.

The 14 new modular models are road_straight, road_unmarked, road_dash, sidewalk,
sidewalk_corner, curb, curb_ramp, sidewalk_transition, rock_face,
rock_face_variant, boulder, campfire, road_gate and stump_seat. Their catalog
IDs use `outdoor:showcase_`; their GLBs are under
`assets/environment/outdoor/props/models/`. Two existing 128px textures are
reused byte-for-byte. The road materials are normal albedo aliases, without
lighting brightness floors or emissive compensation.

Both existing sheet-ghost facial meshes conform to their animated cloth
surfaces with shared deformation joints and positive face clearance. Normal,
UV, accessor, loop-seam, topology and animation checks cover these outputs.

## Systemic static model lighting

Static GLB triangles now keep neutral albedo and receive the compiler's real
HDR visibility-tested surface lighting. Source NORMAL data is retained and
transformed by inverse transpose; absent normals derive from actual winding.
Source UVs remain independent from per-face atlas UVs. The normal transport
scene supplies architectural, model and self shadows to static receivers,
respecting each placement's existing authored occlusion flag. Animated ghosts, skeletons
and the fire model retain the established probe lighting path.

Architecture retains its MAXRECTS layout/density and two-texel gutter. Model
charts use deterministic disjoint guillotine pages in the same hard eight-page
budget. Small models use 8 texels/m; large/cutout models use 1 texel/m lighting
LOD. Every model chart retains one dilated texel for the renderer's verified
single-mip ClampLinear lightmap sampler. There is no painted brightness,
vertex-lighting disguise or increased atlas-page allowance. Solver revision 10
invalidates stale baked packages.

Explicit real-map planning passed:

| Map | Medium pages | Full pages | Charts |
|---|---:|---:|---:|
| Lantern Hollow | 6 | 7 | 230,083 |
| Model Zoo | 5 | 7 | 24,323 |
| Places Demo | 7 | 8 | 153,486 |

Final six-map-variant planning passed in 24.01s. Earlier instrumented Lantern
Hollow planning took 9.784/9.329s for Medium/Full. The final normal Off/Medium/Full sky bake took
323.78s with ten workers, produced 115,443,902 bytes, and returned no warnings.
Peak compiler RSS was 2,585,608,192 bytes and peak footprint 2,801,273,664 bytes;
there were zero swaps. Medium reports 76 pre-existing sub-texel architectural
slivers without atlas failure. The final package fingerprint and current-source
fingerprint both equal
`602f5cb311894b50b85e79d2625594c6a7f5f4c0c765571973cd53edd741eb67`.

## Interior lighting correction

The wall discontinuity was a receiver-coordinate defect. A gable trapezoid
was treated as a parallelogram, discarding its fourth corner. At one visible
wall sample this moved the baked receiver 1.41 metres away from the rendered
surface. Charts now use exactly the same two piecewise-affine triangles as
the native mesh, including the inverse mapping, true area and folded-triangle
sampling. Five regressions cover actual emitted gables, strongly tapered
quads, inversion, adjacent filtered charts and preservation of real cabinet
occlusion. There is no shadow blur, fill-light workaround or corner blending.

All four houses also have closed physical corner joins and their decorative
roof soffits sit behind the interior wall planes. Actual GLB geometry tests
check soffit clearance and ridge overlap. Native same-camera before/after
views show continuous walls, removal of the erroneous roof band, and a
correctly shaped cabinet shadow while table, chair and shelf shadows remain.

The dense model charts serialize about 54 MB of metadata. The runtime,
writer and CLI validator use the same dedicated 64 MiB lightmap-metadata
limit; the material-data limit remains 16 MiB. The regression reads and
validates the actual dense showcase package under these bounds.

The eight-page Demo HDR array occupies 256 MiB before its 196-byte KTX2
header. A dedicated atlas-record limit allows 64 KiB of bounded container
overhead in the writer, runtime and CLI. Ordinary entry and aggregate limits
are unchanged. A regression rejects the old cap and validates/loads the real
full Demo array. Incremental checks reuse all three completed packages; their
SHA256 values remain unchanged after this container-reader correction.

## Ghost face correction

Both original sheet characters have attached visible eyes and mouths. The
cat's translucent body previously overpainted its face because the facial
triangles preceded rear cloth triangles in one blend primitive. The generator
now draws cloth first and the attached facial primitive last, using the
original human ghost's dim facial material so body emission does not wash out
its dark features. Original body material, texture, geometry attributes, skin,
nodes and both original animation clips per model remain intact. The human face maintains at
least 6 mm clearance and the cat at least 5.077967 mm in measured animated
poses. Clip-boundary, 60 Hz deformation and front/oblique native views pass.
No global ghost shader or body-brightness change is used.

## Verification already completed

- Six actual-controller audits passed: all four kitchen/couch/bedroom routes,
  doors closed/open, five trails, crouched swimming and exit, both gate joins,
  84 boundary jump challenges and diagonal corners.
- Strict authored geometry check: zero errors and zero warnings. Its narrow
  declared intents cover 196 outdoor perimeter wall omissions and 22 exterior
  checker voids outside the playable world; actual visible prop containment
  is independently tested through the controller. No house exemption.
- Ten authored-map Python checks and all new road/fire/ghost checks pass.
- Asset catalog validation: 247 assets, 134 placeables, zero warnings.
- Rust 1.98.1 native release build and strict canonical lint/format gates passed
  for the final receiver, metadata and atlas-container changes. Canonical
  commands were completed in resumed batches after an execution transport reset;
  the presentation portion remains incomplete, not a passed desktop gate.
- Both final native Metal Low-lighting resource tests passed: 2/2 in 1.34s. They
  verify actual Low path equivalence, retained texture quality, and live
  atlas/reflection/material retirement and recovery.
- Final package suite: 47 tests passed in 36.524s. Real compiled-build desktop
  suite: 10 tests passed in 43.664s. Metal bootstrap: 26 tests in 66.874s,
  with 15 passed and 11 explicitly skipped because the Mac console locked.
  Presented-frame acceptance remains incomplete until those 11 can run.
- Full Rust 1.98.1 workspace suite: 1,893 library tests and 3 additional
  workspace tests passed, zero failures; 21 optional diagnostics ignored. The
  library run took 1,579.54s with two test threads and optimized test code.
  Required ignored atlas and native GPU audits were then run explicitly.
- Twenty new map/road/ghost Python contracts passed; the independent final
  packaging, GLB, tool execution, zoo, benchmark, lightmap and geometry repair
  run passed 69 tests in 470.757s.
- Nine native recorded routes passed position assertions: road entry, bedroom
  and return for all four cottages; pond swim and exit; winding trail to the
  fire; repeated jumps into both gates and the northern forest rock edge.
- Seven native live settings changes completed eight ready-world installations.
  High textures with Low lighting, Medium textures with Low lighting,
  override OFF and Medium recovery, High, Low, Medium and High all installed
  the expected renderer resources with stable level identity, player position,
  four interactables and eleven routes. Matched Low/Medium/High/High-with-Low
  native interior, pond and fire captures were inspected.
- The final sky radiance is 0.22 with the existing cool sky colour and star PNG.
  Visibility-tested sky/bounced light reveals pond rocks, paths and tree
  silhouettes; warm authored lamps/fire remain the focal lighting. The change
  uses normal compiler transport, not a brightness floor or emissive disguise.

## Load and performance evidence

All three final normal packages are built. Places Demo took 1297.79s with
ten workers (113,268,854 bytes; peak RSS 3,883,859,968 bytes; peak footprint
6,403,315,088 bytes). Model Zoo took 421.6s (79,228,585 bytes). All three
incremental builds reused the completed packages, passed `--require-current`
and decoded every variant. SHA256 values were unchanged.

Eight serial native fresh-process runs, two per setting, measured request-to-
ready and process RSS after the CPU-intensive checks finished. Filesystem
caches were warm; these are package decoding/renderer installation timings,
not GPU completion or presented-frame benchmarks. Host: Apple M2 Pro/Metal,
16 GB RAM, logical window 1280x720, actual drawable 2560x1440. Selected Vsync
was off; the adapter reported Fifo presentation mode. All runs installed the
expected level, four interactables, eleven routes and twenty characters.

| Selected quality / lighting | Mean request-to-ready | Mean process-to-ready | Peak RSS |
|---|---:|---:|---:|
| Low / Low | 454.69 ms | 702.34 ms | 456.47 MiB |
| Medium / Medium | 1437.53 ms | 1675.25 ms | 1399.55 MiB |
| High / High | 1579.34 ms | 1817.96 ms | 1604.97 MiB |
| High / Low override | 463.69 ms | 701.58 ms | 571.31 MiB |

Low installs 20,546 architecture vertices and 471,690 prop vertices; Medium
and High install 9,733 architecture vertices and 683,616 prop vertices.
There are 641 installed prop draw ranges, before camera culling. Architecture
reports zero missing texture draws. High and High-with-Low both retain
65,710,744 bytes of architecture texture residency and High filtering, compared
with Low's 9,087,640 bytes: the override retains selected-quality textures.

The Mac locked before the final presentation suite. Eleven of the 26 Metal
bootstrap tests therefore explicitly skipped; fifteen passed. An unlock was
requested, but the console remains locked. The locked load-only runs report
zero presented draw calls; their printed FPS is excluded from acceptance.
The prepared 16-run street/fire steady-state benchmark (120 warmup frames,
600 measured frames, two repeats, four settings) must run after unlocking.
No steady-state frame-rate claim is made and full desktop acceptance is pending.

Deterministic camera coordinates: `tools/bench/lantern_hollow_views.json`.
Native capture runner: `tools/bench/capture_lantern_hollow.py`. Logs, routes,
load traces, GPU audits, before/after images and all final screenshots are
preserved outside `target` in the recovery directory below.

## Retained build and continuation

Recovery directory:
`/Users/connordawkins/Documents/Codex/Recovery/Places-showcase-delivery-2026-10-01`.
Its `bin/places`, `bin/places-compile`, `qa/`, `screenshots/`, this report and
benchmark helpers are copied with a verified SHA256 manifest. The retained executable independently
loaded High Lantern Hollow with Full lightmaps, the override default Off and
all twenty characters; its native 3024x1676 launch capture was inspected.
They remain available after a later `cargo clean`; the source/assets/packages stay in the
isolated checkout. Launch the retained binary with:

```sh
PLACES_ASSET_ROOT=/Users/connordawkins/Documents/GitHub/Places-night-showcase \
PLACES_LEVEL=lantern_hollow \
/Users/connordawkins/Documents/Codex/Recovery/Places-showcase-delivery-2026-10-01/bin/places
```

After unlocking the Mac, finish presentation and steady-state acceptance:

```sh
cd /Users/connordawkins/Documents/GitHub/Places-night-showcase
python3 -m unittest tests.test_wgpu_bootstrap
python3 /Users/connordawkins/Documents/Codex/2026-10-01/task/night_showcase_bench.py --root "$PWD"
```

Require zero presentation skips, nonzero measured draw calls and valid quality
installs. Refresh the report and recovery evidence after those runs. The queued
Rust 1.99/latest-stable upgrade starts only after current showcase acceptance
finishes. The user-authorized final `cargo clean` follows the upgrade and all
validation, with no active target users and refreshed evidence preservation.
Neither upgrade nor cleanup has started here. No push or publication occurred.

## Scope limits

This adds static model receivers, not a dynamic-shadow rewrite. Moving ghost
and skeleton lighting follows the integrated probe contract; animated models
do not freeze bind-pose atlas coordinates. The campfire uses five animated
flames and supported point illumination; smoke/embers/audio are not added.
Coarse forest lighting LOD is required by the existing atlas budget. Dense
forest trees and grass receive baked lighting but opt out of casting transport
shadows; tree trunks retain physical collision. Furniture, cliffs, gates,
boulders and stumps retain authored occlusion and actual cast shadows. Existing
PS1/PS2/Source-inspired art and desktop-only target remain unchanged.

## Screenshot delivery

Eight original native PNGs are staged outside `target` at
`/Users/connordawkins/Documents/Codex/2026-10-01/task/showcase-delivery`: overview,
street, campfire, pond, furnished interior, moving sheet cat, human sheet ghost,
and the matching interior before image. All final screenshots are actual native
captures at 3024x1676, not generated recreations or retouched evidence.

Library delivery is currently blocked at helper discovery: the unmodified
current helper reports `Library prepare_uploads is not available`; its
authenticated discovery lists no Library tools, although model-side reads work.
No upload, replacement, transfer or finalization started. The eight local files
and ordered request retain the five existing replacement identities/version
guards for a supported parent-assisted transfer. Existing Library pictures are
not represented as these final images.
