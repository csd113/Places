# Desktop gameplay and tooling expansion — September 2026

The September expansion delivered movement, authored interactions, animated
entities, reusable props, curved architecture, capacity controls and the generated
Model Zoo. This report consolidates the completed feature results and their
acceptance limits. The [map authoring guide](../MAP_AUTHORING_GUIDE.md),
[asset specification](../ASSET_SPECIFICATION.md) and
[offline tooling guide](../OFFLINE_TOOLING.md) govern current formats and commands.
Later movement, lighting and loading reports qualify the historical measurements
below; they are not fresh measurements of the current checkout.

## Gameplay and interfaces

Player collision uses a feet position and an upright body. The expansion added
crouching through the remappable C action, falling beyond floor edges, prop-top
support, head clearance, water entry/exit and authored ladders. Gravity remains
9.8 m/s² and the step allowance 0.4 m. Later whole-body sweep and timestep repairs,
including the current 1 m jump apex, are documented in the
[movement controller audit](movement-controller-audit.md).

Water immersion and stance are separate state: buoyancy and held jump support
surfacing while crouching changes body height. Ladder volumes define position,
width/depth, bottom/top height and facing. Validation bounds their count and
requires finite positive dimensions. Approach-side movement attaches to a ladder;
climbing uses 2.2 m/s, backward input or jumping detaches, and blocked exits stay
bounded. Decorative ladder props are non-solid where the authored volume owns
climbing. There is no general mantle system or separate climb-down animation.

Interaction identity belongs to each placed instance, using its authored ID or a
stable generated identity. Two placements of one model can have different labels
and actions. E selection applies reach, aim and occlusion tests before dispatching
one action batch. Area triggers use swept entry, cooldown/once state and deferred
pending actions; reset reseeds trigger state and clears input edges and held state.
Supported action variants include `toggle_label`, `reset_to_start`,
`play_animation` and `toggle_animation`. Unsupported `play_audio` is rejected
explicitly. A luminous sign's artwork does not automatically create room lighting;
physical lights and switched light behavior must be authored separately.

Entity routes are bounded lists of loopable steps with per-instance state. The
route controller owns world motion; the animator supplies the pose. It uses
collision/headroom checks, at most 1/60 s route substeps, a 0.1 s frame clamp,
0.3 m step allowance and 1 mm blocked-motion tolerance. Arrival tolerances are
2 cm and 0.02 radians; turns are bounded at 240°/s. Routed labels follow the same
instance transform. Clip selection, looping and action overrides are independent
per instance, so operating one placement cannot toggle every copy of its model.

Rigid animation retains the imported node hierarchy with one bind weight per
node. Scrub-style toggles traverse their clip in 0.35 s, reverse from the current
pose, hold at either endpoint and emit arrival once. Characters without locomotion
clips retain their bind pose. The importer supports STEP, LINEAR and CUBICSPLINE
channels, node/skin transforms and morph deformation, preserving deformed normals
and tangents. Static reflection probes and transport occluders do not follow every
animated frame; dynamic shadow redesign and attack AI were outside this expansion.

Floating props are non-solid dynamic instances validated inside a water region.
An absolute clock supplies surface-minus-draft height, sinusoidal bob and heel;
XZ placement stays fixed. Spawn is idempotent and level changes clear the state.
They are excluded from static batches and bake occluders. This is an authored
display behavior, not a fluid or rigid-body simulation.

## Assets, geometry and capacity

The work delivered the rat, concrete mannequin, skeleton and Spoonerman motion,
plus household/table-setting props, luminous signs, fixtures and a hinged switch.
Imported models remain in metres, Y-up, with established forward orientation,
instance pivots and committed PNG artwork. Rig counts were 25 rat, 23 mannequin,
24 skeleton and 26 Spoonerman joints. Rat walk/run metadata uses approximately
0.1985/0.5731 m/s. Subsequent Spoonerman clips and measured deformation limits are
in the [motion report](spoonerman-cat-motion.md); geometry and material outcomes
are in the individual asset reports.

Curved walls and circular pillars are native architecture, so rendering,
collision and baking share their surfaces. The reference curves use 2 m radius,
24 cm thickness, 2.8 m height and 12 facets per quarter turn. A 0.6 m pillar uses
16 facets. UVs maintain world scale. The geometry checker reports errors and
warnings rather than accepting overlapping or malformed authored surfaces.
Wood rails, vent faces, ceiling frames and pool sheen received corresponding
geometry/material repairs without new renderer dependencies.

Capacity expansion introduced explicit bounded parsing and allocation rather
than unbounded collections. At delivery, limits included 2,000 rooms,
20,000 walls/ceilings/props, 1,024 distinct models, six million prop vertices,
eight million level vertices, 32 MiB source JSON and 64 character instances.
These are dated delivery values; current atlas/payload limits belong to the
quality and package guides. Broad-phase collision uses a conservative CSR grid,
deduplicated ascending candidates and exact linear fallback when its bounded
cell layout cannot represent a source. Identity validation uses hash sets and
route lookup builds one index.

The dense diagnostic contained 5,312 props, 120 fixtures, 18 routes, 12 curves,
48 decals, about 4.11 million vertices and 739 observed draws. Its excess
characters exercised the supported capacity behavior. The former 2 km sparse
fixture was subsequently retired because its navigation request exceeded the
existing cell bound; it is not a currently accepted playable map. The
[lighting energy report](lighting-energy-repair.md) records that distinction.

Model Zoo is generated from the catalog, with stable
`zoo:<catalog-id>:<role>` identities, inspected real bounds and 1.4 m clear aisles.
Animated envelopes use 48 sampled poses. Model growth and removal are reflected
without editing a hand-maintained placement list or excluding registered assets.
Catalog order, output hashing and worker assembly are deterministic. Inspection
cache identity includes model content metadata; `--no-cache` supplies the
independent control. Catalog counts in early runs are historical snapshots,
not the current asset inventory.

The editor was removed from the desktop product. The later Pocketchip retirement
and preparation/cache implementation are documented in
[desktop cleanup and loading](desktop-cleanup-and-loading.md). That corrective
pass ended with an unbuilt, untested pacing edit; the completed September feature
gate does not certify it.

## Integrated acceptance

The final September gate passed 1,288 Rust cases with eight existing ignored
diagnostics, formatting, strict Clippy, 43 package checks, 23 tooling checks,
eight compiled-build cases and 14 native wgpu cases. The late short-tap input fix
was exercised through the actual player: crouch height traversed
0.7 → −0.1 → 0.7 m, and a kitchen E interaction changed its placed-instance label.
That interaction used a diagnostic 0.39 m eye height; it does not establish
ordinary standing-player reach for every placement. The complete timed Spoonerman
route and manual keyboard/mouse feel were not exhaustively observed.

Linux/ARM validation passed 175 Rust cases with one existing ignore, built the
player and inspected an Xvfb software-rendered Low capture. This was functional
coverage, not a performance result. Windows native validation was unavailable.
Later reports contain additional native movement and presentation acceptance.

Three High-quality samples per map used 120 frames after 20 warmup frames on
Apple M2 Pro at 3024×1676, 74° FOV and VSync off:

| Map | Median frame | p95 frame | Observed draws | Separate capture load | Peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| Places Demo | 6.901 ms | 13.550 ms | 170 | 1.49 s | 701.3 MiB |
| Model Zoo | 7.327 ms | 14.368 ms | 56 | 14.45 s | 761.4 MiB |
| Dense diagnostic | 7.830 ms | 13.713 ms | 739 | 56.37 s | 1,438.9 MiB |

The load column measures a separate capture invocation through four ready frames.
Filesystem caches were not purged and it is not a first-interactive timer. These
observations do not demonstrate a speedup or a universal frame-time bound.

## Offline tools and measured limits

Offline tooling gained bounded workers, deterministic output assembly, task-count
caps and content-based duplicate work avoidance. Validation compares output
bytes/hashes; known JSON timing/root metadata is normalized explicitly. The
following complete CLI measurements used Python 3.13.5 on a 12-core M2 Pro with
16 GiB RAM, sequential runs and an original saved working-tree baseline:

| Task | Original | Updated, 1 worker | 4 workers | 8 workers | 12 workers |
| --- | ---: | ---: | ---: | ---: | ---: |
| Prop previews | 4.694 s | 4.569 s | 1.611 s | 0.988 s | 0.856 s |
| Texture validation | 183.179 s | 88.462 s | 31.643 s | 21.334 s | 15.151 s |
| Four small props | 0.308 s | 0.326 s | 0.309 s | 0.309 s | 0.311 s |
| Six animation checks | 5.750 s | 5.921 s | 2.346 s | 1.850 s | 1.834 s |
| Geometry repair | 0.345 s | 0.380 s | 0.280 s | 0.257 s | 0.312 s |
| Rat Blender export | 2.660 s | 2.217 s | 2.205 s | 2.220 s | 2.213 s |
| Entity validation | 1.067 s | 5.341 s | 1.603 s | 1.084 s | 1.103 s |
| Zoo generation | 1.720 s | 2.141 s | 1.713 s | 1.719 s | 1.715 s |
| Seam checks | 1.854 s | 1.592 s | 0.629 s | 0.621 s | 0.629 s |
| Texture resize | 2.122 s | 2.080 s | 1.914 s | 1.874 s | 1.909 s |
| Hole capture helper | 0.285 s | 0.277 s | 0.218 s | 0.221 s | 0.223 s |
| Eight comparisons | 0.924 s | 0.892 s | 0.362 s | 0.326 s | 0.307 s |

Texture validation's 2.07× serial improvement and 5.84× worker scaling are
different effects; their combined 12.09× improvement is not 12× parallel scaling.
Preview and clip improvements were 5.48× and 3.14×. Tiny prop/repair jobs did not
benefit materially. Blender warm startup dominated its short export. Entity
validation and Zoo already used parallel paths, so their original columns are
not serial baselines. Task limits cap six distinct clips and four seam jobs.
Observed process-tree RSS peaked near 867 MiB, with shared pages counted by the
observer; this is not a universal memory bound.

No historical `sitsearch.py` solver was recovered. Its legacy forced full-scale
static-cat output is not evidence for the current character dimensions. The
implemented pose checks and maintained animation tools are the reproducible
results. The original timing baseline was a saved working tree, so timing claims
cannot be recreated solely by checking out a nominal Git revision.

Use the documented generator/currentness commands in the offline tooling guide
and `sh tools/verify.sh` for the current acceptance gate. For loading measurements,
use the exact harness and limits in the loading report. Native capture reports
identify their tested renderer, camera, quality and source package; offline
contact sheets establish asset inspection only.
