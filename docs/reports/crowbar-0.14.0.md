# Map-granted crowbar — 0.14.0

This step adds one tool, independently of carried physics props and health.
Maps grant it through `tools.crowbar` (default false). Damage, reach and swings
per second are authored under Project > Map settings > Tools. Existing maps
grant nothing and retain disabled health. The shared runtime serves the native
player and embedded editor test.

The crowbar starts drawn on a granting map. `1` holsters/redraws it. One eligible
captured left-click starts one swing, with contact at 32% and recovery completing
at 100%. Holding never repeats; recovery does not queue clicks. Holstering or
control interruption cancels pending contact without clearing recovery. Tools
use the same clamped world clock as health and movement, preventing a frame stall
from advancing a newly started swing directly to contact.

With a prop carried, the same attack input releases it safely and starts the
equipped crowbar. A blocked safe release preserves ownership and refuses the
swing. With the tool holstered or absent, existing throwing, interaction and
elevator-button behavior remains. `E` is unchanged.

At contact, current eye/aim selects only the nearest physical surface or visible
character's current posed bounds. Architecture and doors stop reach. A physics
hit applies the existing throw-strength mass curve at contact; fixed bodies
stay fixed. HP damage uses the existing health action and hit-protection rules
only while the map explicitly enables health. Map-granted inventory cannot
activate health. Compiled granting maps require `player-tools-v1`.

## First-person meshes

The compact pair lives in `assets/shared/tools/`: a 176-triangle crowbar and a
740-triangle right forearm/hand cropped from the accepted mannequin. The
original mannequin GLB, rig, clips and PNGs are unchanged. Both new GLBs embed
existing committed PNGs byte-for-byte. The arm preserves its source UVs and uses
the player's saved colour. Ordinary material/environment lighting remains;
there is no fixed body attenuation or new generated texture imagery.

`tools/player/build_crowbar.py` reproduces the meshes with the installed Blender:

```sh
blender -b -t 2 --python tools/player/build_crowbar.py
```

The exporter guards the source mannequin SHA-256. `provenance.json` records mesh
counts, bounds, output hashes and PNG source hashes. Structural inspection found
no GLB, topology or UV errors. The source sheets are retained at 1024 square to
preserve the existing fitted contract; native prop-size advisories are reviewed
here, rather than resizing the accepted sheets.

A retained GPU pair uses private depth after world rendering and before display
resolve, so nearby walls cannot cut through the hand. It samples lighting at
the eye and does not enter world collision, grounding, shadows or reflection
captures. Colour changes reconcile material bindings without rebuilding model
geometry every frame. The grip follows the lower drawable edge across FOV/aspect
changes and keeps the cropped elbow outside the view. Holstering, carrying,
cinematics and player death hide it. Respawn/reload restores map inventory and
discards an old swing.

## Validation evidence

Logs, original screenshots, actions, fixture source and JSON receipts are local
developer evidence under `target/verification/crowbar-20261110/` (ignored).
The native fixture is a small Test Room derivative, not a shipped map or horde
mode. No full-map bake is required by the added Rust regressions.

Strict workspace Clippy with `-D warnings`, the locked release build with all
features, formatting and all 584 Rust tests pass. Seven narrow CPU regressions
cover inventory/timing/reset, the shared stalled-frame clock, ray/tuning
boundaries, same-click carry release, physical throw-strength contact, mouse
edge ownership and editor save/history. They add no asset or map-baking cost to
the default test suite.

Native Mac/Metal checks covered:

- An old map with no crowbar, no health and a body-free look-down view.
- Cancelled holster/pause windups, retained recovery, discarded busy clicks,
  a fresh click after recovery, and ignored key repeat. The cancelled windups
  caused no damage; the subsequent accepted hit left 10/12 HP.
- One held click, holster/redraw, discarded recovery clicks, range rejection and
  a wall protecting a second target. The nearest target read 12 → 10 → 8 HP;
  the actor behind stayed at 12. The shielded target stayed at 12.
- An isolated health-disabled hit, followed by a readout after re-enabling:
  target stayed at 12/12 HP. The longer sequence also retained 8/12 across its
  disabled-health strike.
- A moving light prop hit, same-click drop/swing, and ordinary holstered throw.
  Forward velocity samples were about 7.3–7.7 m/s for tool hits and 7.7 m/s for
  the throw; the small CPU physics fixture checks the exact mass curve.
- Hidden presentation while carrying, holstered and dead; first-person physical
  death and clean respawn inventory. Default teal, a live change to Slate blue,
  Low/High graphics round-trip, 45°/110° FOV, 16:9/4:3 resizing and
  close-wall presentation. A separate 72-case projection review covers 45°/60°/110°
  FOV, 4:3/16:9/21:9 aspects and eight swing phases with the actual cropped mesh.
- Native editor controls for inventory and all tuning values, one held click,
  holstering/redraw, stopping and reopening the saved project. Granting the tool
  left that map's health disabled.

All 36 native snapshot/pixel checks passed on the final release build.
Pixel samples independently confirm the teal/blue arm and that holstering
reveals the world at the same pixel. Health readouts were inspected directly,
with raw text extraction supporting the scaled captures. These are correctness
checks, not FPS comparisons.

## Limits

NPC health remains state/event data: automatic NPC defeat, death animation,
ragdoll, AI suspension or retirement policy is future combat work. This step
does not impose such policy on existing actors. Character contacts use posed
bounding boxes rather than per-triangle or per-limb damage.

There is one map-granted crowbar, with no weapon pickup/ammo system. Holster
selection and partial swing state are not persisted in checkpoints. The rigid
grip view uses a simple arc, without a full arm IK system, dedicated swing audio,
third-person tool attachment, multiplayer or horde mode. Windows native
validation remains outstanding. Existing player mannequin crouch-animation
limitations remain unchanged.
