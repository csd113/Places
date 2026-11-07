# Tall Guest whole-prey integration

The opt-in controller follows a native Feltling using correlated sampled
acoustic echoes, stages at the observed pose, lowers its face over the crown,
latches on actual lip contact, leans back and swallows the unchanged creature.
One controller owns the action and one full rigid root drives prey collision,
rendering and sound. No hand grab, shrinking, remote grab or timer-only latch is
used. A visible blocked escape route makes capture possible; the clear-lane
control keeps Feltling faster.

## Exact assets and deformation

Tall Guest v0.3: `74bc2d7882f1952ac2a97db4a3e5d4cab4e3f769e82c362b4fff866625611be1`;
5,970 triangles, 127 skin joints / 130 retained nodes, 11 clips, 2,764 channels.
The original nine sets of 90 authored channels and source vertex attributes are
preserved; extra compensation channels preserve the extended lining during
legacy poses. No global art, joint, channel, texture or audio cap was raised.
Feltling remains `f89602b39812c5112007e324ee358bdef523a8ba9bc697d2636824aea4ee1d7b`.
Only that full-size bind geometry is certified; other prey need their own profile,
adapter and swept geometry acceptance. Original assets/source/QA stay preserved
outside build targets; Library identities are in the asset provenance.

Final posed triangles supply corner-angle normals. Grouping uses source POSITION
rounded to one micron and source NORMAL rounded to 1e-5, within each primitive.
The importer retains source coordinates before bind arithmetic: sub-micron
rounding of baked vertices had otherwise split a lining seam. Hard edges,
UV/material splits and all legacy buffers remain intact. The source certificate
retains exactly 63 inherited ivory-pair exclusions plus source-shared seams and
a 10-micron numerical guard; it is not formal directed interval arithmetic.
Six ivory face-average opposed-normal exceptions remain explicit (pink: zero).
Native +/-X capture 1.1 s and swallow 4.8 s controls review those poses; no threshold
was loosened and no geometry was hidden to pass an audit.

## Registered physical profile

`prey::CaptureProfile` owns clips, socket, full quaternion offset, setup pose,
transit curves and certified lumen selection. Capture lasts 3.2 s, contact 1.25 s;
swallow lasts 5.9 s, whole body inside 1.95 s, consume 2.55 s, then recovery.
The `mouth_capture_socket` uses +X across, +Y up its plane, +Z outward.
Its local prey translation is `[0,0.00407813646223,0.57498523523304]`, quaternion
xyzw `[-0.70960989810316,0,0,0.70459477184693]`; no translation-only shortcut.
The supplied `capture_contract.json` is the complete authoritative contract.

Approach decisions use only stored sampled positions/headings, original times,
correlation, acoustic-path validation, expiry and bounded prediction. Live
nearby geometry can admit/refuse contact. A blocked prey settles smoothly before
capture without suspending its movement owner early. After latch the full bind
pose stays scale one. Candidate steps are no larger than 1/120 s and split at
contact, inside, consumption and recovery boundaries. Refusal retains accepted
physical poses and action time. Every whole triangle has collision coverage,
including joints beyond the former 32-slot limit.

The registered trajectory is source-certified against the full prey mesh and
shell. Live world clearance checks the whole posed actor and prey against floors,
ceilings, solids and other actors; only the exact intended prey leaf is exempted.
Consumption additionally requires all 2,204 actual posed prey vertices inside the
closed certified lumen (row 2 lip cap used for analysis only). A folded exterior
lip is not mistaken for a closed lumen. Retirement removes the exact handle
from render, collision, audio, component and reservation owners once. The hunter
emits `caught` at latch, then `prey_consumed` state `id:key` after retirement.

A persistent forward obstruction (0.6 seconds) or cancellation enters checked
reverse playback at 1.5 times authored speed, retaining at most 1/120-second pose
steps. It returns prey through the actual mouth path, holds its grounded contact
pose while the head clears, then releases movement and reservations without a
position/scale teleport. An unchanged failed contact location is not immediately
re-gripped; fresh movement of at least 0.25 m or a world reset permits another
attempt. The live player is included in whole-prey clearance.

If the return path is newly occupied too, an explicit recovery-wait state keeps
the last safe pose and retries automatically. It resumes when space clears;
release through a permanently sealed space is not promised. Reset/re-resolution
or unload remains an explicit recovery boundary. Missing prey never retires a
new generation. Owner removal uses the existing clear grounded orphan release.
World re-resolution clears old ownership and acoustic state. Whole-prey maps
currently refuse checkpoints; ordinary guide and player-only hunter checkpoints retain their prior path.

## Lighting and scope

The default remains cheap mouth-only attenuation and realtime self-shadows off.
Fresh v0.3 held-native GPU data is `64ff904435eb53a17b2a3abef44d15dc17e17bed84d844cd8a3ce3bd440cae8b`:
624 interior vertices (562 attenuated, 62 unity); all 4,514 exterior values are 255.
The generator, loader/certificate guard, sidecar provenance and asset validator
bind mapping to the exact reviewed export. Fixed exterior/body shading is
rejected. The existing Advanced toggle, persistence and independent floor
grounding are retained. See `../assets/SHADOWS.md` for the general convention.

The compact private fixture is a genuinely Full-baked gameplay demonstration,
not the deferred multi-level duck map. Full atlas/irradiance resources and runtime
bound identity must be verified separately from the word compiled. Source proof,
CPU replays, native controls, performance and video have separate acceptance
scope. No new audio was added: 66,863,584 decoded bytes of 67,108,864 remain used.

## Acceptance record

Accepted on Apple M2 Pro / Metal, November 7, 2026. Strict dev and release
Clippy, the locked all-feature release workspace build, formatting, 529 Rust
tests, 373 Python checks (16 optional skips), asset/audio/prop/clip validators
and all 11 production Luau bounded refusal controls pass. Asset validation has
17 existing placement warnings. An initial isolated-target test invocation
lacked an explicit asset root and failed three editor discovery checks; the
corrected invocation and the final post-recovery run pass all 529.

Repeated 30/60/120 Hz CPU replays record one latch, one contained consumption
and one recovery. The final recovery generation also passes the normal replay,
all six disable/remove/reset/re-resolution controls and two obstruction cases.
The clear-lane control measures Feltling at 4.875 m/s versus hunter 4.5 m/s;
separation grows. Miss, confirmed-stale echo, opaque wall, refuge and duplicate
collider controls refuse invalid capture. Checkpoints are explicitly refused.
The source/QA swept-shell proof retains the exclusions and precision limits
stated above; these are separate from native visual acceptance.

The native v08 recording has exactly one mouth latch, one consumption with all
vertices inside and one recovery. A visible full-width 0.45 m metal barrier
stops the faster prey; it is an architectural fixture correction, not an engine
clearance relaxation. Earlier short-barrier/lintel captures are invalid success
controls and remain preserved. The compact package is genuinely Full baked;
its prepared manifest, atlas and bound runtime resources were checked. Its
SHA256 is `cd27aae993ec45c28a44b720ef7dafb2761c5d4d2451282c9294960853f6957f`.

The final generation007 native obstruction control uses the earlier tight
fixture at fixed 60 Hz: one latch, 36 refused steps, one abort, one swept return
and release, no consume, and no remaining attachment/reservation. A real-time
trial on that fixture stopped earlier and did not obstruct, so is not counted
as a negative control. The separate CPU return-path obstruction waits safely,
then resumes and releases after clearance. Permanent enclosure remains the
explicit recovery-wait limitation described above.

Native checks cover the reviewed normal-exception poses, front/overhead/side/
off/restored room lighting in both modes, old-config default-off behavior,
actual Advanced-menu enable/disable, saved-setting restart, and live shadow/
quality/lightmap cycles on a five-bind-group device configuration. Restored
default images match exactly after cycling. The legacy doorway traversal with
the new rig was reviewed frame by frame; no black flash appeared. These are
bounded representative checks, not a repeat of the unrelated catalog survey.

Four uncapped 1920x1080 controls on the final executable use actual Immediate
presentation, V-sync off, no frame limiter/capture/encoder, 120 warmup and 900
measured frames each. Default measured throughput is 348.56 / 347.06 FPS, frame
p95 5.043 / 5.052 ms, GPU mean 2.109 / 2.127 ms. Realtime self shadows measure
53.85 / 54.00 FPS, frame p95 25.363 / 25.849 ms, GPU mean 19.585 / 19.449 ms.
All four complete consumption and recovery. This is one Mac/fixture comparison,
not a universal benchmark or a before/after engine regression claim.

## Delivered native video and source identity

`Tall_Guest_Chase_Capture_Swallow.mp4`: 7,842,851 bytes, SHA256
`7812f07797601c632a5bae81ba39bd79f8cd7355d73084e63a733e05b41cbf67`.
Video is 1920x1080, 30 fps, H.264 High level 4.1, yuv420p, 529 frames / 17.633 s;
audio is actual native device PCM encoded as AAC-LC stereo 48 kHz / 17.643 s.
The full file decodes and plays successfully. Every decoded frame was reviewed;
the continuous shot shows the chase, contact, whole-size lift, feet transit and
recovery. No cut, intentional light flicker or dark exposure conceals geometry.
No added gulp or new sound was used. The game mix is quiet (AAC peak -27.35 dBFS,
overall RMS -48.87 dBFS), finite and nonzero; human listening was not performed.

The footage was actually filmed with generation006 executable SHA256
`21cf865ef4335322c7e641a36a2f17c69eef4ba5a679c1a0b6dd896968e960a5`;
its complete preserved source manifest SHA256 is
`d0dca87d5db32c7c8225eb72b89c49c3fc5c51ca2754c0e388aaf60a9a5ee12f`.
The final generation007 executable is
`48baa9b8ffaa671a2e704d5b7b162884cbf180de521e5093013cdfdb09b993a5`,
source manifest `765d47e30c88dccebd00ae627615e4fcc9850c4cc3950cdbe0a9e271d8c27df2`.
The later five-file source diff adds checked obstruction rollback, release
ownership and player clearance. All 108 renderer/import/profile/asset inputs
are byte identical, successful forward socket/clip/consume logic is retained,
and the final generation independently passes normal CPU and native throughput
controls. The video is not labelled as filmed from generation007.

The new private Library file is
`libfile_fde3_f-ef2c8a9db5148191b2bf1a3a885dbee7_2e37728cdad08191acacf8a0401feb09`,
version 0. Its exact original local file, source snapshots, all 14 release
executables per generation, source/QA, raw video/PCM, replay traces, native
controls, full-frame review sheets and receipts remain outside build targets
under `/Users/connordawkins/Documents/Codex/2026-11-06/task-3`.
The video is in `deliverables/`; the full machine handoff and publication
receipt are `completion-report.json` and `IMPLEMENTATION_HANDOFF.md` there.
Windows and other-prey native acceptance remain future work.
