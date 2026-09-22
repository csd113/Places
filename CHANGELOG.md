# Changelog

## 0.3.2 — 2026-09-21

Conventional desktop default controls: WASD movement with arrow-key looking.
Bindings stay rebindable, and existing custom layouts still load from
`settings.json`.

### Changed

- Change the default keyboard bindings to WASD movement (`W` forward, `S`
  backward, `A` strafe left, `D` strafe right) with the arrow keys looking
  (`UP`/`DOWN` pitch, `LEFT`/`RIGHT` yaw). The previous PocketCHIP-oriented
  layout (`Z` backward, `S` strafe right, `K`/`L`/`O`/`.` look) remains
  reachable by rebinding each action in Settings.
- Update menu navigation to match: `W`/`S` or `UP`/`DOWN` move through items,
  `A`/`D` or `LEFT`/`RIGHT` adjust them, and `Z` is no longer a menu key.
  On-screen help and the README document the new layout.
- `Restore Default Bindings` now restores WASD + arrow keys.

### Notes

- Existing `settings.json` files load unchanged, so player-rebound keys survive
  the update; only the defaults and the reset target changed.

## 0.3.1 — 2026-09-21

### Changed

- Move the package to the standalone Places repository while retaining the
  `io.vitrallis.liminalrust` application ID and catalog package path for
  Vitrallis App Center compatibility.

## 0.3.0 — 2026-09-21

Stable surface rendering: water-damage overlays authored as duplicate walls are
resolved into material runs on one physical surface, a first-class decal system
draws local surface markings through a dedicated depth-biased pass, and the
rendering diagnostic level exercises both. Coloured lighting is unchanged.

### Added

- Add a level `decals` array: rectangular surface markings (signs, floor
  arrows, hazard stripes) placed with `x`/`y`/`z`, `width`/`height`,
  `rotation_degrees`, a sheet `material` and a `surface` (`floor`, `ceiling`,
  `wall_north`, `wall_south`, `wall_east`, `wall_west`). Decals are lit by the
  room's baked illumination and shaded like the surface they lie on, so they are
  not full-bright stickers in a dark room.
- Add a dedicated decal render pass: decals are batched with the rest of the
  static level geometry, drawn after the opaque world and props with a constant
  `glPolygonOffset(0.0, -2.0)` bias, and cut out with an alpha-discard fragment
  program. Depth testing and depth writes stay enabled, so a decal behind a wall
  stays hidden. The shared generated decal sheet carries four markings (`decal
  test`, a NO DIVING-style sign placeholder, a floor arrow and hazard stripes).
- Add the `rendering_diagnostic` level: a warm-lit room, a blue-lit room and an
  unlit room with wall and floor decals, a partially occluded floor decal, a
  stained wall section, a damp floor patch, a floor-standing crate and rug, a
  fixture near the ceiling and a wall T-junction.
- Add Rust, package and editor tests for decal parsing and validation, decal
  quad placement and rotation, decal lighting, the depth-bias contract, decal
  draw order, and coincident-overlay wall resolution.
- Add level editor support for decals: model class and round trip, plan-view
  markers, selection and dragging, an inspector for sheet/surface/size/rotation,
  and validation matching the game loader, so imported decals survive an
  edit-and-save cycle.

### Changed

- Resolve coincident collinear walls into a single emitted surface: the
  residential levels paint part of a wall with water damage by placing a second
  wall in exactly the same plane, which made two identical surfaces compete for
  the same depth value. The renderer now merges such walls, unions their solid
  profiles (an opaque coincident face covers a hole in the other surface, which
  is what was already displayed) and emits one set of faces with a material run
  per span. Collision keeps using the authored walls.
- Keep wall face shading and the ceiling tint as shared constants so decals use
  the same values as the surfaces they are printed on.

## 0.2.0 — 2026-09-21

Coloured static lighting: every ceiling fixture can emit an arbitrary RGB
colour that lights the room around it, unlit rooms are genuinely dark, and
Level 1 keeps its brightness from its own fixtures instead of a global ambient
floor.

### Added

- Add an optional `color` field (`[r, g, b]`, 0..1 per channel) to ceiling
  light definitions. The baked environmental illumination and the fixture panel
  both use it, so a blue fixture lights nearby floor, ceiling and wall geometry
  blue instead of only tinting its own panel; an omitted colour emits the
  restrained warm fluorescent default (`[1.0, 0.96, 0.88]`), so every legacy
  level loads unchanged.
- Add the `lighting_diagnostic` level: nine connected rooms demonstrating an
  unlit dark room, one warm light, several warm lights, a blue light, a red
  light, a warm/cool overlap, a red/green/blue overlap, a regular nine-fixture
  grid and an eight-metre room.
- Add RGB lighting tests in Rust, in the editor mirror and in the shared
  Rust/JavaScript parity vectors: single-colour channel dominance, mixed-colour
  accumulation, bounded dense grids, ceiling-height response and legacy default
  colours.
- Add an emitted-colour field to the level editor's light inspector (`r, g, b`
  or `#rrggbb`, empty for the default) with validation matching the game loader.

### Changed

- Replace the scalar lighting bake with a three-channel RGB bake: fixtures
  accumulate per channel, opening blending and the ceiling-height correction
  still apply, and the room-baseline density curve is logarithmically
  compressed so a sparse 13.5 m fixture grid (Level 1) is broadly lit without
  also saturating small, densely lit rooms.
- Lower the unlit-room ambient floor from `0.55` to `0.10` per channel and
  remove every later clamp that restored the old floor, so rooms without
  fixtures are genuinely dark while geometry stays barely visible. Level 1's
  large sparse rooms measure within about 3% of their previous bake because
  their brightness now comes from the nine fixtures per room; the denser
  shipped residential levels read roughly 0.1 brighter in lit rooms, and their
  unlit rooms drop to the new ambient floor.
- Drive the fixture panel's visible colour and its emitted environmental colour
  from the same authored value so the two cannot silently diverge, and mirror
  the RGB model in the level editor's 3D preview.

## 0.1.0 — 2026-09-21

First App Manager-ready release: a native ARM payload published through the
Vitrallis catalog, three large hand-authored residential levels, the material
and lighting support those levels need, and the packaging metadata the runtime
expects.

### Added

- Add three large residential levels, each authored from rectangular rooms,
  hallways and the existing prop library: `the_residence` (a sprawling house of
  44 rooms across four wings), `quiet_apartments` (48 rooms of apartments that
  open into one another) and `after_the_leak` (45 rooms around a service core
  that has been leaking for years).
- Add progressive environmental decay to those levels with walking distance
  from the spawn: water staining spreads from ceilings to walls to the carpet
  below, fixtures thin out and dim, furniture drifts out of alignment, and the
  final regions are dark but still readable. Damage is placed deliberately (the
  same leak marks the ceiling, the wall under it and the floor patch below),
  never procedurally.
- Add per-room and per-wall material overrides (`room.material`,
  `room.ceiling_material`, `wall.material`, `wall.faces`) and render the
  documented `floor_patches` regions, using the level editor's existing keys and
  the core material ids. A level that names none of them renders exactly as
  before, from `defaults.wall`/`floor`/`ceiling`.
- Add `app.toml`, `icon.png`, `README.md`, an app-local `tests/` suite and the
  version 0.1.0 changelog entry that the catalog manifest expects.

### Changed

- Include the project license and third-party license texts in the installed package, and normalize Rust source formatting for release validation.

- Resolve the package root from the installed executable
  (`bin/<target-triple>/app`), so levels, props, imported level packs and
  `settings.json` are found when App Center launches the app from its own
  directory instead of the build tree.
- Set the X11 window class and SDL app name to `io.vitrallis.liminalrust` /
  `Liminal` before creating the window, as required for native apps.
- Split the static mesh by surface material, so a stained wall, damp carpet or
  stained ceiling binds its own small texture sheet; the maintained sheets stay
  the level defaults.
- Report static batch counts per surface family in the developer log and check
  the three new levels' geometry, prop and lighting budgets in the lighting
  audit.

### Fixed

- Merge wall runs only while they share a wall material, so one damaged section
  stays its own wall instead of staining a whole facade.
- Keep the room floor's tessellation exact around a floor patch, so a damp
  carpet region has crisp edges without a second overlapping floor slab.

## Development history (pre-release)

The releases below predate the first published App Manager release. They were
development iterations of the renderer, the level format, the editor and the
input handling, and they are kept for reference.

### 0.8.0 — 2026-09-21

Renderer performance pass for the PocketCHIP, driven by measurements on the
physical device. The level format, the shaders' visual result, the assets and
the gameplay are unchanged; this release changes how static geometry is
partitioned, submitted and laid out for the GPU.

### Added

- Add a debug-only frame-telemetry and hardware-benchmark harness (`src/bench.rs`,
  `LIMINAL_BENCH=1`). It times the loop in stages around `SDL_GL_SwapWindow`
  (`update_ms`, `render_ms`, `swap_ms`, `frame_ms`, `loop_ms`), writes one CSV
  row per frame, prints a single `BENCH_SUMMARY` JSON line per run, and is
  completely inert — no file handle, no allocation, no output — unless
  `LIMINAL_BENCH` is set. `LIMINAL_BENCH_OUT`, `LIMINAL_BENCH_WARMUP`,
  `LIMINAL_BENCH_FRAMES` and `LIMINAL_CAMERA` bound and repeat a run.
- Add benchmark-only switches that each change exactly one submission decision,
  so a single release build measures each optimisation's contribution on real
  hardware with batching, draw order and shaders held fixed:
  `LIMINAL_BENCH_NOCULL`, `LIMINAL_BENCH_NOINDEX`, `LIMINAL_BENCH_EXACT_VERTEX`,
  plus `LIMINAL_BENCH_FINISH`, `LIMINAL_BENCH_NORENDER`, `LIMINAL_BENCH_NOSWAP`
  and `LIMINAL_VSYNC` for separating renderer cost from presentation cost.
- Add coarse spatial partitioning and view-frustum culling for static geometry
  (`src/spatial.rs`): an adaptive per-axis X/Z cell grid, a world-space AABB per
  render range, and a conservative box/plane test extracted from the same
  view-projection matrix the GPU clips against, so it cannot disagree with the
  screen at any pitch, aspect ratio or drawable size. Geometry outside the
  frustum is no longer submitted at all.
- Add indexed static and prop geometry. Static quads are reduced from six
  submitted vertices to four distinct corners plus six 16-bit indices, and share
  edges with neighbours where every attribute is bit-identical; prop instances
  keep their model's own index list instead of being expanded into a flat
  triangle list. `glDrawElements` with `GL_UNSIGNED_SHORT` is core OpenGL ES 2.0,
  so no extension or newer context is required.
- Add a 24-byte packed GPU vertex layout (`PackedVertex`): world position and
  texture coordinates stay `f32`, the baked shade becomes normalised `RGBA8`
  expanded by the fixed-function pipeline, reducing the static vertex format from
  36 to 24 bytes. Both the scene and the HUD use it, so there is still one shader.
- Add per-stage level-build timings to the developer log
  (`[level] ... built in X ms (lighting A + props B + surfaces C)`), and a
  `[spatial]` line reporting the grid resolution, the static batch count per
  material and the prop batch count.
- Add `tools/bench/` — a PocketCHIP benchmark suite that cross-compiles, stages
  the payload into `/tmp/liminal-benchmark`, runs a whole scene list in one SSH
  session, downloads the per-frame CSVs and prints a comparison table
  (`run_bench.py`, `gen_levels.py`, `analyze.py`, `runone.sh`), plus a
  pixel-comparison harness for renderer changes (`visual_check.py`) and a design
  note on level-build caching (`notes/level-build-cache.md`).

### Changed

- Partition every static surface and every prop instance by spatial cell before
  upload, so each material is drawn as a handful of cullable ranges instead of
  one range covering the whole level. Ranges stay grouped by material, so the
  draw loop still binds each texture once. The grid resolution adapts to the
  level's extent (`12`–`40` m, eight cells across the longer axis) so the batch
  count stays bounded for any level a creator ships.
- Report what each frame actually submitted, straight from the draw path:
  total/visible/culled vertices, total/visible batches, draw calls, VBO bytes and
  index bytes.
- Keep prop batches whole rather than splitting a single prop across a cell
  boundary, so a prop is never drawn as two ranges.
- Reduce the per-prop level-build cost by roughly 30 %: instancing now
  transforms and lit-shades each distinct model vertex once per placement instead
  of once per flat triangle-list entry.

### Fixed

- Fix the VSync setting being silently ignored. `SDL_GL_SetSwapInterval` was
  called before the GL context existed (which always fails), its result was
  discarded, and `Renderer::new` then requested VSync again unconditionally
  after making the context current — so the setting could only ever be on, and a
  failure looked identical to success. The request now happens once, after the
  context is current, honours the user's setting, is reported together with the
  platform's answer from `SDL_GL_GetSwapInterval`, and an error is printed rather
  than swallowed.
- Fix `Aabb`'s neutral value: a derived `Default` produced a degenerate box at
  the world origin instead of the empty box, which would have made every bounds
  computation include the origin and quietly weakened culling.

### Notes

- Vertex data for the same scene falls from 36 to 24 bytes per vertex (−33 %),
  and indexing removes about 29 % of static and prop vertices, so the combined
  static/prop vertex buffer for the 400-chair stress scene drops from 7.46 MB to
  3.54 MB (−52 %) with an unchanged image.
- The 24-byte layout is pixel-identical to the exact 36-byte one on Level 1, the
  Asset Demo, the prop showcase, the prop stress test and the chair stress
  levels: the smallest unit of change in the final image is below one 8-bit code
  value, because the bake never leaves `[MIN_AMBIENT, MAX_BRIGHTNESS]`.
- Prop batching is intentionally no longer one draw per model; it is one draw per
  (model, spatial cell). A single-model prop field therefore costs a few more
  draw calls in exchange for being able to reject most of it when the camera
  turns away, which is the trade the measurements were taken to evaluate.

### 0.7.0 — 2026-09-20

### Added

- Add `core:ceiling_stained_01`, a water-damaged ceiling material: three of its
  four 1 m panels stay recognisable ceiling tile and one carries the leak — an
  irregular soaked field running to the grid, a leak trail onto its neighbour
  and a browner cast where the water sat. It is a level default like the other
  materials (`"ceiling": "core:ceiling_stained_01"`) and is offered by the
  level editor's ceiling material dropdown next to the maintained panel.
- Add `levels/asset_maintained.json`, the Asset Demo building on the maintained
  material set. A level carries exactly one wall, one floor and one ceiling
  material, so the maintained and water-damaged sets are compared by walking
  the same rooms in the two demo levels.
- Add texture regression tests: every built-in surface sheet must tile (its
  wrapped edges must meet) and every damaged material id must resolve to its
  own sheet rather than the maintained one.

### Changed

- Polish every generated prop except `spooner-man`. The couch and armchair are
  rebuilt around a proper sofa structure (block feet, seat frame and apron,
  panel arms capped by a 6-segment padded roll, a full-width back under a crest
  rail, three seat and three back cushions with soft top puffs, and two muted
  olive throw cushions on their own fabric swatch); the chair's back is widened
  to match its seat and raked 4 degrees with two slats and a top rail; the bed
  gains a raised headboard, a draped blanket with side drops and two pillows;
  the television becomes a thin bezel panel on a pedestal stand with a recessed
  screen; the water cooler's bottle gets a real bottle silhouette (neck,
  shoulder, straight body); the sink gets a counter upstand, a taller faucet and
  a lighter painted basin; the table's legs are tapered square sections and the
  desk's pedestal and knee-hole shelf stand on plinths. Hidden faces are dropped
  while polishing, so the twenty core props together fall from 2912 to 2836
  triangles.
- Rebuild the built-in surface textures. Wallpaper is now a printed 25 cm
  stripe with a groove, a paper grain and a faint age mottle over a **two
  metre** repeat; the carpet is a short pile (per-texel speckle, short
  directional dashes and 5–12 cm mottle) instead of a 4 px loop grid; the
  ceiling is a 2 x 2 m patch of four 1 m mineral-fibre tiles in a T-bar grid
  whose panels differ slightly in tone and scuffing. Wall and ceiling UVs run at
  half speed to match, so the texel density is unchanged (64 texels per metre)
  while the repeats are half as visible.
- Make the worn material variants carry the same design as the maintained ones.
  The stained wallpaper is dominated by vertical runs that continue down the
  wall, with damp fields and a rusty cast in the wet areas; the damp carpet
  keeps its pile and is darker, flatter and greyer over large bounded regions
  instead of being a uniformly darkened sheet.
- Soften the metre checker baked into the derived floor texture (11 % to 6 %
  between adjacent cells) so the floor reads as uneven carpet wear rather than
  as a tiled floor.
- Update the editor's 3D preview to mirror the new wall and ceiling sheets
  (128x128, two metres per repeat) and to offer the damaged ceiling material.

### Notes

- `levels/asset_demo.json` now also uses the stained ceiling, so all three worn
  materials are visible in one walkable map; Level 1 keeps the maintained set.
- Surface texture memory grows from 16 KiB each to 64 KiB for the wall and
  ceiling sheets (about 96 KiB more for a level), and no prop's texture grew.

### 0.6.0 — 2026-09-20

### Added

- Add a full adversarial audit of the static lighting system under `src/lighting_audit.rs` and `src/lighting_audit_cases.rs`: room area and density, zero-light rooms, dense fixture grids (10/50/100/250 fixtures), intensity boundaries, ceiling-height extremes, fixture ownership at room boundaries and overlaps, opening blending variants, one-hop propagation, local pools and saturation, material modulation, prop lighting at extreme vertical offsets, fixtures outside every room, degenerate data, colour safety and bit-for-bit determinism. A fixed-seed stress test builds 48 pseudo-random valid levels and checks every baked value stays finite and in range.
- Add Rust/editor cross-implementation parity coverage: `src/lighting_parity.rs` generates and locks `level-editor/tests/support/lighting_vectors.json` (eight representative scenarios), the Rust suite replays it exactly, and `level-editor/tests/lighting-parity.test.mjs` replays the same file inside the preview's tolerance so the two models cannot drift silently.
- Add a release-build benchmark report (`cargo test --release lighting_benchmark_report -- --nocapture`) over a tiny level, the Asset Demo, Level 1, a 100-fixture room, very large surfaces, 36 rooms, a prop-heavy level and a worst-reasonable community level, with vertex counts, draw calls, bake/build times and budget-estimate cross-checks.
- Add a lighting demonstration wing to the Asset Demo: a long spine corridor with two widely spaced fixtures (bright pool → darker gap → bright pool), four identical rooms with 0/1/2/4 fixtures (density), two identical rooms with 0.5/1.8 fixtures (intensity), two identical rooms at 2.6 m/4.2 m ceilings (height), a bright room joined to a dark one by a wide doorway (opening bleed), and three props plus a `spooner-man` placed where the environmental lighting is easy to see. The wing is walkable: a test drives a 0.3 m player path from the spawn to every comparison room and the dark/bright doorway without intersecting collision boxes.
- Add regression tests for the audited fixes: raised walls and malformed openings must not blend, fractional fixture rotations must agree between the baked pool and the drawn panel, wall reveals must carry both rooms' light, merged lighting cells must keep their exact sampled colours and tile their room, wall strips must share exact edges, and the geometry estimate must bound what the builder emits.

### Changed

- Make per-room baked lighting cheaper without changing a single baked value: every room keeps its own fixture candidate list, the light loop rejects candidates by squared distance before any square root, and the floor/ceiling corner grids and wall strips are sampled once per corner instead of once per quad.
- Merge flat baked-lighting cells and wall segments into larger quads while every corner stays within 1/512 of a colour step, so unlit and far-from-fixture surfaces collapse instead of being densely tessellated. On Level 1 this cuts the static geometry from 61,266 to 42,540 vertices and the release build from ~8.6 ms to ~0.8 ms with unchanged draw calls; the Asset Demo stays ~2.1k static vertices plus its prop batches.
- Stop building the initial level twice at startup: `Renderer::new` now only sets up the context and buffers, and the first level is uploaded once by `Renderer::set_level`.
- Correct the level geometry estimate to replay the same solid-slice decomposition the builder uses, so a wall with many openings can no longer under-count its own vertices; the wall estimate was also tightened so representative levels reserve close to what they use.

### Fixed

- Doorway blending no longer treats an opening in a raised wall (or a malformed zero-size/non-finite opening) as a walk-through passage, so light cannot leak through a hole above head height that has no floor connection.
- Doorway and window reveals are now lit from both faces of the wall instead of sampling the middle of the wall cavity, so jambs blend the two rooms they join rather than dropping to ambient.
- The fixture rotation rule is now one shared helper (`fixture_is_turned`) used by both the baked pools and the drawn panels, and the editor's preview uses the same rule; previously a fractional rotation such as 179.6° could make the two disagree.
- The editor's preview and its lighting mirror now agree on turned fixtures (previously a 135° fixture drew in one orientation and pooled in the other).

### 0.5.0 — 2026-09-20

### Added

- Add a static baked lighting system for the level's interiors (`src/lighting.rs`). It runs once per level load and folds a single brightness value per vertex into the geometry the renderer already draws, so there is no dynamic light, no light map, no extra draw call and no shader change on the PocketCHIP target.
- Bake a **room baseline** from floor area, the ceiling fixtures the room owns (count × fixture intensity) and the ceiling height: a 12 m² room with two panels is bright, a 100 m² room with two panels is clearly dim, and the same fixtures count for more under a 2.6 m corridor ceiling than under a 5 m one. Brightness uses a smooth saturating curve, so rooms are never classified into hard tiers and never exceed the valid colour range.
- Bake **local fixture pools**: each panel adds a broad, smooth pool of light measured to its 1.2 × 0.6 m rectangular footprint, so a floor, wall or prop directly under a fixture is brighter than one far from every fixture.
- Bake **doorway blending**: rooms joined by walk-through openings mix a bounded fraction of each other's baseline near the opening (6 m radius, fading above the door header), so a doorway no longer shows a hard brightness step between rooms. Adjacent rooms are never propagated recursively.
- Keep a **minimum ambient** illumination so an unlit room stays visibly dim but navigable rather than pitch black.
- Tessellate floors, ceilings and wall faces on a bounded lighting grid (2.5 m cells, capped per surface) so the baked pools vary across large surfaces without geometry scaling with room area.
- Receive environmental lighting on **props**: every instance's transformed vertices are sampled in world space and multiplied into their existing vertex colours, so a plant standing on a crate or `spooner-man` on the bed is lit at its true height. Repeated instances still collapse into one batch and one draw call per model.
- Add the optional ceiling-light **intensity** property. `brightness` is the canonical field the editor already wrote; `intensity` is accepted as an alias when loading, and both default to `1.0` when omitted. Fixtures now hang just below their own room's ceiling and glow slightly more or less with their output.
- Add deterministic lighting tests (`src/lighting.rs`) covering density, area, intensity, height, saturation, minimum ambient, numerical safety and malformed input, plus renderer tests proving floors, walls and props are baked, vertically offset props sample their true position, batching is unchanged, and an unlit room stays inside the valid colour range.
- Extend the Asset Demo test with lighting assertions: all twelve fixtures are owned exactly once, every room has a navigable baseline, the corridor reads brighter than the rooms it connects, a fixture casts a visible pool, the two sides of a doorway meet without a seam, and every baked vertex colour is finite and in range.
- Add the same static lighting model to the level editor's 3D preview (`level-editor/js/lighting.js`), so authors see a room's relative brightness while editing; the preview approximates local pools (the game remains authoritative). The inspector now documents the intensity scale and warns above the game's clamp.

### Changed

- Room floors and ceilings are now generated on the lighting grid (one quad per bounded cell) instead of one quad per room, and wall faces are split along their length. Levels grow accordingly but stay small and static: `level_1` 9.3k → 61k vertices, the Asset Demo 0.9k → 2.3k, with a 7-13 ms level build in a release build (still one static upload and no per-frame cost). Small rooms stay a single quad and the geometry budget stays bounded.
- The optional fixture intensity and the level's stained wallpaper / damp carpet tints now multiply together, so material variation survives the lighting instead of being washed out.

### Fixed

- Ceiling-light fixtures are placed at their own room's ceiling height instead of a hard-coded 3.49 m, so the corridor of the Asset Demo (2.6 m) no longer draws its panels above the ceiling.

### Notes

- `tools/levels/build_demo_levels.py` now emits ceiling lights through a helper that can declare an intensity. `levels/asset_demo.json` deliberately keeps all twelve fixtures at the default so it also proves that levels without the field behave as `1.0`; `assets/levels/prop_showcase.json` mixes `0.8` and `1.4` fixtures to exercise the field.

### 0.4.0 — 2026-09-20

### Added

- Add `levels/asset_demo.json`, a walkable demo map that shows off the whole asset pack: four rooms around a corridor, with every catalogue prop placed at least once (all twenty core props plus `spooner-man`, 52 placements in total) and the complete level vocabulary in one level — doorways, a wide passage, three windows, a vent and twelve ceiling lights.
- Exercise the placement features the prop system already supports in that map: several props standing on other props, and `spooner-man` placed on the bed and in the corridor, all with ordinary `y` offsets and rotations.
- Show the material variants Level 1 does not use by defaulting the demo map to the stained wallpaper and damp carpet textures.
- Add `loader::tests::test_asset_demo_level_loads_and_shows_every_asset`, which discovers the map through the normal custom-level path, loads and validates it, asserts every catalogue asset and every opening kind appears, and asserts the level builds real prop geometry with no placeholder boxes.

### Changed

- `tools/levels/build_demo_levels.py` now also generates the demo map (`levels/asset_demo.json`) alongside the two development fixtures, so the map is reproducible rather than hand-edited.

### 0.3.1 — 2026-09-20

- Match Spooner Man’s reference coat: black back, narrow nose blaze, broad black chin patch, and a single right hind-leg white ring connected to the belly.
- Correct the lathe UV seam and map facial features continuously instead of repeating them across cap triangles; retain the existing 880-triangle mesh and 256x256 texture budget.

### 0.3.0 — 2026-09-20

### Added

- Add `spooner-man`: a low-poly tuxedo cat prop (880 triangles, one 256x256 texture, one material) placed through the ordinary prop system, with position, rotation, scale and vertical offset behaving exactly like every other prop.
- Add the cat's generator module `tools/props/parts/spooner_man.py` plus the convenience wrapper `tools/generate_spooner_man.py`, so `python3 tools/generate_spooner_man.py` rebuilds the GLB, the editor proxy entry and the prop-browser thumbnail.
- Extend the asset toolkit with two primitives the cat needs: `lathe` (an explicit-ring surface of revolution with per-region UVs, a separate cap patch and floor-contact shading) and `tube_path` (a tapered tube swept along a curved polyline with parallel-transported frames), plus `Mesh.normalize_origin` for deliberately asymmetric props whose bounding box must still be centred on the placement origin.
- Add the derived editor proxy colours for the cat's parts, so the level editor's 3D preview shows a black cat with white socks instead of a neutral blob.

### Changed

- Run `tools/props/build.py` with `--only <id>` now also refreshes that prop's entry in `assets/props/prop_proxies.json` (entries are merged, never partially rewritten).
- Place `spooner-man` in the `prop_showcase` development level, which now covers every catalogue prop.
- Update the asset validation tests, the editor catalogue mirror and the documentation for a pack of twenty-one props.

### 0.2.0 — 2026-09-20

### Added

- Ship the core prop pack: all twenty `core:*` catalogue entries (`couch`, `armchair`, `chair`, `table`, `desk`, `bookshelf`, `cabinet`, `bed`, `stove`, `sink`, `fridge`, `washing_machine`, `vending_machine`, `water_cooler`, `crate`, `cardboard_box`, `plant`, `rug`, `lamp`, `tv`) now reference real, self-contained `assets/props/models/*.glb` assets instead of placeholder boxes.
- Add `tools/props/` (pure-Python, no Blender): a primitive mesh builder, a procedural texture painter, a minimal GLB writer/reader, a build/validate driver and a software preview renderer, so the whole pack is reproducible with `python3 tools/props/build.py`.
- Add runtime GLB support for props: `src/gltf.rs` parses the narrow self-contained asset profile the toolkit emits, and `src/props.rs` caches each decoded model (mesh, indices, texture) once per catalogue path, complaining once per broken asset instead of retrying.
- Add batched prop rendering: placed instances are transformed once at level load into one shared vertex buffer per distinct model, so ten chairs still cost one draw call, one texture bind and one decoded texture.
- Add automated asset validation: `python3 tools/props/build.py --check` for the files and a Rust test that walks the catalogue and fails with actionable messages on missing models, oversized textures, triangle overruns, wrong scale, off-origin models, non-finite vertices, invalid indices or out-of-range UVs.
- Add the derived `assets/props/prop_proxies.json` (generated from the shipped GLBs) so the level editor previews every prop from its real geometry instead of a hand-maintained duplicate, plus 64x64 prop-browser thumbnails in `level-editor/assets/thumbs/`.
- Add the two development fixtures `assets/levels/prop_showcase.json` (all twenty props, including a deliberately sunk crate and an overlapping box) and `assets/levels/prop_stress.json` (about 150 repeated placements across nine models), generated by `tools/levels/build_demo_levels.py`.
- Add developer-only run flags for hardware checks: `LIMINAL_LEVEL=<level id>` boots straight into a level, `LIMINAL_SPAWN=x,z,yaw` stands at a specific spot, and `LIMINAL_CAPTURE=frame.png` renders one frame, writes it out and exits (the only way to inspect real prop rendering on the PocketCHIP over SSH).
- Add `assets/props/README.md` and `tools/props/README.md`, documenting the registry format, coordinate/scale/origin convention, texture and triangle budgets, the material/GLB restrictions and the workflow for adding a future prop.

### Changed

- Extend `props.json` with the model path for every entry; ids, names, categories, sizes, colours and `solid` flags are unchanged, so existing levels and editor data keep working.
- Props without a usable model (unknown catalogue id, missing file or malformed GLB) now fall back to their catalogue-sized placeholder box with a one-time developer message, instead of always drawing a box.
- Prop textures use CLAMP_TO_EDGE with mipmaps and follow the existing `texture_filtering` setting rather than being forced to a single mode.
- Levels without props are unaffected: no prop buffers, textures or draw calls are created, and existing level files load unchanged.

### Fixed

- Fix prop models being reported as unsupported on desktop only: the loader resolves asset paths relative to the catalogue directory (`assets/props/`), matching how levels and the editor address `models/*.glb`.

### 0.1.0 — 2026-09-20

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
