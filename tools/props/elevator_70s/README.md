# 1970s passenger elevator artwork

Original MIT-licensed artwork, authored offline in GIMP 3.2.6 and Blender 5.2.3.
The retained XCF, Blender document, master/native PNGs and four runtime exports
live in `assets/shared/props/elevator_70s/`. The source document contains four
collections/rigid mesh parts; the game supplies their authoritative transforms.
Neither authoring script runs during startup, loading or ordinary asset checks.

The GIMP atlas uses sixteen opaque fitted cells in row-major order: walnut,
wood rail, linoleum, brushed steel, charcoal, cream ceiling, satin gray door,
aged brass, warm diffuser, amber indicator, button, rubber, lighter walnut,
linoleum inset, number/letter glyphs and service plate. The 1024-square layered/master art is
exported at the ordinary 256-square native prop resolution. UVs use a 4.5%
cell inset; wall grain runs upright and rail grain follows the rail length.
The visual reference's plain gray doors and narrow metal surround remain.

A cabin body, landing frame, sliding leaf and one button are separate GLBs.
All preserve +Y up and metre dimensions. Their origin is the mechanism's
threshold, rather than an ordinary centre-bottom prop origin. Button stock
starts at the first control anchor and is repeated only for other destinations;
the current stop is omitted. Its front cap selects a committed glyph in cell 14.
The cabin shell has 1,324 triangles before button assembly; frame 240, leaf 56
and button 54. Each rigid resource remains below the 1,500-triangle asset limit.
The simple linked flow allows two destination buttons plus the separate green
door-arrow button, for 1,486 assembled triangles. Glyph slot 15 is reserved for
the committed door icon. The backing's concealed rear face is omitted.
The diffuse atlas is shared; only the lamp face emits. One bounded moving light
supplies warm illumination through the engine's existing realtime light path.

These files are `mechanism` catalog resources, excluded from ordinary prop
placement and included in the same GLB/PNG/UV/triangle audit. Use the editor's
**Build > Elevator** tool for a complete functional lift. Exported map
packages record all four model identities, including their embedded PNGs.

To deliberately rebuild art, set `PLACES_ELEVATOR_ART` to the absolute asset
folder and run `paint_gimp.py` through GIMP's `python-fu-eval` batch interpreter,
then `model_blender.py` through Blender's background Python mode. Use an isolated
GIMP profile under `local/` and review scratch exports before replacing accepted
art. The installed application executable paths differ by platform.
For a cabin-only refinement, set
`PLACES_ELEVATOR_EXPORT_PARTS=elevator_cabin_70s`; the retained Blender document
still contains every rigid part, while unrelated GLB exports remain untouched.

Validation uses the ordinary asset/prop checks and direct native campaign:

```sh
python3 tools/assets/validate.py
python3 tools/props/build.py --check
python3 tools/bench/elevators.py --binary target/release/places \
  --package /tmp/elevator-lab.placesmap --out /tmp/elevator-check
python3 tools/bench/elevators.py --binary target/release/places \
  --package /tmp/elevator-lab.placesmap --out /tmp/elevator-views --mode views
```

Keep builds, bakes and captures in explicit local evidence directories. Preserve
editable artwork; remove only task-owned backups and reproducible build caches.

Hollow front pockets provide separate dark channels for landing and cabin
leaves. Full-open travel retains a narrow visible panel lip beside each channel;
the 1.35 m clear aperture stays unobstructed. Concealed rear seals close the
centre, sides and transom. The welded fascia and separated wood/metal joins avoid
exposed coplanar faces. The front pocket assembly requires a 3 m wide footprint.
Use check_seals.py and check_surface_joins.py here for optional real-GLB sightline,
clearance, visible-edge and exposed-overlap checks; native moving views remain
necessary. These checks neither bake maps nor run in ordinary CI.
The narrow frame and doorway folds/returns use brushed steel. The surrounding
front wall, including above the header, retains walnut like the side/rear walls.
Disjoint solid stock provides a metal collar to X=+/-0.755 m and Y=2.24 m, with
wood beyond it. Run check_frame_materials.py for actual-GLB metal-frame and
wood-wall sightlines. Mechanism anchors and the clear aperture stay unchanged.

The inside-facing animation exposed atlas minification bleed on skinny stock.
Uniform charcoal/rubber faces now sample their committed centre texel; elongated
faces crop the cell using the physical face aspect, preserving wood-rail grain
orientation. This keeps neighbouring atlas colours out of exposed pocket edges.
The isolated UV correction preserved geometry. The subsequent fitted steel stop
frame and full-width backed threshold add actual stock to the cabin; full-open
leaf travel is 0.681 m, retaining a visible lip and the 1.35 m clear opening.
Run check_uv_stability.py for the CPU UV guard and tools/bench/elevator_motion.py
for lossless direct-binary animation evidence; static overlap checks alone cannot
establish texture stability. Re-export elevator maps after updating the models,
because packages retain explicit mechanism dependency identities.

Doorway fit refinement: the landing frame is inset into the exact 1.50 × 2.25 m
host opening, with its hall face at local Z=0. The cabin frame finishes on the
walnut wall plane at Z=0.360 m. Both jamb apertures end at X=±0.688 m; 7 mm of
each open leaf remains visible. The sliding steel faces are 20 mm apart (24 mm
including the concealed centre seal), retaining the original conservative
collision envelope and all transform anchors. The frame has 240 triangles;
its formerly projecting, Boolean-welded stock is replaced by fitted returns.
Side/rear bronze skirts reach the floor and meet continuously at the corners,
closing the former 25 mm floor-wall gap without changing the cabin finishes.
`check_doorway_fit.py --baseline 9a771f1b` protects unchanged geometry/UVs outside
the doorway and specifically authorized floor skirts; `check_cabin_corners.py`
probes every shell edge and the floor joins. These are explicit offline checks.
