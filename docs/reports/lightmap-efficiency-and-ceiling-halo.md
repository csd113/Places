# Lightmap efficiency and ceiling fixtures

Arcade now uses five Full atlas pages rather than six, saving **48 MiB of resident
GPU lightmaps** across its three contribution groups. Home uses one rather than
two, saving **16 MiB**. Receiver density, HDR coefficients, gutters, filtering,
switch groups and overflow guards are unchanged. The fluorescent ceiling outline
is fixed by closing the housing's open back. The approved commercial vent design
also replaces Arcade's two fixed-colour vent decals and follows room switches.

Baseline: `arcade-env` at `6e529d7b5acdc5b0807ee197d64f5904d7a70c66`,
with CI `38328890341` passed. Measurements, input/output hashes, native recipes,
publication and cleanup are retained in `local/lightmap-efficiency/verification.json`.
Only Arcade and Home were refreshed; no archived map was baked.

## Waste and the packing change

The prior Arcade expansion from 44.3 to 71.0 MB was mostly legitimate model
lightmaps. There were no duplicate payloads to remove. The actual avoidable waste
was **free space stranded between receiver families**: architecture opened two
pages, with only 26,706 padded samples on its second page, while models opened four
more. Architecture holes were previously available to models only near the cap.

Architecture keeps its inline MAXRECTS placements. At the first model receiver,
an integer sweep partitions the remaining free-rectangle union into disjoint
guillotine holes. Models immediately reuse these holes. Exact adjacent holes are
coalesced on a miss **before** opening another page. This retains the earlier
coalescing fix and avoids expensive overlapping-hole pruning for every model
triangle. A pilot reproduced all 47,800 original Arcade chart coordinates before
changing the policy; the same-sized input then needed five pages. Both accepted
layouts reach the padded-area lower bound of five and one pages respectively.

| Full allocation | Arcade before | Arcade after | Home before | Home after |
|---|---:|---:|---:|---:|
| Pages / contribution groups | 6 / 3 | 5 / 3 | 2 / 1 | 1 / 1 |
| Chart samples | 3,394,993 | 3,404,737 | 371,365 | 371,365 |
| Gutter samples | 1,569,272 | 1,573,016 | 144,068 | 144,068 |
| Unallocated samples | 1,327,191 | 265,127 | 1,581,719 | 533,143 |
| Padded occupancy | 78.90% | 94.94% | 24.58% | 49.16% |
| Resident GPU lightmaps | 288 MiB | 240 MiB | 32 MiB | 16 MiB |
| Raw atlas records | 186,880,664 B | 160,422,920 B | 18,774,584 B | 10,288,152 B |
| ZIP-compressed atlas records | 65,183,482 B | 65,216,778 B | 2,698,872 B | 2,689,651 B |
| Complete map archive | 71,003,099 B | 71,060,432 B | 3,186,305 B | 3,174,118 B |

Each page costs 16 MiB per contribution group: two RGBA16F planes. Arcade's base
and two independent switched contributions therefore cost 48 MiB per spatial
page, now 80 MiB per group. Loaded CPU half-float planes shrink by the same amount.
Static does not allocate these atlases and retains its prepared on/off behaviour.

There is **no Arcade archive saving**: the combined fix adds 57,333 bytes (0.08%).
Empty tiles already compressed well; removing them changes PLLC tile grouping and
reduces the compact-tile fraction from 55.46% to 47.64%. Exact fallback remains
mandatory at the existing error bounds. The raw atlas saves 26.46 MB, but its ZIP
payload grows 33,296 bytes. Home's complete archive saves only 12,187 bytes (0.38%).
No encoding tolerance or compression setting was weakened to improve these numbers.

Allocation rounding also carries real quality costs: Arcade's inclusive endpoint
rows/columns account for 591,003 samples, and folded triangle bounding rectangles
include 112,451 samples outside their triangles. These and the gutters were
retained. Shrinking them or repartitioning tiny model charts would require new
sampling/filtering proofs and yielded no necessary benefit here. Naively keeping
overlapping MAXRECTS holes throughout model allocation was too costly in the
pilot; the one-time conversion is the bounded alternative.

All 47,736 retained Arcade receivers and all 3,563 Home receivers keep their
dimensions. Arcade replaces 64 old flange charts with 16 backplate charts and adds
208 vent charts; its final chart count is 47,960. Both layouts have zero padded
overlap. Arcade switch indices remain `[10, 18]`; collision and navigation records
are byte-identical. Content key 15 and geometry revision 12 invalidate stale
compiled data. Existing packages/codecs remain readable; there is no format migration.

## Ceiling outline and switched vents

The pale rectangle was reproduced in actual Arcade at normal and oblique views,
including with bloom disabled. The fluorescent housing's four-strip top flange
left its centre open. Ceiling receivers could see the analytic source through
that opening, obtaining direct light and visibility-supported recovery fill.
The latter compresses colour channels independently and strongly desaturates the
received pink. This was a geometry/transport leak, not atlas padding or bloom.

In a bounded pink-source diagnostic, filtered RGB at the ceiling was
`[0.02126, 0.00438, 0.00954]`, but recovery lifted it to
`[0.20855, 0.15671, 0.19257]`. A full opaque, textured, non-emissive metal backplate
now blocks the invalid direct/recovery support. Legitimate filtered bounce remains
`[0.00968, 0.00167, 0.00228]`; a farther ceiling sample changes by less than 0.000012.
The diffuser stays below the plate and retains its colour, UVs and aperture.
The new ray regression fails on the old housing, passes for both fixture rotations,
and verifies unobstructed, correctly tinted room-facing light without an atlas bake.

The old vent decal shader retained its baked corner colours when the Full room
circuit switched off. The shared replacement `core:ceiling_diffuser_01` uses the
existing static-model receiver/circuit path: a 600 mm frame, real inset four-way
louvers and a closed plenum, 208 triangles, no emission or collision. The approved
design is preserved. Its opaque paint PNG and 1024-square master are committed;
the [model builder](../../tools/props/parts/ceiling_diffuser.py) records the exact
built-in ImageGen prompt and loads the PNG rather than generating texture imagery.

At the same Arcade vent crop, Full mean RGB is `[62.09, 62.83, 58.55]` on and
`[0, 0, 0]` off; the old decal stayed at `[60.60, 62.74, 62.20]` off. Static darkens
while preserving legitimate ambient. Actual bathroom interaction followed by
Full-off → Static-off → Full-off restores an identical PNG. A tiny pink-source
vent control also has an identical on/off/on return. No ambient, exposure or
map-specific lighting mask was introduced.

## Native evidence and costs

The 13 curated PNGs below are unmodified native captures. Matched pairs use the
same pose, settings and capture frame, with 1280×720 output, High textures,
Full lighting and reflections unless identified otherwise. Every accepted capture
was checked for commitment of the requested scene. Invalid menu-only attempts
were rejected. Normal pink views disable bloom; oblique views enable it.

| Evidence | Before | After |
|---|---|---|
| Pink ceiling, normal | [before](../images/lightmap-efficiency/halo-normal-before.png) | [after](../images/lightmap-efficiency/halo-normal-after.png) |
| Pink ceiling, oblique | [before](../images/lightmap-efficiency/halo-oblique-before.png) | [after](../images/lightmap-efficiency/halo-oblique-after.png) |
| Arcade floor/chart edges and cabinets | [before](../images/lightmap-efficiency/arcade-before.png) | [after](../images/lightmap-efficiency/arcade-after.png) |
| Home floor/props | [before](../images/lightmap-efficiency/home-before.png) | [after](../images/lightmap-efficiency/home-after.png) |
| Bathroom vent, Full off | [old bright decal](../images/lightmap-efficiency/vent-before-off.png) | [new off](../images/lightmap-efficiency/vent-after-off.png) |

Additional captures show [vent on](../images/lightmap-efficiency/vent-after-on.png),
[Static vent off](../images/lightmap-efficiency/vent-static-off.png) and
[diagonal cabinet/contact shadows](../images/lightmap-efficiency/diagonal-shadow.png).
Blue/pink fixtures, Static/Full circuit changes, Medium textures, cabinet art,
Bepsi props, reflections and the rebated bathroom door were inspected. Actual
switch and door return captures are pixel-identical. The GPU timing/device/RSS
overlay path is unchanged. Blizzard code/assets were untouched.

A single bounded runtime pair used the same final binary, pose and settings,
VSync off, 120 warmup + 240 measured frames, with no compiler/audit running.

| Native run | GPU pass median / p95 | CPU loop mean | Peak process RSS |
|---|---:|---:|---:|
| Arcade before | 9.288 / 13.330 ms | 5.932 ms | 996.70 MiB |
| Arcade after | 8.307 / 11.335 ms | 5.574 ms | 895.92 MiB |
| Home before | 4.556 / 7.046 ms | 5.552 ms | 377.89 MiB |
| Home after | 4.476 / 7.184 ms | 5.566 ms | 353.73 MiB |

These are sampled render-pass timestamps, not a general FPS claim. Thermals and
host/device scheduling were not controlled across repetitions. Home's starting
package already had an older compiler/catalogue; unchanged source and referenced
assets were verified, but its lighting/image/timing differences also include the
previous equal-area source-sampling change. Its allocation result is confirmed
independently by the identical-dimension packing pilot.

Final eight-worker bake: Arcade 643.721 s, peak RSS 2,633.39 MiB; Home 35.978 s,
349.42 MiB, with zero warnings/fallbacks. The prior owner's Arcade bake was
674.619 s, but this is not an isolated packing speed comparison. Current-package
hits take 0.863 s / 0.115 s and do no bake. One earlier owned Arcade bake was
cancelled when the vent scope was added, before publication; final acceptance
used only the combined Arcade bake and proportionate Home bake.

## Validation and handoff limits

Passed: formatting/diff checks; strict workspace/all-target/all-feature Clippy;
workspace Rust tests (2,248 library + 8 integration); all-feature tests
(2,254 + 8), both retaining 18 existing ignores; Python source suite (349 run,
four native-class skips); three separately invoked native smoke checks; source
generator; asset validation (570 entries/347 placeable, zero warnings); texture
and prop checks; complete asset audit (347 GLBs, 499 standalone/375 embedded PNGs,
zero errors). Texture checks retain 96 existing warnings. Prop decoded RGBA is
107,208 KiB, under the 106 MiB aggregate budget. No default test or CI requires
a production-map bake.

The exact-free-union test uses a 16-pixel grid; expanded existing packing tests
protect early sharing, coalescing before growth, gutters, determinism and true
overflow. The new fixture-ray test costs about 0.01 s. A 4×4 compiler control
compares unchanged, metadata, light and prop incremental outputs against clean
records: all four are byte-identical, with metadata reuse retained. Arcade/Home
integrity validation, `verify --require-current` and subsequent cache hits pass.

Unrun: Linux/Windows native acceptance, broad performance campaigns and further
production refreshes. Winter, Office, Beach, Frutiger Aero, Pool and Lantern Hollow
packages remain byte-identical and await normal compilation. Places Demo and Model
Zoo remain historical JSON only. The previously reported empty-install smoke opens
the game but has an old menu-text assertion; it remains queued for the tooling audit
and was not weakened or repeated. Compiler progress, Office recovery/shadows,
cleanup campaigns and 0.9.0 work remain outside this task.
