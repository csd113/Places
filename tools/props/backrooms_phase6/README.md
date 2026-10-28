# Places 0.13.1 — institutional utility + HVAC kit

25 new static props, authored in the cloud with **Blender 4.3.2 and GIMP 3.0.4**. Low-poly institutional metal, restrained sage/cream, square manufactured stock, mild age; no bubbly forms or unrelated props. Existing source audited at `81d1893514f1409c02243f4b41448b05d99c3f82`.

## Delivered scope
- HVAC: 2.4 m and 1.2 m horizontal straight stock, 2.4 m vertical stock, 90-degree horizontal/up/down elbows. All ducts are genuinely hollow with open ends, 12 mm sheet stock, external flange rings and folded seams. Ordinary yaw gives either horizontal direction.
- Electrical: floor cabinet and wall breaker panel, each closed and visibly hinged-open.
- Doors: single utility and double wired-glass institutional doors. Separate stiles/rails surround a real glazing aperture. Opaque milked wired glass is deliberate, not a transparency claim. Door models are static, not animated or interactable doors.
- Suspended ceilings: missing, crooked, stained and exposed T-grid; 1.2 m stock modules, 0.6 m bays, real empty apertures.
- Fluorescents: recessed, surface, cable-hung, damaged and missing-diffuser, 1.2 x 0.6 m footprint.
- Wall clock and PA speaker; laminate folding table and stackable steel chair.
- Maintenance cart/supplies are **reused, not duplicated**: existing `janitor:janitor_cart`, bottle/mop/box/cloth stock. The accepted cart was fetched read-only and actually rendered in `qa/detail_janitor_cart_existing.png`; its angular utility style already fits. No existing model was modified.

## Source and export layout
- `models/`: final self-contained GLB files.
- `sources/`: individual editable packed Blender files, retained pass1/pass2 sources, and layered GIMP XCF.
- `textures/`: 1024-square master, 256-square native fitted atlas, optional 128-square derivative. Every GLB embeds its own copy of the 256-square PNG; keep the standalone equivalent beside models on import.
- `workflow/`: actual Blender/GIMP construction, export, round-trip rendering, validation and packaging scripts.
- `passes/` and `metadata/`: the first two exported construction stages and measured metadata for all three stages.
- `qa/`: actual exported-GLB round-trip renders, close-ups, validation report and review notes. Contact-sheet ceiling/fixture assemblies are flipped **only for inspection** to expose their undersides. This is not an export transform. Close-up hanging-fixture cables therefore appear below its displayed underside, but extend above the housing in the delivered model.

## Coordinates, mounts and connections
All final GLBs: **1 unit = 1 metre, +Y up, +Z front, X/Z centred, occupied base at Y=0**. `manifest.json` gives exact sizes and hashes. Wall items mount by their rear measured bound; ceiling pieces mount by their measured top Y. Crooked/damaged pieces include their displaced stock in the bounds. Do not assume the bounding box is the snapping grid.

HVAC raw passages are 0.60 x 0.40 m. Horizontal X stock has Z width .60/Y height .40; vertical stock has Z width .60/X depth .40 to mate with vertical bends. Match positions and opposite outward normals in `hvac_connectors.json`, then apply ordinary world placement/yaw. Small flange/seam projection is included in measured bounds. The ceiling stock grid is exactly 1.2 m even though small tabs project beyond it. Use a 1.2 m placement increment, not the occupied-envelope width.

## Materials and image contract
One 4x4 fitted GIMP atlas, 256 native, 1024 master; UVs stay in 0..1, clamp, no runtime image generation. Roles and region order are in `metadata/texture_contract.json`. Metal uses scalar metal .55/roughness .48; other roles matte. All materials are opaque, and all are non-emissive. The wire pattern is milked wired glass. Lens/tube artwork is albedo, not a real light source. No material normal, baked AO or directional-light texture is required. Closed sheet stock casts ordinary opaque shadows; genuine holes contain no surface.

## Import into Places
1. Copy `models/*.glb` to `assets/environments/backrooms/props/` and `textures/liminal_utility_256.png` beside them. Retain master/XCF/editable .blend files under the chosen authoring-source location.
2. Merge `catalog_entries.json` into `assets/catalog.json` and `completed_model_entries.json` into `assets/completed-models.json`. These are **fragments**, not replacements for those files. Reuse existing IDs listed in the manifest for cart/supplies; do not overwrite or duplicate them.
3. Recompute the complete pack budget. New images add **6,553,600 decoded RGBA bytes (6.25 MiB)** across 25 embedded copies. The audited old spec says about104.4/106 MiB; assess/increase that outdated aggregate budget with justification rather than dropping image detail silently.
4. Default catalog collision is conservative. Add narrow compound collision for duct walls/doors/legs if required; do not fill a hollow passage or exposed grid merely because its visual bounds are a box. Open cabinet covers are fixed geometry, not an interaction state machine.
5. Fluorescent props are passive models: integrate switched runtime lights with the corresponding model/fixture if desired. Do not make their metal housings emissive. Clock is fixed10:08; PA speaker has no automatic sound.
6. HVAC audio must be runtime proximity-only to these six duct IDs (including edited/rotated placements), not the old generic ceiling grille. This bundle deliberately does not wire sounds or modify game code. Integration must validate walk-away fade/stop, no ducts/no sound, up/down runs, map switches and editor delete/move cases.
7. Run repository content/asset validation plus the authorized native editor/game checks. Cloud authoring validation is **not** a substitute for native engine acceptance.

## Reproduce
Set `PLACES_PHASE6_ROOT` to this unpacked bundle directory. Run GIMP Python batch `workflow/paint_atlas_gimp.py` in an isolated writable GIMP profile, then `blender -b -t 10 --python workflow/build_assets.py -- --pass 1 --render`, repeat with `--pass 2` and `--pass 3`, and run `python3 workflow/validate_glb.py`. Construction scripts import only the bundled helpers. Python validation/contact composition uses Pillow. Blender renders use Cycles CPU24 samples with denoising disabled because this Blender build has no OpenImageDenoise support. The workflow has no paid/network asset dependency.
