# wgpu lighting and shadows (Stage 8)

Status: **Stage 8 is implemented in the working tree; Stage 9 extended it with
the reference lightmap atlas, reflections, props, emission, decals, post
processing, fog and the HUD.** The wgpu world pass now
renders the Places baked lighting model and the reference renderer's
view-dependent sheen. It consumes the renderer-neutral bake (`src/lighting/**`)
through the historical vertex-lit build, so the room baselines, fixture pools,
opening blends, partition and storey isolation and static-occluder shadows all
reach the image exactly as they do in the OpenGL reference's vertex-lit mode.
The normal atlas path, reflections, dynamic-object probes, fixtures, props and
reflections are covered by [WGPU_STAGE9.md](WGPU_STAGE9.md).

This document is the Stage 8 handoff. It complements
[WGPU_MATERIALS.md](WGPU_MATERIALS.md) (Stage 7: the material system this stage
shades), [WGPU_WORLD_GEOMETRY.md](WGPU_WORLD_GEOMETRY.md) (Stage 5: geometry and
coordinates) and [WGPU_TEXTURES.md](WGPU_TEXTURES.md). It is deliberately
explicit about the two things a later stage must not rediscover: that the
reference has no realtime lights and no shadow map at all, and where the light
multiply sits in the display-space fragment assembly.

## 1. Stage 8 scope

Implemented:

- the wgpu level build switched to the reference's historical **vertex-lit**
  build (`LightmapBuildOptions::for_profile(quality, LightmapMode::Off)`), so
  the neutral bake's light reaches the GPU in the vertex colour;
- the camera uniform extended with the world-space eye position (the
  reference's `u_camera_pos`);
- the world fragment stage extended from the material multiply to the
  reference's full un-shadowed world assembly: `lit + sheen`, assembled in
  display space and converted to the sRGB target once, after both terms;
- the reference's view-dependent sheen, including its response gate, its
  roughness-driven lobe shapes and its normal-map participation;
- a documented `surface_light()` seam where Stage 9 places the lightmap atlas
  sample and Stage 10 the dynamic-object probe;
- a `PLACES_CAPTURE` read-back path for the wgpu renderer, so the canonical
  baseline views can be captured and compared against the OpenGL reference;
- tests for the uniform layouts, the light seam, the sheen equation and gate,
  the display-space assembly and the unlit bypass.

Deliberately absent (later stages):

| Term | Owner |
|---|---|
| lightmap atlas (the default reference mode) | Stage 9 |
| reflection probes and planar reflections | Stage 9 |
| dynamic-object light probe (`u_light_scale`) | Stage 10 (needs the prop/dynamic passes) |
| fixtures, props, decals, emissive geometry and bloom | Stage 10 |
| fog | later presentation stage |
| UI and post-processing | later stages |

## 2. The reference lighting model

**There are no realtime lights.** The OpenGL reference renderer has no light
selection, no light array, no attenuation curve in a shader and no shadow map.
Every fixture contribution is baked once per level load on the CPU by
`src/lighting/**`, and the world fragment stage contains exactly one light
expression:

```glsl
float lightmap_on = u_lightmap_enabled * (1.0 - step(254.5, v_lightmap_page));
vec3 light = vec3(1.0);
if (lightmap_on > 0.5) { light = <lightmap atlas sample>; }
light *= u_light_scale;
```

The two storage modes the reference can run in are:

- **atlas** (default; `lightmaps: true`): architectural vertex colours are the
  material factor `tint × directional face shade` with no baked light, the
  atlas carries the per-texel light, and `light` is the atlas sample;
- **vertex-lit** (`PLACES_NO_LIGHTMAPS=1`, or the atlas fallback): the bake is
  folded into each vertex colour by `shade(base, light) = clamp(base × light)`,
  every vertex carries `LIGHTMAP_NONE`, and `light` is exactly `vec3(1.0)`.

Stage 8 implements the vertex-lit mode. The bake is the same CPU code in both
modes; only the storage differs, and `LightmapMode::Off` always bakes with
`BakeConfig::HARD` (one visibility tap, 0.15 m prop-occlusion cell) whatever
the quality profile. That is what makes the Stage 8 image a faithful
reproduction of `PLACES_NO_LIGHTMAPS=1`, and what makes the lightmap stage a
storage change rather than a lighting change.

### 2.1 Light sources

A level authors lights in two places; both become the same neutral
`LightSource`:

- `ceiling_lights[]` — a fixture placement (`fixture` catalog id, `x`/`z`,
  `rotation_degrees`, `brightness`/`intensity`, `color`, `mount`, `y`,
  `range`, `falloff`, `enabled`, `emission`). The fixture family
  (`FixtureKind`) decides only the emitting rectangle and the visible mesh:
  fluorescent panel 0.6 × 0.3 m, round downlight 0.22 × 0.22 m, wall sconce
  0.20 × 0.09 m, flush mount 0.16 × 0.16 m.
- `props[].lights[]` — up to eight per prop, in the prop's local frame,
  with an explicit `shape` (point, rectangle or line), offset, rotation,
  colour, intensity, range and falloff.

A `LightSource` is a shape (`Point`, `Rect`, `Line` — a line is a thin rect),
a world position, a colour, an intensity clamped to `0..=8`, a range clamped to
`0.05..=64` m (default 6 m) and a falloff (`Smooth` — the historical
`(1-t)²(1+2t)` cushion — `Linear` or `Constant`). It is active when enabled,
with a positive intensity and a non-black colour. There is no maximum active
count because nothing is dynamic: the bake visits them all.

### 2.2 The baked equation

Per sample point and channel:

```text
room baseline   = clamp(0.50 · c + 0.10, 0.10, 0.60)
                  c = n / (1 + n), n = ln(1 + (power / area) · 500)
                  power = Σ intensity · sqrt(3.5 / ceiling_height) · colour
local pools     = clamp(0.45, Σ 0.42 · intensity · height_factor · falloff · visibility · colour)
opening blend   = 0.5 · smooth_falloff(d / 6) · (neighbour_baseline - own_baseline)
light           = clamp(baseline + pools + blend, 0.10, 1.0)
```

- `AMBIENT_LEVEL = 0.10` is the floor; an unlit room is dark by design.
- `BASELINE_MAX = 0.60` deliberately leaves the highlight headroom to the
  pools, because a pool is the only term a static occluder can remove.
- A room split by opaque internal walls gets one baseline per connected area;
  a doorway still blends the two areas through the aperture.
- Floor interfaces and ceiling bodies isolate storeys, so a fixture cannot
  light through a slab.
- Pool visibility is a binary (vertex-lit) or five-/nine-tap soft (atlas)
  test against the same wall solids, floor interfaces, ceiling bodies and
  prop-derived boxes the geometry and collision use.
- Openings transmit light through the hole they cut (the wall solid is
  removed); glass and grille panes do not have their own material alpha
  consulted, so a translucent pane transmits exactly like the opening.

Ambient, baseline and the pools exist **only in the CPU bake**. The shader has
no ambient, no light position and no attenuation input, so "changing the
lighting at runtime" means re-baking — exactly as in the reference.

### 2.3 The sheen

The only view-dependent term in the reference world stage:

```glsl
vec3 view = normalize(u_camera_pos - v_world_pos);
if (u_response_enabled > 0.5) {
    float facing  = clamp(abs(dot(normal, view)), 0.0, 1.0);
    float gloss   = 1.0 - u_roughness;
    float grazing = pow(1.0 - facing, mix(1.0, 16.0, gloss));
    float ahead   = pow(facing, mix(1.0, 24.0, gloss)) * gloss;
    sheen = u_specular * (grazing * 0.55 + ahead * 0.45) * light;
}
```

There is no light direction to place a real highlight; both lobes are scaled by
the `light` factor and neither invents a source. `u_roughness` is `1 - shine`
(or the legacy authored roughness, or a per-surface shine override), the
normal decode is the Stage 7 one, and the master gate is the material's
response bit — cleared for a material with neither normal map nor sheen, and
for the whole scene under Low.

## 3. What the wgpu port does

```text
LevelDef
   │
   ▼  build_level_geometry_timed_with_lightmaps(..., LightmapMode::Off)
LevelMesh (vertex-lit: colour = clamp(tint × face shade × baked light))
   │
   ▼  pack_world_ranges + WgpuWorldGeometry::upload  (Unorm8x4 colour)
WorldVertex.color ──► WGSL in.color
   │
   ▼
lit_display(base, in.color, light)       // display space, light == 1
   + surface_sheen(in, front_facing, light)
   │
   ▼  srgb_to_linear( ... )  ->  sRGB target
presented pixel == the reference's vertex-lit pixel
```

Measured on an isolation level with no props, fixtures, decals or glass, the
two renderers agree within 2/255 everywhere (the residue is the extra sRGB
round trip); on the canonical views the difference is dominated by the
unported geometry and fog. See §8.

- The level upload is the only build change. The textures, materials, alpha
  passes and world draw set are the Stage 5–7 ones, unchanged in kind. The
  vertex-lit bake *does* subdivide the lighting grid where the light varies (a
  per-vertex bake can only produce a gradient by emitting vertices), so the
  shipped demo's architectural range count grows (89 → 110 for Places Demo;
  the Stage 7 figure is recorded in
  [WGPU_MATERIALS.md](WGPU_MATERIALS.md) §21).
  That is the reference's own vertex-lit mesh — the OpenGL path builds the same
  one for `PLACES_NO_LIGHTMAPS=1` — not a wgpu regression.
- `surface_light()` returns the reference's vertex-lit `vec3(1.0)`. Stage 9
  replaces its body with the atlas sample and the atlas enable switch; Stage
  10 multiplies it by the dynamic probe. Nothing else has to move, because the
  factor is already used where the reference uses it.
- The camera uniform gains the world-space eye position because the sheen needs
  the same view vector the reference computes from `u_camera_pos`.

### 3.1 Colour space

> **Stage 10 note.** The sRGB texture decode and the compensating
> `linear_to_srgb` described below were measured as a real deviation: blending
> decoded values is a convexity bias against the reference's display-space
> blending, and it showed as a broad +1 display level on minified surfaces.
> Stage 10 returned base-colour textures to raw `Rgba8Unorm` and removed the
> in-assembly encode; the fragment now assembles in the reference's framebuffer
> space and only the sRGB surface converts. See
> [WGPU_STAGE10.md](WGPU_STAGE10.md) §2 and §7. The Stage 8 rationale remains
> below as the historical record.

The reference has no sRGB anywhere: its textures are sampled raw, its
framebuffer is not sRGB, and it computes `lit = tex × v_color × light` and
`color = lit + sheen` in raw display space. The wgpu pipeline samples an sRGB
texture and writes an sRGB target, so Stage 7 introduced two explicit IEC
61966-2-1 transfer functions and performed the material multiply in display
space (see [WGPU_MATERIALS.md](WGPU_MATERIALS.md) §11).

Stage 8 extends that decision rather than reversing it. The fragment stage now
assembles the **whole** un-shadowed world colour in display space and converts
once:

```wgsl
var color = srgb_to_linear(lit_display(base.rgb, in.color.rgb, light) + sheen);
if (all(in.color.rgb >= vec3<f32>(1.0)) && all(light >= vec3<f32>(1.0))
    && all(sheen == vec3<f32>(0.0))) {
    color = base.rgb;   // the Stage 6 byte-exact unlit path
}
```

Adding a display-space sheen to a linear-space lit term would be a different
image; a unit test proves the order matters, and a second one proves the
display-space product rounds to the reference's 8-bit value for every authored
texel byte across a grid of vertex colours (a hardware encoder can still differ
by one step at a rounding boundary; that is the measured ±1 speckle).

The whole contract assumes the colour target is an sRGB format. The renderer
logs one warning when an adapter offers only a linear surface format, because
no shader variant exists that could present the reference's display values
without the hardware's encode.

Alpha never passes through the transfer functions: it stays the reference's
straight `texel.a × vertex.a × opacity`.

The clear colour is the same class of decision: the reference clears its
non-sRGB framebuffer to the raw display value `(0.08, 0.08, 0.09)`. Stage 10
returns every raw offscreen target to that same display value
(`CLEAR_COLOR`); only the sRGB surface paths keep the linear form
(`CLEAR_COLOR_SRGB`), which presents the same background after the hardware
encode.

### 3.2 Camera uniform

| Offset | Field | Type | Meaning |
|---:|---|---|---|
| 0 | `view_projection` | `mat4x4<f32>` | the Places camera with the clip correction |
| 64 | `position` | `vec3<f32>` | world-space eye (`u_camera_pos`) |
| 76 | `_padding` | `f32` | explicit tail padding |

80 bytes, 16-byte aligned, pinned by tests on both sides. The bind group is
visible to the vertex and fragment stages now; it was vertex-only before.

### 3.3 Quality profiles

For a vertex-lit build the bake is `BakeConfig::HARD` under **both** profiles,
so the geometry and the light are identical; only the response gate differs:

| Profile | Baked light | Normal map | Sheen |
|---|---|---|---|
| Full | identical (`HARD`) | drawn | drawn |
| Low | identical (`HARD`) | gated off | zeroed (`specular = 0`) |

That is the reference's own Low behaviour for this mode. The atlas-mode
differences (tap count, prop-occlusion cell, texel density, page size) belong
to Stage 9, because they only exist when an atlas is baked.

## 4. Shadows

**The reference has no GPU shadow system.** There is no shadow render target,
no shadow camera or projection matrix, no depth texture sampled as data, no
comparison sampler, no PCF kernel, no caster list and no per-light shadow
pass. The only `glPolygonOffset` in the repository is the decal pass's
`(-1, -4)` depth bias, which has nothing to do with shadowing. This is not an
omission to be filled in by a faithful port: adding one would be new rendering
behaviour, not parity.

Every shadow in Places is a surface losing a baked pool:

- opaque wall solids block pools, with every opening kind (door, window, vent)
  cutting the hole it really cuts;
- room floors contribute zero-thickness interfaces and ceilings solid bodies,
  which is what stops light crossing a storey;
- each static prop's real (or placeholder) model is ground into oriented boxes
  that block pools and darken the prop's own contact area;
- alpha and blend state are never consulted: a cut-out grille pane and a
  translucent glass pane transmit light exactly like the opening they fill;
- trim (baseboards, thresholds) does not block; structural pieces (half walls,
  columns, archways, guardrails, stairs) do.

Stage 8 therefore completes the static-world shadow parity that *can* exist
today by delivering the baked light to the GPU. What remains:

| Item | Owner / status |
|---|---|
| per-texel shadow resolution and the soft five-tap penumbra | Stage 9 (lightmap atlas) |
| prop model drawing (its baked contact shadow already renders) | Stage 10 |
| dynamic objects (they cast no baked shadow at all, by the reference's design) | Stage 10 |

## 5. Known differences from the default reference capture

The default OpenGL capture runs the **atlas** mode plus fog, post-processing,
fixtures and props. A Stage 8 wgpu capture is the vertex-lit mode only, so the
two differ by design in these ways, all of which are named and owned:

| Difference | Cause | Owner |
|---|---|---|
| blockier light gradients on large surfaces | per-vertex light vs a 16 texels/m atlas | Stage 9 |
| sheen not attenuated by dark rooms | vertex-lit mode's `light == 1`; the reference's own `PLACES_NO_LIGHTMAPS=1` behaves identically | Stage 9 (atlas light) |
| fixture panels and their glow absent | fixture geometry is Stage 10 | Stage 10 |
| props absent (their shadows still darken the floor) | prop mesh pass is Stage 10 | Stage 10 |
| no distance fog | later presentation stage | later |
| no reflection response | Stage 9 | Stage 9 |
| no emissive signs or bloom | Stage 10 | Stage 10 |
| thin static surfaces sample a coarser mip than the reference | the wgpu texture cache has a mip chain and the reference samples without one (Stage 6's deliberate choice); the difference is confined to extreme minification, e.g. a ~6 px wall reveal | Stage 6 texture system |
| background clear colour | the reference clears its non-sRGB framebuffer to raw `(0.08, 0.08, 0.09)`; raw targets clear to that value (Stage 10) and sRGB surface paths to its linear form | closed in Stage 10 (Stage 8's linear-only clear was corrected) |

Every one of these was measured on the canonical views (see §8); the
static-architecture region of every view matches within a few display units,
and an isolation level with no props, fixtures, decals or glass matches within
2/255 except for normal-map minification (max 9, under 0.04 % of pixels above
2/255).

The deterministic comparison procedure is
`tools/bench/capture_baseline_views.sh` for both renderers and
`tools/bench/compare_captures.py` for the per-view numbers;
`tools/bench/compare_baseline.py` remains the byte-equality gate for the
OpenGL reference itself.

## 6. Resources and lifetimes

- No lighting resource is created per frame. The only new per-frame upload is
  the camera uniform (matrix **and** eye), written with `Queue::write_buffer`
  and skipped entirely while both are unchanged.
- No light buffer, light array, shadow target, shadow sampler or lighting bind
  group exists, because the reference model has none.
- The bake's per-level cost is unchanged from the reference's vertex-lit path;
  the wgpu renderer owns no `LightmapCache` (that is Stage 9's).
- The `PLACES_CAPTURE` read-back creates its offscreen texture and staging
  buffer for the capture call only and releases them afterwards.

## 7. Tests

`src/render/wgpu/world.rs` pins, besides the Stage 5–7 contracts:

- the camera uniform's 80-byte layout, its `position`/`_padding` offsets and
  the shader's matching field order;
- the camera-write predicate: the first frame writes, an identical value is
  skipped, and both a moved eye and a moved matrix are written (`camera_uniform_changed` is a pure function so the skip path is testable without a GPU);
- the `surface_light()` seam (the unit vertex-lit factor) and the absence of a
  realtime light array, light count or shadow sampler;
- the sheen equation's exponents, weights, gloss and response gate, with a CPU
  mirror checking head-on, grazing, dark-light and matte/glossy values, a
  second mirror re-deriving the formula independently on a tilted normal, and a
  third proving a decoded normal-map sample changes the lobe and that the
  shader consumes `material_normal`;
- the display-space assembly order, including a check that adding the sheen in
  linear space would visibly differ, and a per-byte check that the
  display-space product rounds to the reference's 8-bit value for all 256
  texels across a grid of vertex colours;
- the unlit bypass's three exact conditions;
- the Stage 8 build: `LightmapMode::Off`, no atlas, byte-for-byte the
  historical vertex-lit mesh, identical under Low and Full, with the light
  present (dimmed vertices and the subdivided lighting grid).

`src/render/wgpu/surface.rs` pins the clear colour as the linear form of the
reference's raw display value through the same IEC curve.

The bake itself is covered by the existing `src/lighting/**` and
`src/render/tests.rs` suites, which the Stage 8 change does not alter.

### 7.1 Coverage gaps, stated rather than implied

- The WGSL side of the uniform layouts is pinned by field-order source checks
  and by the GPU's own pipeline validation every run; a `naga` reflection test
  would pin the computed offsets in a unit test but needs a test-only
  dependency on the shader compiler, which was judged not worth the supply-chain
  surface for Stage 8.
- The end-to-end pixel claim is verified by the capture comparison procedure in
  §8, not by an automated test: it needs a GPU and two renderers. An automated
  isolation-level parity test (a custom no-prop level captured with both
  backends) is the smallest test that would close it, and is left as a
  follow-up.

## 8. Verification record (working tree)

Measured on the Stage 8 working tree (macOS, Apple Silicon, Metal backend;
`PLACES_RENDERER=wgpu` unless stated).

| Check | Result |
|---|---|
| `cargo fmt --all --check` | clean |
| clippy (`-D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo`) | clean |
| `cargo test --workspace --all-features` | 916 passed, 3 ignored, 28 failed — every failure is a pre-existing working-tree asset edit, below |
| `python3 tools/assets/validate.py` | 8 errors, all the three deleted glass PNGs |
| `python3 tools/textures/build.py --check` | 3 errors, the same three files |
| `python3 tools/props/build.py --check` | pass (33 props, decoded budget within limits) |
| `python3 -m unittest tests.test_package` | fails on the same three missing sheets |
| `(cd level-editor && npm test)` | 144 passed, 0 failed |
| `cargo build --release` | clean |
| `python3 -m unittest tests.test_compiled_build` | 7 of 8 pass; the packaged-build clean-log case fails on the same asset diagnostics |
| `python3 -m unittest tests.test_wgpu_bootstrap` | 12 of 16 pass; the four failures are the same missing sheets |
| OpenGL reference, committed asset tree | `compare_baseline.py`: 25/25 High and 25/25 Low byte-identical |
| wgpu vs OpenGL vertex-lit, 25 views × 2 profiles | mean channel difference 0.22–8.56; largest views are the ones the reference fills with props and fixtures (office 8.56, home main room 7.32, reception 6.88), smallest are architecture-only (ceiling panels 0.22–0.40, corridor 0.43–0.47); the difference image of every view is dominated by the Stage 10 geometry, see below |
| wgpu 200-frame run | no validation error, no warning, no panic |

### 8.1 The pre-existing working-tree asset state

The working tree carries an in-progress asset edit that is *not* part of Stage
8 and is not repaired here (textures and models are finished production assets
for this migration):

- `git status` deletes `assets/core/textures/glass/glass_clear_01.png`,
  `glass_dirty_01.png` and `glass_tinted_01.png` while `assets/catalog.json`
  still references them (8 catalog errors, 4 smoke-test failures, and the
  packaged-build clean-log case);
- 78 assets are modified, including `assets/core/textures/white_01.png`, which
  the committed revision stores as a 2x2 sheet and the working tree as a
  1024x1024 sheet. Three tests still assert the 2x2 contract
  (`assets::tests::the_shared_white_sheet_is_a_committed_opaque_white_png`,
  `render::tests::the_renderers_white_sheet_loads_from_the_committed_catalog_asset`,
  `render::wgpu::texture::tests::the_fallback_is_the_committed_two_by_two_white_sheet`);
- several prop GLBs load as invalid (`POSITION, TEXCOORD_0 and COLOR_0
  attribute counts differ`), which fails the prop-asset tests while the renderer
  degrades to the catalogue placeholder boxes exactly as designed.

None of these tests reads a Stage 8 code path: they are asset-state assertions,
they fail identically at the HEAD revision's asset tree for the white-sheet
contract, and they pass on the tree the migration's own captures are compared
against. The OpenGL byte-equality result above is the proof that no renderer
changed: captured against the committed asset tree, all 50 reference images are
byte-identical.

### 8.2 Visual comparison method

Because the default reference capture is the *atlas* mode plus fog,
post-processing, fixtures and props, the meaningful comparison is against the
reference's own vertex-lit mode:

```sh
PLACES_BASELINE_ASSET_ROOT=target/agent-work/stage8/asset-root \
    sh tools/bench/baseline_asset_root.sh
PLACES_RENDERER=wgpu PLACES_ASSET_ROOT="$PWD/target/agent-work/stage8/asset-root" \
    PLACES_CAPTURE_DIR=target/agent-work/stage8/wgpu-baseline-assets \
    sh tools/bench/capture_baseline_views.sh
PLACES_ASSET_ROOT="$PWD/target/agent-work/stage8/asset-root" \
    PLACES_NO_LIGHTMAPS=1 PLACES_NO_BLOOM=1 PLACES_NO_REFLECTIONS=1 \
    PLACES_NO_OFFSCREEN=1 PLACES_QUALITY=full \
    PLACES_CAPTURE_DIR=target/agent-work/stage8/opengl-vertexlit-direct \
    sh tools/bench/capture_baseline_views.sh
python3 tools/bench/compare_captures.py \
    target/agent-work/stage8/wgpu-baseline-assets \
    target/agent-work/stage8/opengl-vertexlit-direct
```

The difference image of the office view is black on every wall, ceiling and
floor pixel and bright exactly where the reference's desks, chairs and ceiling
panels are; the home main room differs on the fridge, lamp, couch, table and
rug; the pool differs on the guardrails, ladder, curtain and sign. Static
architecture differences are a low-amplitude fog term (a fraction of 1 % of the
range at interior distances).

The strongest form of the comparison is an isolation level: one room, no props,
no visible fixture, no glass, no decals, captured with both renderers in both
profiles. On it, wgpu matches the OpenGL vertex-lit reference within **max
2/255** (Low, no mip-magnified detail) to 9/255 (Full, normal maps at extreme
minification; under 0.04 % of pixels differ by more than 2). That is the
measured bound on the Stage 8 lighting and sheen math: the residue is the extra
sRGB decode/encode round trip and texture minification, not the light term.
That is the Stage 8 result: the light term matches; the unported geometry does
not.
