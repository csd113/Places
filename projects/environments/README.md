# Places / The Quiet Collection

Eight environments share one central hub: Home, Pool, Office, Cosmic Lanes &
Arcade, Lantern Hollow, Winter, Beach and Frutiger Aero. Each hub door has the
destination's materials, a familiar prop and a name plate. A physical door in
every destination returns to the hub.

WASD moves, E interacts/picks up/puts down, left click throws, F toggles the
flashlight and C crouches. Enter dismisses conversation. Home's caretaker asks
for the checked cushion; bring the actual cushion and speak while holding it.
F1 gives the Home hint. The hall switch resets that visit.

Visits start fresh, including objectives, door states, scripts and held objects.
Objects stay in their environment. Door travel requires a fresh aimed E press;
spawning near a door never automatically sends the player back.

Collection favors are saved automatically for this installed project. Open the
same installation again to resume; the Hub ledger reports saved progress and
save failures. Local retry and object-restoration controls keep those favors.
The separate CLEAR switch on the eastern Hub wall asks for a second press
within 12 seconds before erasing this collection's saved progress. It restarts
the Hub with the reward room locked; other projects keep their own saves.

Each destination has a host and F1 guidance:

- Home: hand the checked cushion to the caretaker, then give them privacy at
  the end of the winding west halls.
- Pool: release the duck upright in the pool for four consecutive seconds.
- Office: file one loose page from each of the three floors in the foyer bin.
- Arcade: bowl the blue ball at lane one's ten pins. Ball return keeps fallen
  pins; Fresh rack starts another unlimited try. The miniature secret remains.
- Lantern Hollow: find Jun near the northern conifer and reunite the fire circle.
- Winter: pass the three ice lanterns in order, then speak to the coach.
- Beach: escort the crab past the first parasol to the far garden shade.
- Aero: open controls totaling five measures, then speak to the gardener.

The JSON and Luau in this project are the editable authority. The original
environment generators are historical construction tools and must not overwrite
these edited maps. The complete package retains these exact sources and the
project thumbnail for recovery in the editor.

To play the existing verified package after building the direct tools:

```sh
PLACES_ASSET_ROOT="$PWD/assets" ./target/release/places --import /absolute/path/environments.placesmap
PLACES_ASSET_ROOT="$PWD/assets" PLACES_LEVEL=hub ./target/release/places
```

For the preserved local delivery, no build or import is needed:

```sh
PLACES_ASSET_ROOT="$PWD/assets" PLACES_LEVEL=hub \
  ./local/environment-collection-0.12.0/payload-collection-final/places
```

This uses the accepted package installed as `assets/levels/environments.placesmap`.
Only export again after editing the authoritative project:

```sh
./target/release/places-compile export-project projects/environments \
  --out assets/levels/environments.placesmap --asset-root assets --workers 6
```

The [retirement handoff](../../docs/reports/release-0.12.0.md#environment-collection-0120-retirement)
records superseded standalone IDs, preserved material and deferred work.

`local/environment-collection-0.12.0/` holds the stage's explicit developer
evidence and preserved direct executables. Its final report/handoff records
native acceptance, delivery commands, platform limits and package identity.
Ordinary Cargo tests and CI never bake this collection.
