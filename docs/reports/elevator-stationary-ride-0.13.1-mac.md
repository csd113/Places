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
NPC state and one-instance return. The final accepted campaign and package
currency are recorded below.

## Combined production runtime

The retained production player is
`local/elevator-stationary/production/places`, SHA-256
`e93e39af5171ee686759dcd6052dbb1805d332d3e1c3e2bbcd262e0e10002561`.
Its engine includes stationary transport, final doorway art, enclosed lighting,
the motor source fix and published predator-search work. Later editor-only
targeting corrections do not change ride/renderer behavior or compiler preparation
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

## Airborne admission follow-up

After all eight collection routes and the final editor campaign passed, ordinary
Space input exposed a vertical-only admission edge. At 1.9 seconds the rider
was still inside the sealed source cabin with eye Y=2.109 m. The floor-only
arrival test treated that airborne rider as an outside hall caller, selected the
upper cabin, and eventually recovered the player to the lobby spawn at Z=-2
instead of retaining their cabin-relative Z=1.2. The actual direct-binary
checkpoints are retained in `final/jump-before`.

The source now reopens if the rider still occupies its interior but cannot be
admitted at the transfer boundary. It preserves the source selection and all
occupants, matching the safe refusal used by linked/cross-level rides. Real
outside hall calls retain their fixed-destination behavior. An existing tiny
controller regression now includes this case and a successful grounded retry;
the workspace still has 450 tests. No models or map bake enter that test.

`elevator_cargo.py --vertical --mode held --airborne` checks ordinary jump input,
unchanged source rider/NPC/held ownership and reachable controls, then retries
and continues the existing save/cross-level/return campaign. This final engine
change affects the advanced vertical `Moving` branch. The collection's eight
cross-level routes use the unchanged `Loading` branch. Source-current exports
must nevertheless be refreshed against the updated compiler identity.

## Accepted pre-carry snapshot

This validation snapshot predates the later `c4cf6676` carry integration and
the user's thick-pillar placement clarification. Its artifact identities and
results remain retained evidence; current-source delivery is being refreshed
after that follow-up. The old 24 cm placement refusal is not the final contract.

The airborne correction is `2fa0437b`. The editor review correction
`0b107bbd` is integrated through the identical tested source candidate
`5f0e25e3` and its final evidence cherry-pick `0852edf3`. Only the separate
changelog commit `8fffa7b4` was imported, as `2673cd29`; its inherited branch
history was not merged. The root `CHANGELOG.md`, under **0.13.1 — Unreleased**,
records the actual 32-distinct-traveller project/session ledger cap, including
travellers left in visited levels and checkpoint persistence, excluding the
player. Its main portable-subset exclusions are explicit.

Accepted artifacts are under `local/elevator-stationary/`:

| Artifact | Relative path | SHA-256 |
| --- | --- | --- |
| Direct player | `production-accepted/places` | `eb3f722f1e466480d724386d67f01430f005b2e286f93ef471ed2b85a1bd4e1e` |
| Direct compiler | `production-accepted/places-compile` | `e4838f18786ba538c1a367d63d4580c5d56d8aef99c1a3d37fe35f4288927ab6` |
| Direct editor | `production-accepted/places-editor` | `9fb186ffb274104487f1e71ba965019458fde42b10ca7918cecba76cd9d17164` |
| Collection 1.0.5 | `final-airborne/environments-1.0.5.placesmap` | `d5faa999e7a9fd12ff089cb239136a65fc62e264ca7a4e0a69d968304575b027` |
| Vertical lab 1.0.6 | `final-airborne/lab-1.0.6.placesmap` | `72081ab4ffa725da2c4b4a379128d881111806e8ac46f4f504d9b1f985baff73` |
| Linked lab 1.0.6 | `final-airborne/links-1.0.6.placesmap` | `47c0fe63e241dabac2325825ec57e235d658c22cff4b62eeef6d46d17ff7f8a2` |

The final collection archive is 35,829,235 bytes. Package validation reports
385 intact dependencies. All nine collection members and both lab projects
pass `verify --require-current` with the accepted compiler. The collection was
prepared with Static only; no full collection lighting solve was performed.

- All 450 workspace tests pass, including the bounded airborne/source-retention
  extension to an existing controller test. Strict workspace/all-target/all-feature
  Clippy and the locked all-feature workspace release build pass.
- The final player/package pass all eight native round trips with two saved
  favors, ordinary boarding/exiting and aimed controls, exact arrival offsets,
  collision, both loading directions, identical retained loading frames,
  reachable return calls and matching hub bays. All diagnostic assertions remain
  enabled. A separate all-eight-favors Home round trip retains the unlocked,
  fully open reward room. Receipts: `final-airborne/collection-{partial,all}`.
- The final airborne held-cargo campaign retains the player at
  `[-0.45,1.6,1.05]`, the held-body owner and the NPC at the source, with reachable
  controls. The grounded retry arrives upstairs, then saves/resumes, travels
  cross-level, saves/resumes there and returns without duplicates or lost state.
  Receipt: `final-airborne/airborne-held/passed.json`.
- Latest inside/exterior recordings retain 405 and 409 lossless native frames.
  The source threshold/floor patch is identical across all 385 compared frames
  after the exterior camera settles, including the upper arrival. The normal
  passenger has only endpoint heights, without an intermediate ascent.
- Eleven final native PCM recordings contain 23 matching motor events: one
  normal vertical ride, four events across the interrupted/retried cargo campaign,
  and two per collection round trip. Minimum normalized waveform correlation is
  0.98075. All recordings are unclipped, and the inside-motion post-arrival
  witness is silent. Per-route audio errors/drops are zero.
- The final production editor passes the four-facing native wall-fit campaign,
  exact-gap acceptance, undersized-gap refusal and thick-wall refusal.

`final-airborne/accepted-manifest.json`, `motor-summary.json`, per-case
`passed.json`, source-currency receipts and raw recordings retain exact evidence.
The first collection attempt was rejected for premature startup/return sampling
and a Home script wall-time diagnostic during concurrent compilation. The driver
now settles startup and return loading, and both subsequent full campaigns pass
with script diagnostics asserted. No Lua budget was relaxed or error hidden.

The publication target is `0.13.1-dev`; no main merge or release is part of this
work. Package files remain untracked under the repository's established policy.

## Current integrated development delivery

The latest published carry work through `c4cf6676` is preserved by merge
`cbfc9874`. Conflict resolution retains the fixed-cabin renderer, per-landing
light receivers and airborne source-retention correction. The final doorway
art is unchanged. Task-6's volume-based wall/pillar implementation `b1f50bbb`
and final production receipt `e658cdc0` are integrated as `43b6bdad` and
`1b14caf4`; neither changes player, renderer, compiler or asset sources.

The final placement contract has no wall-thickness-only refusal. The editor
hollows a selected sufficiently large wall, rectangular column or circular
pillar into saved architecture, while real cabin/pocket overlap still refuses
placement. The dedicated final production campaign accepts eight placements,
refuses two undersized pillars and the 2.47 m adjacent-room gap, and opens all
eight exported cabins. Its 32 final inside/outside images and additional curved
shell/floor views were inspected. See the
[pillar report](elevator-pillar-fit-0.13.1-mac.md). The old 24 cm refusal above is
historical, not a current limitation.

Refreshed exports are in `local/elevator-stationary/current/`: collection 1.0.5,
vertical lab 1.0.6 and linked lab 1.0.6. All nine collection members and both lab
projects verify current; collection integrity reports 113 entries, 385 intact
dependencies and no warnings. Static preparation is serialized as the `off`
variant. No full collection bake was performed. Asset hashes match the final
accepted task-6 art. `runtime-source-hashes.json` and a clean source comparison
against `cbfc9874` establish that the final editor integration leaves the tested
engine, game, compiler, Cargo and asset sources unchanged.

Latest direct-binary checks use the retained carry-integrated player at
`production-carry-core/places` (SHA-256
`0ff8bc26e96f8ccceda11423a205d1edb77801ae857d899d4b5013f2aeb69c70`):

- `current/airborne-held` passes source cancellation, grounded retry, held crate
  and NPC vertical transfer, local/foreign save resume, cross-level travel and
  one-instance return. Native PCM contains exactly four matching 1.8-second
  motor cues, with minimum correlation 0.98506.
- `current/rotated-held` passes the 90-degree, 3 m elevated destination and
  return with ownership/state retained. Its PCM contains exactly three cues.
- `current/blocked-vertical` retains original player/NPC/held poses on a blocked
  arrival, reopens the source and accepts movement afterward.
- `current/auto-drop-static` preserves the current body pose and releases it
  safely at 2.86627 m, beyond its 2.85155 m size-adjusted limit; it remains held
  at 2.72673 m and restores unheld after save/resume. The first harness attempt
  requested an absent Full variant and was rejected at loading. The helper now
  accepts `--lightmaps off`; no new bake or gameplay relaxation was needed.

The complete eight-route/reward, fixed-floor film and four lighting campaigns
above remain retained evidence for the same final art and ride/lighting code.
After the carry merge, targeted passenger/recovery checks cover the changed
physics path; editor-only pillar changes do not alter the collection's authored
routes. The latest combined production editor and collection smoke receipts are
recorded with the final build below.

### Final combined gates and artifacts

All **455 workspace tests**, formatting, strict workspace/all-target/all-feature
Clippy and the locked workspace/all-feature release build pass. The release
build completed in 6m 26s (`release-pillar-combined.log`). Final production
executables are retained in `production-current/`.

The combined production editor passes all eleven wall/pillar cases in
`current/editor-pillars/`: eight accepted placements, two small-pillar refusals
and one adjacent-room gap refusal. Its saved level JSON exactly equals the
owner's accepted final production fixture, so the owner's exported geometry
and native inside/outside evidence is reused without a duplicate geometry
campaign. The latest player/package passes the Home round trip with all eight
favors, unlocked/open reward room, ordinary collision/boarding/controls,
pose-preserving arrival, identical retained loading frames, live walkout and
return. No script diagnostic or audio-error assertion was disabled.

The three latest PCM recordings contain nine matching motor cues in total,
all unclipped. Home has exactly two for the outward and return rides. Current
receipts and binary/package identities are in `current/accepted-manifest.json`;
the earlier full eight-route and lighting receipts retain their precise scope.
All nine collection members and both labs were verified again using the final
production compiler. The compressed collection archive is 35,829,230 bytes.

| Artifact relative to `local/elevator-stationary/` | SHA-256 |
| --- | --- |
| `production-current/places` | `a027887fdde476be183618cc54d9304992f960460f406f563da253146a412fc3` |
| `production-current/places-compile` | `561b18deec71d73cdf37a090fb1970b1563bf7607a3b8365cc22e8190a86044c` |
| `production-current/places-editor` | `91bad08e85d7f4775c630e696dd34b91eb83c8cb960700b115c04924040d8e04` |
| `current/environments-1.0.5.placesmap` | `9ca1cf0140825eee1600ee2c8b37fe14e61d30028854e98dce0ccdc0ebe125ef` |
| `current/lab-1.0.6.placesmap` | `ba8de549588c7a2f5b44f262c92715d3d36e0b5faacf702a83e3f6f6ba724587` |
| `current/links-1.0.6.placesmap` | `3a6f2dca443429407618436e8eaf72f3afcfacd392f69d07f163935acaac5aae` |

Publication is limited to `0.13.1-dev`, without force, a main merge or release.
The operation result and verified remote commit are recorded after the push in
`local/elevator-stationary/publication.json`.

## Scope and remaining checks

No full Quiet Collection light bake is needed for this behavior and asset
validation. Tiny explicit lighting fixtures exercise Full and Static paths;
the collection uses a Static acceptance export. Native captures are correctness
evidence, not FPS or throughput measurements. Windows and subjective listening
on other speakers remain untested.
