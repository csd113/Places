# Flashlight and Office gameplay — 0.10.0 staging

The player now has a reusable handheld flashlight. **F** toggles once per press;
Settings → Controls can rebind it and saves the binding. Conflicting and reserved
keys are refused. Console typing, menus, focus loss and map preparation cannot
toggle it. Graphics changes retain power; loading a world starts it OFF.

The existing developer console remains opt-in under Controls and opens with
**backtick**. `/noclip [on|off]` preserves validated walking recovery and now
continues door animation during flight. `/give <catalog-id>` already integrates
eligible physical items with pickup/drop/throw; this work reuses that implementation.
Unknown commands, unknown items and file-path-like input return bounded errors.
No first-person model, interaction-hand replacement, tutorial or throw label was added.

## Shared lighting and entity behavior

The spotlight extends the existing attached-light material path for world,
decals, movable props and posed characters. Its final bright core is **20.55°**
across, with a soft **40.27°** outer cone. Intensity remains 12, range 24 m, with
windowed `1/(1+d²)` falloff. The camera attachment is 14 cm right, 18 cm down and
6 cm forward, converging at view centre 6 m away. Shared triangle visibility
retracts the source before nearby geometry.

One lazily allocated **1024² / 4 MiB depth target** and four PCF comparisons use
the existing world submission, culling, transforms and material coverage. OFF
submits no visibility pass; resources are retained for subsequent toggles. Posed
ray resources cache pose/opacity revisions and share existing PNG data. Live
planar captures retain the beam; offline probe captures remain unlit by it.

The generic `light_response` component observes five body anchors through the
same cone/falloff and static/live triangle visibility, excluding its own receiver.
Office's mannequin stops, selects its existing standing clip and faces the
player. Its original AI state, route and local timers remain intact. Activation
is 0.035, release threshold 0.005, with a 0.20 s dark hold. Completed-frame
observations are consumed on the following simulation tick; normal movement and
animation resume after release. The implementation contains no Office geometry
or mannequin-name special case.

Office's two failing fluorescent circuits independently spend **90.9% / 92.1%**
of their 50.3 / 62.9 s cycles OFF. Unequal 7.4–21.1 s blackouts alternate with
brief returns and an approximately 3.15 s final return. Fixture faces and emitted
illumination switch together. Exposure, ambient light and fixed navigation lamps
are unchanged.

## Validation

Formatting, workspace check, strict debug/release Clippy, default and all-feature
workspace tests passed: **2,321 / 2,328 library tests**, plus integration tests.
The 19 pre-existing opt-in GPU tests remain separate. New regressions use scalar
beam/attachment fixtures, source-only validation and fewer than twenty ticks of
a 4 × 4 m navigation fixture; default tests and CI do not bake shipped maps.
The configured generators, asset validator (573 assets / 350 placeable; zero
warnings), mannequin identity/clip checks and 356 Python tests passed (four skips).

Only Office was rebuilt, with eight workers. Its archive integrity and exact
source/dependency/compiler currency passed. Existing Arcade and Pool packages
loaded through the unchanged runtime format.

Nine core sequential SDL/Metal cases passed **201 numeric checks**, with actual
native captures inspected. They cover direct stop/look/resume, dim-edge hold
below activation (0.014–0.029 incident intensity; **zero position drift**), wall,
floor and cabinet occlusion, close-wall retraction, static/moving/held/dropped/
animated lighting, repeated F presses, conflict rejection, R rebinding, console/
menu/focus ownership, live Static/Full changes, texture Low/Medium/High and reload.
Filtering was separately exercised at 4×/8×/16× while Full lighting stayed ON.
Office has no authored doors: its door regression uses Arcade's real swinging
door, including noclip animation and the existing planar mirror. Preserved Pool
captures verify transparent water and the lit basin without an opaque black mask.
No full-bright or black-smudging regression was observed in these views.

## Native performance

Apple M2 Pro / Metal; High textures and Full lighting; **2560 × 1440** actual
drawable; Immediate requested; 120 warmup plus 600 measured frames per run.
Two OFF/ON pairs per view reverse order in the second repeat. Values below are
means of the two run statistics; worst is the maximum observed frame, in ms.

| View / metric | Median OFF → ON | p95 OFF → ON | p99 OFF → ON | Worst OFF → ON |
| --- | ---: | ---: | ---: | ---: |
| Dark / frame-loop wall time | 8.338 → 8.323 | 9.297 → 9.511 | 12.425 → 11.798 | 23.886 → 17.585 |
| Dark / GPU pass-span union | 4.046 → 5.217 | 4.450 → 5.603 | 4.769 → 5.967 | 5.759 → 7.925 |
| Busy / frame-loop wall time | 8.270 → 8.323 | 9.288 → 9.071 | 12.341 → 11.735 | 17.488 → 13.454 |
| Busy / GPU pass-span union | 4.148 → 5.040 | 4.484 → 5.549 | 4.789 → 6.850 | 5.175 → 7.660 |

Median CPU-side render spans grew about 0.20 ms in both views. AI actually reacts
in ON runs, so these are gameplay comparisons, not isolation of pure shader cost.
An additional GPU trace measured the cone-culled visibility pass at 0.026–0.251 ms;
the three traced scene spans were about 0.978 ms OFF and 1.169 ms ON. No shadow
quality or essential occlusion was removed. Distinct shadow pipelines, cone
culling, lazy allocation and revision/PNG reuse bound its work.

Separate OFF-after-ON controls measured loop/GPU medians of 4.800/2.293 ms (dark)
and 2.078/1.487 ms (busy), with retained resources. Peak native RSS across the ten
runs was **605.2–631.5 MiB**. Later diagnostic runs also ran faster: scheduling,
presentation and GPU clock state materially affect these short measurements.
GPU spans include scheduling stalls, exclude gaps and count overlapping spans
once; they are not active-utilization measurements. First-use resource creation
is outside the warmed comparison. No universal frame-rate guarantee is claimed.

## Deliverables and limits

`local/flashlight-0.10.0/` retains binaries, package checks, native receipts,
performance distributions, the acceptance summary and Library delivery manifest.
The final media directory contains a **24 s / 720-frame walkthrough**, a
**15 s / 450-frame sweep**, a **26 s / 780-frame 360° turn**, and five PNGs.
All three H.264 videos are 1280 × 720,
30 fps, YUV420P, BT.709 limited range, sampled from consecutive native source
frames at stride two without inserted or duplicated frames. Movement/look/F use
ordinary input; diagnostic camera aims leave entity motion autonomous. Decoded
video frames were also inspected. Library uploads succeeded.

The walkthrough Library ID is `libfile_e9309d61746081918d47f1eea1a4cf94`; the sweep
is `libfile_b0d861389f488191a11cc603f6432c43`. The requested final-width preview is
`libfile_88b63b25352c8191bd3ad17f286ec027` (40.27°). Exact final commit, remote SHA,
CI receipt and cleanup status are recorded in the local handoff after publication.

The later requested complete rotation is `libfile_c8f3c77dc6bc81918cf2266eec96e699`.
Native state logs verify 360.001° of continuous clockwise rotation at a fixed
position, with the beam retained. The existing viewpoint puts a cabinet at about
1.6 m, chair at 2.9 m, desk at 3.1 m and the doorway water-cooler landmark at
10.8 m. Office is sparse around the remaining sectors; no props were invented or
map data rebaked for the clip. Frame-based ordinary look input preserves the
Controls speed bounds while producing a smooth recorded turn.

Raster shadows retain opaque/cutout and nearly opaque blend coverage; ordinary
glass/water transmit rather than casting colored or partial-blend shadows. CPU
observations retain scalar partial transmission. Static retains its historical
coarse vertex illumination and existing `hold_in_static` fixture behavior;
the prolonged fluorescent blackouts are Full-lighting behavior. Native acceptance
is specific to this Mac; other platforms and universal performance are untested.
