# Stage 2 contracts — 2026-10-08

This is the authoring-to-display boundary implemented on Art-style. The hero's
source, geometry, asset PNGs, lights, weather, cameras and presentation constants
are unchanged. Stage 2 changes the interpretation and preservation of values.
[Asset specification](../../ASSET_SPECIFICATION.md#16-formats-and-colour) and
[renderer documentation](../../RENDERER.md#3-colour-space) are canonical companions.

## Colour and units

| Boundary | Meaning and conversion |
| --- | --- |
| Colour PNG / embedded model PNG | Ordinary sRGB RGB, straight alpha coverage. File gamma/ICC metadata is ignored; the resource role supplies the declaration. Real repository PNG artwork remains unchanged. |
| Normal PNG / emissive mask | Numeric UNORM channels; no sRGB decode. Normal RGB maps 0…1 to −1…1. Architectural masks multiply linear emission per channel; glTF emissiveTexture is an sRGB colour modulator. |
| Catalog tint, glTF baseColorFactor/COLOR_0, light colour, emissive factor | Linear numeric RGB factors. Existing numeric authoring defaults are retained; do not additionally decode these factors. |
| CPU albedo for transport | Decode texture RGB once; architecture uses tint × mean decoded texture, excluding face shade and receiver illumination. Model triangles use centroid vertex factor × clamped centroid UV sample. Reflectance remains in 0…1. |
| Solver / stored irradiance | Nonnegative linear HDR energy and signed direction moments; receiving albedo is excluded. Existing intensity units are relative engine units, not physical lux or nits. World coordinates and ranges are metres, Y up. |
| CPU vertex lighting | Existing bounded legacy distribution equations remain; resulting numeric light is linear. RGB product remains float through upload rather than clipping/quantizing to UNORM8. |
| Texture upload | Colour `Rgba8UnormSrgb`; numeric data `Rgba8Unorm`. Semantic, source/content revision, size class and quality participate in cache identity. Hardware decodes colour before bilinear/trilinear filtering. |
| Material shader | Linear albedo × linear light, plus shared sheen, reflection and emission. No irradiance soft clip, byte clamp or unlit bypass. Fog/storm authored display colours decode once before linear mixing. |
| Scene / emission / bloom / planar / probe | `Rgba16Float`, eight bytes per texel. Reflection prefiltering averages HDR without exposure/curve/clamping. |
| Presentation | Bloom add → exposure → existing shoulder in linear light → sRGB encoding → existing display grade/clamp. Exposure is 1.0 at all presets; High knee .75/saturation 1.03/contrast 1.02, Medium knee .75/identity grade, Low identity presentation with no knee/grade when bloom is off. Bloom strength .42 remains the independent setting. |
| Presented image / HUD / native capture | Encoded `Rgba8Unorm`. HUD remains authored display RGB. Screenshot readback copies these bytes. Final sRGB surface copy decodes then hardware-encodes to preserve already encoded bytes; raw 8-bit window fallbacks copy directly. |

The compatible stylized emission equation is factor × intensity × decoded base
colour × optional modulator; vertex emission replaces the factor for fixtures.
This retains the existing asset look rather than claiming full glTF PBR emission
semantics. The separate emissive pass still returns opaque alpha for blend
materials; Stage 5 owns coverage-aware bloom and cross-family transparency.

No display curve is baked into lightmaps, irradiance fields, reflection cubes or
source textures. Stage 5 can tune presentation without rebaking display changes.
Low's absence of a shoulder can intentionally clamp energy at the final display;
HDR intermediates still retain that energy. No screenshot-specific exposure,
ambient multiplier or per-model brightness correction is introduced.

## Geometry and normals

Positions, indices, winding, UVs and material boundaries remain asset-owned.
Missing glTF NORMAL means flat per-triangle geometric shading. Static receivers
and vertex-lit fallbacks duplicate corners as needed so shared source vertices
cannot average across a hard edge. Authored normals retain the asset's smooth
or hard choices. Dynamic meshes retain source normals instead of replacing them
with an up vector. Character posing uses the inverse transpose of the weighted
pose transform for authored normals; absent normals use posed geometry.

Normal transforms use inverse transpose; tangents use the linear object transform,
are projected against the shading normal and carry determinant/mirrored-UV
handedness. Singular pose normals fall back to geometric shading. Static GLB node
transforms already use inverse transpose; runtime environment now does too.
Per-triangle UV tangents preserve the existing low-poly convention. Architectural
normal maps use that numeric frame. glTF normalTexture import is still unsupported;
no generated detail or new tangent-map framework is added.

`WorldVertex` is 76 bytes: position 0, normal 12, UV 24, Float32x4 colour 32,
UNORM16x2 lightmap UV 48, page 52, tangent 56, handedness 68, padding 72.
`EnvironmentUniform` adds a 48-byte padded normal matrix at offset 2848, total
2896 bytes. Model placement JSON supports positive uniform scale; retained glTF
node/skin transforms and renderer model matrices support non-uniform transforms.
Rotation/non-uniform normal correctness is tested deterministically rather than
remodeling the hero. The matched static/entity chair remains the native comparison.

## Compact material model and compatibility

Architecture retains its established tint, shine (= 1 − roughness), specular RGB,
optional normal detail, emission and reflection flags. glTF scalar roughness and
metallic controls now feed the same sheen implementation on static, movable and
character routes. This is a compact stylized response, not full PBR: dielectric
sheen starts at .04, the metal contribution is .55 × base factor, and both are
attenuated by (1 − roughness). Metal tint is useful without adding a BRDF system.
Omitted glTF controls preserve matte legacy behavior (roughness 1, metal 0),
including the existing model toolkit's defaults. Scalar controls validate 0…1.

OPAQUE, MASK and BLEND classify consistently across all three mesh paths. MASK
uses the cutoff pipeline; BLEND uses straight alpha without depth writes and
sorts back-to-front within its draw family. Cross-family/intersection ordering
and full transmission remain later-stage work. Colour/mask alpha is never gamma
converted. Numeric factors and material response are shared; lighting inputs
remain static atlas versus bounds-centre prepared entity irradiance/fallback.
Stage 4 owns probe transport, spatial coverage and contact improvements.

Props record v4 stores specular RGB and roughness; v3 reads with explicit matte
response (zero specular, roughness 1). Probe positions v3 declares HDR cube storage (`probes-hdr` capability); v2 legacy RGBA8 cubes
are decoded to linear at load, preserving their already bounded historical data.
Their old clipping cannot be recovered, so final non-hero rebuilds belong to
Stage 7. Geometry revision 4 and solver revision 12 invalidate prepared bake
currency. No non-hero package is rebuilt in Stage 2.

## Sampling and antialiasing

Texture class budgets retain Low/Medium/High fitting behavior. Colour size fitting
and mip generation use decoded linear energy and alpha-weighted RGB, then encode
sRGB for storage; coverage is averaged separately. Black/white averages to byte
188 rather than 128; transparent hidden RGB cannot contaminate visible mips.
Odd colour edges include all source pixels. Numeric normal/mask mips retain raw
channel averaging and the shader normalizes reconstructed vectors.

World Texture Filtering stays trilinear at every level, requesting 4×/8×/16×
anisotropy, with supported-device fallback to 1× trilinear. UV derivatives select
ordinary texture LOD; explicit reflection LOD follows roughness. Architecture
repeats; model/fixture sheets clamp; sky repeats U and clamps V. The shared white
and HUD font fallback remain nearest with one mip. Lightmaps retain their fixed
clamped linear sampler and are independent of texture filtering and size quality.

The actual renderer is single-sample with no temporal or edge-AA resolve. Its
HDR targets, alpha blend and depth attachments retain compatible sample counts.
Texture shimmer is addressed through correct energy mips/anisotropy; silhouette
jaggies and lower-preset scene upsampling remain a precisely bounded Stage 5
presentation question. No new temporal architecture or blanket sharpening is
introduced. Transparent mips preserve averaged coverage; alpha-test silhouette
coverage at tiny sizes is still not a dedicated coverage-preservation algorithm.

## Boundaries for subsequent stages

Stage 3: receiver-grid seams on coplanar sofa triangles, light distribution,
transport quality and transparent transport. Stage 4: per-object probe coverage,
contact, moving/skinned illumination and transient graphics transitions.
Stage 5: exposure/shoulder/bloom/fog/reflection composition and targeted edge-AA.
Stage 7: non-hero prepared package rebuilds, intentional model scalar adoption,
source/catalog/fixture compatibility and final documentation reconciliation.
The hall resin table remains beyond its authored light's range; Stage 2 does not
mislabel that coverage choice as a conversion defect or brighten it artificially.
