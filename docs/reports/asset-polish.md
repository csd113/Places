# Places asset refinement — 26 September 2026

This pass refines the assets already present in the working tree. It preserves their logical IDs, model paths, dimensions, metre scale, +Y up / +Z front conventions, rigs and gameplay roles. No commits, dependencies, engine architecture changes or unrelated asset rebuilds were made.

## Deliverables

| # | Asset / clip | Refinement and final location |
|---|---|---|
| 1 | Rat | Fuller haunch and narrower shoulders; continuous weight blending across joined anatomy and rigid soles. `assets/entities/rat/model/rat.glb` |
| 2 | Human skeleton | Fuller cranium, shaped rib cage, narrower sternum, paired radius/ulna forms; retained articulated hands, feet and poses. `assets/entities/skeleton/model/skeleton.glb` |
| 3 | Stop sign | Flat-top octagon with matching fitted source artwork, corrected rim winding. `assets/core/props/models/stop_sign.glb` |
| 4 | Hanging green Exit sign | Closed bevelled housing and inset luminous face; original rods, canopy, lettering and emissive material preserved. `assets/core/props/models/exit_sign.glb` |
| 5 | Hanging white ball light | Twelve-sided globe, corrected face winding, socket collar and retained cord/rose. `assets/environment/home/props/models/ball_light.glb` |
| 6 | Wall light switch | Bevelled closed plate and rocker, round screw heads, recessed rocker surround; preserved hinge and `toggle` clip. `assets/environment/home/props/models/wall_switch.glb` |
| 7 | CRT TV | Deep tapered rear housing and cooling slots; repaired repeated screen UV strips, retained curved glass and controls. `assets/environment/home/props/models/crt_tv.glb` |
| 8 | Knife | Bevelled walnut handle and bolster; corrected blade top winding. `assets/environment/home/props/models/knife.glb` |
| 9 | Fork | Bevelled handle, neck, head and four individually readable tines. `assets/environment/home/props/models/fork.glb` |
| 10 | Spoon | Thin-walled, concave oval bowl replaces the sealed circular mouth; bevelled handle. `assets/environment/home/props/models/spoon.glb` |
| 11 | Plate | Sixteen-sided rim/well silhouette and corrected winding on the ceramic shell. `assets/environment/home/props/models/plate.glb` |
| 12 | Bowl | Deeper usable interior and sixteen-sided rim, corrected shell winding. `assets/environment/home/props/models/bowl.glb` |
| 13 | Small potted plant | Defined glazed rim around the soil; retained seven closed folded leaves and original table-scale proportions. `assets/environment/home/props/models/plant_table.glb` |
| 14 | Yellow rubber duck | Broad flat bill, fuller crown and low-relief folded wings; corrected body/head winding. `assets/environment/pool/props/models/rubber_duck.glb` |
| 15 | Ceiling vent | New clean stylized grille artwork with bevel, slats and corner fixings; retained square 128×128 cut-out contract. `assets/core/decals/ceiling_vent_01.png` |
| 16 | Curved wall geometry | Reusable 90° and 180° engine-native modules: 2 m centreline radius, 24 cm thickness, 2.8 m height, 12 facets per quarter turn. `docs/asset-presets/curved_architecture.json` |
| 17 | Circular pillar geometry | Reusable engine-native 60 cm diameter, 2.8 m height, 16-facet pillar with centre-floor origin. Same preset file. |
| 18 | Spoonerman idle | Quieter breathing/tail movement, denser keys, exact standing endpoint; in `spooner-man.glb`, clip `idle`. |
| 19 | Spoonerman walking | Smooth body-bob curve, denser sampling, repaired skin transitions; clip `walk`. |
| 20 | Spoonerman sit-down | Denser smooth transition and repaired haunch weights, continuous seated endpoint; clip `sit_down`. |
| 21 | Spoonerman sitting-idle | Subtle breathing, planted front-paw compensation, exact seated endpoint; clip `sit_idle`. |
| 22 | Spoonerman stand-up | Denser eased transition and exact standing endpoint; clip `stand_up`. |
| 23 | Rat idle | Reduced tail sweep; repaired skin junctions, stationary foot solve and exact loop closure; clip `idle`. |
| 24 | Rat walking | Calmer tail movement, continuous weights and retained analytic foot placement; measured speed metadata; clip `walk`. |
| 25 | Rat running | Reduced exaggerated tail lift, repaired ankle/haunch weights, retuned stride and measured speed metadata; clip `run`. |
| 26 | Mannequin standing | Broader chest/waist, outward-facing body surfaces, new light-grey concrete; `assets/entities/mannequin/model/mannequin.glb`, `pose_stand`. |
| 27 | Mannequin arms-up | Same refined geometry/material and preserved validated rig; `pose_arms_up`. |
| 28 | Mannequin arms-forward | Same refined geometry/material and preserved validated rig; `pose_arms_forward`. |
| 29 | Skeleton standing | Updated anatomy on the shared rig; `pose_stand`. |
| 30 | Skeleton floor-sitting | Updated anatomy, retained solved feet and hand placement, validated static hold; `pose_sit_floor`. |
| 31 | Skeleton chair-sitting | Updated anatomy, retained 45 cm seat reference and flat feet, validated static hold; `pose_sit_chair`. |

Spoonerman's five clips are embedded in `assets/entities/spooner-man/model/spooner-man.glb`. Its original mesh, textures, joint hierarchy and proportions remain intact. The six human pose variants ship as named hold clips in their shared rigged GLBs, not duplicate static files. The architectural assets use the project's native level format, not decorative GLBs; they retain real collision, baked-light occlusion and world-scale texture mapping.

## Artwork and rigging

Two new artworks: the mannequin's opaque 256×256 `concrete_grey_01.png` and the 128×128 transparent vent sheet. Their builders now load the finished PNGs rather than repainting them. Five fixture/sign/electronics atlases were preserved as standalone source PNGs beside their GLBs (`stop_sign`, `exit_sign`, `ball_light`, `wall_switch`, `crt_tv`); their builders embed those files. Spoonerman's three existing embedded sheets were also extracted unchanged into `assets/entities/spooner-man/textures/`. Other fitted atlases retain their layouts and artwork.

No new rigs or joints: Spoonerman 26, rat 25, mannequin 23, skeleton 24. Skin weights were repaired on the rat and Spoonerman without changing their rest skeletons. Loops close; Spoonerman's idle/sit/stand boundaries match within 0.00001 in the exported channels. Locomotion remains in place with reference-speed metadata; sit/stand use local pelvis movement, not route/root translation.

## Previews and technical evidence

- [Props, sheet 1](asset-polish/sheet_1.png): stop sign, exit sign, globe, switch, CRT; knife, fork, spoon, plate, bowl.
- [Props, sheet 2](asset-polish/sheet_2.png): potted plant, rubber duck.
- [Mannequin](asset-polish/mannequin.png), [skeleton](asset-polish/skeleton.png), [rat](asset-polish/rat.png), [Spoonerman](asset-polish/spoonerman.png): front/side/three-quarter clip contact sheets.
- [In-game integration capture](asset-polish/game-showcase.png).
- [Exact changed asset/tool file inventory](asset-polish/files.json) and [per-model triangle, rig and clip inventory](asset-polish/models.json). [Export hashes and matching PNG sources](asset-polish/exports.json) identify the delivered bytes.
- [Architecture placement instructions](../asset-presets/README.md).

The software preview camera was corrected so signs are no longer mirrored. The Blender contact-sheet tool now explicitly selects action slots, disables imported NLA overrides and provides room for raised-arm poses. The entity deformation validator now examines every skinned primitive, including all three Spoonerman parts. `tools/entities/check_clip_boundaries.py` adds an export-level check for normalized skin weights, loop seams and sit/stand transitions.

The rat passes 62 builder checks plus the independent 60 Hz sweep (maximum edge stretch under 1.8×). Spoonerman passes the all-primitive 60 Hz sweep (under 2×). Its walking/transition samples can still dip about 9 mm below the floor, within the project's contact tolerance; this is recorded rather than described as perfect foot locking. Separate closed components intentionally meet/intersect at prop assembly joints and humanoid articulations. UV/material seam vertices remain where needed.

## Validation

Final validation passed: **1,265 Rust tests passed, 0 failed, 8 ignored**, plus formatting, strict Clippy and the asset checks. Exact command outcomes are recorded in [validation.txt](asset-polish/validation.txt). Detailed intermediate logs and reversible before-state backups are under `target/asset-polish/` (not shipped).

All requested items have usable outputs. No asset was left as a missing export. The character contact sheets are offline visual checks; the in-game capture verifies the props, vent and native curved architecture. No claim is made of exhaustive interactive gameplay testing of every animation transition. Existing world-texture preferred-resolution warnings are unrelated to this pass.
