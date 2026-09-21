# Liminal Level Editor

A small browser level editor for `liminal-rust`. It builds the same JSON the game
loads, previews the result in 2D and 3D, and keeps everything a level needs —
rooms, walls, doors, windows, lights, props, the player spawn — one click away.

Open `level-editor/index.html`. There is no build step and no dependency install.
For the prop catalogue (and to avoid `file://` restrictions) serve the app folder:

```sh
cd apps/liminal-rust
python3 -m http.server 8765      # then open http://127.0.0.1:8765/level-editor/
```

## The workflow

**Build → Place → Preview → Play/Test → Save**

| Step | What you do |
| --- | --- |
| Build | `Room` tool: drag a rectangle (optionally with walls). `Wall` tool: drag a wall or divider. |
| Place | `Door` / `Window`: click a wall, or drag along it to size the opening. `Light`, `Prop`, `Spawn`: click. |
| Preview | `2D`, `3D` or `Split` in the top bar. Both views share one level and one selection. |
| Play/Test | `▶ Play / Test` validates the level and saves the file for the game. |
| Save | `Save` writes `<level-id>.json`; `Pack…` writes a `.zip` with imported textures. |

### Tools

| Tool | Key | Notes |
| --- | --- | --- |
| Select | `V` | Click to select, drag to move, drag the white handles to resize, `Shift`+click adds. |
| Room | `R` | Drags a floor + ceiling slab. Tool options can add its four walls. |
| Wall | `W` | Drag horizontally or vertically; thickness and height come from the tool bar. |
| Door | `D` | Click a wall for a standard doorway, or drag along the wall to set its width. |
| Window | `N` | Same as doors, with a sill height. |
| Light | `L` | Ceiling light; brightness and fixture in the tool bar. |
| Prop | `P` | Opens the prop browser: search, filter by category, click, then click in a view. |
| Spawn | `M` | The player start; drag to fine-tune, `Q`/`E` to rotate. |
| Floor patch | `T` | Advanced only: damp carpet / stain patches. |

Everything above the "usual" set (exact coordinates, per-face materials, object
ids, imported texture ids, scale, vertical offsets, clipping) lives behind the
**Advanced** switch in the top bar. Advanced changes what is *shown*; it never
changes the level format.

### Keyboard

`Ctrl/Cmd+Z` undo · `Ctrl/Cmd+Shift+Z` redo · `Ctrl/Cmd+D` duplicate ·
`Ctrl/Cmd+S` save · `Delete` remove · `Esc` cancel · `G` grid snap ·
`Q`/`E` rotate 15° · `F` fit/focus · `1` `2` `3` view modes · `?` help.

### 3D preview

Right-drag looks, `WASD` (+`Q`/`E`) flies, the wheel dollies, middle-drag pans,
left click selects, dragging a selected object moves it on the floor plane,
`F` focuses, `R` resets. Ceilings auto-hide while the camera is above the level
so you can see inside; `X-Ray` makes walls translucent. The 3D view is a preview,
not the game renderer — Play/Test is how you see the real thing.

## Architecture

```
index.html          markup only
css/editor.css      one dark workspace theme
js/geometry.js      pure geometry: wall openings, collision boxes, 3D mesh spec, picking
js/model.js         level data model + validation (mirrors src/level.rs)
js/ops.js           every editing operation, DOM-free (create/move/resize/delete/...)
js/props.js         prop catalogue: parsing, search, fallbacks
js/history.js       undo/redo snapshots (one entry per user action, incl. a drag)
js/renderer.js      2D plan renderer (canvas)
js/camera3d.js      pure camera math (matrices, screen rays)
js/viewport3d.js    WebGL realtime preview (reads app.level, never mutates it)
js/properties.js    contextual inspector
js/io.js            JSON/ZIP import + export, Play/Test saving
js/editor.js        input handling for the 2D view (uses ops.js)
js/app.js           shell: view modes, simple/advanced, tool options, status bar
tests/              node test suites (no browser required)
```

Rules that keep this maintainable:

* `ops.js` is the only place that mutates a level; the UI calls it.
* `geometry.js` is the only place that decides what shape a level is; both views use it.
* The 3D viewport reads the shared level and reports picks/drags back to `app`.
* One selection: `app.editor.selectedIds`, shared by 2D, 3D and the inspector.
* Simple mode hides controls; it never changes data.

## Wall openings (doors and windows)

Doors and windows are **rectangular cuts through a wall**, not hand-split wall
segments. A wall owns its openings:

```json
{ "x": -6, "z": -12.4, "width": 12, "depth": 0.4, "height": 3.5,
  "openings": [
    { "kind": "door", "offset": 4.0, "width": 1.2, "height": 2.1 },
    { "kind": "window", "offset": 8.0, "width": 1.8, "height": 1.2, "sill": 1.0 }
  ] }
```

* `offset` is metres along the wall's length axis from its minimum corner; the
  length axis is X when `width >= depth`, otherwise Z.
* `sill` is the bottom edge height above the wall base; `0` means walk-through.
* `kind` is `door`, `window`, `passage` or `vent`. Unknown kinds are preserved for
  forward compatibility.
* Openings cut the full wall thickness, so a doorway is genuinely open: the game
  splits the wall into solid slices for rendering *and* collision
  (`wall_solid_slices` in `src/level.rs`, mirrored by `wallSolidSlices` in
  `geometry.js`).
* The geometry is non-destructive — moving or resizing the wall carries its
  openings with it, deleting an opening restores the solid wall, and a later
  door/glass mesh can occupy the opening without touching the format.

Old levels that build doorways from separate sill/header walls still load and
still work; the editor keeps editing them as plain walls.

## Props

Props are placed instances of a catalogue entry:

```json
{ "model": "core:stove", "x": 3.0, "y": -0.15, "z": -2.0,
  "rotation_degrees": 90, "scale": 1.0, "solid": true }
```

The catalogue lives in `assets/props/props.json` and is shared with the game
(`PropCatalog` in `src/loader.rs`):

```json
{ "id": "core:stove", "name": "Stove", "category": "Appliances",
  "size": [0.6, 0.9, 0.6], "color": "#8f8a80", "model": null, "solid": true }
```

* `size` is the full box extents in metres `[width, height, depth]`, resting on the
  prop's base.
* `model` is reserved for a future mesh asset; until then the game and the editor
  draw a coloured box, so levels never depend on assets that do not exist yet.
* `solid: true` makes the prop block the player (an axis-aligned box in the game's
  collision list). Decorative props stay walk-through.
* `y` may be negative on purpose: sinking a chair into the floor is allowed, as is
  a prop inside a wall. Validation only rejects malformed data, never artistic
  choices.

**Adding a model later:** add an entry to `assets/props/props.json`, point its
`model` at the asset, and it appears in the editor's prop browser automatically.
No editor or game code changes.

## Tests

```sh
cd level-editor
node --test 'tests/*.test.mjs'
```

* `geometry.test.mjs` — walls with/without openings, sills, wall ends, overlapping
  openings, collision boxes, mesh contents, picking (incl. rotated props).
* `model.test.mjs` — serialization round trips, id stability for undo, validation
  messages, intentional clipping.
* `ops.test.mjs` — create/resize/move, doorway and window placement, selection,
  delete/duplicate, one history entry per drag, advanced property persistence.
* `props.test.mjs` — catalogue parsing, search, fallbacks, and a check that the
  built-in editor catalogue matches `assets/props/props.json`.
* `camera3d.test.mjs` — camera matrices, screen rays, movement, focus.
* `viewport3d.test.mjs` — the 3D viewport against a mock WebGL context: batches
  drawn, mesh rebuild caching, view toggles, picking, 3D drag intents, no level
  mutation, graceful behaviour without WebGL.
* `app-smoke.test.mjs` — boots the real editor in a stubbed DOM and walks the whole
  workflow (build → place → select → move → undo → advanced edit → save → reload →
  validate), including keyboard shortcuts and a check that every element id the
  code looks up exists in `index.html`.

The game's own tests cover the Rust side: `cargo test --offline` in the app root.
