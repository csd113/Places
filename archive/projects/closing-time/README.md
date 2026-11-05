# Closing Time

A short single-map closing shift in the deserted arcade and bowling venue.
Uses only assets already integrated in Places 0.12.0-staging.

Open this folder with the direct `places-editor` executable. Use Project → Export
complete project to produce a `.placesmap`; Project → Recover editable project
restores its exact declared JSON, metadata and Luau to a new folder. The supplied
Full-lighting package is ready to import into the matching staging player.

Use WASD, mouse look, E to interact/pick up/set down, F for the flashlight, Space
to jump and C to crouch. Return the loose gift to the gifts table, the cat plush
to the lobby box and the burgundy ball to the rack. Shut down the three circuits
and interact with the front entrance to lock up. The completion label confirms
the end; interact again to restart. The exit sign restarts during a shift.
The noticeboard has an optional concise checklist. There are no throw tutorials.

Use Full lighting for the intended circuit shutdown atmosphere. Static currently
holds fixed prop circuits at their authored state; the authoring report records
that concrete limitation for the later combined implementation pass.

The project source is portable within this staging checkout; runtime models,
textures and sounds remain installed assets rather than duplicated custom assets.
Do not bake this map as a Cargo test or automatic CI fixture.

Authoring, manual source-edit reasons and validation:
`docs/reports/release-0.12.0.md#editor-0120-closing-time`.
