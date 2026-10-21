# Approved concrete mannequin R2

`model/mannequin.glb` is the exact approved export pinned in
`assets/completed-models.json` (SHA-256 `6627807d4cddf7331cc7f96b03adb43f3686af71366bc8dc257f881b6ddf1156`).
It uses metres, +Y up, +Z forward and a floor-centred rest origin. Preserve the
existing catalogue envelope, physics/navigation values and black preset tint.
The opaque fitted 1024² `textures/mannequin-skin.png` matches its embedded PNG.
Historical source sheets are retained; do not repack the new UV layout.

The original `pose_stand`, `pose_arms_up`, `pose_arms_forward`, `walk`, `run` and
`run_arms_forward` interfaces remain. All 17 original body rest transforms are
unchanged; 20 finger descendants extend the rig to 37 joints. Walk is in place
at 1 m/s reference, run/run_arms_forward at 2.244582 m/s. Zero-duration poses
hold at time zero. Ordinary idle/walk selection must not select carry variants.

New loops: `carry_idle`, `carry_walk`, `seated_idle`. New one-shots:
`door_open_push`, `door_open_pull`, `door_close_push`, `door_close_pull`,
`pick_up_tabletop`, `set_down_tabletop`, `pick_up_floor`, `set_down_floor`,
`sit_down`, `stand_up`. The embedded manifest is authoritative for duration,
loop/kind/reference speed and reference events. Map route/action loop overrides
remain explicit. One-shots clamp at their endpoint.

Door clips contain baked root displacement. The native review retains it
against a fixed actor anchor, including the open-push/close-pull pair's matching
nonidentity endpoint. Do not both retain it and transfer it to actor movement.
Existing posed `glow.socket` attachments work on body and finger nodes; their
offset is only a fallback. Prop grasp, door/lever synchronization, metadata event
crossing, root handoff to navigation and runtime IK are not controllers provided
by this adoption. Keep the delivered alignment and contact tracks for separately
scoped future interaction work.

The complete approved Blender/GIMP sources, animation manifest,
`FUTURE_INTEGRATION.md`, alignment data and QA are in the canonical Downloads
asset integration delivery under `Accepted/Mannequin-R2/`. Original pending-review
language in immutable source documents is superseded by its approval note.
`projects/mannequin-r2-review/` is the editable opt-in native fixture; no map bake
is required by ordinary tests or CI. See `docs/reports/release-0.12.0.md#asset-integration-0120`
for native evidence and retained limits. Rejected V1 and Spoonerman candidates
are excluded.
