# wgpu materials (Stage 7)

Status: **Stage 7 is implemented in the working tree.** The wgpu backend now
resolves every static world surface to the same material semantics the OpenGL
reference renderer uses — base texture, material colour factor, shine and legacy
roughness, normal maps, alpha classification, per-surface shine overrides,
fallbacks and reflection-eligibility metadata — and draws the opaque, alpha
cut-out and ordinary translucent architectural passes with the reference's
states and ordering. Lighting is deliberately absent: Stage 8 owns it.

This document is the Stage 7 handoff. It complements
[WGPU_BOOTSTRAP.md](WGPU_BOOTSTRAP.md) (lifecycle),
[WGPU_WORLD_GEOMETRY.md](WGPU_WORLD_GEOMETRY.md) (geometry) and
[WGPU_TEXTURES.md](WGPU_TEXTURES.md) (textures), and it is explicit about the
colour-space decision, because that is the part a later stage must not
rediscover.

## 1. Stage 7 scope

Implemented:

- the renderer-neutral resolved material state
  (`render::common::materials::ResolvedSurfaceMaterial`), folding the material
  table, the surface's shine override and the quality profile into one
  description using exactly the OpenGL draw path's rules;
- a material-only level build (`LightmapBuildOptions::material_colors`) whose
  vertex colours are the material factor `tint × directional face shade` with no
  baked light and no atlas, matching a lightmapped build's colours byte for
  byte;
- the wgpu material cache (`render::wgpu::material`): one 80-byte uniform and one
  pair of bind groups per distinct `(material index, shine override)` identity;
- normal maps through the Stage 6 `TextureCache` under the dormant
  `DataLinear` semantic, with the reference's white fallback and fetch gate;
- the complete material fragment math in WGSL: the display-space material
  multiply, the material normal decode and preparation, alpha with the
  material's opacity and cut-out threshold;
- opaque, alpha cut-out and translucent pipelines, the reference's pass order,
  two-sided alpha passes, and a per-frame back-to-front translucent sort;
- per-surface shine overrides reaching the GPU (and therefore the sheen and
  reflection weight);
- reflection-eligibility metadata (mode and weighted strength) stored in the
  GPU record, sampled by nothing;
- tests for resolution, layout, decode, frames, ordering, alpha, fallbacks,
  colour math and the runtime diagnostics.

Deliberately absent **in this Stage 7 document** (all delivered later):
every lighting term, lightmaps, shadows, reflection sampling/capture, planar
reflections, fog, emission and emissive masks, decals, props, fixture
rendering, emissive bloom, UI and post-processing. Stages 8-9 implemented all
of them; see [WGPU_STAGE9.md](WGPU_STAGE9.md) and
[WGPU_STAGE10.md](WGPU_STAGE10.md). **Stage 11 note:** the OpenGL/GLES2
renderer was removed from mainline and preserved at the `renderer-gles2-reference`
tag; the wgpu renderer is now the only implementation.

## 2. The source material model

A level references material ids; the engine resolves them through the catalog
(or a level pack) into a `MaterialTable`. The rendering-facing properties are:

| Property | Authored | Default | Consumer |
|---|---|---|---|
| base texture | catalog/pack `texture` | the diagnostic magenta pattern when unresolved | base-colour sample |
| tint | catalog/pack `tint` | `[1,1,1]` | the level build bakes it into the vertex colour |
| `tile_metres` | catalog/pack | 2.0 m | the build's world UVs |
| `shine` | catalog/pack | `DEFAULT_ROUGHNESS = 0.6` internally | sheen, reflection |
| legacy `roughness` | catalog/pack | — | kept verbatim as the internal roughness |
| `specular` / `specular_color` | catalog/pack | none / white | sheen colour |
| `normal_texture`, `normal_strength` | catalog/pack | none / 1.0 (cap 2.0) | material normal |
| `alpha_mode`, `opacity`, `alpha_cutoff` | catalog/pack | opaque / 1.0 / 0.5 | pass classification, alpha |
| `reflection_mode`, `reflection_strength` | catalog/pack | none / 0.45 | reflection-eligibility metadata |
| `emissive`, `emissive_intensity`, `emissive_mask` | catalog/pack | none | **not ported** (Stage 8+) |

`shine` is the author's glossiness (`0.0` matte, `1.0` extremely glossy); the
engine stores its inverse, `roughness = 1 - shine`
(`material_response::roughness_from_shine`). The catalog rejects authoring both
`shine` and `roughness`; a pack accepts both and lets `shine` win, a
pre-existing engine rule the wgpu port inherits unchanged.

## 3. Resolved material representation

`render::common::materials::resolve_surface_material(key, materials, table,
response_allowed)` is the one renderer-neutral resolution rule. For a Floor,
Ceiling or Wall key it produces:

```rust
pub struct ResolvedSurfaceMaterial {
    pub texture: Option<u16>,       // base-colour texture index, None = fallback
    pub normal: Option<u16>,        // normal-map texture index, None = gated off
    pub normal_strength: f32,
    pub specular: [f32; 3],         // zeroed when the response is gated
    pub roughness: f32,             // the shine override wins
    pub alpha: MaterialAlpha,
    pub reflection: MaterialReflection,
    pub response_enabled: bool,
}
```

The rules, in order:

1. A key without a level material, or a fixture/prop-placeholder/decal key,
   resolves to `ResolvedSurfaceMaterial::plain()` (fallback texture, no
   response, opaque, no reflection) — their own later-stage draw paths expect
   exactly that.
2. The response is live when the material authors a normal map or a sheen
   **and** the quality profile draws the surface response. A gated profile
   zeroes `specular` and hides the normal map; the albedo, alpha, tint and
   reflection *mode* are untouched.
3. `roughness` is the per-surface `SurfaceShine` override's inverse when one is
   authored, otherwise the material's own resolved roughness.
4. `reflection_strength()` is `specular × reflection.strength` — the weight the
   reference shader folds into `u_reflect_strength` — so a gated profile zeroes
   the reflection weight with the sheen, exactly like the reference.

The launcher's per-frame `reflections_enabled` switch and the reflection
routing/active-plane selection are **frame-level** concerns and deliberately not
part of the resolved material: Stage 11 applies them when it renders.

## 4. Per-surface override precedence

Every override resolves at geometry-build time into the neutral
`SurfaceKey { kind, material, shine }`; the wgpu draw carries that key's
`MaterialIndex` and `SurfaceShine`, and the material cache keys on them. The
carriers and their precedence:

| Carrier | Slot | Rule |
|---|---|---|
| `defaults.wall/floor/ceiling` + `*_shine` | Wall/Floor/Ceiling | fallback for surfaces with no own material |
| `rooms[].material` + `shine` | Floor | the room's floor cells |
| `rooms[].ceiling_material` + `ceiling_shine` | Ceiling | the room's ceiling |
| `walls[].material` + `shine` | Wall | wall body, sills, headers, reveals |
| `walls[].faces[name]` + `face_shine[name]` | Wall | the face's own shine wins; else the wall's shine applies to faces drawing the wall's own material |
| `walls[].openings[].glass` + `glass_shine` | Wall | the pane (a wall surface) |
| `floor_patches[]` + `shine` | Floor | latest patch wins |
| `floor_regions[]` + `shine` | Floor | regions win over patches |
| `floor_regions[].edge_material` + `edge_shine` | Wall | transition skirts |
| ramp / stair / half wall / column / archway / guardrail / threshold / baseboard `material` + `shine`, plus each piece's end/cap/riser/side/post/reveal fields | Floor or Wall as documented in `MAP_AUTHORING_GUIDE.md` | the piece's own override wins; a piece with a different material but no own shine keeps that material's default |

Two surfaces with the same material and different shine resolve to different
GPU material states. Two surfaces with the same `(material, shine)` share one
uniform and one pair of bind groups.

## 5. Material identity and cache key

```rust
pub struct MaterialKey { pub material: MaterialIndex, pub shine: Option<SurfaceShine> }
```

`material_identities(draws)` assigns each draw its slot in first-use order;
`WorldMaterials` creates one `GpuMaterial` per slot. The key is not a source
material id alone: a per-surface shine override changes the resolved roughness
and therefore always earns its own state. `MaterialIndex::MATERIAL_NONE` is a
valid key component (the plain state, fallback texture).

Lifetimes match the reference's texture lifetimes: material GPU records are
created once per level load and dropped with the level. Normal-map textures are
owned by the Stage 6 cache (renderer-lifetime for catalog assets, level-scoped
for pack assets) and kept alive additionally by the material's `Arc`.

## 6. GPU material layout

`MaterialUniform`, 48 bytes, `#[repr(C)]` + `bytemuck::Pod`; WGSL declares the
same field order:

| Offset | Field | Type | Meaning |
|---:|---|---|---|
| 0 | `specular` | `vec3<f32>` | sheen colour (Stage 8 consumes; recorded now) |
| 12 | `roughness` | `f32` | `1 - shine`, or the legacy value |
| 16 | `normal_strength` | `f32` | normal-map `xy` scale |
| 20 | `alpha_cutoff` | `f32` | cut-out discard threshold |
| 24 | `opacity` | `f32` | alpha multiplier |
| 28 | `flags` | `u32` | see below |
| 32 | `reflection_strength` | `vec3<f32>` | `specular × authored strength` |
| 44 | `reflection_mode` | `u32` | 0 none, 1 probe, 2 planar |

Flags, explicit bits with tests:

```text
bit 0  MATERIAL_FLAG_NORMAL_ENABLED         the normal map is bound and may be sampled
bit 1  MATERIAL_FLAG_RESPONSE_ENABLED       the profile draws the surface response
bit 2  MATERIAL_FLAG_REFLECTION_ELIGIBLE    eligible for a future reflection
```

No other property is stored: no lights, no shadows, no lightmap pages, no probe
matrices, no reflection textures. Unit tests pin the size, every offset, the
flag bits and the WGSL struct order.

## 7. Bind groups

| Group | Binding | Resource | Lifetime |
|---:|---|---|---|
| 0 | 0 | camera uniform (view-projection) | renderer |
| 1 | 0 | base-colour texture | texture cache |
| 1 | 1 | base-colour sampler (linear/nearest) | renderer |
| 2 | 0 | material uniform | level |
| 2 | 1 | normal-map texture (or the white fallback) | texture cache |
| 2 | 2 | normal-map sampler | renderer |

Group 2's layout is created once with the device and shared by every pipeline
rebuild and material binding. Each `GpuMaterial` creates two bind groups at
level load — one per filtering mode — so the player's filtering setting is a
handle swap. No bind group, buffer or pipeline is created per frame, and the
material uniform is written once at creation.

A material without a normal map binds the shared white fallback with the
reference's clamped nearest sampler and bit 0 clear, exactly like the OpenGL
path; a real normal map follows the player's filtering setting (repeat, mips,
linear or nearest).

## 8. Base textures

Unchanged from Stage 6: `Rgba8UnormSrgb`, CPU mip chain, repeat, the shared
linear/nearest samplers, the committed white fallback and the diagnostic
pattern for broken materials. Stage 7 only consumes the neutral resolver's
texture index through the same `TextureCache::get_or_upload` path, so a surface
that resolved a texture under Stage 6 uploads nothing new.

## 9. Normal maps

- Resolved by the engine into `MaterialTable::textures` with
  `TextureClass::Surface`, and interned by the wgpu cache under
  `TextureSemantic::DataLinear` (`Rgba8Unorm`, no sRGB decode).
- Same repeat + mip + user-filter sampler policy as the reference's material
  texture class; the CPU mip chain is the same averaging, with no
  normal-specific renormalization (the reference has none either; the shader
  renormalizes after sampling).
- A material that authors no normal map binds the white fallback with the fetch
  gate off and uses the geometric normal. A declared-but-unresolvable normal
  map degrades the whole material to the diagnostic pattern in the engine's
  resolver, cleared of response — the wgpu path sees `normal: None`.
- The base and normal textures of one logical id never collide: the cache key
  includes the semantic, pinned by a Stage 6 test.

## 10. Normal-map coordinate conventions

The WGSL reproduces the reference fragment stage:

```wgsl
sampled = textureSample(normal_texture, normal_sampler, uv).xyz * 2.0 - 1.0;
scaled  = vec3(sampled.x * normal_strength, sampled.y * normal_strength, sampled.z);
normal  = select(-normal, normal, front_facing);          // geometric, flipped for a back face
tangent = normalize(world_tangent - normal * dot(normal, world_tangent));
bitangent = cross(normal, tangent) * handedness;
normal  = normalize(tangent * scaled.x + bitangent * scaled.y + normal * scaled.z);
```

- **No green-channel flip**; the sign lives in the per-vertex handedness
  attribute, exactly like the reference.
- Strength scales `xy` only, before the final normalization; the default is 1.0
  and the cap 2.0 (a pack's parser silently caps at 1.0, a pre-existing engine
  quirk).
- The world vertex now carries `tangent: Float32x3` and `handedness: Float32`,
  taken unchanged from the neutral `Vertex` (computed by
  `compute_surface_frames` from the geometry's winding and UV derivatives).
- No production pipeline culls (the reference never culls); every pass gets
  `front_facing` from the rasterizer and the normal preparation happens before
  the tangent Gram-Schmidt, matching the reference's order. Stage 7's
  back-face-culling sentence was corrected in Stage 10.
- The decoded normal feeds the Stage 8 sheen and the Stage 9 reflection/mirror
  response.

## 11. Material colour-space model

> **Stage 10 note.** Base-colour textures are raw `Rgba8Unorm` again: Stage 10
> measured the sRGB-decode round trip as a convexity bias against the
> reference's display-space blending and filtering, and the whole fragment
> assembly now happens in display space with the sRGB surface as the only
> conversion. The "known unavoidable difference" below (blending in linear
> space) no longer applies. See [WGPU_STAGE10.md](WGPU_STAGE10.md) §2 and §7.

The OpenGL reference has no sRGB anywhere: its RGBA8 textures are sampled raw,
the framebuffer is not sRGB, and the shader multiplies display-space values
(`lit = tex_color.rgb * v_color.rgb * light`). Stage 6 deliberately samples
`Rgba8UnormSrgb` textures and writes an sRGB surface, which is exact for a
direct unlit sample but not for a multiply: multiplying in linear space would
diverge from the reference by up to ~24 display percentage points.

Stage 7 therefore reproduces the reference arithmetic **in display space**,
with two explicit IEC 61966-2-1 transfer functions in WGSL:

```text
hardware sRGB decode            linear
    -> linear_to_srgb           reference display-space texel
    -> × vertex colour          reference display-space material multiply
    -> srgb_to_linear           linear
    -> hardware sRGB encode     presented display value == reference product
```

- An all-white vertex colour bypasses the round trip, so Stage 6's byte-exact
  unlit path cannot regress.
- Alpha never passes through the transfer functions; it is the straight scalar
  product `texel.a × vertex.a × opacity`.
- There is no brightness hack, no arbitrary gamma and no `pow` outside the two
  documented transfer functions.
- Known unavoidable difference: the sRGB target blends in linear space, while
  the reference blended display-space values. The factors and equation are
  reproduced exactly; only the space the hardware blends in differs, the same
  class of accepted difference Stage 6 documented for filtering. Linear-space
  filtering of the sRGB base texture is likewise inherited from Stage 6.
- The vertex colour is uploaded as `Unorm8x4` through the neutral
  `quantize_unit`, exactly the reference's 8-bit quantization.

Unit tests implement both transfer functions on the CPU, prove they are
inverses over all 256 authored bytes, prove the display-space product matches
the reference for white/black/mid-grey/tinted cases and prove the linear-space
product demonstrably differs.

## 12. Shine

`shine` and its inverse are computed entirely by the engine before wgpu sees
anything: `roughness = 1 - shine` or the legacy authored roughness, clamped to
`0.0..=1.0`. The uniform carries the resulting roughness. Stage 7 renders no
specular highlight — there is no light to place one — but the value is resolved,
uploaded and tested, ready for Stage 8's sheen.

## 13. Roughness compatibility

Legacy `roughness` materials keep working unchanged: the catalog's `roughness`
value is the internal value (not inverted), `shine` and `roughness` together are
a catalog error and a pack shines-wins case, and the default when neither is
authored is `DEFAULT_ROUGHNESS = 0.6`. A level's per-surface `shine` override
quantises to whole percent (`SurfaceShine`) and wins over the material's value
for both the sheen and the reflection weight. No authoring changes were made and
no level or material JSON was touched.

## 14. Alpha modes

Classification is the neutral `batch_pass_for` rule, unchanged from the OpenGL
pass structure:

| Material | Pass | Pipeline state |
|---|---|---|
| `opaque` (default) | opaque | depth write on, no blend, back-face culled |
| `cutout` | cut-out | depth write on, no blend, `discard` below `cutoff`, two-sided |
| `blend`, `opacity > 0` | translucent | `SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`, add, depth test LEQUAL, **depth write off**, two-sided, sorted |
| `blend`, `opacity == 0` | opaque | the reference's own classification; drawn as a normal opaque surface |

Only Floor/Ceiling/Wall ranges with a level material can be cut-out or
translucent; fixtures, prop placeholders and decals are opaque by kind and stay
out of the world pass. The alpha formula is the reference's
`texel.a × vertex.a × opacity`; static vertices carry alpha 1, preserved in the
`Unorm8x4` upload.

## 15. Cut-out

The cut-out pass is a separate fragment entry point (`fs_cutout`) and pipeline,
not a branch in the opaque shader, because a `discard` disables early depth
testing for every draw that uses the program. The condition is the reference's:
`alpha < cutoff` discards (strictly less; equality is kept), with the cutoff
from the material (default 0.5). Depth writes stay on, blending off. The level
build's alpha-averaging mip chain is unchanged, so distant grille texels shrink
or grow exactly as they do in the reference.

## 16. Transparency

The translucent pipeline reproduces the reference: straight alpha, no
colour × alpha in the shader, depth test on, depth writes off, and the pass runs
after the opaque and cut-out passes. The blend state is written out explicitly
as `SrcAlpha`/`OneMinusSrcAlpha`/`Add` for **both** the colour and alpha
channels: the reference's single `glBlendFunc(SRC_ALPHA,
ONE_MINUS_SRC_ALPHA)` applies the same factors to both, while wgpu's
convenience `BlendState::ALPHA_BLENDING` uses `ONE` for the alpha channel. A
unit test pins the explicit state. Translucent draws are ordered per draw
(never per triangle) by the squared distance from the camera to the draw's AABB
centre, farthest first, with stable ties preserving the packed draw order — the
reference's own sort. The camera position comes from the frame, and the sort is
the one small per-frame CPU cost Stage 7 adds.

The reference never enables face culling, and a window pane is a legitimate
two-sided surface (drawn from both rooms). The wgpu alpha passes therefore use
`cull_mode: None` with the `front_facing` normal flip, while opaque architecture
keeps Stage 5's deliberate back-face culling. Special glass passes, refraction
and reflective glass do not exist in the reference and are not invented here.

## 17. Fallback behaviour

| Situation | Result |
|---|---|
| no material / `MATERIAL_NONE` | white fallback sheet, plain state (no response, opacity 1, cutoff 0.5) |
| unknown material id or unresolved texture | the engine's magenta diagnostic texture; the material's optional terms are cleared, so wgpu sees a plain opaque state |
| material with no normal map | white fallback bound, fetch gate off, geometric normal |
| declared-but-unresolvable normal map | whole material degrades to the diagnostic, response cleared |
| stale normal index | the fallback is bound with the gate off; never an undefined sample |
| legacy material with no shine | no sheen (`specular = 0`), roughness 0.6, no reflection |
| absent alpha fields | opaque, opacity 1, cutoff 0.5 |
| an empty level | no materials, no draws; diagnostics report zeros |

The white fallback and the magenta diagnostic stay distinct, exactly as Stage 6
established.

## 18. Reflection eligibility

`MaterialReflection` (mode and strength) is resolved by the neutral layer; the
GPU record stores `reflection_mode` and
`reflection_strength = specular × authored strength`, and bit 2 of `flags` is
set when the result can change a pixel. **No reflection resource exists**:
there is no probe cubemap, no planar target, no reflection sampling and no
Fresnel application. The player's reflections switch is not part of the
resolved material; Stage 11 applies it together with routing and the active
plane. A profile that gates the response zeroes the reflection weight, exactly
like the reference.

## 19. Quality-mode differences

- Full draws the surface response (normal map + sheen); Low gates it:
  `response_enabled = false`, `specular = 0`, normal fetch off, reflection
  weight zero.
- Low's texture budgets still apply to base textures (unchanged from Stage 6).
- One deliberate resource difference: because the response is gated off before
  resolution, the wgpu backend does not upload a normal-map texture at all on
  Low, while the reference keeps it resident-but-unbound. The rendered policy
  (geometric normal, no sheen) is identical; only the wasted upload is avoided.
  A live Low→Full switch releases the profile textures and re-resolves, so the
  map appears.
- Geometry, draw order, pass classification, alpha, tint and tiling do not vary
  by profile.

## 20. Intentionally deferred Stage 8+ behaviour

Status after Stage 8: the lighting and sheen entries below are delivered — the
wgpu build is now the reference's vertex-lit build and the world fragment stage
assembles `lit + sheen` in display space (see
[WGPU_LIGHTING.md](WGPU_LIGHTING.md)). What remains:

- **Lightmaps** — no atlas, no page selection. Stage 9. Note that the *light*
  is no longer absent: it arrives per vertex, exactly as the reference's
  `PLACES_NO_LIGHTMAPS=1` mode renders it.
- **Shadows** — the reference has no GPU shadow system; the bake's static
  occluder shadows reach the image through the vertex colours. Per-texel shadow
  resolution and the soft penumbra are Stage 9.
- **Reflections** — eligibility metadata only.
- **Emission and emissive masks** — resolved by the engine, not uploaded or
  evaluated; they belong with the emissive/bloom stage.
- **Fog, decals, props, placeholders, fixtures, UI, post-processing** — absent
  as in Stage 5/6.

## 21. Verification record (working tree)

- `cargo test --workspace --all-features`: 933 passed, 0 failed, 3 ignored
  (includes the Stage 7 material, layout, decode, ordering, colour-math,
  vertex-frame, pipeline-state and fallback tests).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings
  -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo`:
  clean.
- `python3 -m unittest tests.test_wgpu_bootstrap`: 14 passed, including the
  material line's pass-count invariant, the Full/Low response gate and the
  reload reuse case.
- OpenGL canonical captures: 25 Full + 25 Low byte-identical to the frozen
  baseline; hole checks clean on both profiles.
- Places Demo material resolution (Full): 30 unique base textures, 32 resolved
  material states, 21 with the response enabled, 4 reflection-eligible,
  3 normal-map states (2 uploaded, 1 cache hit), 84 opaque / 1 cut-out /
  4 translucent of 89 draws. Low: the same geometry and passes, 0 response,
  0 reflection-eligible, 0 normal maps bound, 10.5 MB of textures against
  Full's 167.7 MB.
- Visual spot checks: reception (wallpaper/carpet/ceiling), the office window
  (dirty glass blending over the pool room) and the pool side looking back
  through the pane — textures, alpha and two-sided drawing all correct, with
  no lighting (expected). The wgpu backend has no screenshot path, so the
  colour equation is proven numerically by the unit tests rather than by a GPU
  capture.
