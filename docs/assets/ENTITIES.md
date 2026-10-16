# Entity asset profiles

The committed GLBs and [catalog](../../assets/catalog.json) are the current
resources. These profiles retain rig, artwork, placement and provenance details;
the [asset specification](../ASSET_SPECIFICATION.md) defines mandatory contracts
and the [map authoring guide](../MAP_AUTHORING_GUIDE.md) defines component and
route syntax. Export, validation and preservation workflows are in
[Offline tooling](../OFFLINE_TOOLING.md#asset-authoring).

## Resource and playback conventions

Every entry below has catalog class/type `entity`, one skin, metres as units,
+Y up and +Z front at zero yaw. Beach animals also have theme `beach`; theme
membership does not change the entity class or move a rig into the static prop
library. Paths are relative to `assets/entities/`:

| Logical id | Model path | Catalog bind size XYZ, metres | Triangles / joints / primitives |
| --- | --- | --- | --- |
| `spooner-man` | `spooner-man/model/spooner-man.glb` | 0.159043 × 0.348864 × 0.622915 | 5688 / 26 / 6 |
| `mannequin` | `mannequin/model/mannequin.glb` | 0.578539 × 1.723035 × 0.316965 | 1124 / 17 / 2 |
| `entity:cockroach` | `cockroach/model/cockroach.glb` | 0.087414 × 0.018563 × 0.132125 | 480 / 25 / 1 |
| `rat` | `rat/model/rat.glb` | 0.112014 × 0.129887 × 0.472315 | 1718 / 25 / 1 |
| `skeleton` | `skeleton/model/skeleton.glb` | 0.414955 × 1.722652 × 0.298664 | 4650 / 24 / 1 |
| `carved-pumpkin` | `carved-pumpkin/model/carved-pumpkin.glb` | 0.579556 × 0.631126 × 0.521600 | 2384 / 4 / 4 |
| `sheet-ghost` | `sheet-ghost/model/sheet-ghost.glb` | 0.994 × 1.619 × 0.716 | 1270 / 3 / 2 |
| `pumpkin-skeleton` | `pumpkin-skeleton/model/pumpkin-skeleton.glb` | 0.497499 × 1.822028 × 0.583294 | 5554 / 94 / 5 |
| `sheet-ghost-cat` | `sheet-ghost-cat/model/sheet-ghost-cat.glb` | 0.226 × 0.323 × 0.593 | 902 / 5 / 2 |
| `beach:seagull` | `beach_seagull/model/beach_seagull.glb` | 1.096 × 0.747 × 0.886 | 458 / 13 / 1 |
| `beach:crab` | `beach_crab/model/beach_crab.glb` | 0.832090 × 0.381 × 0.561201 | 692 / 18 / 1 |
| `beach:fish` | `beach_fish/model/beach_fish.glb` | 0.294 × 0.377 × 0.638 | 288 / 6 / 1 |

The six completed rigs were adopted from the verified v36 delivery on
2026-10-14. Exact export hashes, measured bounds and image budgets are in
`assets/completed-models.json`; the table uses conservative root-centred X/Z
catalog envelopes. Their original root pivots are retained. Spooner-Man and
mannequin use 512² embedded sheets, pumpkin-skeleton 512×256, and
rat/skeleton/carved-pumpkin 256². Unchanged ghost, cockroach and Beach entries
retain their previous contracts. Older standalone PNGs remain as retained
source artwork; they do not authorize replacing a completed export's embedded
sheet. Keep the supplied Blender/GIMP delivery masters separately.

Current playback uses each delivered rig's own hierarchy, inverse binds and
named clips; clip declaration order is immaterial. Completed-source records and
`tools/props/build.py --check` guard those exports. Historical constructor
recipes below are identified explicitly and must be reviewed in scratch.

`asset.extras.places_entity_clips` v2 records clip kind, duration, loop flag and
`reference_speed_mps`. Ground routes advance the placement and scale playback
by requested speed / reference speed; these clips have no horizontal root
motion. A pose with duration zero is a single-key hold. Named
`play_animation` actions select poses and clips; an animation component alone
does not emit the initial renderer cue. Use an interaction, route play step or
an autostart timer, as appropriate.

Place routed examples with `solid:false` so their own collision box does not
block the route. A flying or swimming clip still needs an authored position;
a ground route would resolve its height against the floor. Prop `y` is an
offset above the resolved floor, not an absolute world elevation.

## Spooner-Man

The [current tuxedo-cat export](../../assets/entities/spooner-man/model/spooner-man.glb)
contains 26 joints, six material primitives and one 512² embedded sheet. Its
rest proportions and skinning come from the completed delivery. Preserve that
mesh, artwork, hierarchy, inverse binds and bind pose when reviewing motion.
Seven named clip durations and loop/kind semantics remain; measured walk/run
references are 0.156/0.466 m/s.

| Clip | Duration | Playback | Kind / reference speed |
| --- | --- | --- | --- |
| `idle` | 4.0 s | loop | idle |
| `walk` | 0.6 s | loop | walk / 0.156 m/s |
| `run` | 0.46 s | loop | run / 0.466 m/s |
| `sit_down` | 1.8 s | once | sit_down |
| `sit_idle` | 5.0 s | loop | sit_idle |
| `stand_up` | 1.5 s | once | stand_up |
| `pounce` | 1.4 s | once | pounce |

These clips keep horizontal root motion in place. Pounce starts and finishes
in the idle stance and contains a vertical pelvis arc; level movement and
attack behavior require separate authoring. A route cue is
`{"step":"play","clip":"pounce","seconds":1.4,"loop":false}`.

The [body refinement](../reports/spoonerman-body-refinement.md) and
[motion](../reports/spoonerman-cat-motion.md) reports retain the earlier
three-sheet export, gait/skin-repair method, preservation hashes and accepted
concept differences. Those exports predate the completed model. The retained
`tools/props/animate_spooner_man.py` describes that earlier baker; it cannot
reconstruct the adopted six-primitive mesh and must not replace the current
clips or skin from its historical defaults.

## Concrete mannequin

The current concrete artist's lay figure has 17 joints, including
`hand.L`/`hand.R`, two material primitives and one 512² embedded sheet. Its
root-centred X/Z envelope, triangle count and accepted identity are recorded
above and in `assets/completed-models.json`. Native pose checks measured
raised hands near 1.896 m and forward reach near 0.621 m. Constant Blender
scale tracks select two-sided character rendering; current posed bounds
determine visibility.

| Clip | Duration | Playback | Kind / reference speed |
| --- | --- | --- | --- |
| `pose_stand` | 0 | hold | pose |
| `pose_arms_up` | 0 | hold | pose |
| `pose_arms_forward` | 0 | hold | pose |
| `walk` | 1.1 s | loop | walk / 1.0 m/s |
| `run` | 0.68 s | loop | run / 2.244582043 m/s |
| `run_arms_forward` | 0.68 s | loop | run / 2.244582043 m/s |

`run_arms_forward` keeps the running legs/pelvis and the arms-forward hold;
`pose_stand` supplies the arms-down idle. All six clips loop. Choose AI
`move_clip` and `idle_clip` explicitly, or crossfade named holds through
`play_animation`:

```json
{"id":"pose_dummy","model":"mannequin","x":4,"z":3,"solid":false,
 "components":[{"component":"interactable","prompt":"Arms up"}],
 "bindings":[{"on":"interact","actions":[
   {"action":"play_animation","clip":"pose_arms_up"}
 ]}]}
```

The [entity showcase fixture](../../tests/fixtures/levels/entity_showcase.json)
cycles poses. Inspect the adopted model without exporting:

```sh
python3 tools/entities/build_mannequin.py --check-run-arms-forward
python3 tools/entities/validate_entities.py \
  --glb assets/entities/mannequin/model/mannequin.glb \
  --workers 1 --samples-per-second 120
```

For the completed model, the first command verifies its accepted SHA identity;
it does not regenerate clips. The retained constructor and historical
`--add-run-arms-forward` adapter belong to the older 23-joint model. That
adapter reuses existing animation accessors and preserves the BIN chunk,
geometry, skin, UVs, materials, PNGs and existing clips; `--source`/`--out`
support separate scratch files. The default completed GLB is protected from
replacement. A historical constructor comparison must use a scratch output
such as `--out /tmp/mannequin-constructor.glb`. The original concrete artwork
and model were authored in this repository without third-party asset sources.

## Cockroach

`entity:cockroach` is a tiny Office creature with six articulated legs, paired
wing covers, a pronotum shield, two articulated antennae and short rear cerci.
The flattened brown silhouette is built from 39 closed, outward-wound solids.

### Cockroach runtime contract

- Model: `cockroach/model/cockroach.glb`, one skinned mesh, primitive, skin and material.
- Geometry: 480 triangles, 1,440 vertices, 25 joints; one rigid joint influence
  per vertex. No morphs, scaling channels or external resources.
- Coordinate frame: metres, +Y up, +Z front, horizontally centred bounding box,
  lowest bind vertex at `y = 0`.
- Bind size: `[0.0874143732, 0.0185625857, 0.1321252692]` metres including antennae,
  tarsi and rear cerci. The body itself is approximately 0.088 m long.
- Opaque nonmetal material with roughness `0.48` for a restrained cuticle sheen.
  Geometry and per-part vertex pigment carry the detail; environmental lighting
  supplies its light and shadow response, with no emission or painted lighting.
  glTF `COLOR_0` is linear. The material's linear `baseColorFactor`
  `[0.2231106261, 0.1249638443, 0.0856918012, 1]` anchors the existing palette to
  the intended dark-brown `#462412` shell under unit white light. The source
  sheet's nearly white centre introduces less than one display-byte difference.
  The single embedded PNG is exactly the committed `cockroach/textures/white_01.png`.
- Sheet: 32×32, square, opaque, non-tiling, clamped; every UV samples its centre.
  It has no fitted atlas regions. Texture memory is 4,096 decoded RGBA bytes.

| Clip | Loop duration | Reference ground speed | Motion |
| --- | --- | --- | --- |
| `idle` | 1.80 s | none | subtle body settling and independent antenna sweeps |
| `walk` | 0.44 s | 0.0454545455 m/s | alternating tripods, 12 mm stride, 2 mm foot lift |
| `run` | 0.22 s | 0.1212121212 m/s | alternating tripods, 16 mm stride, 3 mm foot lift |

The planted phase lasts 60% of each cycle; the opposite tripod is half a cycle
behind. The two-link leg solver places each ankle, and a third joint holds each
tarsus level. Reference speed is stride divided by planted duration. Clips use
LINEAR keys and include identical first and final poses. The root never moves;
world navigation and gait-rate scaling belong to the ordinary entity runtime.

### Cockroach rebuild and inspect

```sh
python3 tools/props/build_cockroach.py
python3 tools/props/build_cockroach.py --check
```

The normal build reads the committed PNG without modifying it. The pure-Python
check reads the written GLB with both repository model inspectors, verifies
positions, skin indices and weights, per-part pigment, linear material factor,
nonmetal shell response, UVs, PNG identity, opaque white pixels,
closed topology, floor alignment, length and triangle budget. It samples 33
poses per clip to detect floor penetration, stretching and endpoint mismatch.
It does not bake a map or run the player.
No Rust test or CI work depends on this authoring command.

Native appearance and navigation remain part of the integrating level's
explicit visual validation.

### Cockroach sheet provenance

The new native sheet is a uniform resize of the existing committed shared white
sheet. It adds no texture artwork and retains opaque 252–255 RGB pixels. This
keeps the entity within the model texture class in [asset specification](../ASSET_SPECIFICATION.md)
§8 and the shared white contract in §12.2, without embedding the 4 MiB full-size
surface sheet. The original file was left unchanged.

```sh
sips -z 32 32 assets/shared/textures/white_01.png \
  --out assets/entities/cockroach/textures/white_01.png
```

Source PNG SHA-256:
`b8ece440e5c8cb37b58d8246bf4eb8e3bd6747d2103193cc96f208634bd1222f`

Native PNG SHA-256:
`68cecb89a6a560f93cdfe880754e98ad3a68feaeddfec44656c6e70f0fa4ec6f`

GLB SHA-256:
`2844f044a131e1639096613460cf8d303a196a96c2dcd6a87d890d5a8fff3940`

## Rat

A low-poly rat with a 25-joint hierarchy: root/pelvis/spine/chest/neck/head,
ears, four upper-leg/lower-leg/paw chains and five tail joints.
`textures/rat_fur_01.png` is the fitted native fur sheet. The original exact
Boolean construction joins the ears, neck, muzzle, limbs and tail into a
sealed body; UV splits retain matching skin weights so seams do not separate.

The origin centres the full bind box, including the long tail. It lies about
0.12 m behind the torso centre; +Z is the facing direction. Preserve the
runtime bounds expansion when evaluating tail swings beyond the bind box.

| Clip | Duration | Playback | Kind / reference speed |
| --- | --- | --- | --- |
| `idle` | 2.4 s | loop | idle |
| `walk` | 0.4 s | loop | walk / 0.198783102 m/s |
| `run` | 0.24 s | loop | run / 0.573099257 m/s |
| `dead` | 1.2 s | once | dead |

Walk is a diagonal-pair gait with planted paws; run has a longer stride,
streamed tail and brief suspension. The route resolver chooses run at 1.5×
the walk reference, about 0.298175 m/s. Use the exported metadata's speeds
when judging foot slide; older rounded values in historical reports are not
new reference declarations. The death clip requires an explicit playback cue.

`build_rat.py` is the original three-clip constructor and requires Blender
for anatomical union; the documented source export used Blender 5.2.2. The
committed model contains subsequent geometry work and the additional `dead`
clip, so a constructor rebuild must be reviewed in scratch. Provenance and
native lighting context are in the
[asset polish](../reports/asset-polish.md) and
[entity lighting integration](../reports/entity-lighting-integration.md) reports.

## Skeleton

The delivered 24-joint rig retains named holds. Its chair pelvis is approximately
0.4976905 m, supporting actual bones at a 0.45 m seat. The constructor pose
recipe below is historical; native contact acceptance uses the adopted silhouette.

A 24-joint human rig: root/pelvis/spine_01/spine_02/chest/neck/head/jaw,
paired shoulder/upperarm/forearm/hand chains, and thigh/shin/foot/toe chains.
The skull, segmented spine, five separated rib pairs, pelvis, long bones,
fingers and tapered feet are geometry; eye sockets, nose and teeth use the
fitted `textures/bone_01.png` sheet's skull/face/bone/dark cells.

Weights stay within each anatomical chain. Skull, jaw, pelvis, sternum,
vertebrae, clavicles, hands and feet are explicitly pinned; the source builder
rejects weights reaching a joint outside that part's chain.

| Historical constructor pose | Placement geometry |
| --- | --- |
| `pose_stand` | pelvis 0.920 m; soles y=0 |
| `pose_sit_floor` | pelvis 0.060 m; hips 135°, knees 93.5°; soles flat |
| `pose_sit_chair` | pelvis 0.450 m; hips 96.1°, knees 83.9°; skull top 1.250 m |

All three are duration-zero, looped `kind:pose` holds. Named actions crossfade
between them. Chair fit is measured against `core:chair`: pad top 0.465 m,
backrest front z=-0.1449. With chair centre `(cx, cz)` and yaw θ, place the
skeleton at the same yaw with `dz=+0.0025 m`:

```text
x = cx + dz * sin(θ)
z = cz + dz * cos(θ)
```

The documented fit gives 3.1 cm torso clearance, knees 0.19 m beyond the seat
edge and toes 0.20 m clear of the base. The lowest pelvis point is 0.440 m,
so it overlaps the cushion by 0.025 m; lowering the chair by 0.040 m is the
original alternative recipe where the level permits that offset. The
[showcase fixture](../../tests/fixtures/levels/entity_showcase.json) uses the
standard offset and a second floor-seated instance.

`build_skeleton.py` and `rig.py` are the original in-repository, standard-library
construction sources; the source check measures sole contact, pelvis heights,
joint angles, weight sums, edge stretch and chair fit. Review a rebuild against
the current GLB before publication. Bone art and model were authored here
without third-party asset sources.

## Halloween entities

These four models were authored and rigged outside the repository's primitive
toolkit, then registered in the same entity pipeline. Their committed GLBs are
canonical. `author_halloween_assets.py` maintains the material contract while
preserving geometry, UVs, vertex colours, skin and clips. Standalone native
artwork is `textures/surface_01.png` in each entity directory.

Emissive surfaces provide appearance; a moving light uses a runtime `glow`
component at the named animated socket. A static `props[].lights` entry would
leave baked illumination at the spawn. Ghost material opacity is 1 before the
runtime fade multiplier. The [lighting integration report](../reports/entity-lighting-integration.md)
records the original integration; the [asset polish report](../reports/asset-polish.md)
records subsequent repairs.

### Carved candle pumpkin

The floor-contact origin sits under the shell. Its four joints are root,
body, stem and flame. The completed model has four material primitives and
2,384 triangles; preserve its current material bindings and flame socket.

| Clip | Duration | Playback | Kind / reference speed |
| --- | --- | --- | --- |
| `laugh` | 2.4 s | loop | idle |
| `hop_forward` | 1.2 s | loop | walk / 0.5 m/s |

`hop_forward` puts a roughly 0.24 m jump arc on the body's Y translation while
the floor-contact root stays put. Route speed 0.5 m/s gives playback rate 1;
the resolver accepts `kind:walk` despite the different clip name. Use
`glow.socket:"flame"`; `laugh` suits endpoint pauses.

### Sheet ghost

Root/body/hem form the three-joint rig. The hem floats about 0.171 m above the
origin, with the top about 1.79 m above it; `y:0` preserves this authored gap.
Cloth and dark painted facial features occupy separate BLEND primitives,
with emissive factors `[0.14,0.5,0.58]` and `[0.03,0.1,0.12]` respectively.
The dimmer face stays readable against the luminous sheet.

| Clip | Duration | Playback | Kind / reference speed |
| --- | --- | --- | --- |
| `float_forward` | 4.0 s | loop | float / 0.3 m/s |
| `idle` | 4.8 s | loop | idle |

The body bobs ±0.035 m without horizontal displacement. Optional routes drive
travel. A `fade` component takes `period_seconds`, optional `phase`,
`min_opacity` and `max_opacity`; `glow` with socket `body`, cyan colour and
`fade:true` follows the same opacity. An omitted phase is deterministic;
author distinct phases when several instances should fade at different times.

### Pumpkin-head skeleton

The 94-joint skin comprises the 24-joint human hierarchy plus 70 rigid
`piece_*` joints; the 95th node is the mesh. `piece_pumpkin_head`, a child of
`head`, owns the pumpkin triangles. Partitioning those triangles into a
separate material group preserves the head's animation binding. The completed
model has five primitives; preserve that accepted grouping and skin. The head's emissive factor is `[0.6,0.24,0.07]`.

| Clip | Duration | Playback | Kind / reference speed |
| --- | --- | --- | --- |
| `walk` | 1.4 s | loop | walk / 0.571428571 m/s |
| `collapse_reassemble` | 12.0 s | once | gesture |

The collapse is intentionally excluded from loop-seam checks. Use either a
route or a `nav_agent` plus `ai.behavior:"wanderer"`. The original navigation
recipe uses radius 0.25 m, height 1.81 m, speed 0.571428571 m/s, step height
0.4 m and maximum slope 2.6667; set its wander radius for the actual supported
area. `glow.socket:"piece_pumpkin_head"` keeps a small warm light with the
animated head.

### Sheet ghost cat

Root/body/hem/tail_base/tail_tip form a five-joint rig. The hem floats about
0.046 m above its origin. The current fitted face extends the bind depth to
0.593 m. Both cloth and facial features have separate BLEND primitives with
the same bright-cloth/dim-face emissive factors as the sheet ghost.

| Clip | Duration | Playback | Kind / reference speed |
| --- | --- | --- | --- |
| `idle` | 3.6 s | loop | idle |
| `float_forward` | 3.0 s | loop | float / 0.2 m/s |

The original proximity-fade recipe uses `near_radius:1.0`, `far_radius:2.2`,
`fade_out_seconds:0.8` and `fade_in_seconds:1.6`. A cyan `glow` at socket
`body` with `fade:true` follows its opacity. Optional routes provide travel;
keep the authored float clearance rather than adding a second hem offset.

## Beach animals

The [Beach concept board](../../assets/references/beach/Low-Poly%20Tropical%20Beach%20Concept%20Board.png)
provides B15 seagull, B16 crab and B17 fish. Builders use rigid weight-one
closed pieces and triangle soup with geometric flat normals. Each retains an
opaque 1024² `textures/surface_master.png` and its exact 4×4 box-average
256² `textures/surface_01.png`; the native PNG is embedded unchanged. Fitted
atlases have four columns, two rows and four-native-texel UV insets:

| Entity | Top row | Bottom row |
| --- | --- | --- |
| Seagull | white / grey / light-grey / dark-tip | bill / feet / eye / glint |
| Crab | coral / top / underside / leg | claw / stalk / eye / glint |
| Fish | yellow / light-yellow / turquoise / blue | fin / light-fin / eye / glint |

Normal builders load committed artwork. `--author-textures` deliberately
repaints master and native PNGs offline. Their `--check` exports in a temporary
directory, compares committed bytes and evaluates geometry, skin, loop,
rigid-edge and bounds contracts. See the [Beach/Aero program record](../reports/two-environment-implementation-plan.md)
for the original acceptance and scene work.

The following local bounds are the retained 120 Hz authoring sweeps, rounded
outward in metres. Runtime envelopes use bind half extent × 1.15 + 0.05 m per
axis. These are geometric records; native lighting, water and silhouette
acceptance remain separate checks.

| Entity / clip | Minimum XYZ | Maximum XYZ |
| --- | --- | --- |
| Seagull `idle` | [-0.22030, 0, -0.50178] | [0.22030, 0.74806, 0.44429] |
| Seagull `fly` | [-0.55736, 0.15477, -0.44320] | [0.55736, 0.74806, 0.44429] |
| Seagull envelope | [-0.68020, -0.10603, -0.55946] | [0.68020, 0.85303, 0.55946] |
| Crab `idle` | [-0.41605, 0, -0.28061] | [0.41605, 0.38414, 0.28828] |
| Crab `walk` | [-0.42875, 0, -0.29840] | [0.42914, 0.38414, 0.28828] |
| Crab envelope | [-0.52846, -0.07858, -0.37270] | [0.52846, 0.45958, 0.37270] |
| Fish `swim` | [-0.15430, -0.00852, -0.31945] | [0.15250, 0.38430, 0.31901] |
| Fish envelope | [-0.21906, -0.07828, -0.41686] | [0.21906, 0.45528, 0.41686] |

### Seagull

The 13-joint rig has root/body/neck/head/tail plus paired wing/tip/leg/foot
joints. `idle` is a 3 s loop with folded wings; `fly` is a 0.8 s loop with
articulated wing beats, folded legs and `kind:airborne`. Neither declares a
ground reference speed or moves the flying placement.

On a real supported deck at world y=1.1, a perched gull uses prop `y:0`.
Above a floor at world y=0, an airborne gull can use `y:2.7`. Keep both
non-solid, clear of roofs/poles; initialize `idle` or `fly` explicitly using
the timer pattern below. Airborne motion stays at the authored position.

### Crab

The 18-joint rig has root/body, eight walking legs, two eye stalks and
mirrored claw/palm/finger articulations with visibly open pincers. `idle`
loops at 2.4 s; `walk` loops at 0.8 s with reference speed
0.16062456434191058 m/s. Its exported stance sweep measured 0.160624557 m/s:
a toe travels backwards 0.0770998 m over the 60%-duty stance interval.

This route assumes supported dry sand at y=0 along x=1..2, z=3, with the
resolved footprint clear throughout. Let route steps select walk/idle;
a permanent named-animation override would prevent those gait cues.

```json
{
  "props":[{"id":"sand_crab","model":"beach:crab","x":1,"y":0,"z":3,
            "solid":false,"occludes":false}],
  "routes":[{"id":"sand_crab","loop":true,"steps":[
    {"step":"play","clip":"idle","seconds":2.4,"loop":true},
    {"step":"move_to","x":2,"z":3,"speed":0.160624564},
    {"step":"play","clip":"idle","seconds":2.4,"loop":true},
    {"step":"move_to","x":1,"z":3,"speed":0.160624564}
  ]}]
}
```

A stationary crab can use an explicit idle timer cue instead.

### Fish and initial playback

The six joints are root/body/tail/dorsal and paired pectorals. The 1.2 s
`swim` loop has `kind:swim`, no ground reference speed, a tail beat, small
body turn and 7 mm vertical bob. Ear-clipped caps preserve the forked tail's
open centre notch and closed fin shells.

Allow at least 2 cm below the base for the bob and 0.385 m above it for the
posed fish. This fragment assumes water at world y=0 and a real submerged
floor at y=-1.0. Floor-relative `y:0.45` gives a base at -0.55 and a posed top
below -0.165. Adjust to the actual level and keep `solid:false` and
`occludes:false` for the underwater presentation:

```json
{
  "props":[{"id":"shallows_fish","model":"beach:fish","x":3,"y":0.45,"z":-6,
            "solid":false,"occludes":false,
            "components":[{"component":"animation","clip":"swim","looped":true}]}],
  "timers":[{"id":"fish_init","seconds":0.01,"autostart":true,"repeat":false,
             "bindings":[{"on":"timer","actions":[
               {"action":"play_animation","target":"shallows_fish","clip":"swim","loop":true}
             ]}]}]
}
```

The same initialization pattern starts seagull `idle`/`fly` or stationary
crab `idle`, using unique instance and timer ids.
