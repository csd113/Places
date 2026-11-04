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

## Combined production runtime

The retained production player is
`local/elevator-stationary/production/places`, SHA-256
`e93e39af5171ee686759dcd6052dbb1805d332d3e1c3e2bbcd262e0e10002561`.
Its engine includes stationary transport, final doorway art, enclosed lighting,
the motor source fix and published predator-search work. Later editor-only
targeting corrections do not change this runtime or compiler preparation
identity. Exact final editor-source checks and publication remain separate.

- `final/inside-motion`: 318 native lossless frames; cabin selection changes
  only from 0 to 3.5 m, and rider eye height only from 1.6 to 5.1 m.
- `final/exterior-motion`: 298 native lossless frames, with an explicit
  acceptance observer staged outside after an ordinary cabin-button press.
  The source threshold and adjoining floor pixels are exactly identical in all
  261 compared frames after the camera settles, through the complete ride and
  upper arrival. The source cabin does not rise or disappear.
- `final/vertical-held` and `final/rotated-held`: supported crate/NPC ownership,
  held state, saves, state retention and return pass; the rotated destination is
  90 degrees and 3 m higher. `final/blocked-vertical` rejects an obstructed upper
  arrival, restores the original floor/poses/owners, reopens and accepts live
  movement afterward.
- `final/door-three` and `final/door-vertical`: ordinary independent open/close,
  obstruction, save/resume, predeparture cancellation, locked ride controls,
  linked/cross-level travel and return pass. Actual PCM contains exactly three
  and two motor cues respectively, despite the additional door-only cycles.
- `final/light-{full,static,lit,bloom}`: all four 31-image campaigns pass with
  the fixed cabins. Full and bloom closed panel/frame/floor witnesses are zero;
  the open floor receives 7.675/2.675/0.029 display RGB. Static closed exterior
  floor matches its reference at 37.734/30.488/17.944. A real cool hall fixture
  lights the closed left panel at 31.478/40.427/48.003. Cabin interior, floor
  joins, metal returns and the exposed thin panels were visually inspected.

The final motion PCM contains one matching 1.8-second cue (correlation 0.9962)
and a silent post-arrival witness. Vertical and rotated passenger recordings
contain three matching motor cues each. `tools/bench/elevator_audio.py` reproduces
the waveform/count checks, including the original missing-source negative
control. Use `--idle-after-last-ride 2` for the short inside-motion fixture;
its cue-relative witness accounts for variable startup/shader time. Recorded
native audio includes startup, so absolute capture seconds are not action
timestamps. Occasional existing spatial shaft ambience remains independent of
the bounded rider motor; no continuous idle motor loop is introduced.

## Scope and remaining checks

No full Quiet Collection light bake is needed for this behavior and asset
validation. Tiny explicit lighting fixtures exercise Full and Static paths;
the collection uses a Static acceptance export. Native captures are correctness
evidence, not FPS or throughput measurements. Windows and subjective listening
on other speakers remain untested.
