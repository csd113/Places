# Fixed-cabin elevator rides — Mac acceptance

## Reported floating floor

The legacy vertical-landing path interpolated the entire cabin's Y coordinate
with a smoothstep and carried only the player upward. Its rendered deck, inner
doors and ceiling practical followed that coordinate. This was real geometry
motion, rather than a loading-screen effect. The linked-cabin path already
teleported, so the defect depended on which source representation a demo used.

A retained direct-binary baseline at `9a771f1b` reproduced it in the actual
elevator lab: cabin Y progressed through 0.021, 0.706, 2.502 and 3.453 m before
settling at 3.5 m. Rider eye height followed the same intermediate ascent.
Evidence is under `local/elevator-stationary/baseline/lab-motion`: 410 lossless
native frames, source actions, timestamps and checkpoint poses.

## Resulting behavior

Each actual landing now retains its own fixed cabin, deck, inner and outer
doors, collision, support and enclosed ceiling practical. Dynamic object
identities remain stable when a different landing is selected. Only door leaves
animate. A sealed 1.8-second ride transfers passengers once, using the same
pose-preserving admission, cargo ledger and recovery as linked cabins. A hall
call opens its fixed destination cabin without translating scenery. Existing
source `speed_mps` remains readable but does not animate geometry or change the
ride interval; the editor no longer offers a misleading speed control.

The old vertical-only cargo refusal is removed. Supported props, held ownership
and NPCs now travel through the existing bounded transport path. The actual
fixed-cabin budget is eight per map, including all stops in advanced sources.
Existing conservative side-shaft clearance admission remains in place.

The integrated doorway is the dedicated art worker's `6e9d328f`: flush hall and
cabin faces, 20 mm visible steel leaves and a floor-skirt seam repair. The
matching light containment and real landing receivers are integrated from the
lighting worker. Each fixed cabin samples its own floor and retains its own
sealed aperture when another landing is active. The approved wood interior,
metal frame, links and independent green door control are preserved.

## Actual motor output

The new committed `elevator_travel/01.wav` is an original, offline-rendered
1.8-second traction-motor/bearing rise and release. It plays once at the rider's
ears during the sealed ride, including the pre-load interval of cross-level
travel, and is stopped when that interval ends. Ordinary door-only closure and
idle cabins do not start it. It uses the effects volume and no runtime synthesis.

Native PCM checking caught an initial integration error: the source resolver
retired the new source immediately. Giving it a current passenger anchor fixed
the actual output. The accepted preliminary focused capture contains one motor
cue, with normalized waveform correlation 0.996 against the committed sound.
Its rendered interval is 1.8 seconds and the final idle portion is silent.
The held-passenger capture contains exactly three matching cues, for vertical,
cross-level and return trips, each with correlation 0.99997. The recordings are
actual CoreAudio mixer submissions at 48 kHz stereo, not substituted source WAVs.
This proves nonzero audible-band output and timing; no subjective listening
judgment or sound-pressure calibration is claimed.

Recordings and receipts are in
`local/elevator-stationary/audio-fixed/{vertical-motion,vertical-held}`.
`actual-ride.wav` is an extracted native recording. The catalog/provenance audit
passes: 31 cues, 69 WAVs, 44.39 MiB decoded against the 64 MiB library bound.

## Validation status

The preliminary direct binary retained player eye height at exactly 1.6 or
5.1 m and cabin selection at 0 or 3.5 m, with no intermediate ascent. The
every-tick CPU regression checks actual renderer cabin parts at both fixed
heights throughout rides at 30 and 120 Hz. The cross-level regression requires
the sealed ride interval before emitting one departure.

A native held-crate/NPC campaign passed vertical transfer, exact rider and NPC
height, save/resume upstairs, cross-level ownership, foreign save/resume, custom
NPC state and one-instance return. The final integrated campaign, package
currency and publication status are recorded below when complete.

## Scope and remaining checks

No full Quiet Collection light bake is needed for this behavior and asset
validation. Tiny explicit lighting fixtures exercise Full and Static paths;
the collection uses a Static acceptance export. Native captures are correctness
evidence, not FPS or throughput measurements. Windows and subjective listening
on other speakers remain untested.
