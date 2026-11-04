# Places / The Quiet Collection

Eight environments share one central hub: Home, Pool, Office, Cosmic Lanes &
Arcade, Lantern Hollow, Winter, Beach and Frutiger Aero. Each signed hub bay has
the destination's materials, a familiar prop and a passenger elevator. Call it
with E or left click, walk inside, then select the destination button. The
return elevator in each place brings you back to the matching hub bay.

WASD moves, E interacts/picks up/puts down, left click throws, F toggles the
flashlight and C crouches. Enter dismisses conversation. Home's caretaker asks
for the checked cushion; bring the actual cushion and speak while holding it.
F1 gives the Home hint. The hall switch resets that visit.

Visits start fresh for local objectives, doors and scripts. Elevators retain
the rider's cabin-relative pose through loading and open automatically on
arrival. Ordinary supported props and NPCs fully inside can travel too; actors
with map-bound tasks refuse travel. Destination buttons require a fresh aimed
E press or click, so arriving never automatically sends the player back.

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

The historical 0.12.0 local delivery is retained at the following location;
its old package must be re-exported before use with the current compatibility
contract:

```sh
PLACES_ASSET_ROOT="$PWD/assets" PLACES_LEVEL=hub \
  ./local/environment-collection-0.12.0/payload-collection-final/places
```

This uses the accepted package installed as `assets/levels/environments.placesmap`.
Only export again after editing the authoritative project:

```sh
./target/release/places-compile export-project projects/environments \
  --out assets/levels/environments.placesmap --asset-root assets --variants off --workers 6
```

The [retirement handoff](../../docs/reports/release-0.12.0.md#environment-collection-0120-retirement)
records superseded standalone IDs, preserved material and deferred work.

`local/environment-collection-0.12.0/` holds the stage's explicit developer
evidence and preserved direct executables. Its final report/handoff records
native acceptance, delivery commands, platform limits and package identity.
Ordinary Cargo tests and CI never bake this collection.


## Lantern Hollow street (introduced in collection 1.0.1)

The outdoor member now occupies 48 by 50.8 metres, about 65% less ground than
before. Four houses stand on each side of the street, with eight separate fenced
plots and open gates. Their interior plans are a side suite, rear galley, T-plan,
central library, split studio, dogleg hall, knitting nook and twin rear rooms.
Sidewalks, streetlights and trees connect the homes to the western woodland path.

Ada, Ben, Mira and Theo, Elias, Nora, Lena and Owen, Ruth and Sam live here.
Each resident uses the man or woman skin and has three individual welcoming
lines. Aim at them and press E for the next line; Enter dismisses the subtitle.
The two paired households have independent conversations for each person.

Follow the western path to the skeleton circle. Jun is northeast of the fire,
near the cool blue lantern and conifer. Leave space for the return walk and sit;
the favor completes when Jun is seated. The signed elevator at the street's eastern
end returns to the Hub, whose ledger remembers the favor across restarts.

Collection release 1.0.5 is independent of the game version. Export records the
current Places target automatically. Use the complete project for travel/save
acceptance. The command above requests Static preparation; Full lighting solves
remain explicit feature or final-stage work.


## Hub elevators (collection 1.0.4)

All eight bays use the current walnut-surround passenger cabins with narrow metal frames, with one direct
destination each and a matching return cabin. Home has a wider entrance alcove
for the hall call; the Office return is in its upper entry foyer, preserving
the existing three-floor ramps and paper route. Outdoor booths have full cabin
depth. The eastern reward-room door, ledger, CLEAR switch and eight favors keep
their existing behavior. Hints name the elevators.

For explicit acceptance, export only this project with `--variants static`
(as in `--variants off` above), then run the executable directly:

```sh
python3 tools/bench/collection_elevators.py --binary target/release/places \
  --package /absolute/path/environments.placesmap --out local/collection-elevators \
  --progress partial
```

This opt-in campaign uses isolated saves, checks all eight round trips, aimed
controls, closed collision, walking in/out, retained loading frames, native
audio and saved-favor/reward-lock continuity. `--progress all --only home`
checks the unlocked reward room; `--only <level>` limits a rerun. It never
bakes maps or starts a Mac app bundle. Static-only outputs are development
validation packages; final release exports retain the normal lighting policy.


## Fixed-cabin rides (collection 1.0.5)

The current export includes the flush metal doorway, repaired cabin floor joins,
enclosed lighting and a bounded in-car motor cue. Cabins and decks remain fixed;
passengers transfer only after the sealed ride. The eight destinations, saved
favors, return bays and reward-room progression retain their existing behavior.
Re-export with the final current compiler and assets before using this version;
older dependency hashes are refused by installation admission. See the
[stationary ride evidence](../../docs/reports/elevator-stationary-ride-0.13.1-mac.md).
