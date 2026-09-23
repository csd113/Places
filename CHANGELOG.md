## Unreleased — Surface shine and the Places Demo material pass

Reflections were reading far too strong for the Places aesthetic: linoleum
mirrored the room, ordinary brushed metal read like chrome, and floors and
panels looked wet or freshly lacquered. Shininess is now an explicit, authorable
material property instead of something a material class implied, and the
official demo was re-authored with it.

### Material model

- **`shine` is the author-facing glossiness**, `0.0` matte … `1.0` extremely
  glossy, on a catalog or pack material. The engine still stores the shader's
  `roughness` as `1 - shine`; the legacy `roughness` field is accepted unchanged
  (authoring both is a named catalog error), so an existing catalog or pack
  keeps its exact surface response.
- **A level can override one surface's shine** with a sibling key wherever it
  names a material: `defaults.*_shine`, `rooms[].shine` / `ceiling_shine`,
  `walls[].shine` / `face_shine`, `floor_patches[].shine`,
  `floor_regions[].shine` / `edge_shine` and `openings[].glass_shine`. An
  out-of-range or non-numeric value is a named loader error, not a silent clamp
  or a crash.
- **Shine is not material identity and not a mirror.** It shapes the sheen and
  the reflection — weight, sharpness, and how broad a reading a probe returns —
  while the sheen colour, the normal map and `reflection_mode` keep a metal
  reading as metal at every value. `shine: 1.0` without a reflection mode never
  samples the room; a mirror remains the dedicated `reflection_mode: planar`.
- **The response shader is duller by construction.** The tight near-normal sheen
  lobe now scales with the gloss, so a semi-gloss surface no longer gets a flat
  face-on glow that read as plastic; a reflection's weight is grazing-dominated
  at low shine, a rough probe surface blends towards a wide, unsharp reading of
  the cubemap, and the planar blur scales with the shine. High-shine surfaces
  keep their tight, recognizable reflection.
- **A planar mirror now reflects the room instead of itself.** The mirror
  plane's own geometry is skipped while the reflection image is drawn, so the
  mirrored camera sees the room *through* the plane; before, the mirror surface
  was the nearest thing to the mirrored camera and filled the image with its own
  colour. A planar surface is an aperture — a floor/ceiling plane, a floor patch
  or region, or an opening pane — and a wall slab is still reported as
  non-planar rather than mirrored.

### Places Demo

- Ordinary office linoleum is near-matte (`shine: 0.05`) through a per-surface
  override; the catalog material keeps its deliberately waxed `shine: 0.55`
  default and its faint probe reflection for floors that are meant to be
  buffed.
- The aged brushed-metal notice board drops to `shine: 0.35` with a softer probe
  (0.25), and the second board's frame is deliberately polished stainless
  (`shine: 0.6`) so not every metal in the demo is equally shiny.
- Pool deck, basin and wall tile keep a low glazed sheen (`shine: 0.28`–`0.3`)
  instead of no response at all, and the wet deck keeps its one planar mirror
  (`shine: 0.78`, strength `0.4`, a darker wet tint) so a puddle reads as
  standing water that reflects the room rather than a bright block.
- Walls, wallpaper, ceilings, carpet and the painted grille stay matte; glass
  and the backlit signs keep their smooth-sheen defaults.

## Unreleased — Batch 5: compiled-build readiness, cleanup, QA, documentation

Batch 5 is a stabilization pass, not a feature batch. It makes the compiled
binary a first-class citizen, removes the accumulations of four development
batches, hardens the content error paths, and rewrites the authoring guide
against the current engine.

### Compiled-build and runtime behavior

- **The writable runtime state has one deliberate location.** `settings.json`,
  the drop-in `levels/` and `import/` directories and the lightmap cache resolve
  below the package root (the parent of the resolved `assets/`), never against
  the process working directory;
  `LIMINAL_STATE_ROOT` overrides it for tests and benchmark runs. The lightmap
  cache moved from `target/level-cache/lightmaps/` to `cache/lightmaps/` so a
  compiled build never creates a development-flavoured `target/` directory.
- **A fresh launch initializes itself.** The game creates `levels/` and
  `import/` and writes a default `settings.json` on first run; Places Demo
  exists even when no asset tree and no level files exist.
- **Normal startup is quiet.** The `[package]`/`[props]`/`[level]`/`[spatial]`/
  `[lighting]`/`[lightmaps]`/`[dynamic]`/`[vsync]`/`[framebuffer]` telemetry is
  printed only under `LIMINAL_VERBOSE=1`. Genuine problems (missing asset root,
  an unreadable settings file, a skipped level, an unresolved material, a failed
  upload) still print, deduplicated once per item through the new
  `src/logging.rs` instead of repeating per caller.
- **Configuration is created, validated and recovered.** Values are clamped and
  bindings repaired on load; an unparseable `settings.json` is preserved as
  `settings.json.invalid`, defaults are used and a clean file is written. The
  three configured actions are reported to the player with readable labels
  ("Strafe Right", not `strafe_right`), and a save failure is surfaced instead
  of discarded.
- **A level file that cannot be used is skipped with its name and reason**, so a
  broken drop-in level is diagnosable instead of silently absent.
- **Compiled-build smoke tests** (`tests/test_compiled_build.py`) run the real
  release executable from outside the repository: a portable package started
  from an unrelated directory, an empty first-run install with the embedded
  demo, configuration reload across restarts, malformed settings recovery,
  malformed custom levels, unknown materials/props/fixtures/glass, and a clean
  exit. They skip themselves when no release binary or no display is available.

### Content error paths

- **ZIP entries are capped by actual output, not the header's declared size** —
  a deflate stream that lies about its size can no longer expand past the
  per-file or per-pack limit.
- **GLB accessors with `byteStride: 0` and a large `count` are rejected** (they
  used to defeat the bufferView bounds check), and every accessor count is
  clamped to what its bufferView can physically hold, so a malformed model falls
  back to the placeholder box instead of attempting a huge allocation.
- **New standalone level file cap** (`MAX_LEVEL_JSON_BYTES`, 8 MiB), a floor
  patch cap (`MAX_LEVEL_FLOOR_PATCHES`) and a per-wall opening cap
  (`MAX_WALL_OPENINGS`), all rejected with named loader messages.
- **An unbounded benchmark session now caps its retained frame records.**

### Player-facing UI

- **Screen status lines are correct and screen-local.** Error text is red by an
  explicit flag instead of prefix guessing (Level Select failures used to render
  green), and switching screens clears the message that belonged to the old one.
- **Nothing clips.** Level names and long diagnostics are truncated to their
  panel, the rebind prompt moved left and names the action in words, and the
  longest action label fits its box.
- **Every legend is a centred, consistently-worded line**; titles are centred in
  their panels; the version label lines up with the menu items.
- **`ESC` and `-` are reserved keys**: a gameplay action can no longer be bound
  to a control the shell always intercepts, `settings.json` bindings are repaired
  on load, and duplicate/empty bindings fall back to their defaults.
- **Honest settings.** VSync says it applies after restart, "Restore Defaults"
  resets every persisted preference, and a rebind that cannot be saved says so.
- The Level Select import row is labelled "Import Levels" with player-facing
  status text, and an empty list says so instead of showing a bare menu. The
  dead disabled-row styling and the duplicate overlay-toggle input state were
  removed.

### Places Demo QA

- A full capture playthrough (offices, stairs, pool, corridors, final doorway,
  unmade world) in Full and Low found no Z-fighting, holes, floating props,
  broken transparency, reflection artifacts or light leaks. Two real fixes:
  the second pool notice board's bottom rail was buried below the deck and is
  now a visible rail, offset so no architecture faces are coplanar (the
  shipped-demo surface audit covers it).
- `tools/bench/capture_views.sh` replaces the Batch 3/4 capture scripts and adds
  a full walkthrough view set with corrected camera aiming.

### Tooling and documentation

- **One current asset/benchmark workflow.** Removed the broken PocketCHIP-over-
  SSH suite (`run_bench.py`, `runone.sh`, `gen_levels.py`, `analyze.py`), the
  redundant `bench_repeat.py`, the superseded Batch 3/4 capture scripts, and the
  committed ARM binary under `platforms/pocketchip/`. `bench_local.py` now
  reports min/median/max and is the single local runner. Generators for assets
  that are still shipped (textures, props, fixture levels, the spooner-man
  entity, the validators) are kept.
- **Stale asset documentation corrected** (build.py's skip behavior, decal
  wrapping, the surface shader's response/alpha fields, the lightmap cache path).
- **`docs/MAP_AUTHORING_GUIDE.md` rewritten and re-verified** field-by-field
  against the parser, loader and shipped content: the complete current schema,
  per-limit enforcement, corrected animated-emission and prop-light rules, the
  reflection and quality-profile contracts, emission-versus-illumination, the
  pack `materials.json` schema, export-validation split and refreshed recipes.
- `settings.json`, `cache/`, `import/` and drop-in level files are gitignored,
  so running the game no longer dirties the repository.

## Unreleased — Batch 4: post-processing, selective reflections, dynamic polish, visual repair

Batch 4 finishes the presentation path the offscreen target made possible and
repairs three defects the earlier batches left behind. It adds no new lighting
model: bloom follows *emission*, reflections are weighted by the sheen the
materials already author, and the fog is a scalar mix.

### Restrained post-processing

- **Bloom follows emission.** The emissive term is drawn alone into a
  quarter-resolution target, blurred with two separable passes and added back by
  the resolve stage. A brightly lit wall can never bloom however bright its bake
  is; only a surface or fixture face that emits does. `u_emission_only` is the
  one flag, and the pass submits only the handful of emissive batches.
- **A tone shoulder, not a look.** The resolve stage applies exposure and a soft
  shoulder above `0.75`: everything below is untouched, so the baked lighting's
  own contrast survives exactly and only genuinely over-bright pixels (an
  emissive face at intensity > 1) roll off instead of clipping.
- **Atmospheric fog** (`src/render/atmosphere.rs`) is exponential-squared
  distance fog with a mild height term, mixed in the world fragment stage: about
  4 % at 20 m, 15 % at 40 m and 63 % at the far plane, a little denser near the
  floor. It is in the world shader, so both profiles and the direct fallback get
  it, and it costs no pass.
- **A barely-there grade** (saturation 1.03, contrast 1.02) runs in the resolve
  stage on `Full` only.
- **Low skips all of it.** `Low`'s resolve settings are the identity, so the
  renderer presents the scene with the plain copy quad and costs what the
  pre-Batch-4 presentation did. Bloom, exposure, the shoulder and the grade are
  `Full`-only; the fog is both.
- **The UI is outside it by construction**: `render_ui` still draws on the
  default framebuffer after the resolve, and it now *re-applies* a plain surface
  state rather than trusting the cache (see the defect below).

### Selective reflections

- **Per-material, opt-in, two kinds.** `MaterialReflection` /
  `ReflectionMode` (`src/materials/reflection.rs`) add `reflection_mode`
  (`none` / `probe` / `planar`) and `reflection_strength` to a material.
  `none` is the default and reflects nothing.
- **Static probes** (`src/render/reflections.rs`) are cubemaps baked once per
  level load at the centroid of the probe-reflective geometry, clustered by room
  (at most two). Sampling one is a single texture read; `Full` bakes 64-texel
  faces, `Low` 32. The whole bake costs 3.6 ms on Places Demo.
- **Planar mirrors** are a real second view of the level, mirrored through a
  plane *derived from the emitted geometry* — so a material reused on two planes
  is resolved per batch, and a material on a non-planar surface is reported and
  skipped rather than reflected wrongly. At most one plane is drawn per frame
  (the nearest one on screen) and at half resolution, and `Low` never allocates
  the target at all.
- **Reflections respect the response.** The reflected colour is weighted by the
  material's own specular colour, its roughness and a Fresnel term, so a surface
  with no sheen never reflects and a rough one suppresses what it does catch.
- Places Demo marks the wet pool deck (`planar`), the polished linoleum and the
  brushed-metal panel (`probe`).

### Dynamic visual polish

- **Animated emissions** (`src/render/animation.rs`, level
  `animated_emissions[]`): a named material's emission can `pulse` (a slow
  sinusoid) or `flicker` (an occasional, bounded stutter). Both shapes are pure
  functions of the level's elapsed seconds, start at full brightness and are
  bounded by `depth`; the animation scales the emissive term only, so the
  fixture blinks while its baked pool of light stays steady. The clock advances
  from the simulation's own delta, so a first-frame capture is always the
  authored image.
- Places Demo's backlit signs breathe at 0.09 Hz, and one sign on the north pool
  wall is on a failing ballast: a 7.5 Hz flicker to 40 % about a tenth of the
  time.
- The Batch 2 rotating washer drum is unchanged and still the dynamic-object
  path's demonstration.

### Visual repair

- **The pause menu leaked the world's material state.** `render_ui` bound the
  world program and then *claimed* the surface cache held a plain state without
  uploading it, so the uniforms still held whatever the last world material left
  in them — most visibly the last translucent surface's `u_opacity`, which made
  the pause panel see-through. It now applies `SurfaceState::plain` through the
  same `apply_surface_state` path the world uses, so no emission, sheen, opacity,
  reflection or animation can reach the UI.
- **The vertex frame was never wired.** `set_vertex_attributes` pointed
  `a_pos`, `a_color`, `a_uv` and the lightmap attributes but never `a_normal`,
  `a_tangent` or `a_handedness`, so every surface read the generic attribute
  default `(0, 0, 0, 1)`: `v_normal` was the zero vector, the sheen's grazing
  lobe was pinned at 1 on every fragment, and normal maps were dead. That is the
  white wash on the pool's brushed-metal screen and wet deck. The exact layout's
  lightmap offsets were also wrong (they pointed into the normal bytes). The
  pointer table is now one pure function, walked by the renderer and pinned by a
  test.
- **Window reveal caps were not translated.** `emit_wall_slice_cap` built its
  quad in the wall unit's *local* length space and never added the wall's length
  origin, so every sill and header on a wall whose min corner is not zero was
  shifted by that origin: a gap at one jamb and a buried overhang at the other,
  letting the player see into the wall. The caps are translated like every other
  emitter, and a regression test builds a wall with a non-zero origin and
  asserts the cap spans its opening exactly.
- **The two bare pool panels are now notice boards.** The brushed-metal and
  moulded-plastic slabs against the pool walls (Batch 3's surface-response
  demonstration) read as unexplained white boxes. Each now sits inside a
  brushed-metal frame and carries a backlit sign face: the pool's two
  illuminated notice boards, one of them on the flickering ballast.

### Validation and tooling

- New regression tests: the attribute table against the vertex structs and the
  shader's declarations, the exact-layout offsets, the HUD's plain state, the
  window cap's world span, the shipped demo's reflection routing (plane
  normal/offset, material mapping and probe points), the two post-processing
  fallbacks (no offscreen target, and `Low`'s identity resolve), and the
  animation shapes (bounds, first-frame identity and the flicker's resting
  share).
- `tools/assets/validate.py` validates `reflection_mode` /
  `reflection_strength` on materials and the level `animated_emissions` array.
- New benchmark switches: `LIMINAL_NO_BLOOM=1` and
  `LIMINAL_NO_REFLECTIONS=1`, so one build can measure each post-processing and
  reflection stage in isolation. The benchmark CSV and summary now report
  `reflection_passes`, and `LIMINAL_PAUSE=1` opens the pause menu on the first
  frame so the pause UI can be captured without a keyboard.
- Measured on macOS at 960×544: bloom 0.19 ms and the planar reflection 0.23 ms
  of a 0.89 ms `Full` frame, `Low` 0.47 ms (the same shape as Batch 3's 0.45 ms),
  two extra draw calls, and about 3.2 MiB of reflection and bloom targets.
  `tools/bench/notes/post-processing-reflections-validation.md` has the full table.
- The emissive image the bloom blurs shares the scene's depth buffer, drawn at
  the scene target's resolution, so an emitter hidden behind a wall cannot glow
  through it. A view with no emissive surface on screen skips the stage.
- `tools/bench/capture_batch4.sh` captures the fixed validation view set, and
  `tools/bench/notes/post-processing-reflections-validation.md` records what the runs
  showed.

---

## Unreleased — Batch 3: surface response, transparency/glass, offscreen framebuffer

Batch 3 makes the surfaces *react* to the Batch 1/2 lighting, gives materials a
real alpha contract (including glass you can look through), and puts the 3D scene
behind an offscreen target that a later batch can post-process. It adds no new
lighting model: everything here multiplies or blends with the light the bake
already delivers.

### Lightweight surface response

- **Optional per-material response.** `ResolvedMaterial` gained
  `MaterialResponse` (`src/materials/response.rs`): an optional normal map with a
  strength, a sheen colour and a roughness. The world fragment stage adds a
  view-dependent Fresnel sheen scaled by the baked light and perturbs the shading
  normal from the map — dull paint, plastic, brushed metal, glossy tile,
  linoleum and wet surfaces read differently with no BRDF, no light direction to
  sample and no shadow map. The engine deliberately has no realtime specular.
- **A tangent frame on every vertex.** `Vertex`/`PackedVertex` carry a normal,
  a tangent and a bitangent sign; the packed layout grows from 32 to 36 bytes
  (three normalised signed bytes per vector, one for the sign) and the exact
  layout from 44 to 72. The frame is computed once per range from the emitted
  triangles (`compute_surface_frames`), so no emitter knows about normals and a
  future emitter cannot forget one; the tangent comes from the surface's own
  UVs, which is what orients a normal map with its tiling.
- **Defaults are the old look.** A material without response fields is exactly
  the pre-Batch-3 material: no normal map, no sheen, and the shader gate is off.
- **Full / Low.** `Full` draws the response; `Low` leaves it out
  (`QualityProfile::draws_surface_response`) and keeps albedo × light × emission ×
  alpha — the same assets, one shader gate fewer.

### Transparency, glass and alpha modes

- **A material's alpha contract** (`MaterialAlpha`, `AlphaMode`): `opaque`
  (ignore the texture's alpha; the default and the legacy behaviour), `cutout`
  (discard below `alpha_cutoff`, drawn through a second, alpha-tested fragment
  stage) and `blend` (sorted translucent pass, depth writes off, texture alpha ×
  `opacity`).
- **Pass routing is derived, never authored.** `batch_pass_for` puts a batch in
  the opaque, cut-out or translucent pass from its material; the translucent pass
  is collected and sorted back to front by camera distance
  (`collect_translucent_draws`) with a reusable scratch list, so the frame loop
  allocates nothing.
- **A second scene program** for the alpha-tested pass, compiled from the same
  source with `ALPHA_CUTOUT` defined, so the opaque program keeps early depth
  testing. Uniform state is per program; the renderer tracks which pass is
  current and re-uploads the frame state after a switch.
- **Windows can hold real glass.** An opening may name a `glass` material
  (`WallOpeningDef::glass`), which emits one pane quad in the wall's centre
  plane, lightmapped like the wall around it. Clear, dirty, tinted and emissive
  translucent materials all work, and a `cutout` material gives a grille or
  screen instead of a pane.
- **Transparent emissive materials work**: emission is added before the alpha
  blend, so a backlit sign is both bright and see-through.
- **Emission degradation is consistent**: a material whose albedo, mask or normal
  map cannot resolve now loses its emission, response *and* alpha with the
  diagnostic texture, rather than half-rendering.

### Offscreen scene and presentation

- **`src/render/framebuffer.rs`**: an offscreen colour (RGBA8) + depth (24-bit,
  falling back to 16) target, recreated only when the drawable's size or the
  quality profile changes, presented by one fullscreen quad. The UI still draws
  on the default framebuffer at the drawable's resolution, so it stays sharp.
- **No distortion.** The target scales the drawable by a single factor, so its
  aspect ratio is the drawable's; `Full` renders at native size and `Low` renders
  no wider than the PocketCHIP reference width.
- **Clean fallback.** An incomplete framebuffer, a zero-sized drawable or a
  failed allocation disables the offscreen path for the session (one diagnostic
  line) and the scene draws straight into the default framebuffer as before.
  `LIMINAL_NO_OFFSCREEN=1` forces that path for an A/B comparison, and the two
  paths are pixel-identical (see `tools/bench/notes/surface-response-validation.md`).
- **Diagnostics.** A `[framebuffer]` line reports the target size and depth
  format once per resize; `RenderStats` gained `texture_binds` and
  `material_changes`, and the benchmark CSV/summary report them.

### Places Demo

- Five windows are glazed (clear, dirty and tinted sheets), a transfer grille
  fills a new vent above the office door, a polished-linoleum patch and a wet
  pool-deck patch join the existing carpet and tile, a brushed-metal and a
  moulded-plastic panel stand against the pool wall, and a backlit translucent
  sign hangs in the corridor. Nothing existing was repainted.

### Validation and tooling

- `tools/assets/validate.py` validates the new material fields by name and
  resolves `normal_texture` to a file-backed PNG, exactly like `texture` and
  `emissive_mask`; it also checks that every `glass` id in a level is declared.
- `tools/textures/extra_art.py` generates the eight new sheets (glass, linoleum,
  metal, plastic, grille and two normal maps) deterministically.
- `tools/bench/bench_local.py` runs the benchmark on this machine and compares
  Full / Low / offscreen / direct / baseline runs;
  `tools/bench/capture_batch3.sh` captures the fixed validation views.

---

## Batch 2: baked lightmaps, prop occlusion, dynamic objects

Batch 2 replaces the coarse painted-vertex light on static world geometry with a
real baked lightmap atlas, makes static props occlude the bake, and adds a
separate render path for objects that move every frame. The Batch 1 foundations
(Full/Low quality profiles, the generic `LightSource`, emissive materials) are
unchanged and still the only lighting/material model.

### Baked lightmaps for static world geometry

- **Lightmap atlas.** Static floors, ceilings (including gable slopes), wall
  faces, reveals, headers and floor-region skirts are baked into atlas pages of
  RGB8 texels (`src/lighting/lightmap/`). One deterministic shelf packer places
  every chart with a dilated padding gutter, so bilinear filtering cannot bleed
  one chart into its neighbour; the same level always produces identical pages.
- **The lighting model is unchanged.** Every texel is one
  `LevelLighting::sample_in_room` call: the same room baseline, local fixture
  pools, doorway blends, vertical isolation and wall occlusion the vertex bake
  uses. Only the resolution changes — the vertex bake sampled floors every
  2.5 m, the lightmap every 6.25 cm at Full.
- **The mesh carries a second UV channel.** `Vertex`/`PackedVertex` gained
  lightmap atlas coordinates (16-bit per axis) and a page byte; the packed
  stride goes from 24 to 32 bytes. A vertex whose page is `LIGHTMAP_NONE` keeps
  the historical vertex-lit colour, so both paths share one buffer and one
  draw call.
- **Shader.** The world fragment stage multiplies the sampled surface texture by
  the atlas texel on units 2/3 behind a global switch and the per-vertex page
  byte; emission is still added after the multiply. With no atlas resident the
  shader is behaviourally the previous vertex-lit pass.
- **Exact fallback.** Lightmaps off, an atlas overflow, a failed bake or a
  failed upload all rebuild the level with `LightmapMode::Off`, which reproduces
  the historical vertex-lit geometry bit for bit — never a black surface.
- **Full / Low.** `LightmapConfig::for_profile` picks 16 texels/m at a 1024 page
  (Full) and 8 texels/m at a 512 page (Low) from the *same* patch set, so Low
  needs no separate level or hand-authored bake.
- **Cache.** A deterministic content key over the lighting-relevant inputs
  (level definition, light definitions, lightmap config, quality profile,
  format version, and the fingerprint of the occluder set the bake actually
  used — walls, slabs and the derived prop boxes) decides whether a cached bake
  may be reused. A texture-only edit correctly keeps the atlas; a moved prop or
  an edited prop model re-bakes. The cache lives under the project-owned
  `target/level-cache/lightmaps/` and a miss simply bakes again.

### Static props occlude the bake

- **Derived occlusion geometry** (`src/lighting/occlusion.rs`): each distinct
  placed prop model is ground into a bounded column grid, merged into a small
  set of boxes and transformed with the instance's own scale, yaw and placement
  — a desk stays thin, a rotated couch shades along its rotation.
- **Contact darkening and blocked light are automatic.** No authoring change: a
  machine darkens the floor under it, a fridge blocks the pool behind it, and a
  prop against a wall darkens that wall.
- **Yaw-rotated occluders** in the visibility set; prop boxes never affect
  wall-containment (a floor sample under a prop is shaded, not moved) or
  partition detection.
- **Emission is still not illumination**: a prop's occluders come from its
  vertices, and only an authored `LightSource` illuminates anything.

### A separate dynamic-object path

- **`DynamicScene`** (`src/render/dynamic.rs`) owns objects whose transform
  changes at runtime. Geometry is uploaded once in model space and each object's
  transform reaches the shader through `u_mvp`, so moving an object never
  rebuilds a vertex buffer, a static batch or a lightmap.
- **One material system**: dynamic objects use the same `PropModel` assets,
  textures and emission routing as static props.
- **Lighting for now**: a probe of the static bake at the object's current
  position, fed through the new `u_light_scale` uniform, documented as the
  batch's temporary behaviour (no dynamic shadows, no realtime lights).
- **Demonstration**: a `core:washing_machine` static body in Places Demo with a
  rotating `core:washer_drum` dynamic component in front of it.

### Repair: dark rings around fixtures and steps at material boundaries

Visual validation of the batch found two artifacts that the automated checks had
missed. Both are engine fixes; no level, texture or fixture was touched.

- **Dark circular rings and blotches around ordinary fixtures.**
  `segment_hits_box` (the visibility clip) treated a segment whose endpoint lay
  exactly on a box face as crossing it when the entry parameter rounded a few
  ULPs below `1.0`. Every ceiling sample lies exactly on its room's ceiling body,
  so a fixture's whole local pool was deleted on 10–14% of the ceiling texels
  within its reach; because the failure depends only on the horizontal distance
  to the emitter footprint, the deleted samples formed concentric rings (up to
  78 RGB8 levels between neighbouring texels at 12 texels/m, where the old 2.5 m
  vertex grid had smeared them). The clip now requires a minimum overlap of
  `SEGMENT_CLIP_EPS` of the segment's length, which absorbs the rounding without
  weakening real occlusion; the start nudge and exact box sizes are unchanged.
- **Lighting steps wherever a texture/material boundary met a lightmap chart
  boundary.** `fill_chart` sampled texel centres, so a chart's geometric edge
  reconstructed the light half a texel *inside* that patch. Two coplanar patches
  each reconstructed their own inward-shifted value and formed a first-order step
  of `grad * (tA + tB) / 2` (2–5 RGB8 levels at the shipped densities, scaling
  with 1/density) even though the lighting was continuous. Chart texels now
  *span* their patch — the outermost texels sit exactly on the geometry edges —
  so two coplanar charts evaluate the same world point on a shared edge and
  agree exactly. A real 90-degree corner, a wall or another room is unaffected
  because those are different world points to begin with; nothing is averaged.
- **Floor/ceiling boundary rows buried in a wall** now take the same walked path
  the vertex bake uses, instead of reading the room baseline only and leaving a
  dark rim along their own wall base.
- **`merge_light_runs` off-by-one**: a wall face's final boundary was never
  checked against `MAX_CHART_SPAN_M`, so a long wall could become one over-long
  chart and break the Full/Low shared-split invariant. Fixed; the chart-span cap
  now holds for every emitted quad.
- **Cost**: demo cold lightmap fill 162.9 → 168.2 ms (+3%), warm load unchanged,
  atlas pages/KiB/charts/texels and static vertices unchanged, runtime draw calls
  and frame time unchanged. `LIGHTMAP_FORMAT_VERSION` is `3`, so an atlas baked
  by the earlier build is rejected as a cache miss.
- **Regression tests**: `a_slanted_segment_to_the_ceiling_is_not_blocked_by_that_ceiling`
  and `a_fixture_pool_reaches_its_whole_ceiling_without_a_ring` (the ring), the
  four-lightmap-continuity tests in `src/lighting/lightmap/continuity.rs`
  (material boundary across both axes, a same-material chart split, and a
  right-angle corner that must not be averaged), and the boundary-exact fill
  contract tests in `src/lighting/lightmap/fill.rs`.

# Changelog

## Unreleased — Batch 1 foundation: quality profiles, generic lights, true emission

This batch lays three foundations that later rendering work (lightmaps, surface
response, transparency, reflections, post-processing, dynamic objects) will build
on, without implementing any of them.

### Asset pipeline and quality

- **GLB import expanded** from "one mesh, one primitive, one material, one
  texture" to the production-friendly subset: a scene graph with composed node
  transforms, several meshes, several primitives per mesh, one material per
  primitive, several embedded PNGs (decoded once per distinct image),
  `baseColorFactor` for textured and untextured materials, `emissiveFactor`,
  `emissiveTexture`, and `KHR_materials_emissive_strength` (the only accepted
  extension). Skins, animations, morph targets, sparse accessors, external
  images and every other extension are still rejected with a named error, and a
  malformed model still falls back to the placeholder box.
- **Budgets are split into art budget vs engine ceiling.** 1500 triangles and a
  256 px prop sheet remain the shipped Places art budget (`tools/props` still
  refuses to build above them); the engine now loads up to 6000 triangles and a
  1024 px sheet with a one-time art-budget warning, and refuses only what it
  genuinely cannot draw (32 primitives / 16 materials / 16 images / 65 535
  vertices per model).
- **Full and Low runtime quality profiles** (`src/quality.rs`,
  `"quality"` in `settings.json`). Full is the historical runtime size
  (surfaces/fixtures/decals 1024, props 256) and uploads shipped assets
  unchanged; Low uses the same assets and box-filters each one once at level
  load (sheets 256, props 128, emissive masks 128). Downscaling happens at
  upload, never per frame, and the result is cached with the texture.
- **Multi-material props batch**: instances of a model are grouped per spatial
  cell and drawn one range per primitive, so a model with three materials costs
  three draws per batch no matter how many times it is placed.

### Generic engine-level lights

- **`LightSource`** (`src/lighting/light.rs`): shape (point, rectangle, line),
  world position, yaw, RGB colour, intensity, `range`, `falloff`
  (smooth/linear/constant) and `enabled`. The bake consumes only this type.
- **Fixtures are geometry that owns a light.** The three shipped fixture
  families map their luminous footprint to a rectangle via
  `FixtureProfile::shape()`; their placement, colour and intensity are unchanged,
  so the bake is numerically identical for existing levels (all lighting,
  isolation, partition, vertical and leak audits pass unchanged).
- **Props can own lights** (`props[].lights`): a prop positions generic sources
  in its own local frame (offset scaled, rotated by its yaw), so a machine,
  screen or sign illuminates a room without a new hardcoded light family.
- **New authored light fields**: `range`, `falloff`, `enabled` (fixture and
  prop light), plus `emission` on a fixture to set its face brightness
  independently of the light it casts.

### True emissive materials

- **`MaterialEmission`** (`src/materials/emission.rs`): colour, intensity and an
  optional mask texture, kept deliberately additive so `albedo`, `normal`,
  `specular`, `roughness` and `opacity` can land beside it later.
- **Emission is not illumination.** The fragment shader adds the emissive term
  after the baked-light multiply, so darkness cannot extinguish it, and no
  material ever creates a light. Catalog materials author `emissive`,
  `emissive_intensity` and `emissive_mask`; GLB materials author their own;
  fixture faces use per-vertex emission so a level can colour every fixture
  differently while they share one batch.
- **Old content is unchanged**: a material with no emission draws exactly the
  expression it always did, and the demo's lights bake to the same values.

### Demonstrations

- `places_demo.json`'s far east corridor keeps its last tube at `brightness`
  0.18 with `emission: 1.0`: the diffuser reads fully bright while the room
  keeps its dim pool — the authored proof that the two are independent.

## Unreleased — doorway floors own their threshold plane

Some doorways in `Places Demo` flickered between the two adjoining rooms'
floor textures as the camera moved: the office/stairs door at x = 19 and the
pool-deck/corridor door at x = 26. The cause was generated geometry, not depth
tuning. Both doors have a raised sill, so the wall's solid below the opening is
a plinth whose top lands exactly on the walkable floor plane. The wall emitter
drew that plinth's top cap as a full-thickness quad — x 18.85..19.15,
z 0.3..1.5 at y = 0 for the first door, x 25.85..26.15, z 12.5..14.1 at
y = -0.9 for the second — while the two rooms' floors already meet at their
shared boundary and jointly cover the same footprint (the documented threshold
ownership rule). That was 12 coplanar triangle pairs over 0.84 m², one of them
the plinth cap and the other a room floor, fighting for the same depth value in
every frame.

Two defects came together. The cap was never clipped against the floor that
owns its plane, and for Z-axis walls the cap's winding was transposed: a top
cap came out facing down and a bottom cap facing up. The reversed cap was why
the surface audit's opposite-facing skip treated the pair as "a wall's own back
face", and it would also be culled away in a culling-enabled build.

### Fixed

- **Wall caps are emitted only where they are actually exposed.**
  `render::emit_wall_caps` subtracts, rectangle by rectangle, the room-floor
  coverage at the cap's world plane — resolved from the same `LevelSurfaces`
  floor grid cells the floor mesh draws, so coverage can never disagree with
  the rendered floor — and the unit's own solid volume that continues the wall
  above or below the cap. A cap covered only in part keeps exactly its exposed
  remainder, which is what preserves a real sill ledge over a lower floor.
- **Z-axis cap winding is corrected.**
  `render::emit_wall_slice_cap` now winds X- and Z-axis caps so a top cap faces
  +Y and a bottom cap faces -Y, matching the X-axis branch and the outward
  convention every other face uses.

### Added

- `src/surface_audit.rs`: the coincidence detector compares canonical planes,
  so two triangles in one plane are reported whether they face the same way or
  opposite ways, and it runs against the architecture kinds only (decal
  offsets and prop floor contact are documented features). New regression
  coverage builds real meshes for same-material and mixed-material doorways,
  both wall orientations, wide and narrow openings, corner-adjacent openings,
  multiple doorways, differently sized rooms, a raised-threshold region, a sill
  ledge over a lower floor, a doorway chain, the two affected `Places Demo`
  doorways and Z-axis cap winding.

## Unreleased — decals own their depth plane

Wall and floor decals could flicker in `Places Demo`: the base surface showed
through the marking, in patches at some camera distances and angles and
completely at close range, and the patches moved with the camera. The cause was
the depth relationship, not the artwork: a decal was emitted exactly coplanar
with its parent surface and the *entire* separation was the decal pass's
constant `glPolygonOffset(0, -2)` bias. Two different tessellations of the same
plane do not interpolate to the same depth: the rasteriser fits each triangle's
plane equation separately, and the disagreement grows with the depth slope and
the triangle size. Measured on `Places Demo`, the decal needed between four and
eight depth-buffer steps to win at one normal gameplay camera — the old bias
was two, so the decal lost the `LEQUAL` test outright; at shallower angles it
lost only some pixels, which is the flicker as the camera moved.

### Fixed

- **Decals are displaced off their surface instead of relying on the bias
  alone.** Every decal quad is lifted `DECAL_SURFACE_OFFSET_M` (0.2 mm) along
  its surface normal in `render::add_decal_quad`, after the horizontal decal
  has been snapped to the real floor or ceiling. The lift is a real geometric
  separation: sub-pixel at every practical viewing distance, invisible as
  hover, but several depth-buffer steps through the interior range, so the base
  texture cannot win a pixel. It also cannot push a marking into neighbouring
  geometry: the direction is the surface normal, never a world-space nudge.
- **The decal pass bias is slope-aware.** `DECAL_POLYGON_OFFSET` is now
  `(-1.0, -4.0)` instead of `(0.0, -2.0)`. The constant term carries a few
  depth-buffer steps; the slope-scaled term tracks the interpolation error at
  grazing angles and long range, where no sub-millimetre physical offset is
  resolvable. Depth testing and depth writes stay on, so a decal behind a wall
  is still hidden.

### Added

- `src/render/tests.rs`: decal depth-regression coverage through the standard
  build path — all six `surface` kinds, rotations of 0–270 degrees, decals
  tucked against a wall/floor corner, and the emitted planes checked against
  the shared offset.
- `src/surface_audit.rs`: `every_shipped_demo_decal_owns_its_depth_plane`
  checks the acceptance case triangle by triangle, and
  `decals_are_offset_from_the_surface_they_mark_by_the_shared_bias` asserts the
  renderer invariant that no decal can share its parent surface's depth plane.
- `tools/bench/visual_check.py`: decal viewpoints (wall, floor, grazing) in the
  pixel-comparison shot list, so a future renderer change that reopens the
  depth conflict shows up as a large component.

## Unreleased — partition-aware baselines and vertical light isolation

Two lighting-architecture gaps are closed. A room is no longer assumed to be one
open space, and a floor or ceiling is now a real light boundary rather than a
pair of decorative planes.

### Changed

- **Internal partitions split the baseline spatially.**
  `lighting::bake` flood-fills a room's baked-lighting cell grid across the same
  wall-solid geometry the fixture pools use, probed just below the ceiling, and
  gives every disconnected area its own baseline
  (`LevelLighting::baseline_in_room`): a lit half of a partitioned room no
  longer lends its baseline through the wall to the dark half. A door's header
  separates as a solid wall does while the doorway keeps its bounded blend
  between the two areas, a window does not connect baselines at all, and a wall
  that stops short of the ceiling (or an interior stub) is not a partition.
  An unpartitioned room keeps its historical uniform baseline bit for bit:
  every existing level bakes exactly as it did.
- **Floors and ceilings occlude light.**
  `lighting::visibility` now builds each room floor as a stair-step of
  zero-thickness horizontal interfaces at the same heights collision walks, and
  each ceiling as a body above the ceiling plane (a gable gets a stepped body
  above the slope). A fixture cannot light through a solid slab — stacked rooms
  no longer contaminate one another, in colour as well as brightness — while a
  raised platform, a lowered basin and an intentional vertical opening stay
  open, because an interface never occupies room air. A sample sitting exactly
  on its own floor or ceiling is not blocked by that plane, which is what keeps
  a room's own fixtures lighting its own surfaces.
- **A ceiling fixture may author a world `y`** to choose its mounting height and
  therefore its storey, the way a wall fixture already does
  (`LevelLighting::fixture_y_for`). `sample` and the wall-face lookups resolve
  whole positions by height as well as footprint
  (`LevelLighting::room_index_at_height`), so two stacked rooms with the same
  footprint no longer resolve to whichever the area tie-break preferred.
- **The lighting summary reports areas and blockers.** The developer log now
  prints rooms, baseline areas, fixtures, wall boxes and slab/interface boxes;
  `LightingSummary` gained `zones` and `walls`.

### Fixed

- **The stained wallpaper no longer shows a tiling seam.** The 1024x1024 sheet
  had a left-to-right wrap step about five times the texture's own
  interior-pixel variation (a visible vertical seam where the stain ran out at
  the edge); the two carpets had the same class of defect at about twice the
  interior variation. `tools/textures/seam_repair.py` measures the wrapped step
  against the interior step distribution and repairs the low-frequency base of
  the wrap with a sized cross-fade band, leaving the high-frequency detail
  untouched, then verifies the result. The shipped office sheets and the pool
  wall tile are repaired reproducibly by that tool; the 1024x1024 artwork was
  not regenerated or downscaled.
- **The surface tiling test compares distributions, not a flat tolerance.**
  `render::tests::test_shipped_surface_textures_tile` accepted a fixed 40-level
  per-channel difference, which both missed the noisy carpets' seams and would
  have failed a legitimate fine-grained surface. It now compares the wrapped
  edge step's mean and p95 with the sheet's own interior step distribution, on
  both axes and all three channels, for every shipped surface texture, and is
  gated independently by `tools/textures/seam_repair.py --check`.

### Assets

- **Texture dimensions are validated by an explicit policy.**
  `assets::ShippedTextureKind` (`Surface`, `FixtureFace`, `DecalSheet`) and
  `MAX_SURFACE_TEXTURE_BYTES` replace the stale "everything is 128x128"
  assertions in `src/materials/tests.rs`, `src/render/tests.rs` and
  `tests/test_package.py`. Surfaces must be square and within the 1024x1024 hard
  limit and 4 MiB decoded budget; fixture faces and decal sheets must be
  power-of-two; the deliberate 96x64 diagnostic stays an explicit exception.
  The intentionally upgraded 1024x1024 artwork is accepted, and the no-diving
  sign test now asserts its real cut-out contract.

## Unreleased — coplanar surfaces and wall-boundary light

Places could emit a wall surface in the same plane as another wall's surface,
and the static lighting let a fixture's pool slip through the few millimetres
between two solid pieces. Both showed up as flicker: two faces fighting for the
same depth value as the camera moved, and light appearing on the wrong side of
an opaque wall.

### Fixed

- **Coincident wall geometry is resolved once.** `render::wall_layout` groups
  walls that share a plane and thickness and overlap in length *and* height
  (previously they also had to share their base and top), and each
  `(length, height)` cell takes the last covering member's material: the wall a
  room's boundary continues into the next room no longer emits a second
  coplanar face over the shared span. A wall's end cap or reveal is no longer
  emitted under the wall it abuts either — `cross_section_covered` and
  `subtract_rectangles` remove exactly the part of the face another wall's
  solid volume covers. `Places Demo` went from 39 coincident wall triangle pairs
  to none, and emits 316 fewer static vertices.
- **Fixture pools are tested against exact wall solids.** `lighting::visibility`
  no longer shrinks every opaque box by 5 mm; that shrink left a slit wherever
  two solid pieces met (beside every window and door jamb, at every corner),
  which is how window light crossed a wall it had no opening through. A query
  that starts exactly on a face is displaced `SEGMENT_START_EPS_M` along its own
  direction instead, so a sconce still lights the room it faces without opening
  a seam.
- **The scene projection uses OpenGL's `[-1, 1]` clip depth**
  (`perspective_rh_gl`), not glam's `[0, 1]` default: the old matrix only ever
  wrote the depth buffer's upper half, halving the precision coplanar decals
  rely on. `SCENE_NEAR_M` / `SCENE_FAR_M` are named constants with a test that
  pins the near/far mapping and the decal bias's sub-visible size. The same
  pass documents the doorway-threshold ownership rule in the floor emitter.
- `level-editor/js/lighting.js` mirrors the exact-box visibility and the
  start-point displacement, so the editor preview keeps matching the game.

### Added

- `src/surface_audit.rs`: mesh-level regression tests that measure the emitted
  triangles rather than the level JSON — a doorway threshold covered by exactly
  one floor surface, adjacent rooms with different floor materials, three
  openings on one wall, a wall continued at a different base, a wall abutting
  another, decals exactly coplanar with the surface they mark, intentional room
  overlap still emitting both floors, and the shipped demo emitting no
  coincident static surface at all.
- `src/lighting_leak_audit.rs`: the shipped demo's fixture pools and wall-face
  room assignments compared against an independent exact-visibility reference.
- `lighting::tests::{a_window_jamb_does_not_transmit_beside_itself,
  a_lit_corner_does_not_transmit_diagonally}` and
  `lighting::visibility::tests::{the_seam_beside_an_opening_is_airtight,
  abutting_wall_pieces_leave_no_seam,
  a_surface_mounted_fixture_is_not_blocked_by_its_own_wall}`.

## Unreleased — fixture surfaces are authored artwork

A light fixture's **mesh** is still generated geometry, but what that mesh
shows is now an ordinary external PNG: the catalog's `asset_type: "light"` entry
names the file, exactly like a file-backed decal sheet, and the runtime resolves
it through the same catalog -> PNG -> texture-cache path a surface texture uses.
Nothing about lighting changed: the bake, its pools, its visibility tests and
every fixture footprint and intensity are untouched, and the fixture's authored
colour still multiplies into the sampled sheet through the same vertex colour it
always used.

### Added

- `assets/environment/office/textures/lights/fluorescent_panel_01.png`
  (256x128) — the office panel's twin-tube acrylic diffuser face.
- `assets/environment/pool/textures/lights/pool_light_round_01.png` (128x128) —
  the round downlight's opal diffuser seen face-on: lamp core, moulded
  concentric rings, faint radial prisms and a shadowed contact edge.
- `assets/environment/pool/textures/lights/pool_light_wall_01.png` (128x64) —
  the wall luminaire's ribbed opal lens.
- `tools/textures/lights_art.py` — the deterministic painters for the three
  sheets, merged into the `build.py` manifest and covered by `--check`.
- `src/assets.rs`: `AssetEntry::is_light` and `AssetCatalog::fixture_sheet_path`
  (the PNG a file-backed light entry names), plus a catalog rule that a
  file-backed light must name a `.png`.
- `src/loader.rs`: `ResolvedFixtureSheet` and `resolve_fixture_sheets`, which
  resolve at most one sheet per fixture family through the catalog and the
  session texture cache (a level pack's own sheet still wins for a `pack:`
  fixture id).
- `src/lighting/tuning.rs`: `FixtureKind::ALL` and `FixtureKind::index`, the
  stable sheet slot a light batch carries.
- Tests: `render::tests::{every_fixture_family_has_a_stable_sheet_slot,
  fixture_faces_carry_their_own_family_sheet_and_the_housing_stays_bare,
  fixture_sheets_are_fitted_once_and_keep_their_aspect,
  the_round_diffuser_ring_has_no_uv_seam}` and three loader tests covering
  per-family resolution, the sheetless fallback and the demo's fixtures.

### Changed

- The three built-in fixture entries are `"source": "file"` and name their PNG.
- `LoadedLevel::fixture` (one optional pack sheet) became
  `LoadedLevel::light_sheets`, one resolved sheet per fixture family.
- `MaterialIndex` on a `Light` key is now the family's sheet slot rather than
  always `MATERIAL_NONE`; the flat metal housing keeps the bare key and still
  draws its authored shade through the shared white sheet.
- `Renderer::upload_prop_texture` became `upload_fitted_texture`: props and
  fixture faces are both fitted (non-tiling) sheets uploaded with `CLAMP_TO_EDGE`
  and mipmaps.

### Removed

- `loader::resolve_fixture`, superseded by `resolve_fixture_sheets`.
- `Renderer::upload_texture` and the per-level overwrite of the shared white
  sheet with a pack's fixture image.

### Fixed

- Prop placeholder boxes no longer inherit a pack's fixture sheet: the white
  sheet is never replaced now, so a pack's artwork only ever lands on the
  fixture faces it belongs to.

## 0.6.0 — 2026-09-22

Goal 6: distribution readiness, rendering and material polish, Places branding,
a project-facing README, an audited set of shipped levels and one official
showcase level. No new gameplay, no renderer rewrite and no source refactor:
this release makes what already existed installable, presentable and
demonstrable.

### Added

- `assets/levels/places_demo.json` — **the official demo**, and the level the
  README sends a new visitor to. One continuous route: a warm office reception
  and workroom, doorways and a window that looks one storey down into the pool,
  a red-lit stair hall reached by descending five 0.30 m risers, a passage onto
  the pool deck, the recessed empty basin with the ladder, guardrails, patio set
  and curtain screen, two steps up into a dim corridor, and a final doorway onto
  an unfinished world: a floor, a ceiling, three fixtures spaced into the dark
  and the end of the world's geometry. It exercises a window-only lighting
  boundary, two opaque boundaries that carry different colours on each side, and
  two real floor elevations.
- `tools/package.sh` — builds a self-contained distribution from a release
  binary: a flat `Places/` (executable, `assets/`, `levels/`, `settings.json`)
  and, on macOS, the same payload as a `Places.app` bundle.
- `docs/screenshots/` — the six images used by the README.
- `assets/levels/README.md` — an index of every shipped level: what each one is,
  which automated test or manual check depends on it, and which files are
  generated rather than hand-authored.
- `src/assets.rs`: `ASSET_ROOT_ENV`, `package_root_candidates`,
  `resolved_package_roots`, `asset_root_search_report` and a cached
  `resolve_asset_root`, so one deterministic search resolves the asset root for
  the whole process.
- `src/loader/tests.rs::test_the_official_demo_exercises_every_showcased_feature`
  pins the demo's promises: the opening kinds, three floor elevations, a real
  recess and five raised regions, several lighting conditions, the pool props,
  authored collision boxes for every solid prop, both external decal sheets, a
  spawn inside a room, and that the whole thing builds.
- `assets/core/decals/arrow_01.png` and `assets/core/decals/stripes_01.png` —
  the floor arrow and the hazard stripes as ordinary editable PNG cut-outs, with
  painters in `tools/textures/decal_art.py`.

### Changed

- **Runtime asset root.** `resolve_asset_root` and `catalog_path_candidates` now
  search, in order: `$LIMINAL_ASSET_ROOT`; the executable's own directory, its
  ancestors up to the legacy `bin/<target-triple>/app` depth, and a macOS
  bundle's `Contents/Resources`; the working directory (`assets`, `./assets`,
  `../assets`); and the compile-time crate directory **on development builds
  only**. A release binary can no longer read the source tree it was built from,
  and the startup log names the resolved root. The old
  `package_root()` — which only recognised `bin/<triple>/app` and otherwise fell
  back to `CARGO_MANIFEST_DIR` — is gone.
- **Missing-asset-root errors are actionable.** The diagnostic now names what
  was expected, lists every location searched and whether each one exists, and
  says how to override the search, instead of printing a single relative path.
- **In-game branding.** The main menu reads `Places` over `an experience`
  (previously `LIMINAL` over `PocketCHIP Walking Experience`); the window title
  and `SDL_APP_NAME` are `Places`, and the level editor's titles follow. The
  `SDL_VIDEO_X11_WMCLASS` value `io.vitrallis.liminalrust` and every `LIMINAL_*`
  environment variable are deliberately unchanged: they are launcher and
  developer API keys, not display strings.
- **Decal sheets are content.** `core:decal_arrow_01` and
  `core:decal_stripes_01` moved from renderer-generated atlas patterns to
  external PNG sheets with catalog `model` paths, leaving only
  `core:decal_test_01` generated (it exists to exercise the atlas). Levels and
  decal ids are unchanged, and the atlas's spare cells are asserted transparent.
- **README** rewritten as a project-facing document: what Places is today, the
  screenshots, the current features, how to launch the demo, the real control
  bindings read from the settings defaults, build and validation commands, the
  packaged distribution layout and the asset-root precedence, the asset/theme
  and level formats, the project layout, and an honest list of limitations.
- `assets/README.md` and `assets/environment/*/README.md` follow the decal
  change and the new level index.

### Fixed

- **A recess's transition faces no longer fall back to the room's wall
  material.** Only the lower-indexed cell of an adjacent pair emits the face
  between them, so a recess on that cell's side was keyed by the cell *outside*
  the region and rendered with the level's default wall material instead of the
  region's `edge_material`. The pool basin's far wall rendered as office
  wallpaper in any level whose default wall material differed from the basin's
  edge material — which is exactly what the demo exposed. Both shipped showcase
  levels hid the defect because their defaults happened to match.
- `Cargo.toml`'s description now describes Places rather than the historical
  Liminal walking game (`tests/test_package.py` updated in step).

### Removed

- **`Places Demo` is the only level bundled with the game.** The other
  packaged maps were removed from the distributable content: `level1.json`
  (which was also the embedded fallback level), the three residential levels
  (`the_residence`, `quiet_apartments`, `after_the_leak`), the Office and Pool
  showcases, `texture_diagnostic`, and the generated `asset_demo` /
  `asset_maintained` maps. The game now boots into `places_demo`, the embedded
  fallback embeds the demo, and the tests that only asserted the removed
  levels' own design were retired with them.
- The engine regression fixtures moved to `tests/fixtures/levels/`
  (`test_room`, `prop_showcase`, `prop_stress`, `pool_showcase`,
  `vertical_diagnostic`, `rendering_diagnostic`, `lighting_isolation`,
  `lighting_diagnostic`). They are not scanned by the game and are never
  packaged, but the loader, renderer, collision and lighting suites keep
  loading them by name. `tools/levels/build_demo_levels.py` became
  `tools/levels/build_fixture_levels.py` and now emits only the two generated
  fixtures.
- No asset was removed: every model, texture, material and decal in
  `assets/catalog.json` is still shipped, because `Places Demo`, the engine
  tests and user-created levels all resolve through the catalog. Nothing
  about drop-in level discovery or loading changed.

### Investigated, not changed

- **sRGB/gamma.** The pipeline is gamma-naive by design and stays that way. A
  shader-only "decode both factors, multiply, re-encode" pair is algebraically
  an identity — `encode(decode(a) · decode(b)) = a · b` — and was verified to
  produce a byte-identical frame. The place a linear pipeline genuinely differs
  is the *additive* bake (room baseline + fixture pool + doorway blend): summing
  those in linear space would darken fixture pools by roughly 17–28 % on the
  shipped constants and compress the channel ratios that make a coloured room
  read as coloured. That is a recalibration of the whole lighting and art set,
  not a correctness toggle, so it is recorded here rather than half-implemented.
- **Light-fixture artwork.** The fixtures are generated geometry with flat
  vertex colours, not generated pixels; there is no fixture texture to
  externalise. Pack-supplied fixture images already load from `.png`.

### Validation

`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features`,
`cargo build --workspace --all-targets --all-features` and
`cargo test --workspace --all-targets --all-features` (376 tests, one ignored),
`python3 tools/assets/validate.py`, `python3 tools/textures/build.py --check`,
`python3 tools/props/build.py --check`, `cd level-editor && npm test`, and a
clean-package launch of both `Places/` and `Places.app` from outside the
repository tree.

## 0.5.3 — Goal 5.5: wall-boundary lighting isolation and source refactor

An engine-quality phase rather than a content phase: the baked lighting now
treats an opaque wall as a lighting boundary, the wall-corner artifacts that
Goal 5 surfaced are gone, and the largest source modules were split into
focused ones. No new content, no new gameplay and no change to the level,
catalog or material formats.

### Fixed

- **Light no longer crosses an opaque wall.** A fixture's local pool is tested
  against the level's solid wall geometry before it contributes, so a light
  behind a wall (or around a closed corner) no longer illuminates the room on
  the other side. The test uses the same `wall_solid_slices_profiled` geometry
  the mesh and collision use, so a doorway, window, passage or vent still
  transmits light through exactly the hole it cuts, and the solid header above
  a door still blocks it.
- **Colour stops at walls too.** A red-lit room no longer tints the room behind
  an opaque wall, and two coloured rooms sharing a divider keep their own light
  right up to the shared face — in both the bake and the emitted vertices.
- **A wall face is lit by the room it opens into.** Wall faces resolve their
  room once, from an unambiguous point in the middle of the face, instead of
  sampling whatever room the containment tie-break preferred at a boundary.
  This removes the blue-grey wedge a red room's wall used to carry along a
  shared boundary, and the mirror case on the blue side.
- **Wall corners no longer collapse to ambient.** A wall authored across a room
  boundary ends inside the perpendicular wall; its end sample used to fall
  outside every room and drag the first 2.5 m of the face down to the ambient
  fill. Surface samples that lie inside a wall are now walked into their own
  room before they are measured, and a face's samples use the face's own room.
- **Reveals and end caps take light from both sides of their wall**, resolved
  in the room each side opens into, so a doorway jamb carries the threshold
  light instead of dropping to ambient in the wall cavity.
- **The doorway blend follows the aperture.** The bounded exchange between two
  rooms is only applied where the sample can see through the opening, so an
  opening joins its rooms through the hole rather than through the wall around
  it. The blend is unchanged at the threshold itself: symmetric, bounded by half
  the baseline difference, and still smoothing the doorway instead of stepping
  at it.
### Added

- `assets/levels/lighting_isolation.json`: a deliberately plain thirteen-cell
  diagnostic level that isolates the wall-boundary cases (blocked white light,
  blocked colour, doorway transmission, window sill and header, two coloured
  rooms, a dark neighbour, an interior partition, a lit corner, a two-fixture
  corner and an unlit control). It is a regression fixture, not a showcase.
- `src/lighting/visibility.rs`: the static wall-visibility model. Opaque wall
  geometry is prebuilt into world-space boxes per solid wall patch, with one
  distance-ordered box list per fixture and per opening, and the reach cut-off
  that keeps the common query to a box or two. Everything is built once per
  level load; there is no per-frame visibility work.
- `src/lighting_isolation.rs`: the acceptance suite over the diagnostic level,
  including a test that asserts the wall rules on *emitted vertices* so a
  regression in the geometry emitter cannot hide behind a correct bake, and a
  test that measures the blocked pool's magnitude so the "blocked" assertions
  prove the wall is doing real work.
- `tools/bench/notes/wall-boundary-lighting-validation.md`: the root causes, the
  fix architecture, the diagnostic level, the capture matrix and the
  before/after bake and build timings.
- `LevelLighting::opening_blend`, a diagnostic accessor for the doorway exchange
  used by the regression tests and mirrored in the editor preview.
- `lighting::WALL_FACE_PROBE_M`, shared by the bake and the geometry emitter so
  the room a face is lit by and the room its samples resolve in cannot drift.

### Changed

- `src/lighting.rs` is now a small façade over `lighting/{color,tuning,math,
  bake,visibility,tests}.rs`, `src/materials.rs` over `materials/{image,pack,
  resolve,decal,tests}.rs`, and `src/render.rs` over `render/{view,mesh,api,
  geometry,decals,fixtures,props,renderer,tests}.rs` (the wall emitters and the
  lit-surface grids stay with the façade). Every inline unit-test module in the
  tree moved to a sibling `tests.rs`, taking roughly 11k lines of test code out
  of the production files. Behaviour, serialized formats, material ids and
  public paths are unchanged, and the largest level rebuilds to the same mesh
  vertex counts as before the split.
- `level-editor/js/lighting.js` mirrors the visibility model (wall columns,
  solid spans, the per-site box lists and the aperture-limited blend) so the 3D
  preview does not show light crossing a wall; `level-editor/tests/` gained
  cases for the wall block, the doorway and the straddling-wall floor edge.
- The lighting parity vectors were regenerated for the corrected doorway values.

### Notes

- The dark ambient floor is unchanged: an unlit room is still exactly ambient,
  and no corner-brightness, ambient or saturation compensation was added.
- Lighting remains fully baked and static. The bake grew from ~0.02 ms to
  ~2.9 ms on Level 1 (225 fixtures, 208 walls) and stays under 3 ms there; the
  larger prop levels pay the new cost in prop vertex lighting (~15-22 ms total
  level build on desktop). See the validation note for the measurements.
- Known limitations are listed in the validation note: the room baseline is a
  room-wide term by design, walls are the only blockers (no stacked-room
  separation yet), and long walls are sampled at most eight times along their
  length.

## 0.5.2 — 2026-09-21

The first complete content release: the **Office** and **Pool** themes ship as
real environment families built on the Goals 1-4.5 architecture. Every surface
is an editable external PNG, every prop is an ordinary catalogued GLB, and the
Pool demonstrates the vertical-geometry system with a genuinely recessed, empty
basin. No water, no swimming and no new engine architecture.

### Added

- The final Office surface artwork: a commercial short-pile beige carpet with no
  metre checker, warm institutional printed wallpaper and an aged suspended
  panel ceiling, plus the maintained/stained water-damage variants. Same
  logical ids, same materials, new pixels under
  `assets/environment/office/textures/`.
- The complete Pool content family: pale commercial deck, basin and wall tile
  and a sterile painted ceiling under `assets/environment/pool/textures/`, and
  the `core:pool_*` materials that name them.
- Pool props: the white resin patio table and matching chair
  (`core:pool_table`, `core:pool_chair`), the modular privacy curtains
  (`core:pool_curtain_straight` / `_end` / `_corner`), the chrome pool ladder
  (`core:pool_ladder`) and the modular silver guardrails
  (`core:pool_guardrail_straight` / `_end` / `_corner`).
- Pool light fixtures: `core:pool_light_round` (a round recessed ceiling
  downlight) and `core:pool_light_wall` (a wall-mounted luminaire). A light's
  catalog id now selects a fixture family, so the built-in appearances are
  `fluorescent_panel`, `round_recessed` and `wall_sconce`; a wall fixture is
  authored with `"mount": "wall"` and a world `"y"` and is validated on load.
  Fixture family, luminous footprint and geometry budget all come from one
  table (`lighting::fixture_profile`), so the drawn fixture and its baked light
  pool cannot drift apart.
- External PNG decal sheets: a decal asset may now be `source: "file"` with a
  `.png` model and is decoded, cached and uploaded exactly like a surface
  texture, with its own GPU sheet and the same diagnostic fallback. The
  generated atlas keeps the architecture-test patterns.
- The final `NO DIVING` sign artwork
  (`assets/environment/pool/decals/no_diving_01.png`): an RGBA cut-out plate
  with the prohibition pictogram and lettering, replacing the generated
  placeholder. Authors can replace the PNG without touching Rust.
- `assets/levels/pool_showcase.json`: a composed, sparse indoor Pool complex —
  deck at room level, a real recessed basin built from `floor_regions`, a
  walkable `-0.35 m` entry step, tiled transition faces, the ladder standing on
  the basin floor, patio furniture, a curtain dressing run, guardrailed deck
  edges, both Pool fixtures, the final sign and a cool, restrained light set.
- `assets/levels/office_showcase.json`: a small, sparse institutional office
  suite on the final artwork with warm fluorescent fixtures, genuinely dim
  corners, scattered desks and chairs, one room on the damaged material set and
  floor decals.
- Tests for the new content and mechanisms: fixture families and mounts, wall
  fixtures rejected without a height, external decal sheets resolving through
  the catalog, catalogued fixtures and decals agreeing with the renderer, the
  carpet carrying no metre checker, and the Python checks for the Pool theme,
  the external sign's cut-out alpha and the guardrail collision boxes.

### Changed

- `core:desk` and `core:chair` were rebuilt as a near-black laminate desk and a
  proportionally corrected office chair, at the same logical ids and sizes, and
  their catalog swatch colours now match the new finishes.
- The generated decal atlas no longer contains the NO DIVING placeholder; the
  atlas holds the validation marking, the floor arrow and hazard stripes.
- Wall authoring is documented: a wall is placed by its **minimum corner**, like
  a room, with its `openings` measured from that corner (the level format
  section of `README.md` now covers walls, lights, openings and faces).
  `tools/assets/validate.py` warns when a wall touches no room, which is what a
  centre-authored wall usually does.
- Prop texture budget is expressed per catalogue entry (128x128 preferred each)
  instead of a fixed total, and the demo/showcase levels now split the generic
  and Office props from the Pool family.
- Solid props in the Goal 5 showcase levels author explicit collision `size`
  boxes (rotation-aware): a solid prop without one falls back to the neutral
  0.6 x 0.9 x 0.6 m box rather than its catalog size, so a large desk would not
  block like a desk.

### Fixed

- External decal sheets render upright and unmirrored on floors, ceilings and
  walls; the in-plane mapping is pinned by a unit test and was verified by
  ink-mask comparison of captures against the source PNG.
- The generated decal atlas drew its patterns in transposed cells, so
  `core:decal_arrow_01` sampled an empty cell and `core:decal_stripes_01`
  sampled the arrow; the art and the sheet-slot mapping now agree, checked by a
  test.
- The round Pool downlight's diffuser centre read as a hole; it is now a small
  lamp recess.

### Notes

- The Pool is intentionally **empty**: there is no water, no swimming, no water
  shader and no climbing. The basin is a 1.5 m recess in real level geometry,
  and the 0.4 m walkable-step rule makes the entry step usable and the deep
  basin edge a solid rim, so the player cannot fall in.
- Surface and decal PNGs are the authoritative runtime assets; the
  `tools/textures/` painters exist only to regenerate the shipped sheets and to
  validate their budget.

## 0.5.1 — 2026-09-21

External PNG surface textures. Environment artwork is no longer generated in
Rust: every wall, floor and ceiling material resolves through the asset catalog
to a real external PNG, and a creator edits or replaces that file and restarts
the game — no source change, no recompilation. The renderer knows how to draw a
textured material; it no longer contains the definition of one.

### Added

- `asset_type: texture` entries and `source: definition` materials in
  `assets/catalog.json`: a material names a logical `texture`, a world tiling
  period (`tile_metres`, default 2.0) and an optional static `tint`. The six
  built-in office materials keep their exact legacy ids and now point at
  external PNGs.
- `src/materials.rs`: the runtime material pipeline — PNG decode (RGB, RGBA,
  grayscale, grayscale+alpha, palette, 16-bit, up to 1024×1024, NPOT included),
  a session `TextureCache` (one decode per logical texture per session), the
  logical/resolved `MaterialTable` a level's ids resolve into, pack material
  definitions and the one conspicuous magenta/black diagnostic texture a
  missing or corrupt PNG falls back to (with the material and texture ids in
  the error).
- `tools/textures/build.py`: the deterministic, dependency-free seed-art
  generator and validator (`--check`) for the surface and diagnostic PNGs, plus
  `tools/textures/README.md`.
- Diagnostic texture set (`core:tex_diagnostic_*` and their materials): wall,
  floor, ceiling, a deliberately 96×64 NPOT sheet and an RGBA alpha sheet, and
  `assets/levels/texture_diagnostic.json`, which exercises all three surface
  families, an overlay material run, decals, a gable, a walkable recess, a
  region staircase into an elevated room and warm/blue/white fixtures.
- Tests for texture/material registration and duplicates, id resolution, PNG
  loading (valid colour types, malformed, truncated, missing, oversized),
  legacy-id compatibility, per-session decode caching, shared textures and
  material tiling/tint.
- `tools/bench/notes/texture-material-validation.md`: the capture matrix and
  the creator replacement test used to validate the phase.

### Changed

- The renderer batches static geometry by `(surface family, material index)`
  instead of a closed surface-kind list, binds one GPU texture per distinct
  resolved texture, keeps catalog textures resident across level changes and
  frees a level's pack textures when the next level replaces them.
- The metre checker on the office carpet is baked into the carpet PNG's four
  64 px quadrants; the per-load `generate_floor_checker_texture` bake is gone
  from the normal surface path.
- Wall UVs are oriented so an authored PNG reads upright and unmirrored: the
  image's top row is at the wall top, and each face's `u` runs the way a viewer
  on that side reads it.
- `tools/assets/validate.py` and `tests/test_package.py` understand texture
  assets, material texture references, `tile_metres`/`tint` bounds, PNG
  existence and floor-region material references.
- `assets/README.md`, the Office and diagnostic asset READMEs and the root
  README document the material/texture split and the creator workflows.

### Removed

- The procedural surface generators (`generate_wall_texture`,
  `generate_carpet_texture`, `generate_ceiling_texture`, their water-damaged
  variants and the supporting noise painters) and the hard-coded damaged
  material ids in the renderer.

### Notes

- The shipped PNGs are deliberately plain **seed** artwork that preserves the
  pre-4.5 appearance and proves the pipeline. Goal 5 owns the finished Office
  and Pool artwork; replacing these files needs no engine change.
- Live hot reload is not required or provided: edit a PNG, restart, see it.
- Power-of-two textures are preferred for the deferred PocketCHIP/Mali-400
  target (ES 2.0 does not guarantee NPOT + repeat + mipmaps); arbitrary
  dimensions load on the macOS development renderer and the diagnostic NPOT
  sheet pins that behaviour.

## 0.5.0 — 2026-09-21

Vertical geometry. A room is no longer a flat floor at world Y `0` under one
fixed ceiling height: rooms have a base elevation, the floor can carry local
recessed or raised regions, and the ceiling is a profile (flat or gable). The
same geometry model answers rendering, collision and baked lighting, so the
surface the player stands on is by construction the surface that was drawn.

### Added

- `RoomDef.floor_y`: the world Y of a room's floor plane. Floor, walls, ceiling,
  props and decals are generated relative to it; omitted means `0.0`, so every
  legacy level is unchanged.
- `RoomDef.ceiling`: a ceiling profile, `{"kind": "flat"}` (the default) or
  `{"kind": "gable", "ridge": "x"|"z", "ridge_rise": <m>}`. A gable is real
  geometry: two sloped ceiling planes meeting at a ridge, gable-end walls whose
  tops follow the slope, and walls clipped to the local ceiling.
- `LevelDef.floor_regions`: rectangular local floor areas with an `offset_y`
  relative to their room's floor (negative recesses, positive raises), an
  optional floor `material` and an optional transition `edge_material`. Region
  edges cut the floor grid exactly, the transition faces between heights are real
  quads, and a region touching a room boundary is closed against the room's floor
  plane rather than opening into the void.
- `src/level.rs` centralised vertical queries: `CeilingProfileDef`,
  `ceiling_y_for_volume`, `LevelSurfaces` (`room_at`, `floor_y_at`,
  `ceiling_y_at`, `floor_grid`, `ceiling_grid`, `wall_profile_breaks`) and
  `WalkableFloor`, the owned floor model the player controller samples. Mesh
  generation, collision, spawn resolution and the bake all read the same model.
- Room floor elevations in the player controller: the player tracks
  `player_floor_y`, collision filters against the player's actual vertical body
  band, spawns resolve against the real floor, and rises/drops within
  `PLAYER_STEP_HEIGHT` (0.4 m) are walked; larger discontinuities are refused and
  deep recesses get solid retaining rims. A staircase built only from floor
  regions is walkable without any stair-specific code.
- `assets/levels/vertical_diagnostic.json`: a purpose-built level with a normal
  room, an elevated room reached by a region staircase, walkable and blocked
  recesses, a gable room with eave and ridge fixtures, RGB lighting examples and
  decals.
- Validation and tests: non-finite elevations, impossible gable definitions,
  zero-sized/out-of-room/above-ceiling floor regions, a region count cap, and
  ceiling decals on a gable are rejected with clear messages; automated tests
  cover profiles, regions, rims, step behaviour, elevation rendering and
  fixtures.

### Changed

- The standard default room ceiling height is now **4.0 m** (was 3.5 m) for
  rooms that omit `height`. Levels that author `3.5` keep it.
- Props are placed on the local walkable floor (`prop.y` is an offset above it,
  which is what the format always documented), and floor/ceiling decals are
  snapped to the real floor/ceiling under them, so both follow an elevated room
  or a recessed region.
- Triangle winding is now one coherent convention: every world face (floor,
  ceiling, wall, transition face, prop box, fixture) is wound so its normal
  points out of the solid, matching the decal pass, which always did. Nothing
  rendered differently — face culling is disabled and the vertex format carries
  no normals — but the geometry is now correct for a future cull-enabled path
  and is covered by a winding test.

### Fixed

- Wall faces no longer resolve their ceiling profile at a world coordinate
  interpreted as a length offset. Walls whose origin is not at X=0/Z=0 (every
  room after the first in a level) were clipped to the *first* room's ceiling,
  which left gaps above them; a regression test pins the behaviour.

### Notes

- `REFERENCE_CEILING_HEIGHT_M` (3.5 m) is the lighting calibration reference,
  not a room default: it is deliberately unchanged, so existing bake output and
  the editor parity vectors are identical. Legacy levels bake and render
  bit-identically.
- The ceiling-height factor still uses a room's eave height: a gable ridge adds
  shape, not brightness.
- Known limitations: floor regions are rectangular and flat (no ramps or sloped
  regions); the controller has no falling physics, so a drop deeper than a step
  is a wall unless stairs of shallow regions are authored; a ceiling decal is
  rejected on a sloped ceiling and any horizontal decal that straddles a height
  change is rejected; overlapping regions resolve last-wins; two stacked rooms
  share one spatial batch cell; there is no multi-floor traversal yet.

## 0.4.0 — 2026-09-21

Generalized asset architecture. Logical asset identity is separated from
physical file location, the `office` and `pool` environment themes are
established, entities become a distinct asset class, and Spooner-Man moves into
the entity organization while existing levels keep referring to `spooner-man`.

### Added

- `assets/catalog.json`, the authoritative asset registry: every logical asset
  declares its class (`environment`/`entity`/`core`/`diagnostic`), type
  (`prop`/`material`/`texture`/`light`/`decal`/`entity`), optional theme, source
  (`file`/`generated`) and canonical resource path. Themes are data, so future
  themes need no engine change, and placement is never filtered by theme.
- Environment categories: `assets/environment/office/` (the office material
  set, the fluorescent fixture and five office props, catalogued with
  `theme: office`) and `assets/environment/pool/` (reserved for the upcoming
  Pool content pack).
- Entity organization: `assets/entities/spooner-man/model/spooner-man.glb` is
  the single canonical Spooner-Man resource (`asset_class: entity`,
  `asset_type: entity`, no theme).
- `assets/core/props/models/` for generic, theme-less props and
  `assets/diagnostic/` for development content.
- `tools/assets/validate.py`, validating the catalog, duplicate ids, resource
  paths, canonical Spooner-Man, and every asset id referenced by shipped and
  custom levels. Wired into `tests/test_package.py`.
- Rust: `src/assets.rs` with the catalog types, duplicate-id rejection,
  slug-validated class/theme/type identifiers, legacy `props` parsing and
  regression tests covering classification, themes, entities and shipped-level
  references.
- `LIMINAL_STATE_LOG=file.csv`, a developer diagnostic that records the player
  state while the game runs, so movement and control validation can assert real
  results from a running build. Inert unless set.

### Changed

- Default desktop controls are WASD movement with arrow-key looking, with
  rebinding and `Restore Default Bindings` preserved (introduced in 0.3.2).
- `PropCatalog` is now the placeable (prop/entity) view of the asset catalog;
  `PropAssets` resolves models below the resolved `assets/` root. Entity assets
  place through the ordinary prop format, so Spooner-Man, including its
  lighting, scale, orientation and appearance, is unchanged.
- Asset and level docs (`README.md`, `assets/README.md`, the category READMEs,
  `tools/README` files) document asset identity, themes, entities and the
  resolution flow.
- The Python prop toolkit and the legacy level editor read the catalog's new
  paths; the editor ignores the optional class/theme/type metadata.

### Notes

- Existing levels, the lighting diagnostic and the rendering diagnostic load
  unchanged; Goal 1 RGB lighting and Goal 2 decals/overlays are untouched.
- No duplicate Spooner-Man GLB remains, and no level stores a physical path.

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
