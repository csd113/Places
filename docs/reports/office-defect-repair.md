# Office defects, recovery and colour response

Office: After Hours now blocks the reported pressed-wall jump escape, grounds its
railings, removes the artificial ceiling stripe and preserves dark basement gaps.
It adds three deliberate middle-floor furniture placements and two sparse roach
encounters. A reusable geometry-derived escape fallback restores a validated spawn.
Linear colour controls confirm one albedo multiplication and coloured diffuse bounce.

Work began at `7aa4ad931a586558e399ae68fd6bb5132ec126c9` on `arcade-env`.
The retained local receipt `local/office-repair/verification.json` records package,
binary and image identities, native movement samples, actor states and measurements.
Machine receipts remain local under the repository's existing ignore policy.
Publication SHA and remote CI outcome are recorded in the final handoff rather than
making this document refer recursively to its own commit.

## Pressed face, jump and shared terrain contacts

The exact Office defect is beside the upper flight, reached from the bottom floor:
the ramp spans X31–34, Z4–14 and its side at Z9 reaches Y5.4. Start east at
`[35,1.6,9,270]`, or west at `[30.55,1.6,9,90]`. Hold forward for 2.5 seconds,
then jump at 1.25 seconds, with a fixed 1/60-second simulation step.

Before repair, east walking stopped at X34.0249, only 25 mm from the side; west
stopped at X31. The 0.30 m body already intersected the face and the 0.10 m camera
near plane could look through it. Jumping while pressed crossed the rendered
terrain and both traces fell to eye Y−22.1676. This reproduces both reported phases.

The rendered closed ramp sides were absent from the horizontal body sweep.
Grounded centre-height rejection could stop the centre without protecting the
radius. Airborne support queries rejected the slope above the feet and permitted
entry into the wedge. A second small native topology confirmed the same missing
contact on a stair side: the stair mesh was crossed, although that particular
stair trace did **not** record a void fall. Its ramp counterpart did.

Shared sweeps now use the real ramp/stair side and end faces, clipped to the live
body and step plane, plus actual stair riser bands. One-sided incoming contacts
preserve inside exits and legal climbing. Falling discs catch terrain edges;
grounded stair walking retains its centre-sampled smooth nosing pitch line.
No camera offset, oversized invisible wall, collision schema or speculative
ordinary-wall/door/ceiling rewrite was introduced.

Final Office traces remain east of X34.3 or west of X30.7 throughout pressing and
jumping; sampled eye Y never falls below 1.6. Recovery therefore never substitutes
for the collision repair. The standalone stair/ramp control holds X6.8/X2.8 with
the same full radius and ordinary jumping. Existing maximum/signed-slope, short
tread, ceiling/header, wall, door, water and Home stair regressions pass.

![Pressed face before repair](../images/office-repair/pressed-before.png)
![Pressed face after repair](../images/office-repair/pressed-after.png)

The ordinary native control also blocks a pressed wall at Z2.7 and a jump under a
1.85 m slab at eye Y1.6246. Its new door starts closed. The leaf correctly refuses
to swing into a pressed player; stepping clear, opening and walking through reaches
Z4.5275 with floor support intact. Ordinary wall/door failures were not confirmed.

## Escape recovery

Recovery uses a union of local room envelopes, including their regions, ramps and
stairs, ladder volumes, exact rectangular/circular water footprints and real solid
support bounds. Gaps between rooms and different storey elevations are preserved.
An ordinary brief edge jump receives one second of grace; sustained escape returns
to the resolved spawn. Non-finite motion is handled before collision queries.

The destination is cached only with real support/water and standing-body clearance,
then revalidated when selected. Reset clears motion, vertical accumulation, stance,
bob, ladder/water transitions, ceiling contact and held action edges. Trigger
membership is reseeded without a teleport sweep; doors and AI retain their state.
The single destination-selection method is the extension point for a future
checkpoint destination. No checkpoint or save system was added.

In the native open-edge fixture, the player falls to sampled eye Y−3.2187, returns
once to `[1,1.6,1]`, and remains there for the final two seconds. A normal jump
finishes at `[2.45,1.6,1]`; swimming remains near `[10,−0.4,1.5213]`, without a
spawn return. Small deterministic tests additionally cover recesses, room gaps,
storeys, circular-water corners, held actions, entity/trigger preservation,
non-finite motion and invalid-spawn loop refusal.

The detector deliberately uses conservative local envelopes, not an arbitrary
inside-solid penetration detector. A missing or obstructed safe destination warns
and refuses the teleport. Collision defects must still be repaired independently.

## Ceiling stripe, fixture mounts and basement reach

The top doorway stripe occupied one continuous ceiling chart and ended on a
coarse 1.2 m baseline-zone boundary. It was not a normal/UV/packing seam. Occupancy
had considered other storeys' walls in XZ alone, and a cell straddling a partition
could assign a ceiling sample to the wrong side. Occupancy now uses the actual
roof-probe height; local lookup selects a visible air cell on the sample's side.
Full recovery targets use the existing owning-side receiver inset.

Saved coefficient comparisons at five affected X positions across Z8.30/8.42 show
the median luminance step falling **0.229968 → 0.006537**, a **97.16% reduction**.
Sampled ceiling charts 44/46 retain identical geometry, dimensions and placement.
The remaining differences match nearby smooth gradients. Legitimate doorway
ceiling shadow remains dark, **0.020711 → 0.020193**; floor below a panel is
2.41% dimmer. Top-floor switched contributions are exactly zero.

![Ceiling before](../images/office-repair/ceiling-before.png)
![Ceiling after](../images/office-repair/ceiling-after.png)

Strict source geometry also exposed **157 overlaps and one sliver already present
before this work**: 31 fluorescent backplates shared the ceiling plane. The
ordinary opaque pipeline has no depth bias, and transport ownership was ambiguous.
Those mounts now use the documented 10 mm ceiling clearance; the two wall sconces
are unchanged. The approved opaque backplate, non-emissive housing and diffuser
remain intact. Strict geometry now reports **zero errors and zero warnings**.
No checker finding was suppressed to obtain that result.

Only seven basement light ranges increased: fixed pools **2.7 → 3.8 m**, failing
pools **3.0 → 4.2 m**. Their brightness, colour, emission, timing and Static holds
are unchanged. Native Full captures retain dark termini and visible light failures.
Exposure and ambient settings are unchanged.

![Basement before](../images/office-repair/basement-before.png)
![Basement after](../images/office-repair/basement-after.png)

## Rails, furniture and actors

Four sloped rails had posts 20 cm outboard of the ramp. Their centres now sit at
X25.035/27.965 and X31.035/33.965, with bounded penetration and supported landing
joins. All **56 posts / 504 footprint samples** intersect supporting terrain by
**9.2–30.8 mm**; the clear ramp lane remains **2.86 m**. Native close views show
grounded feet. Both directions on both flights pass, and the three-floor walkthrough
reaches `[8.45,1.6,9]` without a floor leak. The final mount-only refresh retains
byte-identical collision/navigation records; lower-flight traversal was recaptured.

![Rail before](../images/office-repair/railing-before.png)
![Rail after](../images/office-repair/railing-after.png)

Middle-floor dressing adds a sideways table at `[8.4,3.42,1.9]`, a rotated sunken
desk at `[9.35,3.18,8.5]` and a chair clipped into the partition at
`[5.04,3.48,7.4]`. All nine middle openings retain standing-player routes. The
table is a proper offline +90° Z rotation of the repaired shipped model, with exact
visible-corner UV/pigment and embedded oak PNG preservation. Two negligible source
cap triangles are omitted: it has 338 triangles and six boundary edges, not a claim
of fully closed topology. No new raster artwork or runtime texture generation exists.
The top layout and basement maze remain unchanged.

The table occupies a deliberately dark pocket. The paired native final/albedo
views show its normal composition and make its preserved wood texture, orientation
and floor intersection inspectable without changing scene lighting.

![Sideways table, final](../images/office-repair/table-final.png)
![Sideways table, authored-PNG albedo diagnostic](../images/office-repair/table-albedo.png)

The roach was bound to its intended opaque white PNG with valid UVs and per-part
pigment; it was not a missing-texture placeholder. Palette values selected as sRGB
had been exported into glTF's linear `COLOR_0`, making the shell too pale. A narrow
linear material factor anchors the existing palette to dark-brown `#462412`, with
nonmetal roughness 0.48 and no emission. Geometry, skin, animation and PNG bytes
are preserved. Native close views confirm dark-brown environmental response.

![Roach before](../images/office-repair/roach-before.png)
![Roach after](../images/office-repair/roach-after.png)

Roaches increase **two → four**, with one additional filing-cabinet plinth refuge.
All four native encounters perceive approach, scurry at 0.62 m/s along complete
compiled refuge routes, physically arrive and hide, then emerge after their
15/19/23/27-second cooldowns. The two added encounters reach refuges near
`[9.7,0,1.2]` and `[14.5,0,10.1]`. Existing small AI regression covers unreachable
nearest shelters and visible return. No broader AI system was added.

All four chains were inspected before only the final 10 mm lamp correction;
their final serialized collision/navigation and authored AI are unchanged. The
original chain and mannequin poses were also recaptured from the final package.
The mannequin follows its basement route without stuck/caught state, retaining
arms forward while moving and arms down when idle.

![Moving mannequin](../images/office-repair/mannequin-moving.png)
![Idle mannequin](../images/office-repair/mannequin-idle.png)

## Linear material and bounce controls

The explicit developer control uses two 3×3 m rooms and the real committed flat
white fallback PNG, with neutral `[.6,.6,.6]`, ideal red `[.6,0,0]` and ideal blue
`[0,0,.6]` linear material albedos. Fluorescent mounts have the same 10 mm clearance.
Normal white, pink `[1,.1,.45]` and blue `[0,0,1]` sources test receiving colour;
a white-lit red wall/baffle casts a shadow on a neutral receiver to test bounce.

Direct incident pink RGB is `[.321307,.032131,.144588]`, preserving `[1,.1,.45]`.
The existing room/recovery baseline compressed channels independently, desaturating
pink to filled ratios `[1,.444448,.693049]`. One shared strongest-channel amplitude
now preserves hue, with exact white-light scalar calibration. Final filled ratios
are `[1,.098895,.447293]`; small differences are legitimate coloured-surface bounce.
Solver revision 20 and the baseline-policy fingerprint invalidate prior packages.
Direct and indirect stage arrays remain **bit-identical** across this correction.

Pure blue produces **exactly zero** outgoing light on ideal red-only albedo. White
direct light remains equal across RGB channels. The shadow receives zero direct
light, but red-tinted indirect `[.090183,.070001,.070001]`; filtered and filled
values match, so no recovery fill manufactures that bounce. This validates the
existing RGB approximation, not a spectral renderer or a new GI architecture.

![Pink light on neutral, red and blue material](../images/office-repair/colour-pink.png)
![Pure blue; the ideal red strip is black](../images/office-repair/colour-blue.png)
![Native incident-light diagnostic, excluding albedo](../images/office-repair/colour-incident.png)

Matching native BakedLight pixels encode numeric HDR/(1+HDR). Inverting that,
applying the known albedo and unchanged display pipeline predicts neutral
`[165,50,112]`; the actual final pixel is `[165,51,113]`. Incorrect double albedo
would produce `[130,37,88]`. Red/blue strip checks agree within the 3-byte
quantization allowance. The native Albedo diagnostic shows sampled authored PNG
RGB only; it intentionally excludes material/vertex tint.

The diagnostic exporter also now reconstructs verified shared `PLPS` prop recipes
into exact runtime `PLMP` bytes, rather than failing before atlas export. Its existing
tiny metadata/navigation reuse test verifies byte identity without another bake.

## Gates, cost and package currency

- Formatting, patch whitespace, strict configured debug and release Clippy pass.
- Default Rust: **2,260 library tests**; all features: **2,266**. Each also passes
  two compiler CLI tests and seven integration tests. The 18 existing ignored
  developer campaigns remain outside ordinary CI.
- Source Python: **350 tests, four existing native-class skips**, 110.09 seconds.
  Asset validation: **571 entries / 348 placeables**, zero warnings. Whole prop,
  texture, GLB/PNG audit, Office generator, cockroach and mannequin checks pass.
  The texture checker retains its 96 existing warnings; they were not hidden.
- Strict Office and small body-contact geometry: **zero errors / zero warnings**.
  Ordinary tests and hosted CI do not bake production maps.
- Added Rust coverage comprises two body-contact regressions, three compact
  recovery regressions and two scalar/35-cell lighting regressions. They protect
  distinct failures without shipped-map bakes. Existing tests were expanded for
  exporter identity and post-fall recovery. The mount regression parses one JSON
  and fails on all 31 original mounts; it adds no mesh/native/bake dependency.

Final Office normal Static/Full build took **336.65 seconds**, eight CPU workers,
peak RSS **2,167,685,120 bytes**. One earlier 341.77-second build exposed the real
mounting defect; the second was necessary to correct it. Both unchanged follow-up
builds reused their package rather than rebaking. Only Office was refreshed in the
production collection; small explicit native/colour controls were disposable builds.

One matched 240-frame native basement observation, after 120 warmup frames and
with builds idle, records:

| Observation | Before | Final |
| --- | ---: | ---: |
| Deduplicated GPU timestamp samples | 197 | 211 |
| GPU pass-sum median / p95 | 17.188 / 19.753 ms | 10.939 / 18.487 ms |
| CPU loop mean / update mean | 8.334 / 1.283 ms | 6.965 / 1.861 ms |
| Peak RSS | 753,778,688 B | 779,927,552 B |

These are bounded observations, not an isolated performance-improvement claim.
GPU timing/utilization/RSS reporting code was untouched. GPU baking remains an
audit recommendation, not a production implementation.

Final Office package is **25,965,291 bytes**, SHA-256
`d3574083eacb452969b13ee704a96fd640aeababfd2861d677c440b420d20726`.
Normal validation has no warnings; `--require-current` passes; the unchanged build
returns `rebuilt: false`. Before native views used the exact initial installed stale
package `8a6372f7…` and matching catalogue snapshot. Their capture working-source
hash must not be attributed to that package. Office A/B includes the needed normal
refresh of prior approved packing/backplate work; compact regressions isolate the
new engine repairs, and saved packages cannot recover intermediate solver stages.

All eight active sources remain active. Beach, Frutiger Aero, Home, Lantern Hollow,
Pool, Arcade and Winter packages remain byte-unchanged and **stale for catalogue
bytes and compiler executable**, requiring their normal future refresh. Archived
Places Demo and Model Zoo were neither rebaked nor reactivated. Approved Arcade
assets, bottles, rebated doorway, switches, square vents and cloud-only entity work
were not changed. Unrelated user audit files are preserved and excluded from this
commit. No later queue task was started.

The curated 17 PNGs are unmodified native captures (10.31 MB total); compact
receipts remain in `local/office-repair`. Final requested deliverables and the Office package
remain outside `target`. Publication/CI and cleanup custody are confirmed in the
final handoff and retained local publication receipt.
