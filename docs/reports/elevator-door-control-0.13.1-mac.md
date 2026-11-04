# Independent elevator door control / macOS acceptance

The green door-arrow button below the dynamic destination buttons opens or
closes the current cabin without choosing a destination. Door-only closure now
settles in `Closed`, rather than entering the generic travel path. The player
can close a cabin on the current floor to break a pursuer's sight.

The control is automatic on existing and newly placed elevators, with no extra
editor configuration or level-format change. Two-stop cabins show one floor
button and the door button; three-stop cabins show two floor buttons and the
door button. The current stop remains excluded from destination rows.

## Behavior and artwork

- Mouse and E use the ordinary interaction target. The door action has its own
  identity, checks that the operator is inside the cabin, and never queues a
  floor/level destination. Its prompt changes between Open/Close doors.
- Pressing during opening or closing reverses the door. Reopening a closing
  trip cancels the unstarted destination. Moving/loading cars refuse the press,
  remain sealed and do not queue a later toggle.
- The existing swept collision and obstruction reopening apply to manual
  closure, including the player, physical obstacles and live AI bodies.
- Accepted presses use the existing spatial switch-click at the actual button.
  Door-only closure emits no trip/arrival cue. Occasional shaft ambience and
  the normal travel audio are retained.
- GIMP source/master/native artwork adds a contrasting green etched door icon
  in reserved glyph slot 15. Only that 64-square master region changes; the
  native resampling footprint is 304 pixels around the corresponding slot.
  All four GLBs embed the updated shared PNG. Cabin, frame and leaf vertex
  positions are identical to the accepted wood-wall version. The button stock
  loses only its concealed rear backing face, reducing 56 to 54 triangles.
  The normal three-stop assembled cabin is 1,498 triangles.
- The 1970s wood wall, narrow metal frame, backed threshold, pocket recesses,
  visible open leaf edges, seals, travel anchors and collision sizes remain.

## Verification

Integrated source base: **56203451**, including the Quiet Collection hub
elevators and shared editor/player automatic world audio changes. Build output
is isolated under `local/elevator-door-control/target`; native runs use retained
copies in `local/elevator-door-control/production`. No Mac app bundle is used.

Strict all-target/all-feature workspace Clippy passes. The locked release
workspace/all-feature build passes. All **441 workspace tests** pass, including
two new small controller regressions protecting unintended travel, cabin-only
admission, obstruction reopening, predeparture cancellation and busy-car locks.
The isolated test executable needs explicit `PLACES_ASSET_ROOT`; the preliminary
run without it could not discover assets in three existing editor tests. The
properly configured integrated run has no failures. No map bake enters CI.

Actual native acceptance passes:

1. Two-stop panel: rapid reversal, mouse opening, E closure, closed save/resume,
   obstruction reopening, canceled destination, same-map trip, door-only operation
   at the destination, and return without any cross-level request.
2. Three-stop panel: the same checks, plus Annex load, independent door operation
   there, and return. Destination buttons remain distinct and functional.
3. Physical shaft: a real door-button press while moving leaves the car sealed;
   door-only operation at the upper floor stays there; the floor button still
   returns to Ground.
4. Actual chasing pumpkin-skeleton: ordinary player movement boards the cabin,
   then the manual button seals it at Ground. The monster remains outside,
   switches from pursuit to investigation, and its last visual-sighting clock
   stops updating while the player walks behind the closed doors. On reopening,
   visual contact and pursuit resume. A separate late acceptance pose keeps the
   player clear while the live monster occupies the threshold; attempting to
   close then safely reopens. The monster is never teleported or scripted along
   a path. Its delivered `walk` animation is used, with hearing enabled and
   ordinary-door opening enabled in its AI definition.
5. Existing physical travel/save/frozen-frame resize/cross-level flow and the
   held-crate/NPC passenger campaign pass. Boarding, same-map transport, both
   cross-level directions, local/foreign checkpoint resume, custom state,
   outside-object exclusion and single-instance return remain intact.
6. A 179-frame lossless inside-right opening sweep shows no visible artifact
   in the inspected movement/contact views. The last twelve settled frames are
   pixel-identical. Real-GLB checks find zero closed leaks, open obstructions,
   material errors, atlas violations or visible coplanar overlaps. Asset/prop/
   texture checks pass with existing texture advisories. This is bounded macOS
   Metal coverage, not a guarantee for every GPU/view or a throughput claim.

The first monster attempt is explicitly excluded: its timed strafe coasted past
the walking aperture, so the test player never boarded or activated a cabin
control. The corrected route is accepted in `native-monster-v3/`; the earlier
valid hiding run remains in `native-monster-v2/`.
The first shaft capture exercised travel but missed the moving door button:
that closed-cabin trip departed immediately, before the camera aimed at its
original world height. `native-shaft-v2/` corrects the aim and additionally
requires the real door interaction target during movement. That accepted press
leaves the car sealed. The earlier attempt is retained and marked as excluded
evidence for that specific press.

## Reproduction, deliverables and limits

`tools/bench/elevator_doors.py --write-fixtures <new-directory>` writes only
tiny editable `two`, `three` and `monster` projects. Explicitly export a chosen
project, then run with matching `--mode`, `--binary`, `--package` and fresh
`--out`. `--mode shaft` accepts the physical elevator lab package instead.
The benchmark README gives commands. The tool never builds Rust or shipped maps.

The place-and-link demo is version **1.0.4**, and the physical lab **1.0.3**.
Six tiny packages were re-exported with the final compiler, then passed integrity
and every member's `verify --require-current` check. The preliminary compiler's
packages were correctly rejected as stale; that guard was not relaxed.
The updated demo and actual panel/monster screenshot sheet are retained under
`local/elevator-door-control/deliverables/`, alongside native-resolution views.
All raw actions, checkpoints, traces, capture frames, identities and receipts
remain in that local evidence directory, outside `target/`.

Closed doors do not erase AI memory or mute hearing. A monster may search or
wait outside, and one reaching the opening before sealing can prevent closure.
The existing baseline predator pursues the player but does not implement player
damage/catch; custom attack behavior requires its own validation. Transport
category limits are unchanged. No hub maps, lighting logic or audio authoring
logic were modified by this feature, and no full shipped-map bake was run.
**Integration:** existing exported hub/collection packages need refreshing with
the final compiler and shared artwork; their old preparation/dependency
identities are intentionally refused. Preserve the hub task's authored sources
and reuse its preparation cache rather than changing maps for this control.

Production game SHA-256:
`5589b59009c0e18f3ac102f2740e46907f25f7eb5a5622f7fd52fdfabf6f5844`.
Compiler SHA-256:
`aff0c3e6c6761e9dbe02f4ae6b083d80b219b953fecaabd58cf3bfdf73311e63`.
