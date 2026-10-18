# Shadow-only entities: 0.12.0

This stage adds reusable `shadow_only` on placed entities and spawn templates.
Default false preserves ordinary entities. Hidden meshes retain the exact posed
vertex/index buffers and material coverage in the handheld flashlight depth pass;
all colour, reflection and emission submissions omit them. Static transport,
placeholder fallback and projected contact shadows cannot reveal a hidden body.
No shader transparency trick, billboard, new artwork or format revision is used.
Packages declare and validate `shadow-only-v1` so older hosts refuse the semantics.

The editor exposes the property separately from collision and behavior, with
undo/save/reopen support for placements and shared spawn templates. Inspect shows
an authorable model; Environment preview and Play hide it. Luau can inspect
`api.actor(id).shadow_only`, and `api.spawn` preserves template policy from the
first rendered frame. The existing animation/action lifecycle remains the sole
controller. Collision, AI, interaction, labels, audio and attached lights remain
explicitly separate capabilities.

Supported models use the existing skinned or rigid animated character lane and
its combined 128-character limit. Missing, unsupported or exhausted posed
resources fail closed with a bounded diagnostic; no shadow is promised in that
case. Float/physics/span/material-override/fixed-light-owner combinations are
refused. These entities cast real **handheld flashlight** shadows only, with the
existing cone, 24 m range, opaque occlusion and wall/floor/object receivers.
They cast no baked fixture/global or approximate contact shadow. Independent
fade/suspension still affects coverage; deliberately authored glow/labels/audio
can expose an actor's location and should be omitted when unwanted.

## Native evidence

The separate editable `projects/shadow-only-review/` gallery has four placed
hidden mannequins, a scripted hidden spawn and an ordinary visible control.
Actual native editor facilities authored the room, six walls, placements,
fixture and placed property. An explicit advanced source pass added loop routes,
startup/spawn animation actions, interaction/collision, mirror opening and the
clear range lane. Native reopen/toggle/undo/save preserves every final source
byte, including advanced fields. Failed setup attempts remain in the evidence;
they are not counted as acceptance.

The direct macOS/Metal player passes 103 focused evidence assertions: all hidden
actors stay colour-invisible and opaque shadow casters, animation revisions and
silhouette pixels change, ordinary E interaction and independent solid collision
work, Static/Full and Low lighting/quality changes preserve policy, third-person
adds only the player's ordinary visual, reset restores the world and a single
fresh spawn, and reload restores the policy. Every final player and editor
checkpoint records actual SDL input focus. The mirror visibly reflects the
ordinary control; hidden models remain absent.

Matched hidden-present/removed scenes are byte-identical in flashlight-off,
blocked-beam, >24 m and mirror captures (zero changed pixels). The same view with
the beam on differs in 11,902 shadow pixels. These are actual rendered images;
no scene image was synthesized. The final native video is 18 seconds, 1280×720,
540 frames at 30 fps with explicit BT.709 conversion. Its consecutive source-frame
receipt and visual review remain alongside the original captures.

Final review caught and corrected an animation ownership edge: a visibility-only
frame now carries no pose override. A separate tiny native fixture confirms
placed hidden/visible mannequins both use automatic walking with matching
advancing revisions, while uncued spawned hidden/visible mannequins retain their
held pose. This fixture is explicit developer tooling, outside routine CI.

The accepted Beam source and package remain unchanged. Because the correction
touches the shared pose handoff, both its ordinary-win/reset and beam-response
routes were replayed with the final player, separate from this feature's gallery.
No later feature-test campaign, merge, release or branch deletion belongs here.

## Scaling and checks

Two reverse-order runs use one ordinary control plus 1/16/64 looping hidden
mannequins, 120 warmup and 1200 measured frames each. The Apple M2 Pro/Metal
surface actually switches from startup Fifo to **Immediate**, with a 1280×720
physical drawable on the 120 Hz display. V-sync and every frame limiter are off;
no recording/readback pacing runs during measurement. The render camera is
pinned from the first ready frame. The 1/16-actor runs exceed the 120 Hz cadence;
the 64-actor runs plateau near 120 Hz despite Immediate. Uncapped throughput at
64 therefore remains **inconclusive**: only CPU/GPU costs are reported, with no
average-FPS or improvement claim. Its CPU update/pose work is 7.41–7.50 ms and
mean present time is 0.02 ms; the raw intervals and earlier camera-setup trials
remain retained.


| Hidden actors | Frame cadence median, two runs | GPU pass-union mean |
| --- | --- | --- |
| 1 | 1.56–1.62 ms | 1.32–1.42 ms |
| 16 | 2.96–2.97 ms | 1.75–1.79 ms |
| 64 | 8.47–8.58 ms | 1.92–1.94 ms |

Strict all-target/all-feature Clippy, release Clippy, locked all-feature release
build, formatting, all 166 small Rust regressions and the 369 Python source
checks (3 supported local skips) pass. Four focused regressions cover entity/reset
and first-spawn handoff, missing-model fallback leakage, older-host capability
admission, and editor collision-independent undo/reopen. No routine test or CI
step bakes the gallery. Asset validation has zero warnings. Independent focused
renderer review found no blocking pass-routing or first-spawn leak.

Preserved direct player/editor/compiler executables, package, editable source,
inputs, measured traces and machine-readable receipts live outside `target` in
`local/shadow-only-0.12.0/`. Runnable commands are in the gallery README. Final
commit/CI, Library identity, cleanup and preserved-binary smoke outcomes are
recorded in that directory's `handoff.json`. Native certification is macOS only;
Windows/Linux and later queue work remain separate.
