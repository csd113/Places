# Crowbar and health artwork

The current original assets were authored with Blender 5.2.3 LTS and GIMP 3.2.6.
Editable scene/layer masters are in `source/`. Runtime files are committed under
`assets/shared/tools/` and `assets/shared/ui/`; nothing is painted at startup.

Authoring order:

1. Run `paint_tools_gimp.py` through GIMP 3's `python-fu-eval` batch interpreter,
   with `PLACES_ART_ROOT` set to the repository root. It paints layered enamel,
   steel and handling wear, plus the translucent heart, and saves XCF/PNG files.
2. Run `python3 tools/player/pack_health_icon.py`. Pillow only copies the saved
   GIMP icon into the reserved atlas cells. Every other RGBA byte is preserved,
   including invisible RGB that GIMP's compositor may normalize.
3. Run Blender from the repository root with
   `-b -t 1 --python tools/player/build_crowbar.py`. It creates and bevels the
   original forged geometry, loads the GIMP PNG, saves the packed Blender scene,
   exports the closed model and records bounds, triangles and hashes. Only its
   explicitly authored objects are exported; imported bone-display helpers are
   excluded. Metre bounds are checked before saving.

The right-arm export retains the mannequin skin PNG, UVs and original crop.
Its hash guard must be reviewed after any mannequin replacement. Both view
meshes use the grip origin and axis contract in `docs/ASSET_SPECIFICATION.md`.
The robot attachment uses that same crowbar through its existing hand socket.

`tools/entities/add_encounter_clips.py` preserves the original mannequin rig,
geometry and 19 clips while appending six encounter clips. Its exact accepted
input can be recovered with:

```sh
git show '3f61a500^:assets/entities/mannequin/model/mannequin.glb' > /tmp/places-mannequin-r2.glb
```

The script checks that input's SHA-256 before doing any work. Run it with
Blender, passing the input and output paths after `--`. Review the generated
clip sweep and source hash before updating the accepted model ledger or arm
export guard. The new crowbar run uses the original run's stride/reference
speed; gameplay drives playback from measured body displacement.

Validate assets in the real game, including first-person swing, look-down,
holster, multiple FOVs, robot hand attachment, half health and health disabled.
The editable masters are evidence of the authoring process, not a replacement
for native presentation checks.
