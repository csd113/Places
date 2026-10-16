# Pool, Office and Winter model integration

## Delivered models and placement

The finished integration contains **75 delivered exports: 59 replacements and
16 new identities** (23 Pool, nine Office, one snack machine, 39 Winter and three
Outdoors trees). Drinks R2 replaces the provisional drinks export within that
count. The catalog contains 593 entries, 370 placeables and 178 accepted model
records. Exact model hashes and measured bounds are in `assets/completed-models.json`;
original deliveries and editable sources remain in ignored `import/model-rework/`.
Additional Blender authoring remains paused.

Drinks R2 is the exact delivered GLB, SHA-256
`2bec6e5a70c4a637cf9a3c047eccf4d539738bce0953ab61d8cbc6f406181443`:
2,869 triangles, 12 materials, 1 × 1.9 × 0.8 m, bottom-centered origin and +Z front.
Fascia slot 4 and LCD slot 6 follow the existing `break_room_vending` controller
alongside the snack machine, including actual spill illumination. Metal remains
non-emissive; both light anchors retain their specified positions. The exact
source retains intentional mechanical openings and eight coincident rear seam
triangles; the native front views show no visible z-fighting.

Winter changes keep the modular rail-only and linked fence/post stock unchanged;
the two delivered integral-post assemblies have separate catalog identities.
String-light spans use their measured 5/7/11 bulb positions with conserved power.
Attached-source capacity is 16; selected-direct and switched-source budgets stay
eight. Disabled sources and twin Pool fixture apertures count toward the aggregate
source limit. Lantern anchors follow the delivered geometry.

The bank tree moves 4 cm west and 20 cm south: all nine saved-mesh root contacts
are supported, with at least 17.9 mm bank margin. Four small timber pegs support
the lodge cables; end bulbs clear terminal shafts by 11 mm. Stone-wall colliders
use local `[2.4, 0.98, 0.5]` before yaw 90. Pool's drum starts and resets on dry
deck at `(14, 0, 6.7)`, retaining pickup, density, dormancy and sinking. All four
Office records cabinets face into the room at yaw 180. Native fork traversal
confirmed the intentionally narrow Outdoors trunk collider should remain.

### Pool and Office foundation

The first adoption batch on 15 October 2026 contained 33 delivered models:
23 Pool, nine Office and one snack machine, with 19 replacements and 14 new
catalog entries. Its GLBs matched the completed Library deliveries; the ledger
then contained 136 accepted exports. Full editable/reference deliveries remained
in Library and the ignored local import collection. The finished 75-export
integration above supersedes that batch's pending deck reset, records orientation
and provisional drinks delivery.

Pool's two manufactured three-riser entry modules use six hidden player supports
and fitted item colliders following their treads. Basin, shelf, tub and water
volumes retain their dimensions; the ladder collider follows the delivered rails
and rungs. Tile/panel modules are catalog assets rather than extra demo displays.
The planter and safety notice join the existing kit.

Office's reception furniture, noticeboard, tall filing cabinet and snack machine
retain the layout, navigation, AI and modular ramp rails. Either vending machine's
E interaction cycles `break_room_vending` through steady, flickering and dark
states; the two basement fixture failures remain independent. Reusable fields
are `props[].switch_group`, fixed `light` components and numeric
`emission_overrides`.

The three fixture families use delivered GLBs and apertures matching the recessed
round lens, separate wall strips and downward Office diffuser. Metal/gaskets stay
non-emissive. Geometry revision 14 and PLMP v8 introduced prepared fixture/prop
controller identities while retaining readers for older prop records.

The snack export retains 16,240 triangles, 13 material groups, five shelves,
50 dimensional packets, 25 coils and thin BLEND glass. Only its exact accepted
hash enables the bounded 16,240-triangle importer profile; ordinary GLB imports
retain their 6,000-triangle ceiling. Thirteen unique embedded PNGs also exist as
standalone repository assets. Decoded images and ordinary opaque materials share
resources; only luminous uniforms require controller ownership.

Full switches emissions and prepared illumination together, excluding switched
and authored-off sources from the permanent baseline. Static retains authored
illumination and matching emissions, carrying runtime requests into Full.
Reflections retain their authored capture state rather than rebaking dynamically.
Cardinal fixtures were verified; arbitrary wall yaw retains the existing
axis-quantized finite-source limitation. Four independent Office circuits exceeded
the former four-page budget; sharing the vending circuit allowed six pages at
the unchanged density/byte limit, without enlarging renderer resource caps.

## Lighting and package bounds

Geometry revision 15 routes static models above the existing 1,500-triangle art
budget and 3 m world extent through prepared directional probes. All five new
tree variants qualify. They retain indexed geometry as transport casters, source
albedo, normals, UVs and flashlight response. Packaging rejects unfinished probe
lighting; valid dark probes remain dark. Ordinary model lightmap density, the
14-page limit and 64 MiB metadata cap remain unchanged. A tiny deterministic
fixture protects this behavior without baking a shipped map.

All eight supported packages were refreshed and validated with the final assets
and compiler. Every Full variant has an actual atlas and no lighting failure.
Compiled packages remain outside Git; archived `places_demo` and Model Zoo were
not baked.

| Package | Atlas pages | Chart metadata, bytes |
| --- | ---: | ---: |
| Pool | 2 | 5,128,551 |
| Home | 1 | 949,720 |
| Lantern Hollow | 7 | 10,425,380 |
| Winter | 5 | 8,859,511 |
| Beach | 7 | 3,713,212 |
| Frutiger Aero | 4 | 3,421,225 |
| Retro Arcade | 8 | 23,013,433 |
| Office | 5 | 10,006,739 |

## Validation actually run

Passed after the final source changes:

- `cargo fmt --all --check`.
- `cargo test --workspace`: 2,331 library tests plus target tests; 19 existing ignores.
- `cargo test --workspace --all-features`: 2,338 library tests plus target tests; 19 existing ignores.
- Debug and release `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` (release adds `--release`).
- `cargo build --release --bins`.
- 37 focused Python tests across Office/Winter assets, Winter assemblies, string lights and the synthetic zoo generator.
- Asset validation (zero warnings), prop and texture checks, Office/Winter/Lantern generator currency, and `git diff --check`. Texture validation retains 96 existing manifest warnings.

Native Metal checks passed with actual Full atlas uploads and zero failed prop
models. Pool passed two stair routes and four water scenarios (113 assertions),
covering reset grounding, duck buoyancy, dense-object settling, player entry/exit
and real E pickup/drop/sink; the drum settled at Y −1.498 m, fully submerged.
The final Pool Static/Full mesh, props, lighting and collision payloads match the
passing water evidence byte-for-byte. Winter passed stone-wall collision and
lodge/stair/forest traversal; final views show the supported bank tree and cable
mounts. Lantern passed trunk stop and fork-side passage.

Office Full captures show synchronized steady/flicker/dark behavior for both
machines, a darker nearby floor when switched off, and readable materials under
the flashlight. Static captures retain their initial baked illumination: matched
on/dark image regions are identical. Lantern Full shows transport shadows absent
from the brighter Static result, with readable materials and no full-bright trees.

Isolated performance used 60 warmup and 300 measured frames at matched cameras,
3024 × 1898, High/Full, reflections and bloom, on the local M2 Pro. No compiler ran
concurrently. All 360 presentations per run were ready.

| View | GPU median before → after | Peak RSS before → after |
| --- | ---: | ---: |
| Winter | 3.399 → 3.484 ms | 616 → 737 MiB |
| Lantern clearing | 3.225 → 3.755 ms | 881 → 1,723 MiB |

These representative views remain below a 16.7 ms GPU frame budget; they are not
a worst-case or cross-hardware guarantee. **Lantern's denser trees nearly double
peak memory**, a real remaining cost. Probe-lit trees also have coarser received
lighting than individual per-face charts.

### Earlier Pool and Office foundation acceptance

The first 33-model batch passed formatting, configured debug/release Clippy,
all-feature and default workspace tests, 359 Python tests, and catalog, texture,
completed-model and generator checks. Explicit developer runs covered Pool steps,
nine Office collision stations, water settling/pickup/swimming, both vending
interactions, basement failures and flashlight response. Full panel/floor captures
extinguished together; Static retained authored illumination. Office Full used
five atlas pages.

Its separate bounded 300-frame Metal trials at a 1920×1080 drawable measured
median GPU time of 1.19 ms for Pool and 1.17 ms for Office, with peak process RSS
of 294 MiB and 1,074 MiB respectively. These are single local workload samples
from the earlier batch, not the final paired Winter/Lantern measurements above
or a general performance guarantee.

## Preserved output and changed files

The self-contained local payload, eight packages, release player/compiler and
`play.command` are in `local/model-rework-20261015/final/`, outside `target`.
The player requires the existing Homebrew SDL3 installation. Its SHA-256 is
`9e9bb74f470e031419a77e1efb1e22a7e457b48f4b4f37f3c8f9d81660f78a7f`.
A native launch against that preserved payload passed. Essential screenshots and
receipts are in `local/model-rework-20261015/final75/`; they remain local.

This continuation changes 43 GLBs, three PNGs, the asset catalog/completed-model
records, Office/Winter level sources, 13 Rust files, six Python test files, four
authoring/validation tools, both canonical guides and this report. The preceding
published foundation commit contains the Pool/Office/snack installation and
shared vending controls. Git history provides the exact file list for both.
