# Changelog

## 0.1.0 — 2026-09-20

### Added

- Add a browser level editor in `level-editor/` with a 2D plan view, an interactive realtime 3D preview, and 2D/3D/split view modes that share one level and one selection.
- Add wall openings: doors, windows, passages and vents are rectangular cuts owned by their wall, so a doorway can be placed by clicking or dragging directly on the wall with no manual wall splitting.
- Add a registry-driven prop system with `assets/props/props.json`, placing furniture, appliance and decorative entries with position, rotation, scale, vertical offset and optional player-blocking collision.
- Add a prop browser with search, category filters and colour previews that lists every catalogue entry and keeps working when a prop or the catalogue file is missing.
- Add a realtime 3D preview (`js/viewport3d.js`) that draws floors, ceilings, walls with their openings, door and window reveals, ceiling lights, props and the player spawn, with X-ray walls, auto-hidden ceilings and a focus/reset camera.
- Add conventional 3D controls: right-drag look, WASD/QE flight, wheel dolly, middle-drag pan, click-to-select, drag-to-move and an on-canvas control hint.
- Add a simple/advanced editing mode where the advanced switch reveals exact coordinates, dimensions, elevations, object identifiers, per-face materials and imported textures without changing the level format.
- Add easy door and window placement that derives the owning wall, wall-relative offset, orientation and opening geometry, and previews the opening in both views before the click.
- Add a Play/Test flow that validates the current level, saves the level file with a suggested name and explains the game's import step instead of duplicating the game's renderer.
- Add the `solid` prop flag to player collision and to the geometry budget so blocking props are honoured by the game loader.
- Add node test suites for geometry, level model, editor operations, prop catalogue, camera math, the 3D viewport and a full editor workflow smoke test.

### Changed

- Extend the level format with an optional `openings` array on walls and a top-level `props` array; both are serde-defaulted, so existing levels keep loading unchanged.
- Build wall geometry in the game from solid slices with jamb and header reveals instead of whole-wall quads, and derive collision boxes from the same decomposition so doorways are genuinely walk-through and window sills still block.
- Reorganise the editor around build → place → preview → play/test → save: one tool rail, tool options for the active tool only, and a contextual inspector that shows the selected object instead of every property at once.
- Replace the separate floor, ceiling and column tools with the room and wall tools, and move uncommon controls such as floor patches and imported textures behind the advanced switch.
- Move the inspector's layer, texture and room lists into collapsible advanced sections and drop the duplicated zoom buttons, compass rose, coordinate labels and always-on height badges.
- Report validation problems in plain language such as "Door opening extends beyond this wall" and keep rejecting only malformed or unsafe data, never intentional overlap, clipping or props sunk into the floor.
- Record undo/redo entries once per completed action, including an entire drag or a click placement, and keep object ids stable across history snapshots.
- Preserve a wall's chosen material by serializing it through the level format's per-face material map instead of dropping it on export.
- Keep openings inside their wall when a wall is resized, and move openings together with their wall.
- Document the editor architecture, controls, wall-opening format and prop catalogue in `level-editor/README.md`.
- Refresh the shipped sample level to demonstrate a doorway, a window and two placed props, including one deliberately sunk below the floor.

### Fixed

- Fix undo and redo restoring the wrong state, which made the first undo a no-op and could drop the action that was being redone.
- Fix placing a light, prop or player spawn by clicking not being recorded in the undo history.
- Fix wall material choices being silently discarded on save because only per-face materials were serialized.
- Fix 3D picking of rotated props using the opposite rotation direction from the drawn box at angles that are not multiples of ninety degrees.
- Fix a 3D drag only applying its first movement step instead of accumulating the whole drag.
- Fix shrinking a wall leaving its openings outside the wall, which produced a validation error instead of clamping them.
- Fix level packs failing to load or export by loading the bundled JSZip script the editor depends on.
