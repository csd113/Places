# Item physics — 0.10.0 staging

Places now has reusable native pickup, drop, throw, bounce and roll behavior for passive rigid props. Player movement keeps its existing controller, stance, water, collision and void recovery; items and player share **9.8 m/s²** in metre/second units. Existing visual assets and the rendering catalog are retained.

## Architecture and choice

**Rapier 0.36.1 / Apache-2.0** supplies collision response, mass properties, sleeping and CCD. It is a maintained pure-Rust solver with native and WebAssembly support; browser gameplay is not claimed here. Rapier remains serial at this bounded scale. Its required math dependency is isolated from the renderer’s existing glam version. The 31 newly introduced registry packages and available license texts are recorded in `THIRD_PARTY_LICENSES.txt`; no existing dependency was replaced.

| Owner | Responsibility |
| --- | --- |
| `src/physics/` | Strict profiles, shapes/mass, world lifecycle, activation, carry/release, floor contacts, pair impact onsets and escape recovery |
| `src/item_runtime.rs` | Small native-loop, console and renderer adapter |
| `src/item_input.rs` | Mouse edges and focus/console/recapture gating |
| `src/level/physics.rs` | Compact floor, recess, stair, ramp and roof collision from existing level descriptors |

Rapier allocates on the first physical spawn. A **120 Hz** fixed step retains fractional time and consumes at most twelve substeps / 0.1 s per frame. CCD permits two collisions per interval; bodies settle into natural sleep. Rendering interpolates centres and quaternion orientation, then restores model origins. Moving items use economical cuboid, sphere, cylinder, convex-hull or compound shapes, with friction, restitution, damping and derived inertia. Shape-aware carrying sweeps translation and rotation around walls, doors, the camera and player. Blocked release retains ownership. There is no per-frame model/texture reload.

Contacts use at most 2 mm of scale-aware skin, 1 mm allowed error, disabled contact recycling and critically damped 240 Hz ground softness; the solver retains its default four iterations / one internal PGS iteration. These settings fixed a demonstrated 5 kg ball compressing a 25 g cup through its supporting floor. The final native worst floor gap is **−0.26 mm**, using the actual physical transform without a mesh lift.

The future impact seam records solved contact onsets per collider pair, excluding zero-impulse speculative geometry and steady supporting forces. Its cache and 32-event queue are bounded. Audio remains deferred.

## Controls and authoring

Aim at a pickable physical surface within **2.2 m**, then press Interact (**E** by default). Interact drops; **left click throws**. A carried prop returns smoothly upright and stays centred in front of the player. Throw speed depends on actual mass, with bounded inherited player motion. Menus, console typing, focus loss and recapture clicks cannot throw; held items must be dropped before noclip.

The small rounded white hand sits near **79% of screen height**, with the actual bound key in black inside its palm. It appears only for a precise, reachable, unobstructed target. It disappears on the pickup frame and throughout holding, including when facing another interactable. No drop/throw tutorial text replaces it. Native 1280×720 and 2560×1440 captures verify its readability.

![Small lower-screen interaction hand](../images/item-physics-0.10.0/interaction-hand.png)
![Held, illuminated cup with no interaction hand](../images/item-physics-0.10.0/held-cup.png)

Placements select motion explicitly:

```json
"physics": {
  "motion": "dynamic",
  "shape": {"kind": "cylinder", "half_height": 0.061, "radius": 0.0425},
  "mass_kg": 0.025,
  "friction": 0.55,
  "restitution": 0.18,
  "pickup": true
}
```

Absent profiles retain legacy fixed scenery. An explicit `fixed` shape supplies immutable item collision while preserving the player’s existing `solid`/`size` box and static lighting. Fixed shapes enter queries immediately, including before any solver step, and have a separate 1024-placement cap. Routes, floating, animation, AI, balloon and lifetime ownership are refused for explicit fixed shapes; compatible static switches, labels, state, lighting and fade remain available.

**Authored movable props start dormant at their exact authored poses in every map**, even with imperfect scenery overlap. Pickup or a generic activation action wakes them; meaningful active-dynamic impacts also wake dormant dynamic bodies and chains before solver response. Natural sleeping after activation is distinct. `kinematic` props expose prescribed pose control. Movable props currently require passive rigid models; competing animation/AI/routes/components and attached lights are rejected.

The strict optional `assets/gameplay.json` supplies catalog-ID-based reusable physics and floor-friction profiles without changing visual catalog provenance. `/give` creates supported physical types through existing caches; unsupported types are labelled previews. The limits are sixteen given props and 128 movable/runtime bodies. Metadata round-trips through authoring, compilation and runtime; malformed fields, dimensions, mass, coefficients, hulls, scaling and counts are rejected. The full contract is in [MAP_AUTHORING_GUIDE.md](../MAP_AUTHORING_GUIDE.md).

Floor geometry stays continuous across material boundaries. Contact hooks resolve the existing support materials and multiply their friction with item friction; pack authors can set `ground_friction`. Player ice behavior is retained.

## Bowling, reset and recovery

Arcade lane one uses its existing ten pins and Cobalt ball. Five convex sections follow the actual pin silhouette, including shoulder and neck; fixed gutter compounds follow the visible curved plastic instead of the recessed floor below it. Visual models/textures are unchanged. The return housing has a real reset switch.

Tiny unit-density hulls exposed an absolute eigensolver tolerance in the dependency’s inertia calculation. Mass properties now normalize density before combination and rescale to the requested mass. Native evidence retains the **5 kg ball / 1.5 kg pins**, pin COM y≈0.152384 m, and principal inertia≈0.001756 / 0.015031 / 0.015031 kg·m². The first solved rack impulse was 2.478 N·s, with finite activated pin mass before response. No mass-number workaround was used.

`physics.reset_group`, `reset_physics_group` and `activate_physics_group` are generic. Reset validates all destinations before mutation, restores authored poses and dormancy, clears linear/angular motion, forces, held ownership and interpolation, and reuses handles/resources. A live player, door or foreign body blocks the entire reset; unrelated state is preserved. Authored scenery intersections remain compatible.

![Native ball contact and fallen pins above the visible lane](../images/item-physics-0.10.0/bowling-contact.png)

Recovery is a separate bounded safety net, never collision-test forgiveness. Escapes below local coverage or above closed local ceilings receive a one-second grace period; open-sky trajectories and lower floors/recesses remain valid. Invalid motion is caught before broadphase. Supported, full-shape-clear cached/authored/nearby poses restore safely; exact authored returns become dormant. If all candidates are blocked, a body suspends once and hides while retaining resources for reset. Original escaped pose, motion and reason remain diagnostic evidence.

Moving props use the existing dynamic lighting/culling/resource path, including current matrices, normals, probe anchors, direct visibility and contact shadows. Switchable indirect probe planes use prepared **PLPF v4**, with v2/v3 still readable. Only Arcade and the explicit tiny fixtures were baked for affected authored/probe data. The retained compiler that produced those packages verifies their provenance; later runtime/validation changes require no new lighting bake.

## Validation and performance

Formatting, workspace check, strict debug/release Clippy, release build and default/all-feature workspace tests passed: **2,310 / 2,316 total tests**, including 2,301 / 2,307 library tests. Nineteen existing opt-in GPU tests remain ignored. Eighteen focused physics tests take 0.03 s using small deterministic fixtures; extensions protect inertia/geometry, true pair onsets, kinematic reset velocity clearing, fixed-only authoring conflicts and initial query registration. No production-map bake enters default tests or CI. Source Python validation passed 388 tests with two skips; the asset validator reports 571 assets / 348 placeable / eight themes with no warnings, and the Arcade generator is current.

Sixteen final sequential SDL/Metal acceptance runs passed **5,295 checks**: two full bowling campaigns plus contacts/stacking/heavy impacts, seven fast floor/wall/door/corner/ramp/stair trajectories, constrained release, material seams, 30/144 Hz simulation deltas and bounded catch-up, dormancy/reset, target eligibility/HUD, switch/quality illumination, console/pause/noclip/reload and input gating, and twenty-body stress. Ordinary gameplay produced **zero escape recoveries**; two explicitly labelled invalid teleports separately verified recovery. Both solver and interpolated GLB vertices stayed above visible wood and gutter stock throughout both bowling runs. Screenshots and corrected encoded playback were inspected.

The existing bathroom-door regression view used the exact same preserved package/catalog, High/Full lighting, 2560×1440 drawable, 120 warmup frames and 600 measured frames. Three alternating pairs launched both binaries in the same way, with no other owned jobs. Values are medians of per-run statistics:

| Metric (ms) | Baseline median / p95 | Physics median / p95 |
| --- | ---: | ---: |
| Frame loop | 8.307 / 9.755 | 8.331 / 9.658 |
| Frame update | 1.398 / 1.692 | 1.342 / 1.677 |
| GPU timestamp span | 5.069 / 5.990 | 4.938 / 6.130 |

GPU spans include scheduling stalls and are not active-utilization measurements. An earlier mixed bundle/CLI campaign had wider GPU spans; the same-launch campaign above is the matched comparison. These short runs do not establish a causal speed improvement. Draw calls (274), visible indices (219,312), vertex buffers (20,483,056 bytes) and index buffers (493,212 bytes) are unchanged. Median peak RSS was **891.9→897.1 MiB**.

The twenty-body fixture peaked at eighteen active bodies and nineteen sleeping bodies; solver median/p95/worst were **0.030 / 0.081 / 0.382 ms** over 328 simulated frames with all twenty present, and peak RSS was 251.4 MiB. Entirely dormant four-body steps measured 0.000416 ms median / 0.000709 ms p95 (401 samples). Bowling’s twelve-body phase measured 0.154 / 0.584 / 2.201 ms. Logs now distinguish solver timing from the enclosing item-update adapter.

The corrected video is **600 consecutive native frames, 20 s, 1280×720, H.264/YUV420P, 30 fps, BT.709**, SHA-256 `50a1f308d628bd2fc765d217b111e2990fe44d466446bb55fdd884fa390b885a`. It replaces the original Library video as version 2; local binaries, playable packages, receipts and `run-arcade.sh` are retained under `local/item-physics-0.10.0/` outside Cargo’s target directory.

## Limits

Cups use solid economical cylinders, without hollow-shell or liquid simulation. Kinematic control remains an explicit caller-owned seam; there is no new general entity controller or ragdoll system. Native acceptance was run on this Mac with scripted normal input dispatch and diagnostic fixture setup; other operating systems and browser gameplay are untested. The earlier intermittent native crash remains unresolved.
