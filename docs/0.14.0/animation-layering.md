# Animation layering in Places

**Recommendation: prototype a small native overlay separately, if approved.**
Keep the existing clip transitions and gameplay ownership. The Tall Guest already
layers authored inspection onto locomotion, while the companion mannequin uses
separate full-body crowbar clips for gait and swing. These are two real rigs on
which to test reuse. Neither calls for a general animation graph, an editor, or
a Bevy dependency. [Issue #45][issue]

This is a source and asset review at Places `7a0cca2cec503b319aebc9ecf25fd861f9d37e5b`
on `0.14.0-dev`, compared with Bevy [v0.20.1][release], verified at
`a0577b5c6a7b208719dd761c5c6a7a539e472a97`. No prototype, native validation, or
performance measurement was run.

## Keep what already works

- **Places has two playback paths.** `ClipPlayer` selects locomotion clips and
  exponentially crossfades the current and previous samples on a shared clock.
  The animator separately smooths the procedural locomotion weights. Explicit
  cues crossfade from the displayed pose, including an interrupted fade.
  `Gait` uses measured speed and the clip's reference stride, `Action` takes
  gameplay progress, and exact `Sample`, `SampleBlend` and `Articulated` cues
  let gameplay dictate the rendered pose without a second playback clock.
  Rigs already share compiled channels while actors retain separate playback
  state. [Animator][animator] · [Pose cues][cue]
- **Tall Guest already has a mask in practice.** Its current helper blends
  local translation, rotation and scale for `spine_*`, `swallow_*`, `neck`,
  `head` and `mouth_capture_socket`. It samples an entire clip, clones the base
  locals, recomposes the hierarchy and clears the cue. It does not restore base
  morph weights after replacing unmasked locals. Tall Guest supplies the
  posture weight and the inspection weight, samples inspection at 2.0 s, and
  applies clearance and support corrections after layering. The held
  inspection sample does not run the full-body inspection's echo markers.
  [Current overlay][overlay] · [Pose order][guest-pose] · [Aim and markers][guest-aim]

## Borrow the separation, not the runtime

Bevy separates pose composition from playback. A directed acyclic graph
combines clip leaves using normalized `Blend` or ordered, unnormalized `Add`
nodes. Masks exclude groups of named targets for a node and its descendants.
Target IDs normally hash the authored hierarchy path. Group membership is
explicit, and an unregistered target is not implicitly masked out. Graphs
serialize to `.animgraph.ron` with clip asset paths. The examples show live
weight controls and path-based mask groups, not a general authoring editor.
These are shipped APIs in the pinned release. The graph example's save-to-disk
path is native-only. [Graph source][bevy-graph] · [Graph example][bevy-graph-ex]
· [Mask example][bevy-mask-ex] · [Target IDs][bevy-id]

Bevy's separate transition helper owns a main animation and fading weights.
Events are separate again. Untargeted events can fire from non-paused fading
clips regardless of the pose mask or even zero active weight. Targeted events
skip masked targets and zero active weight. The plugin evaluates poses and
queues animation events before transform propagation. None of this substitutes
for Places' accepted-step ordering, socket checks, or effect deduplication.
[Transitions][bevy-transition] · [Untargeted events][bevy-root-events] ·
[Targeted events][bevy-target-events] · [System order][bevy-order]

| Option | Decision and cost |
| --- | --- |
| Keep current transitions and overlays | **Keep as baseline.** No new authoring or dependencies. The name filter and full-clip sampling remain specific to the existing overlay. |
| Small Places-native mask | **Prototype separately.** Reuse the engine's local pose sampler and caller-owned time and weight. Relatively low integration cost, with rig validation and socket parity the main work. Author two reviewed rig-specific target lists before considering any editor UI. |
| Fuller native graph and editor | **Defer.** Useful only if multiple proven layer combinations need authoring independently of code. Node/priority rules, asset versioning, validation, preview, undo and editor/compiler compatibility are more work than the two supported cases justify. A rig, clip, body-region and preview panel would come before a node canvas. |
| Bevy or a library integration | **Reject for this scope.** Even `bevy_animation` without the renderer pulls in Bevy ECS, assets, time, transforms and reflection, plus `petgraph` and RON. Adapting Places curves, rig IDs, morphs and graph scheduling back into the authoritative pose would add a second runtime contract. A standalone sampler or math component has less coupling but would still need those Places-side ownership and rig adapters and would replace sampling that already works. No engine migration. [Bevy crate manifest][bevy-deps] |

Use **ordinary local-space interpolation** for the first experiment. Existing
GLBs store absolute local poses, so interpolate only the addressed channels.
Additive motion instead requires an explicit neutral/reference pose, weighted
translation and quaternion deltas, a defined composition order, and a safe
scale policy. Bevy's additive node does not subtract a reference pose for you.
Defer additive authoring until a real use needs deltas. [Bevy curve evaluator][bevy-add]

Resolve reviewed node names or full paths to reusable indices on the loaded
rig. Include intended helpers and sockets, not just skin slots. Reject missing
or ambiguous required nodes/clip bindings and protected movement-root targets
up front. Diagnose and disable an invalid optional layer without switching
clips. For an unkeyed overlay channel, retain the live base by default.
Preserve any intentional rest-local behavior only as an explicit, verified
exception. Leave morphs alone in the first experiment. Do not copy Bevy's
exclusion convention or use prefixes as the authoring contract.

## Gate for a separately approved experiment

The engine layer should consume samples and weights, not select targets,
advance transitions or emit events. Keep root motion and collision, target
tracking, procedural corrections and effects with gameplay. Keep base and
overlay transition ownership independent. An interrupted overlay should fade
from its current contribution while the live base gait advances. Do not restart
the gait, reapply an old overlay onto itself or use outgoing clip markers as
gameplay signals. Preserve action priority and existing clearance rollback;
commit effects once from accepted gameplay clock crossings, not every pose
sample. The renderer consumes the final accepted pose.

- **Mannequin tool-on-gait.** The current 37-joint GLB includes `run`,
  `crowbar_run` and `crowbar_swing`, all with full-body TRS tracks. In the GLB,
  `run` and `crowbar_run` have identical key times across the 24 root, pelvis
  and leg TRS tracks, with raw components differing by less than `1e-6`.
  That makes ordinary run plus a tool pose a credible candidate, not proof of
  a matching world pose. Compare against the existing tool gait, including
  interruption mid-fade. Keep the lower-body phase and deliberate swing brake.
  Compare the final world transform of `hand.R`, including spine, arm and
  forearm, and the resulting `actor_world * hand_global * grip_offset` tool
  transform. The mesh and tool must agree in the same frame. Changing targets
  or repeating a swing must not
  duplicate the simulation's checked damage contact.
  [Mannequin GLB][mannequin] · [Combat policy and pose][encounter] ·
  [Socket transform][tool-socket] · [Tool attachment][tool]
- **Tall Guest inspection.** The 127-joint GLB has authored inspection and hood
  controls. Both `acoustic_origin` and `mouth_capture_socket` descend from
  `head`. The current filter excludes the acoustic node's local tracks from the
  overlay, but its world origin and +Y axis still move with blended ancestors.
  Inspection has no local mouth-socket track, so for locomotion that socket
  also moves through its parents. Compare both composed socket translations
  and orientations, not just membership, against the existing overlay at zero,
  intermediate and full weights, then with a changed target or clearance
  interruption. Check mouth and hood geometry and compare the accepted
  collision pose with rendering.
  [Tall Guest GLB][guest-glb] · [Socket composition][guest-socket] · [Clearance rollback][guest-step]
- **Compatibility controls.** For Tall Guest run → capture → swallow, preserve
  the transition from the outgoing pose into the moving bow, full rigid mouth
  attachment, obstruction rollback and exactly-once consumption. For run →
  doorway duck → run, preserve phase-matched entry, clearance checks, planted
  feet and retreat from obstruction. For Feltling run → wave/jump → run, reverse
  direction or interrupt the action mid-blend. Preserve travel-driven gait,
  terrain correction, gesture completion and the pose retained on interruption.
  Across controls, check repeated triggers, pause/resume and varying frame deltas
  for flashes, sliding, duplicate effects or render/physics divergence.
  [Capture][capture] · [Door posture][door-posture] · [Feltling pose][feltling]
  · [Feltling movement][feltling-motion]

Advance only if one evaluator replaces Tall Guest's hard-coded composition
and avoids a bespoke tool-on-gait path for the mannequin, while preserving the
pose and event contracts without schema, tooling or dependency changes.
Otherwise retain the current paths.

**Performance is unmeasured.** A cached mask and reused scratch could remove
pose copies or redundant composition, while additional active curves and layer
state could cost CPU and memory. Skinning and uploads do not disappear. Isolate
sampling, pose allocations, skinning/upload work and memory at representative
and stress actor counts in any approved test. Record exact binaries/commits,
assets, hardware/backend, settings, warmup, samples and procedure. If reporting
FPS, disable limiters and V-sync and verify actual presentation mode, drawable
size and uncapped pacing. [Places render pose path][pose-upload] · [Bevy graph preparation][bevy-prep]

[issue]: https://github.com/csd113/Places/issues/45
[release]: https://github.com/bevyengine/bevy/releases/tag/v0.20.1
[animator]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/render/common/character.rs
[cue]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entity.rs#L67-L111
[overlay]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/render/common/character.rs#L3766-L3796
[guest-pose]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entities/tall_guest/mod.rs#L691-L758
[guest-aim]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entities/tall_guest/mod.rs#L2269-L2493
[bevy-graph]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_animation/src/graph.rs
[bevy-graph-ex]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/examples/animation/animation_graph.rs
[bevy-mask-ex]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/examples/animation/animation_masks.rs#L239-L306
[bevy-id]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_animation/src/lib.rs#L1345-L1375
[bevy-transition]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_animation/src/transition.rs
[bevy-root-events]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_animation/src/lib.rs#L1004-L1043
[bevy-target-events]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_animation/src/lib.rs#L1217-L1269
[bevy-order]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_animation/src/lib.rs#L1316-L1341
[bevy-deps]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_animation/Cargo.toml
[bevy-add]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_animation/src/animation_curves.rs#L464-L533
[mannequin]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/assets/entities/mannequin/model/mannequin.glb
[encounter]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entities/encounter.rs#L556-L654
[tool]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/game/companion_tool.rs#L20-L57
[tool-socket]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/render/wgpu/renderer.rs#L3480-L3488
[guest-glb]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/assets/entities/tall_guest/model/tall_guest.glb
[guest-socket]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entities/tall_guest/mod.rs#L948-L966
[guest-step]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entities/tall_guest/mod.rs#L1955-L2043
[capture]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entities/tall_guest/capture.rs
[door-posture]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entities/tall_guest/mod.rs#L2873-L2882
[feltling]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entities/feltling/pose.rs#L4-L90
[feltling-motion]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/entities/feltling/motion.rs#L178-L235
[pose-upload]: https://github.com/csd113/Places/blob/7a0cca2cec503b319aebc9ecf25fd861f9d37e5b/src/engine/render/wgpu/character.rs#L275-L309
[bevy-prep]: https://github.com/bevyengine/bevy/blob/a0577b5c6a7b208719dd761c5c6a7a539e472a97/crates/bevy_animation/src/graph.rs#L911-L972
