# Stage 5 contracts — 2026-10-08 UTC

The normal renderer, material catalogue, prepared transport and package formats
remain the shared paths. No map/camera/asset-specific shader branch, new PNG,
model replacement, runtime texture drawing, reflection system or dependency is
introduced. [Authoring guide](../../MAP_AUTHORING_GUIDE.md) owns the schema.

## Presentation and encoding audit

Colour textures are hardware-decoded sRGB; normal/mask/alpha data remain numeric.
Material/light factors, static incident atlases, residual/direct probe payloads,
world composition and reflection targets are linear HDR. Scene, emission and both
blur images remain RGBA16F. Bloom is added before fixed exposure and the smooth
rational shoulder. One scale derived from the RGB maximum preserves highlight
hue; values below the default 0.75 knee retain their linear response. sRGB encoding
precedes restrained display saturation/contrast. The RGBA8 presented image and HUD
hold display values. The sRGB surface copy decodes only to cancel hardware encoding;
raw surfaces copy bytes. There is no automatic exposure, pre-lightmap shoulder,
per-model brightness override or second reflection tone curve.

Optional `environment.presentation` defaults to exposure 1, knee 0.75, saturation
1.03, contrast 1.02. Every quality uses these defaults, including Low/Medium;
previous quality-dependent grade/curve differences are intentionally removed.
Light/resource budgets and independent texture/filter controls remain unchanged.
Two bloom-off/on 32-byte uniforms replace six preset slots and update on authored
changes only. Final output still quantizes to eight display bits; no dithering or
antialiasing system is added. Native acceptance evaluates visible banding and edges.

Diagnostics map normals and HDR values into documented display RGB, then apply `srgb_to_linear(display)` in `visual_diagnostic_color`. The identity post path re-encodes once, so the inverse conversion cancels it: `E(D(M(I))) ≈ M(I)`. A mapped normal component of 0.5 therefore presents near 0.5, subject to eight-bit quantization. The original Stage 5 receipt description implying a displayed value near 0.735 was inaccurate; the shader conversion is unchanged. HDR views use their documented `HDR/(1+HDR)` mapping. Exposure, shoulder, grade, bloom, sky, decals and weather are bypassed for these diagnostics. Diagnostic `final` executes ordinary composition and requires separate normal-binary equality. Diagnostic display brightness is not irradiance.

## Selective bloom and coverage

The existing emission-only source excludes lit albedo and reflection. First blur
extraction uses a soft knee from 0.5–1.5 HDR, leaving emission at/below 0.5 outside
bloom. It preserves high HDR energy; the second blur never extracts again.
Strength is reduced from 0.42 to 0.22. Fog and storm transmission attenuate emitted
radiance once without contributing fog colour. Straight alpha includes texture,
vertex, material and instance coverage once. Nonemitting transparent panes also
render to attenuate previously drawn emission. Opaque and cutout depth tests remain.

World ranges, props, movable objects and characters share stable back-to-front
centre sorting in both scene and emission. This resolves family ordering, not
intersecting-triangle order: large coalesced ranges, intersecting geometry and
multiple transparent primitives within one object still require sensible authoring.
No sorting or content budget is weakened. Effects retain their established overlay
routes; this is not order-independent transparency or refractive ray tracing.

## Sky, global illumination and atmosphere

`sky.ambient_color` is optional linear RGB in 0–1; default [0.42,0.52,0.72].
Escaping-ray incident radiance is colour × `sky.ambient`, independent of the sky
sheet's visual brightness and presentation exposure. One common transport scene
feeds static atlases and entity probes; runtime never adds that dome fill again.
Existing moon/global directional visibility/cosine and practical-light residual /
selected-direct accounting remain intact. Low's historical vertex fallback does
not reproduce the prepared sky distribution. The equirectangular sky remains a
background, excluded from reflection captures; sheen receives environment light
through incident illumination, while bounded probe reflections capture geometry.

`environment.fog` authors the existing global distance/height atmosphere: sRGB
colour, density, reference height and bounded height gain. Omission retains the
shipped constants. Existing regional fog boxes inherit that colour unless they
author their own. Fog mixes with linear HDR before presentation. Existing severe
snow sightline extinction, shelters and storm-colour sky blending remain separate;
calm snow and aurora keep their existing artwork, deterministic seeds and budgets.
Fog is presentation atmosphere, not additional illumination energy.

## Water, snow and ice

Medium/Full water now uses normal incident-HDR floor charts instead of premultiplied
vertex illumination, so prepared shadow, sky and practical response reach diffuse
and sheen coherently. Low retains vertex sampling. Circular fans use folded quads
compatible with existing chart stamping and indexing. Rectangles/circles preserve
surface height, normals, material, volume shape, collision and swimming.

Optional `water[].attenuation_per_metre` (finite 0–16, default 0) resolves vertical
transmission to `(1-opacity)*exp(-coefficient*depth)`. Zero preserves exact old
coverage. Resolved collision payload layout is unchanged; opacity remains visual
metadata. This bounded approximation uses authored/resolved vertical depth, not
screen-space thickness, angled transmission or refraction. The basin remains
visible through ordinary straight-alpha blending and depth testing without depth
writes. No screen colour/depth copy or new full-size attachment is allocated.

Existing PNGs are unchanged. Catalogue water receives restrained probe reflection
strength 0.30; ice 0.28 with shininess 0.55; snow specular 0.08 / shininess 0.12.
Snow stays opaque diffuse, ice retains its 0.84 material coverage and traction,
water retains per-volume coverage and swimming. All use the surrounding lighting
and ordinary final exposure. These reusable material changes require nonhero
package adoption in Stage 7; no nonhero map is baked or visually tested here.

Geometry revision is 6 for prepared water charts/fan layout. Solver remains 15,
PLPF v 3 and existing reflection/atlas formats remain intact. New optional schema
fields serialize into normal package identity; earlier bundles retain compatible
old executables, catalogues and assets. Stage 6 owns dependency-aware cache work,
including avoiding presentation-only lighting invalidation where appropriate.
