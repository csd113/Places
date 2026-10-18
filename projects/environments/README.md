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

The JSON and Luau in this project are the editable authority. The original
environment generators are historical construction tools and must not overwrite
these edited maps. The complete package retains these exact sources and the
project thumbnail for recovery in the editor.

After building the direct tools:

```sh
PLACES_ASSET_ROOT="$PWD/assets" ./target/release/places-compile export-project \
  projects/environments --out /tmp/Places-Quiet-Collection.placesmap --workers 6
PLACES_ASSET_ROOT="$PWD/assets" ./target/release/places --import /tmp/Places-Quiet-Collection.placesmap
PLACES_ASSET_ROOT="$PWD/assets" PLACES_LEVEL=hub ./target/release/places
```

`local/environment-collection-0.12.0/` holds the stage's explicit developer
evidence and preserved direct executables. Its final report/handoff records
native acceptance, delivery commands, platform limits and package identity.
Ordinary Cargo tests and CI never bake this collection.
