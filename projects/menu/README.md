# Places menu scene

The application-owned, non-playable scene has four depth layers: Office, Pool,
Home and Arcade. The Home closet belongs to its room. The camera is fixed at
`(0,1.6,4.7)`, yaw 0, pitch −5°, with a 55° vertical lens.

Seven complete events are shuffled as units, without repetition until the bag
is exhausted. A fresh entry receives a new seed; the previous last event is
avoided at the boundary. Settings do not reshuffle an active event. Internal
animation steps are not additional events, and there is no fixed ten-minute
padding. The cap remains 15 events.

1. Office parcel relay: the porter enters left, places the original parcel and
   leaves. A different collector enters right, retrieves it and leaves right.
2. Pool duck: an attendant releases the same carried duck into the water, walks
   along the dry deck, retrieves it at the opposite bank and exits the other door.
3. Home ghost chase: the folding closet opens, the visitor raises both arms in
   fear, turns and runs, pursued by a ghost. Cleanup follows both leaving view.
4. Arcade game: the player uses the cabinet, its actual practical light responds,
   and the player celebrates before leaving.
5. Blackout and roaches: longer irregular flickering, about one second of full
   darkness, then Arcade → Home → Pool → Office restoration. Eight existing
   cockroaches are staged during darkness before any restoration; they rapidly
   scatter along the floor below the camera and despawn only after leaving view.
6. Five Arcade balloons: varied heights, spacing, bob and entry times; the whole
   envelope clears the doors. All five despawn only after leaving the camera
   into the concealed opposite wing, so later entrances do not inherit traffic.
7. Vanishing bodies: the seven side doors open outward, one NPC enters from each,
   and all become true physics ragdolls. Irregular flickering leads to darkness;
   the runtime bodies are removed there, and flickering lights reveal an empty room.

The crowd is an explicit exception to sparse staging. Other events use only their
necessary cast. Waiting actors and reset items stay behind opaque wings. No
insect door choreography or visible lifetime timeout is used. Spawn groups and
runtime names bound the temporary cast; removal releases AI, render and ragdoll
ownership. The ordinary whole-bag reset occurs only after the scene is quiet.

Carry requests use the shared posed-contact path and keep the original item's
resource, scale and physical identity. Contact retains its initial world pose,
then continuously seats it in the authored palm transform during the lift. An
initial wrist-to-item gap is not retained permanently. The duck keeps its existing
water/buoyancy body through release, travel and recollection. Lua physical item
queries follow solver positions rather than stale authored placements.

The scene requires Full lighting for independent circuits. That presentation
belongs to the application menu; the player's saved gameplay preference remains
unchanged. Lighting changes use actual supported circuits. The main page retains
the committed font-atlas title and functional navigation, with no decorative
under-title or top-right text. Its existing shading uses continuous vertex alpha
instead of separate stepped strips.

Developer controls `PLACES_MENU_SEED` and `PLACES_MENU_FIRST=1..7` make runs
repeatable. Native recordings establish appearance and behavior; capture-free
VSync-off runs establish performance separately, with actual presentation mode
and drawable size recorded. Source implementation is not native acceptance.

Bake explicitly with the release compiler and native GPU support:

```sh
places-compile build projects/menu --out local/menu.placesmap --asset-root assets --variants static,full --workers 6
places-compile validate local/menu.placesmap --asset-root assets
places-compile verify projects/menu --package local/menu.placesmap --asset-root assets --require-current
```

Only after verification, copy the package to `assets/shared/menu/scene.bin`.
Cargo and ordinary tests do not bake this scene. It is a tracked application
resource and works without gameplay maps or a source checkout. It never appears
as an Explore entry.


## Runtime storage migration (2026-10-24)

The actual `assets/shared/menu/scene.bin` was migrated losslessly from the preserved
17,025,419-byte runtime package to 15,558,745 bytes using supported compact
`places-compile optimize-storage`, without rebaking. Binary ZIP transport uses
Zstd method 93, lightmaps use PLC1 revision 2, and cost-selected prop recipes remain
PPS1 revision 2. Full/Static decoded records, scripts, dependencies and original
build-input provenance are exact; native matched appearance and representative
events pass. See the single `docs/reports/release-0.13.0.md` report and its evidence index.

Runtime SHA-256 is `bbb7cb766b5962d3c2f09b9cade186012500e4fe8213ca2ee0d71b665e3c660d`.
Original source/preparation provenance is retained. Both original and migrated
packages report existing source/preparation staleness against the current project;
storage migration does not make `verify --require-current` pass or claim a fresh bake.


## Windows release preparation (2026-10-27)

The release compiler freshly prepared normal Static and Full lighting/probes
after the compiler fingerprint changed. Native integrity and
`verify --require-current` pass before publication to the application-owned
`assets/shared/menu/scene.bin`. This replaces the previously stale prepared
resource; no old lighting was reused. The earlier storage-migration account
above remains historical evidence.

Current runtime bytes: **16,420,340**. SHA-256:
`dc2e594527dde7789cc5d35bcf105abd3db0b92eb56b77af96f1e7c9d3b4da47`.
See the final Windows section of `docs/reports/release-0.13.0.md` for the compiler,
receipts, native acceptance, download inventory and preserved old resource.
