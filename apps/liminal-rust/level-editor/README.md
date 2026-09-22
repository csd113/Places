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
| Light | `L` | Ceiling light; brightness (intensity) and fixture in the tool bar. |
| Prop | `P` | Opens the prop browser: search, filter by category, click, then click in a view. |
| Spawn | `M` | The player start; drag to fine-tune, `Q`/`E` to rotate. |
| Floor patch | `T` | Advanced only: damp carpet / stain patches. |
| Decal | — | Imported decals appear in the plan view (dashed square) and the inspector, and are preserved on save. Creating new decals from the toolbar is not implemented yet. |

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
js/lighting.js      static baked-lighting mirror for the 3D preview (mirrors src/lighting.rs)
js/geometry.js      pure geometry: wall openings, collision boxes, 3D mesh spec, picking
js/model.js         level data model + validation (mirrors src/level.rs)
js/ops.js           every editing operation, DOM-free (create/move/resize/delete/...)
js/props.js         prop catalogue + derived proxy geometry: parsing, search, fallbacks
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

## Ceiling lights

```json
{ "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 5.0, "rotation_degrees": 0,
  "brightness": 1.0, "color": [1.0, 0.85, 0.60] }
```

* `brightness` is the fixture's **intensity** (output). Omitted means `1.0`, the
  standard panel, so every existing level behaves exactly as before. `0.5` is a
  weak fixture, `0.8` a lower-output one, `1.4` a strong one, `2.0` a high-output
  one; values above `8` still load but are clamped by the game.
* `intensity` is accepted as a **spelling alias** when importing (the game's
  loader reads both) and is normalised to `brightness` on export. Generated
  development levels use `intensity`; both produce identical levels.
* `color` is the fixture's **emitted light colour**, an `[r, g, b]` array of
  `0..1` fractions. The game multiplies it into both the baked environmental
  illumination and the visible panel, so a blue fixture lights nearby geometry
  blue instead of only tinting its own panel. Omitted means the restrained warm
  fluorescent (`[1.0, 0.96, 0.88]`), so legacy levels are unchanged. The
  inspector offers a `r, g, b` / `#rrggbb` field; leave it empty for the default.
  Because ordinary illumination still multiplies the surface's own colour, a
  strongly tinted fixture reads most clearly on light surfaces — coloured light
  on the yellow Backrooms wallpaper still mixes towards olive.
* The game bakes the value into the room's baseline illumination and into the
  pool of light under the panel at level load (`src/lighting.rs`). The editor's
  inspector shows and edits it, and validates it exactly like the game loader:
  non-finite or negative intensities and non-finite or out-of-range colour
  channels are errors, intensities above the game's clamp are warnings, and a
  malformed colour array is normalised back to "no colour" (the warm default)
  on import rather than being written out with invalid channels.

### Lighting in the 3D preview

The preview applies an *approximation* of the game's static lighting to its mesh,
using the same tuned constants (`js/lighting.js` mirrors `src/lighting.rs`):
room baselines from floor area, fixture count/intensity, emitted colour and
ceiling height; broad local pools under fixtures; and bounded blending through
doorways. Colour is accumulated per channel, exactly like the game, so the
preview shows the same warm/blue/red mixes.

It is a preview, not a second renderer. The editor's floors and ceilings are
single quads, so a room reads at its baseline brightness and only walls and props
carry per-vertex pool variation; there are no baked light maps. Use `▶ Play/Test`
to see the real thing.

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
  "size": [0.6, 0.9, 0.6], "color": "#8f8a80", "model": "models/stove.glb", "solid": true }
```

* `size` is the full box extents in metres `[width, height, depth]`, resting on the
  prop's base. It is also the pick/drag box, whatever the 3D preview draws.
* `model` points at the shipped GLB asset (`models/*.glb`, relative to
  `assets/props/`). Until an asset exists the game and the editor draw a
  coloured box, so levels never depend on assets that do not exist yet.
* `solid: true` makes the prop block the player (an axis-aligned box in the game's
  collision list). Decorative props stay walk-through.
* `y` may be negative on purpose: sinking a chair into the floor is allowed, as is
  a prop inside a wall. Validation only rejects malformed data, never artistic
  choices.

### Proxy geometry (what the 3D preview draws)

`assets/props/prop_proxies.json` is written by `python3 tools/props/build.py` and
derived from the shipped GLB meshes — never edit it by hand. The editor loads it
next to the catalogue (same candidate URLs and `no-store` fetch options) and draws
each prop from its real parts: boxes, low-segment cylinders,
tubes and single-sided planes, in metres in prop-local space (origin at the floor
contact, +Z front). Per-part colours are the asset's base colours, shaded with
the game's face multipliers so the preview reflects the real asset instead of a
hand-maintained duplicate.

Everything degrades cleanly: if the file is absent (older checkout, `file://`),
an entry is malformed, or the prop is a custom/unknown id with no proxy, the
editor keeps drawing the catalogue box. Editing, picking and dragging are
unchanged — proxies only change what the 3D viewport previews.

Each proxy also carries the asset's generated metadata (`triangles`, `texture`,
`bounds_min`/`bounds_max`). The prop browser shows the triangle count in a
card's tooltip, and `tests/prop-assets.test.mjs` asserts the pack's budgets and
scale conventions through the editor's own parser and geometry builder.

### Thumbnails

`tools/props/build.py --thumbs` renders `level-editor/assets/thumbs/<short-name>.png`,
where `<short-name>` is the id after `core:` (`core:washing_machine` →
`washing_machine.png`). The prop browser layers the image over the existing
colour swatch; a missing file (or a missing `assets/thumbs/` folder) simply
reveals the swatch, so the browser always renders.

**Adding a model later:** add an entry to `assets/props/props.json`, point its
`model` at the asset, run `tools/props/build.py` (which refreshes the proxies and,
with `--thumbs`, the thumbnails), and it appears in the editor's prop browser
automatically. No editor or game code changes.

## Tests

```sh
cd level-editor
node --test 'tests/*.test.mjs'
```

* `geometry.test.mjs` — walls with/without openings, sills, wall ends, overlapping
  openings, collision boxes, mesh contents, picking (incl. rotated props), and the
  proxy mesh: every part shape, per-part colours/shading, position/yaw/scale and
  the catalogue-box fallback.
* `model.test.mjs` — serialization round trips, id stability for undo, validation
  messages, intentional clipping.
* `ops.test.mjs` — create/resize/move, doorway and window placement, selection,
  delete/duplicate, one history entry per drag, advanced property persistence.
* `props.test.mjs` — catalogue parsing, search, fallbacks, a check that the
  built-in editor catalogue matches `assets/props/props.json`, and proxy
  parsing/fallbacks (valid file, missing file, malformed entries, unknown ids).
* `lighting.test.mjs` — the preview's static lighting: density/area/intensity/
  ceiling-height behaviour, saturation, sanitising, fixture pools, doorway
  blending, and the baked vertex colours the 3D preview draws.
* `camera3d.test.mjs` — camera matrices, screen rays, movement, focus.
* `viewport3d.test.mjs` — the 3D viewport against a mock WebGL context: batches
  drawn, mesh rebuild caching, view toggles, picking, 3D drag intents, proxy
  geometry replacing the fallback boxes, no level mutation, graceful behaviour
  without WebGL.
* `app-smoke.test.mjs` — boots the real editor in a stubbed DOM and walks the whole
  workflow (build → place → select → move → undo → advanced edit → save → reload →
  validate), including keyboard shortcuts, prop-browser thumbnails with the swatch
  fallback, proxy loading marking the mesh dirty, and a check that every element id
  the code looks up exists in `index.html`.

The game's own tests cover the Rust side: `cargo test --offline` in the app root.
