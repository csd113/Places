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
The cabin shell has 1,096 triangles before button assembly; frame 386, leaf 56
and button 56. Seven buttons retain the advanced body below 1,500 triangles.
The simple linked flow currently allows two destination buttons.
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
