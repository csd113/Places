# Places menu miniature

This application-owned, non-playable scene follows the supplied four-room PNG
concept: Office foreground, Pool second, Home third, Arcade fourth. The Home
closet alcove adds no depth layer. The camera is fixed at `(0,1.6,3.5)`, yaw 0,
55° vertical lens. Keep this composition and the central sightline intact.

The supplied UI reference is Library `libfile_3d8f1646686881919749d01eec4941bb`;
the scene reference is `libfile_b71c7ad102d48191b2b167b8e0bf02f8`. Both original
images were inspected. The references and generated logo extraction attempts
remain local evidence; no generated cutout that failed the PNG/alpha contract is
shipped. The cream wordmark uses the committed font atlas with compact display
proportions. The main page has Explore, Settings, Credits and Exit; cards live
only in Explore.

`menu.luau` contains 120 chronological five-second cues across twelve miniature
vignettes, including coordinated crossings, greetings, parcel/book/towel/cushion/
gift pickup and physical putdown, the real folding closet, practical lighting,
and three reusable buoyant ducks moving with the water current. Contact actions
run at 0.9 seconds using one pending actor/action pair. The script retains no
host callbacks and spawns no objects. Six actors, eight movable items and one
module are reused; at 600 seconds the ordinary reset lifecycle restores source
poses, physics groups, audio, lamps, navigation and the VM.

Bake explicitly with the release compiler and native Metal/Vulkan/D3D support:

```sh
places-compile build projects/menu --out local/menu.placesmap --asset-root assets --variants static,full --workers 6
places-compile validate local/menu.placesmap --asset-root assets
places-compile verify projects/menu --package local/menu.placesmap --asset-root assets --require-current
```

Only after verification, copy the package bytes to
`assets/shared/menu/scene.bin`. This prepared application resource is intentionally
tracked; no gameplay package or source checkout is needed at runtime. Packaging
already includes `assets/shared/`. Never add this scene to Explore or load a
collection to render its background.
