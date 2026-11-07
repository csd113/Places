# Private preparation: flush hollow-metal exit doors

Prepared for later Places integration, not installed or native-accepted.
Baseline: `csd113/Places` at `624e4da8273200e7c5b843c106208a5afa7c1a68`.
No Mac commands, canonical checkout edits, builds, commits or pushes were performed.

## Deliverables

- `source/hollow_metal_exit_door_{single,double}.blend`: editable, packed Blender scenes.
- `source/exit_door_materials.xcf`: native five-layer GIMP master, 1024 square.
- `source/exit_door_materials_master_1024.png`: flattened source-size artwork.
- `textures/exit_door_materials.png`: ordinary fitted, clamped, opaque 256-square PNG.
- `runtime/hollow_metal_exit_door_{single,double}.glb`: complete rigid mechanisms.
- `qa/`: front, reverse, 90-degree open and hardware views of actual re-imported GLBs, plus logs and exact re-exports.
- `metadata/asset_gate_report.json`: exact GLB identities, node/material/primitive order, bounds, UV/normal/topology and animation measurements.
- `metadata/integration_proposals.json`: measured host fit, placement offsets and named collision boxes. These are proposals, not engine-validated source.
- `metadata/blender_reopen_import_gate.json`: saved-source re-export parity and independent Blender import bounds.
- `metadata/provenance.json`, `artifact_identities.json`, `immutable_research_sources.json`: provenance and exact hashes.
- `reference/image.png`: unchanged actual supplied reference, inspected before design.
- `research/`: byte-verified immutable authoring/shadow/importer contracts, not project modifications.

## Name and appearance

Generic official terminology: **flush hollow-metal exit door**, or **pair of flush hollow-metal exit doors**, with a **surface-mounted vertical-rod exit device** (panic hardware). The reference shows plain grey leaves, exposed top/bottom rods, crossbars and overhead closers. The single adapts that hardware arrangement. Reverse faces are deliberately plain exit-only faces, with no invented keyed trim.

Terminology references: [Steel Door Institute nomenclature](https://steeldoor.org/glossary/) and [surface vertical rod exit device definition](https://kc.allegion.com/kb/article/surface-vertical-rod-exit-device/).
No reference manufacturer, brand, fire rating, gauge, certification, security class or compliant real-world egress specification is asserted. Dimensions below are practical game dimensions, not certified product dimensions.

## Budgets and geometry

| | Single | Pair |
| --- | ---: | ---: |
| Triangles | 728 | 1,268 |
| Exported vertices | 1,440 | 2,496 |
| Primitives | 9 | 16 |
| Materials | 2 opaque | 2 opaque |
| Joints | 0 | 0 |
| Nominal host aperture | 1.0 x 2.15 m | 1.9 x 2.15 m |
| Closed occupied size | 1.152 x 2.21 x .252 m | 2.052 x 2.21 x .252 m |
| Full-open conservative body-path width | .9205 m | 1.7570 m |
| Proposed collision boxes | 11 | 18 |

Both meet the existing 1,500-triangle art maximum and ordinary 6,000-triangle engine ceiling without an exception. The pair's extra visible mechanical detail justifies exceeding the preferred 800-triangle review threshold. No importer, joint, material, texture or global pack budget is raised. One unique PNG costs .25 MiB decoded RGBA8; two embedded copies total .5 MiB before engine deduplication. Whole-pack resident memory was not measured here.

Metres, +Y up, +Z push side. Leaves are 45 mm thick, begin 14 mm above the floor, and swing away from the push side through 90 degrees. The threshold is 8 mm high. Source shapes are closed manufactured stock with sharp or single-cut bevel normals, not hidden screw threads. Four shared contact edges in the compound frame are reported explicitly; the frame is closed stock, not a Boolean-welded single shell. No open boundary edges or inverted stock were found.

## Mechanical and placement contract

`exit_open` is a zero-based 1.1-second LINEAR rigid clip. The single uses `hinge_L`; the pair mirrors `hinge_L` and `hinge_R` and opens both together. Fixed frame and closer shoe nodes remain fixed. Two rigid closer arms follow each leaf without animated scale; maximum endpoint gap over 281 sampled poses is .0617 mm. This models the appearance of a closer, not automatic closing logic.

Preserve all semantic nodes. Each leaf retains separate `core`, `panic_hardware`, `hinges` and `closer` nodes under its real hinge; the fixed `frame_stock`, closer brackets and two arms remain independently identifiable. Clip names, primitive/material order and exact hashes are recorded in JSON.

The source was centred under its closed occupied footprint without moving hinges relative to their hardware or frame. The frame/wall plane is therefore at local Z=-.028 m, while hinge axes are at local Z=.002 m. At yaw zero, place the prop Z=.028 m beyond the desired wall centre plane. Use the measured transform metadata for other rotations, not guessed centring. The authored host wall depth is .160 m. Do not nonuniformly scale the assembly to fit another depth. Returns sit 2 mm inside the host aperture to avoid coplanar wall-reveal faces; source stops reduce frame clear width by 16 mm and frame clear height above the threshold to 2.134 m. Reserve the full sampled sweep plus a practical margin.

The current `animated_door` compound-prop route can represent these rigid clips and named boxes without a new format or engine change. It must use `solid:false`, the real `exit_open` clip, `initial_open:false`, and the named boxes in the proposal. Never use one whole-frame bounding box that fills the opening. Hardware boxes are intentionally conservative. They still require native collision validation.

This delivery does not implement native slab `DoorKind`, its lock/state API, independent pair-leaf controls, a right-hand single variant, or an editor door-picker change. Those need an explicit later integration decision. If the native slab route is chosen, preserve the physical hinge convention and adapt its visual resource contract in the canonical task rather than pretending a static prop is an interactive door.

## Validation and later acceptance

Completed offline: actual reference-pixel inspection; GIMP XCF reopen with all five layers; finite fitted UVs and outward unit normals; nondegenerate triangles and closed stock; PNG byte parity and opaque alpha; all ordinary profile/budget caps; exact floor/footprint origin; 281 rigid upright poses and closer-link samples; mirrored 90-degree pivots; saved `.blend` byte-identical re-export; actual runtime GLB Blender re-import, bounds checks and front/reverse/open/detail review renders.

Not performed: executing Places' importer, whole-catalog asset checks, native/editor placement, collision/interaction, live opening/reversing/reset/obstruction, AI/item/held-object clearance, lighting/reflections, Windows validation or native acceptance. Blender's floor shadows are preview renderer output, not proof of Places grounding.

Later canonical integration must first preserve existing work and use the queued source owner. Register exact accepted exports and original PNGs with source ownership; verify current catalog/schema, completed-model identity, pack allocation and asset gates. Then use a bounded native review fixture to test closed startup, repeated open/close and mid-swing reverse, blocked movement and resumption, pair opening, frame aperture and player/item/AI clearance, correct placement/rotation/reset, and editor save/reopen. Do not silently add automatic closing or lock semantics.

Follow `docs/assets/SHADOWS.md`: ordinary material lighting, actual rigid transforms, independent flashlight casting and eligible floor grounding. No cavity attenuation or baked body self-shadow is supplied. Validate closed/open/rotated and raised controls, changed light direction, Static/Full, low/high textures and reflection routes. Keep frame support separate from moving leaf bounds and do not force a footprint onto overhead brackets. Native acceptance remains a later explicit result.

## Reproduction

Run only in this private asset folder, not the canonical checkout. Installed tools used: Blender 4.3.2, GIMP 3.0.4. No new installations, downloaded models, third-party textures or image-generation stand-ins were used.

```sh
export EXIT_DOOR_ROOT="$PWD"
# Repaint only when deliberately replacing artwork; ordinary builds read PNGs.
GIMP3_DIRECTORY="$PWD/.gimp-config" gimp-console --new-instance --no-interface --no-data --no-fonts \
  --batch-interpreter=python-fu-eval \
  -b "exec(open('$PWD/source/paint_exit_door_gimp.py').read())" --quit
EXIT_DOOR_SKIP_RENDERS=1 blender -b --factory-startup -t 4 --python source/build_exit_doors.py
python3 source/validate_exit_doors.py
blender -b --factory-startup -t 4 --python source/reopen_export_and_render.py
python3 source/prepare_handoff.py
```

These are offline authoring/QA tools. Runtime loads saved PNG/GLB files and must not generate these textures at launch. All prepared assets and previews remain outside Rust build targets.

Preparation stops here, ready for the later door integration slot. The map blueprint/layout/build remains deferred until the user is awake and approves it. No Drift Lung or cat work is included.
