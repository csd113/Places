# Shadow Gallery

Opt-in 0.12.0 native review, separate from the accepted Beam game. Four placed
hidden mannequins and one scripted hidden spawn keep their real posed flashlight
shadows. The fifth placed mannequin is the ordinary visible control. F switches
the flashlight; sweep it across the floor and two partition walls. The left
corridor gives an unobstructed >24 m range check; approach its far mannequin to
bring it into range. The opaque right partition blocks another actor.

The left wall's opaque mirror pane reflects the ordinary control while hidden
meshes stay absent. The near hidden actor deliberately keeps `solid: true` and
an E interaction; its notice proves interaction and collision are independent.
The visible actor resets the gallery. Other hidden actors are walk-through.

From the repository root, after this stage's preserved delivery:

```sh
local/shadow-only-0.12.0/play.sh
local/shadow-only-0.12.0/editor.sh
```

Rebuild with direct executables if needed:

```sh
cargo build --locked --release --workspace --all-features
target/release/places-compile export-project projects/shadow-only-review --out /tmp/Shadow-Gallery.placesmap --asset-root assets --workers 8
PLACES_ASSET_ROOT="$PWD/assets" PLACES_STATE_ROOT=/tmp/places-shadow-review target/release/places --import /tmp/Shadow-Gallery.placesmap
PLACES_ASSET_ROOT="$PWD/assets" PLACES_STATE_ROOT=/tmp/places-shadow-review PLACES_LEVEL=room_1792320687064 target/release/places
```

The native editor created the room, six walls, five mannequins, fixture and
placed shadow flags through retained visible-control recipes. A subsequent
explicit source pass added names, loop routes, startup/spawn animation actions,
script query/spawn, independent collision/interaction, mirror opening and the
clear far-lane position. The editor reopens that source, preserves its exact
bytes after toggle/undo/save, and displays models in Inspect preview; Environment
preview and Play apply invisibility. No artwork or accepted game was replaced.

Replay actual native acceptance using the prepared package:

```sh
python3 tools/authoring/shadow_only_native.py --binary local/shadow-only-0.12.0/payload/places --package local/shadow-only-0.12.0/Shadow-Gallery.placesmap --level room_1792320687064 --out local/shadow-gallery-replay --case acceptance
```

Use a new output directory. Native UI requires a desktop session. These are
explicit developer tools; Cargo and CI never bake this map. See
[stage evidence](../../docs/reports/release-0.12.0.md#editor-0120-shadow-only).
