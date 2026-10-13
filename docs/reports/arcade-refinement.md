# Arcade cabinets, Bepsi bottles and closed-door privacy

13 October 2026 UTC. Arcade now has **36 cabinets**, arranged as close wall rows,
a north bank and two three-wide back-to-back islands. The side aisles measure
**1.88 m and 1.90 m**; cross aisles connect them. Bowling, Arcade, party and
restroom retain their separate bounds and connections. All 262 non-cabinet
placement records, the 22 ceiling fixtures, two existing circuits and carpet
baseboards retain their authored values. All 94 protected party/audit files
match their handoff hashes.

Both approved concept boards were inspected as pixels; the copies under
`local/arcade-reference-inputs/` match the originals under
`assets/references/neon_bowling_arcade/`. Original generated print/gameplay
inputs, 1024-square masters, 256-square native sheets and GLBs are retained
under `assets/environments/arcade/props/`. The
[prompts and reproduction commands](../../assets/environments/arcade/props/illustrations/CABINET_ARTWORK_PROMPTS.md)
and [asset contracts](../ASSET_SPECIFICATION.md) document the pipeline.

Three purposeful passes were inspected in the real Places Metal renderer:
illustrated sides/marquees and distinct profiles with fitted CRT frames;
detailed period gameplay/HUDs, grounded colored controls and ordinary edge
molding; then quiet marquee backing and a lowered, tilted racing wheel with
a real deck notch that clears the complete CRT while retaining its exact
1.0 m footprint. The five models total **2,864 triangles**, up 376. Individual
counts are racing 560, shooter 516, action 516, fighting 784 and sci-fi 488.
Each retains eight primitives and five materials; their combined decoded
texture storage remains **1.5 MiB**.

CRT and marquee faces emit at strengths 1.15 and 1.20. Bodies, side printing,
controls and hardware use ordinary materials. Each placed cabinet owns two
restrained existing point sources near its screen and marquee. A small explicit
native fixture using the actual authored sources compared enabled/disabled
illumination with the same GLBs: a floor patch gained mean RGB
**[0.94, 3.60, 2.81]** display levels in Full and **[2.58, 7.80, 7.08]** in
Static, while screens and marquees remained visibly illuminated. Room ambient,
exposure and bloom controls retain their authored values. Static remains the
existing brighter, flatter vertex approximation; Full uses prepared transport.
Machine spill is baked. Sci-fi pigment and the carpet retain their separate
blacklight response.

Both table bottles and the fallen bottle now embed one dedicated Bepsi atlas.
Three upright logo repeats use the existing approved vending branding, with
matching master/native/GLB UV frames. Shape, cap, glass opacity and placements
retain their original contracts. Standing views 1.25 m behind each table bottle
show the globe and wordmark without changing FOV.

The reported closed-door crack was reproduced from Arcade at
**(0.35, 1.6, 2.085), yaw 267.44°**: a narrow latch-edge strip exposed restroom
tile detail. The 8 mm leaf clearance exceeded the old 4 mm jamb overlap, and
the commercial frame lacked a rebate. Its liner now follows the resolved wall
depth, with an overlapping stop behind the closed leaf, opposite its swing,
and a head that closes the aperture. Door frames are built at load time;
the static geometry revision remains 11. A tiny triangle-ray regression checks
privacy from both directions for both swings and a clear open walking aperture;
the isolated test takes approximately 0.15 seconds and requires no map bake.

Native closed views covered both sides and multiple oblique angles. Real E
edges opened the leaf, then aimed at its live open collider to close it.
Closed movement stopped at **X=0.201 m**; the open traversal reached **X=-1.5 m**
and returned. Both close-only and traversal/return/close images were
pixel-identical to the initial closed view. All four interactive doors still
start closed. Held movement traversed both side aisles from Z=21.5 to 3.5002,
the cross aisle from X=2.3 to 7.45, and the three-room connection from X=6.7
to 25.2997, with eye Y=1.6 throughout.

All six Low/Medium/High texture × Static/Full combinations loaded correctly.
Bathroom physical off/on repetition restores identical images in both modes;
Full-off → Static-off → Full-off also restores identical pixels. Its Full
on/off mean RGB difference is 104.48. The Cosmic UV circuit restores identical
pixels, with on/off mean RGB difference 3.44. Full mirrors visibly reflect the
stall geometry. The matched party image differs by only 0.0023 mean RGBA
display levels. Native coverage is Apple M2 Pro / Metal at 1280×720.

## Executed gates and measured cost

Formatting, strict workspace/all-target/all-feature Clippy in debug and release,
default Rust tests (**2,246 library plus eight executable/integration checks**),
and all-feature tests (**2,252 plus eight**) passed. Eighteen pre-existing GPU
tests remain ignored. The source Python gate passed **349 tests with four
native classes skipped**. Catalog validation has zero warnings; the complete
346-model/497-PNG audit has zero integrity errors. Texture/prop checks and
generator freshness passed; the texture tool retains 96 pre-existing
informational manifest/resolution warnings. Geometry checking has zero errors
and the same 34 accepted balloon-anchor warnings. Normal tests/CI bake no
production map; only Arcade was rebuilt here. The focused Arcade/package Python
suite also passed all 61 checks.

| Matched production cost | Before | After |
| --- | ---: | ---: |
| Cabinet placements | 7 | 36 |
| Visible vertices / draws | 97,916 / 142 | 145,798 / 174 |
| Texture binds | 62 | 66 |
| GPU render median / p95 | 3.806 / 4.049 ms | 4.534 / 4.639 ms |
| CPU loop mean | 1.960 ms | 2.133 ms |
| Process peak RSS | 868.23 MiB | 1,014.38 MiB |
| Full atlas pages / resident atlas storage | 5 / 240 MiB | 6 / 288 MiB |
| Compressed package | 44,274,305 bytes | 71,003,099 bytes |

Frame samples use the same final binary/camera, High/Full, VSync off, 120
warmup frames and 240 measured frames, with compiler/audits idle. GPU numbers
are hardware pass timestamps; CPU loop durations and process peak RSS are
separate measurements. One bounded pair is not a general performance trend.
The package grew **60.37%**; 25,502,479 bytes of the 26,728,794-byte increase
are compressed lightmap records. The targeted storage review measured:

| Compressed section | Before bytes | After bytes | Explanation |
| --- | ---: | ---: | --- |
| Lightmap coefficients | 39,681,003 | 65,183,482 | More cabinet receiver surfaces and more exact coefficient tiles |
| Shared prepared props | 3,082,696 | 4,097,758 | Additional placed geometry and lighting records; exact chunks remain shared across qualities |
| Lightmap chart metadata | 833,585 | 1,025,284 | 9,626 additional cabinet charts |
| Other payloads and ZIP framing | 677,021 | 696,575 | Source records and small lighting/collision changes; architecture meshes essentially unchanged |

All **1,174,870 additional receiver texels** and all chart growth map to the
cabinets; architecture and other-prop receiver totals are identical. Receiver
texels per cabinet rise only **1.8%**, from 39,624 to 40,340. The new sixth page
is **91.0% occupied**, and the last three pages are 95.5%, 96.4% and 91.0%
occupied including gutters. No padded charts overlap or repeat geometry. The
existing sparsely filled second page is unchanged. There are still three
lighting groups, for the base, Cosmic UV and restroom circuits.

The compact codec's exact-fallback tile count rises from **20,399 to 32,840**;
its compact fraction drops from 66.8% to 55.5%. Additional illuminated receivers
and less uniform coefficients are consistent with the higher storage cost;
precision was not reduced. Inspection of all physical shared-prop chunks found
**zero duplicate contents** in both packages. The second segment follows the
existing 16 MiB segment cap. Neither package embeds PNG texture payloads; master
and illustration inputs remain external source assets. Native decoded prop
textures remain **32,184 KiB**. No avoidable inflation introduced by this task
was found, and the existing packer was not changed.

The normal final bake took **674.619 seconds**,
delivered Static/Full without failure, and passed integrity and
`verify --require-current`. The whole prop pack remains **104.45 MiB / 106 MiB**.

Final ignored package SHA-256:
`2bd18ed052d58be4d5d568b8dae12f4859533763b1d772d44b2811c3d8960fdc`.
The compact `local/arcade-refinement/verification.json` receipt preserves
capture recipes, gate results, hashes and numerical evidence outside `target`.
Publication, CI and cleanup receipts are added there and in the delivery message.

Three selected authorized native smoke checks passed: malformed-settings
recovery, isolated Arcade discovery and no player-side static preparation.
The separately executed empty-install check opens successfully but fails a
pre-existing assertion for an old menu sentence; the current sentence was
already present at handoff. This remains for the queued Python audit.
Winter's unchanged package now needs a compiler-identity refresh; the six
previously queued active packages still need normal recompilation. No archived
Demo or Model Zoo source/package was rebaked or reactivated.

## Curated native views

Twelve unaltered PNGs total 6,788,394 bytes. Matching pairs use the same
camera/settings, ready frame 30 and fixed 1/60 simulation.

| Before | After |
| --- | --- |
| ![Arcade before](../images/arcade-refinement/arcade-before.png) | ![Arcade after](../images/arcade-refinement/arcade-after.png) |
| ![Machine before](../images/arcade-refinement/machine-before.png) | ![Machine after](../images/arcade-refinement/machine-after.png) |
| ![Bottle before](../images/arcade-refinement/bottle-before.png) | ![Bepsi bottle after](../images/arcade-refinement/bottle-after.png) |
| ![Latch crack before](../images/arcade-refinement/door-before.png) | ![Rebated doorway after](../images/arcade-refinement/door-after.png) |

[Clear aisle](../images/arcade-refinement/aisle-after.png),
[second table bottle](../images/arcade-refinement/bottle-second.png),
[open doorway](../images/arcade-refinement/door-open.png),
[closed from inside](../images/arcade-refinement/door-inside.png).
