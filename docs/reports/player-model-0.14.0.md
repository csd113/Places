# Local player mannequin — 0.14.0

The local player reuses the installed mannequin rig, geometry, PNG albedo and
existing animation/ragdoll paths. Settings > Player selects eight saved colours;
the default is muted teal `#4F7774`. Colour bytes are decoded from sRGB before
applying the existing per-instance albedo multiplier. No asset was replaced.

The living controller keeps its existing collision hull, movement and eye.
The local body is hidden in first person, including when looking down. A
cinematic or explicit external diagnostic camera can show the world mannequin.
The reserved runtime identity `player:avatar:local` and existing character
sockets provide an integration point for future tool presentation.

On an enabled-health map, terminal player death transfers the current pose and
controller velocity to the shared 15-body/14-joint ragdoll. The controller's
physics proxy becomes inactive. Vision retains the entry eye offset relative
to the physical head, then follows its translation, direction and roll. A small
camera sweep against level geometry prevents that offset crossing a surface.
The audio listener uses the same camera pose. Pause stops the physical view.
Healing cannot revive it. Explicit respawn, world reload, reset and editor test
exit retire the old visual and physics body. Disabled-health maps never enter
this death path.

## Validation

- Strict workspace/all-target/all-feature Clippy, locked release workspace
  build and formatting passed. All 577 workspace tests passed. Two tiny added
  regressions protect old preference preservation/colour round-trip and
  head-relative vision with roll; they require no assets, GPU or map bake.
- A freshly compiled small Test Room static package passed validation and
  current-source verification. No shipped map was baked or edited.
- Repository asset validation passed with the same 17 existing map-geometry
  warnings. The mannequin GLB hash and all tracked asset files were unchanged.
- Visible native Mac/Metal player checks covered ordinary disabled health,
  looking down with no body, walking, jumping/landing, physical death, exact
  pause stability, no revival by healing, respawn and reload cleanup.
- A separate external inspection showed the teal mannequin, walking animation,
  normal settings navigation to Slate blue, persisted selection, physical
  ragdoll, graphics-quality changes and reset. The normal first-person path
  remained hidden.
- Holding crouch lowered the eye to 0.7 m before death. The death view retained
  that entry eye and stayed finite and above the floor through the fall.
  An earlier tap-only recipe did not crouch under the current hold controller
  and was excluded from acceptance.
- Native embedded editor checks covered alive movement/look-down, immediate
  zero-starting-HP ragdoll death, a fallen head camera, matching listener,
  Escape retirement and a fresh second test session.
- A fresh final-build player launch retained Slate blue. A final-build editor
  test with deliberately malformed preferences entered death and stopped
  normally while preserving that file byte-for-byte, without renaming it.

Local logs, action recipes, JSON snapshots and original screenshots are under
`target/verification/player-model-20261110/` (ignored developer evidence).
`native-checks.json` records 27 passing snapshot assertions alongside the visual
inspection. A final rebuild initially ran out of disk space; removing only the
disposable Rust incremental cache freed space for the retry.
The player used a 1920×1080 physical drawable; editor captures were 2880×1632.
These are correctness checks, not a performance comparison.

## Limits

The mannequin retains its existing locomotion library; no dedicated crouch
animation was authored. Crouched death preserves a stance-specific eye offset
relative to that rig. Tight crouch tunnels need further pose/clearance review.
First-person hiding currently removes the local model from all rendering
passes, including its own reflection/shadow. Per-view multiplayer ownership,
spectating and held-tool arms are future work.

Existing character/ragdoll budgets remain unchanged. A refused physical handoff
reports a diagnostic and retains terminal health without silently reviving the
player. No weapons, game-mode library, horde map or death screen effect was added.
Validation here is macOS; Windows native validation remains outstanding.
