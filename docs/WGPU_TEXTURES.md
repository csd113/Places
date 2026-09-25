# wgpu textures (Stage 6)

Status: **Stage 6 is implemented in the working tree; Stage 7 added the material
system on top of it** (see [WGPU_MATERIALS.md](WGPU_MATERIALS.md)). The wgpu
backend turns Places texture assets into cached GPU textures with CPU-generated
mip chains, shared samplers, deliberate sRGB/linear formats and a deterministic
fallback. Every world draw samples its material's base-colour texture through
the UV the Stage 5 vertex carries, and Stage 7's material stage consumes the
same cache for normal maps through the `DataLinear` semantic this document
reserved. The OpenGL/GLES2 renderer remains the complete reference
implementation and the default (`PLACES_RENDERER` unset); its output is
unchanged.

This document is the Stage 6 handoff. It complements
[WGPU_BOOTSTRAP.md](WGPU_BOOTSTRAP.md) (lifecycle) and
[WGPU_WORLD_GEOMETRY.md](WGPU_WORLD_GEOMETRY.md) (geometry), and it is explicit
about the colour-space decision, because that is the part a later stage must
not rediscover.

## 1. Stage 6 scope

Implemented:

- decoded Places PNG assets as GPU textures, reusing the engine's existing
  decoder and session cache — the wgpu backend parses no files and no catalog;
- one renderer-owned texture cache keyed by semantic identity;
- CPU mip-chain generation, with every allocated level initialized;
- shared samplers: repeating linear, repeating nearest and clamped nearest;
- the player's `texture_filtering` setting applied live without re-uploading;
- a fallback sheet for surfaces with no resolvable base texture;
- minimal per-draw texture selection: `WorldDraw` carries the range's existing
  material key, which resolves through the existing neutral material table;
- texture-specific unit and runtime tests and one load-time diagnostic line.

Deliberately absent (Stage 7+): the material response (shine, roughness,
normal maps, specular), lighting, shadows, lightmaps, reflections, decals,
props, transparency/blending, alpha cut-out, emissive masks, texture arrays,
atlas conversion, compression, streaming, hot reload and PBR.
*(Stage 7 delivered the material response and transparency; lighting and the
rest remain absent. See [WGPU_MATERIALS.md](WGPU_MATERIALS.md).)*

## 2. Source texture pipeline

```text
level material id / pack material
        │  (engine, unchanged)
        ▼
LoadedLevel.materials : MaterialTable          src/materials/resolve.rs
        │  ResolvedTexture { key, origin, class, image: Rc<RawImage> }
        ▼
WorldTextures::resolve                         src/render/wgpu/world.rs
        │  resolve_base_texture(draw, MaterialRenderState, MaterialTable)
        │      -> ResolvedTexture for Floor/Ceiling/Wall
        ▼
TextureCache::get_or_upload                    src/render/wgpu/texture.rs
        │  fit_image(image, profile, class)     src/quality.rs
        │  mip chain: halve_image() per level
        ▼
GpuTexture { texture, view, 2x bind groups, meta }
        │  bound when a run of draws shares it
        ▼
world.wgsl fs_main: textureSample(base_texture, base_sampler, in.uv)
```

The decode is the existing one: `crate::materials::decode_png`
(`src/materials/image.rs`), which normalizes any PNG colour type to 8-bit RGBA
and caps each edge at `MAX_TEXTURE_DIMENSION` (1024). The wgpu cache consumes
the `Rc<RawImage>` the engine already decoded; it never opens a file. The
quality fit is the existing `crate::quality::fit_image`, so Full keeps the
native sheet and Low box-filters it to the profile's budget.

## 3. Semantic texture identity

> **Stage 10 note.** `BaseColorSrgb` is now `BaseColorDisplay` and uploads as
> raw `Rgba8Unorm`: the reference filters and blends display-space bytes, and
> Stage 10 measured the sRGB decode/blend/encode path as a broad +1 display
> level on minified surfaces. See [WGPU_STAGE10.md](WGPU_STAGE10.md) §2/§7.
> The key still separates colour from `DataLinear` data so a future format or
> conversion change cannot collide the two.

`TextureKey` (`src/render/wgpu/texture.rs`) is:

```text
logical texture id   e.g. core:tex_wallpaper_yellow_01
semantic             BaseColorDisplay | DataLinear
quality class        Surface | FixtureFace | DecalSheet | Prop | EmissionMask
quality profile      Full | Low
```

- The logical id is the session key the OpenGL backend also caches by
  (`ResolvedTexture.key`; pack textures are `pack:<namespace>:<path>`).
- The semantic captures the colour interpretation, so the same source image
  used later as a normal map cannot collide with its base-colour entry. Stage 6
  created only `BaseColorSrgb`; Stage 7's material normal maps are the first
  `DataLinear` entries, exactly as the variant and its format were pinned by
  the Stage 6 unit test.
- Class and profile capture every input the GPU realization depends on: the
  quality budget and the fitted dimensions.

Nothing in the key is a material instance, draw index or pointer: two surfaces
that reference the same texture share one entry, one upload and one pair of
bind groups.

## 4. Cache architecture

`TextureCache` is owned by `WgpuRenderer` (`src/render/wgpu/renderer.rs`) and
holds the bind group layout, the four shared samplers, two maps and the
fallback:

| Map | Contents | Lifetime |
|---|---|---|
| `persistent` | catalog and missing/diagnostic textures | renderer lifetime |
| `level` | `TextureOrigin::Pack` textures | one level (dropped by `begin_level`) |

`release_profile_textures` clears both maps; the loaded level's draws keep their
`Arc`s alive until `set_level` replaces them, so a frame between the release and
the rebuild still draws valid resources. There is no LRU and no eviction beyond
those lifetimes: the shipped catalog holds 38 texture assets, so a
renderer-lifetime map is bounded, and a level can only introduce pack textures,
which die at the next level.

`get_or_upload` returns `(CacheOutcome, Arc<GpuTexture>)`; `Uploaded` means this
call created the GPU texture and mip chain, `Reused` means the cache already
held it.

## 5. GPU formats

> **Stage 10 note.** Both classes now upload raw: `BaseColorDisplay` is
> `Rgba8Unorm` (see §3's note). The reference has no sRGB textures, so neither
> does the renderer; the sRGB surface is the single conversion point.

| Class | wgpu format | Colour intent | Usage |
|---|---|---|---|
| base colour (Stage 6; raw since Stage 10) | `Rgba8Unorm` | authored display values, sampled raw | `TEXTURE_BINDING \| COPY_DST` |
| normal maps (Stage 7) | `Rgba8Unorm` | numeric data, never gamma-converted | `TEXTURE_BINDING \| COPY_DST` |

No compression, no texture arrays, no view formats and no other format is
created. Every texture is `TextureDimension::D2`, one sample, one array layer.

## 6. sRGB / linear rules

> **Stage 10 note.** The two bullets below describe the Stage 6 sRGB round trip
> that Stage 10 removed. Filtering, blending and mip selection now happen in
> the reference's display space; the sRGB surface still receives one
> `srgb_to_linear` conversion in the world fragment stage. The measurements
> that drove the change are in [WGPU_STAGE10.md](WGPU_STAGE10.md) §7.

The OpenGL reference has no gamma handling anywhere: textures upload as
non-sRGB `GL_RGBA`, the framebuffer is not sRGB, and the shader multiplies in
display space. The wgpu surface is deliberately sRGB (Stage 4's
`select_surface_format`). Stage 6 resolves the difference without manual
correction:

- base-colour textures upload as `Rgba8UnormSrgb`, so sampling decodes to
  linear;
- the fragment shader writes that value to the sRGB target, which re-encodes
  it;
- for a direct unlit sample the two conversions are exact inverses, so the
  presented 8-bit value equals the authored texel.

There is no `pow`, no gamma constant and no brightness compensation in the
shader. The canonical check: an office floor region captured from the wgpu
window has mean RGB `(121.2, 105.3, 74.0)` against the authored
`carpet_beige_01.png` mean of `(120.9, 104.5, 73.0)` — a byte-level round trip,
not an approximation.

Consequences a later stage must respect:

- filtered sampling is not byte-identical with OpenGL: GL filters
  display-space values while an sRGB texture is filtered in linear space (the
  magenta-free, correct behaviour). Stage 6 parity is semantic, not pixel-exact.
- Stage 7's material math must happen in the linear space the sRGB texture
  provides. It cannot copy the reference's display-space arithmetic verbatim;
  the reference look must be reproduced deliberately.
- if a platform ever reports only non-sRGB surface formats, Stage 4's fallback
  selects one, and these textures would be written without the final encode.
  That is a pre-existing surface policy tradeoff, not a texture-layer decision;
  the verification host selects `Bgra8UnormSrgb`.

## 7. Upload path

The smallest robust upload path, with no staging framework:

```rust
queue.write_texture(
    wgpu::TexelCopyTextureInfo { texture, mip_level, origin: ZERO, aspect: All },
    &level_pixels,
    wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(width * 4), // tightly packed, unpadded
        rows_per_image: Some(height),
    },
    wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
);
```

`Queue::write_texture` stages the copy internally and does **not** require the
256-byte row alignment that `CommandEncoder::copy_buffer_to_texture` imposes
(the wgpu-types documentation and `wgpu-core`'s `write_texture` both state
this), so no padding is applied and none is needed. One texture allocation per
entry; every mip level is written before the texture is used.

## 8. Row layout

`row_bytes(width) = width * 4` for RGBA8. The tests pin the 96x64 diagnostic's
384-byte rows (not 256-aligned) as the case that proves no padding assumption
slipped in. Uploaded dimensions are exactly the fitted image's dimensions;
nothing is forced square or power-of-two, and the fallback's 2x2 sheet is a
one-level upload.

## 9. Mip policy

Every base-colour texture gets a full chain:
`mip_level_count = floor(log2(max(width, height))) + 1`, stopping at 1x1
(1024x1024 -> 11 levels, 2x2 -> 2, 1x1 -> 1, 96x64 -> 7, 3x3 -> 2). This
matches the reference's `glGenerateMipmap` on a repeating sheet. The fallback
sheet is the one deliberate exception: exactly one level, like the reference's
white sheet.

No level is ever allocated and left uninitialized: `GpuTexture::upload` writes
level 0 and each successive level in the same call. `TextureMeta` records the
allocated count and the resident bytes.

## 10. Mip generation

CPU, deterministic, display-space:

- `halve_image` averages blocks of up to 2x2 source texels, rounding to the
  nearest channel value. The output extent floors (`max(1, edge / 2)`), so an
  odd edge's final unpaired row/column is dropped, exactly like
  `glGenerateMipmap`'s floor behaviour: 3x3 -> 1x1 averages the top-left 2x2
  block, not all nine texels, and a constant image stays constant;
- alpha is averaged the same way and preserved, never discarded;
- no gamma conversion is applied, because the reference's `glGenerateMipmap`
  on a non-sRGB `GL_RGBA` texture also averages raw channel values. This is a
  parity choice, not a claim about ideal filtering; a colour-managed pipeline
  would average in linear space.

Unit tests cover exact 2x2 averages, odd edges, 1x1 idempotence, a constant
image's chain staying constant, non-square and odd dimensions, and the resident
byte count.

## 11. Samplers

Three shared samplers are created once with the device; every field is set
explicitly (no wgpu default is relied on), `compare` is always `None` and
`anisotropy_clamp` is always 1 (the reference has neither):

| Policy | Address U/V/W | Mag | Min | Mip | Used by |
|---|---|---|---|---|---|
| `RepeatLinear` | `Repeat` | `Linear` | `Linear` | `Linear` | base colour under the `linear` setting |
| `RepeatNearest` | `Repeat` | `Nearest` | `Nearest` | `Nearest` | base colour under the `nearest` setting |
| `ClampNearest` | `ClampToEdge` | `Nearest` | `Nearest` | `Nearest` | the fallback sheet |

These mirror the reference's `set_repeat_filter` (`LINEAR_MIPMAP_LINEAR` /
`LINEAR` or `NEAREST_MIPMAP_NEAREST` / `NEAREST`) and its clamped nearest
upload for the white sheet. `lod_min_clamp` is 0 and `lod_max_clamp` 32, so the
chain is never cut short by the sampler.

## 12. Repeat and clamp

Places bakes tiling into the vertex UV (`tiled_uv` divides world coordinates by
the material's `tile_metres`), so walls, floors and ceilings routinely sample
with UVs far beyond one repeat. Base-colour textures therefore **repeat**, and
a UV > 1 tiles instead of clamping into a stretched edge. Stage 6 applies no
UV offset or scale: the tiling is already in the geometry, and applying
`tile_metres` again would square it.

The fallback sheet clamps, matching the reference's white sheet, and the
diagnostic `core:tex_missing` sheet uses the repeating policy like any other
base colour (which is what the reference does with it).

The player's `texture_filtering` setting is applied live: `WgpuRenderer::
set_texture_filtering` only records the mode, and each draw binds the matching
bind group, so no pixel data is re-uploaded. The load-time diagnostic names the
active mode.

## 13. Fallback texture

The fallback is the committed `assets/core/textures/white_01.png` — the same
2x2 opaque white sheet the OpenGL renderer loads at startup and embeds for an
assets-less install. The wgpu module embeds its own copy of the bytes (a
backend may not import its sibling) and `src/render/tests.rs` pins the two to
identical pixels. It is decoded with the normal decoder, uploaded as a raw
`Rgba8Unorm` texture with exactly one mip level (the Stage 10 semantic change
applies to it too), and bound with the clamped nearest sampler. A corrupted install degrades to a generated 2x2 white fill,
never a failed start.

It is a dedicated resource, not a cache entry, mirroring the reference's
separate white sheet: if a level ever named `core:tex_white_01` as a material
texture, the resolution path would upload it as an ordinary repeating sheet
under its own key (the reference's catalog path does the same). No shipped
material does.

It is used for:

- an architectural draw whose material key is `MATERIAL_NONE` (an empty id);
- a material index outside the resolved table (a stale or synthetic table);
- the fixture luminous faces and material-less housings and the prop-fallback
  boxes that reach the world pass today (they resolve through the fixture-sheet
  and fallback paths, not a base texture).

A material whose *authored* texture is missing or fails to decode is a
different case: the engine's existing resolver already degrades the whole
material to the 64x64 magenta/black diagnostic texture (`core:tex_missing`)
and logs the error, exactly as the OpenGL path does. The wgpu cache uploads
that diagnostic like any other texture; the load-time line reports those draws
in its `missing` count so a broken asset is visible, not silent.

## 14. Bind group structure

- Group 0 is the Stage 5 camera uniform (`@group(0) @binding(0)`).
- Group 1 is `@group(1) @binding(0) texture_2d<f32>` and
  `@group(1) @binding(1) sampler`.

The group 1 layout is created once by `TextureCache` and shared by every world
pipeline rebuild, so a changed surface format never invalidates a cached bind
group. Each `GpuTexture` creates two bind groups at upload, so the player's
filtering setting is a handle swap; the fallback's two groups both use the
clamped nearest sampler (§11), every other texture's use the repeating linear
and repeating nearest samplers. There is no bindless descriptor array, no
texture array and no per-frame bind group creation.

The draw loop rebinds group 1 only when a run of draws changes texture. The
Stage 10 measured Places Demo frame records 150 draw calls and 171 texture
binds at 1280x720 Full (171 material changes).

## 15. World texture selection

Stage 5 deliberately dropped material state from `WorldDraw`. Stage 6 restores
exactly one field — `WorldDraw.material: MaterialIndex`, copied from the
neutral range's existing `SurfaceKey` — and nothing else:

```rust
pub struct WorldDraw {
    chunk, index_start, index_count, vertex_count, bounds, kind,
    material: MaterialIndex, // Stage 6: which base texture to sample
}
```

`resolve_base_texture` is the whole lookup: for `Floor`/`Ceiling`/`Wall` it
follows `MaterialRenderState.texture_slots[material]` into
`MaterialTable::textures`, and for `Light`, `PropFallback` and `Decal` it
returns `None` (those index spaces are later stages' and must never be read as
material slots). The lookup ignores every other material property: no normal
map, no mask, no shine, no roughness, no alpha, no reflection.

It is not a complete material object, and it is not intended to become one:
Stage 7 extends the draw or introduces its own material state deliberately.

## 16. Cache lifetime and reload behaviour

- A first load uploads each distinct base texture once (Places Demo: 26 unique
  textures over 105 draws, 26 uploads, 0 fallbacks).
- A reload of a level that references those materials uploads nothing: the
  measured second load reused 2 textures with 0 uploads and 0 fallbacks.
- `release_profile_textures` (a quality change) drops both maps; the next
  `set_level` re-fits and re-uploads at the new budget. Full fits to 1024 (145
  MB resident for the demo), Low to 256 (9 MB).
- Pack textures are dropped at the next level load; catalog and diagnostic
  textures persist for the renderer's lifetime.

## 17. Diagnostics

One line per level load (never per frame), alongside the Stage 5 world-upload
line:

```text
[wgpu] textures: 26 unique, 26 uploaded, 79 cache hits, 0 fallbacks,
0 missing of 105 draws (145402504 bytes resident, max edge 1024px, linear filtering)
```

`unique` counts distinct base textures the draw set uses, `uploaded` the GPU
uploads this load performed, `cache hits` the lookups the cache answered
without uploading, `fallbacks` the draws sampling the fallback sheet and
`missing` the draws sampling the diagnostic pattern. `RenderStats::texture_binds`
reports the frame's texture bind-group changes (Stage 6 fills it in; the other
material counters remain 0).

## 18. Known limitations

Intended, because Stage 7+ owns them: no full material response; no shine,
roughness or specular; no normal maps or tangents; no lighting; no shadows; no
lightmaps; no reflections (the wet deck samples its own albedo only); no decals;
no props or placeholder boxes; no emissive masks; no transparency, blending or
alpha cut-out, and the opaque Stage 6 pass writes alpha 1.0 while the texture's
RGBA8 alpha is preserved for later; no vertex-colour/material-tint multiply and
no baked face shading (both are material/lighting terms the reference applies
as `tex_color * v_color * light`).
*(Stage 7 delivered the material response, normal maps, per-surface overrides,
alpha classification and ordinary transparency; the lighting terms remain
absent. See [WGPU_MATERIALS.md](WGPU_MATERIALS.md).)*

Genuine, understood limitations of the current implementation: filtered output
differs from OpenGL where GL filters in display space; no anisotropic filtering
(the reference has none either); the fallback is a solid colour so its
clamp/nearest policy is unobservable; a non-sRGB surface format on some future
platform would skip the final encode (see §6).

## 19. Stage 7 handoff (delivered)

Stage 7 added base-material semantics (tint/vertex colour), shine, roughness
compatibility, normal maps, alpha behaviour, transparency, per-surface
overrides, fallback material behaviour and reflection eligibility **without
redesigning** the texture subsystem:

- the cache key already distinguished `BaseColorSrgb` from `DataLinear`, and
  Stage 7's normal maps upload through the latter with no cache change;
- normal maps are interned through the same `TextureCache::get_or_upload` with
  `TextureSemantic::DataLinear`, fitted with the material's `Surface` class;
- alpha survives decode, fit and the mip chain (averaged, not discarded), and
  the cut-out threshold sees the same coverage the reference does;
- samplers, wrap modes, filtering, mip generation, upload layout and the
  fallback stayed shared policies, not per-material objects;
- the sRGB/texture path is unchanged; Stage 7's material colour arithmetic is
  documented in [WGPU_MATERIALS.md](WGPU_MATERIALS.md) §11.

One deliberate resource difference on Low: a gated surface response means the
normal map is never uploaded (the reference keeps it resident but unbound).
The rendered policy is identical.

## 20. Verification record (working tree)

- `cargo test --workspace --all-features`: 900 passed, 0 failed.
- `cargo clippy ... -D warnings -D clippy::pedantic -D clippy::nursery`:
  clean.
- `python3 -m unittest tests.test_wgpu_bootstrap`: 11 passed, including the
  Places Demo texture count, profile fit, reload reuse, empty level,
  material-less fallback and missing-material diagnostic cases.
- OpenGL canonical captures: 25 Full + 25 Low byte-identical to the frozen
  baseline; hole check clean on both profiles.
- Colour round trip: captured office floor mean matches the authored PNG mean
  within 1/255 (see §6).
