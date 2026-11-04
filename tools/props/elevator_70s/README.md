# 1970s passenger elevator artwork

Original MIT-licensed artwork, authored offline in GIMP 3.2.6 and Blender 5.2.3.
The retained XCF, Blender document, master/native PNGs and four runtime exports
live in `assets/shared/props/elevator_70s/`. The source document contains four
collections/rigid mesh parts; the game supplies their authoritative transforms.
Neither authoring script runs during startup, loading or ordinary asset checks.

The GIMP atlas uses sixteen opaque fitted cells in row-major order: walnut,
wood rail, linoleum, brushed steel, charcoal, cream ceiling, satin gray door,
aged brass, warm diffuser, amber indicator, button, rubber, lighter walnut,
linoleum inset, vent and service plate. The 1024-square layered/master art is
exported at the ordinary 256-square native prop resolution. UVs use a 4.5%
cell inset; wall grain runs upright and rail grain follows the rail length.
The visual reference's plain gray doors and narrow metal surround remain.

A cabin body, landing frame, sliding leaf and one button are separate GLBs.
All preserve +Y up and metre dimensions. Their origin is the mechanism's
threshold, rather than an ordinary centre-bottom prop origin. Button stock
starts at the first control anchor and is repeated only for the declared stops.
The cabin shell has 952 triangles before button assembly; frame 300, leaf 44
and button 68. Eight buttons keep the assembled body below 1,500 triangles.
The diffuse atlas is shared; only the lamp face emits. One bounded moving light
supplies warm illumination through the engine's existing realtime light path.

These files are `mechanism` catalog resources, excluded from ordinary prop
placement and included in the same GLB/PNG/UV/triangle audit. Use the editor's
**Elevator / floor travel** tool for a complete functional lift. Exported map
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
