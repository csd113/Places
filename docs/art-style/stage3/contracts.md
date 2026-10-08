# Stage 3 transport and surface contracts — 2026-10-08 UTC

This stage extends the [Stage 2 linear/HDR contract](../stage2/contracts.md).
Source PNGs, glTF models, concepts, fixture calibration, presentation exposure,
ambient/fill tuning, transport quality presets and atlas page budgets are unchanged.
The implementation advances solver revision **13** and geometry revision **5**;
no serialized chart/texel or probe representation is replaced.

## Compiler inputs and energy

The native control exposed a blocking omission in the prior compiler: preparation
used `MaterialTable::logical`, while reflection capture resolved the PNG images.
The material adapter's decoded architectural reflectance therefore did not reach
offline bakes. Stage 3 passes one resolved table to preparation and capture, sharing
decoded images through `Arc`; CPU-only compilation resolves the same table.
Architectural bounce uses tint × mean decoded colour, with alpha weighting; prop
bounce uses the existing clamped sheet centroid and imported material factors.
Stored lighting remains incident linear HDR, excluding receiver albedo. Numeric
alpha never receives sRGB conversion. Compiler dependency hashes already include
these PNGs and the catalog, so forced builds and normal package currency agree.

Finite sources keep their existing authored strength, range and falloff. Every
tap integrates its own visibility, alpha throughput, cosine and directional moment
before averaging. Normalizing the mean incoming direction previously overestimated
some broad-source cosines. Source size determines the tap positions and resulting
penumbra; a bounded receiver footprint integrates discontinuous coverage locally.
There is no new source, uniform lift, name-specific correction or bounce-count increase.

Diffuse orders read the previous order: `D + K D + K² D` for the existing Full
preset, `D + K D` for Medium. Sky enters the first order on escaping paths once.
The saved `indirect` component consequently includes sky and diffuse interreflection;
it cannot isolate a particular emitter or wall without an additional controlled
solve. Authored baseline fill remains a separately saved final stage. Direct
light and shadow coverage are not blurred by the diffuse denoiser.

## Visibility and transmission

| Surface contract | Static transport meaning |
| --- | --- |
| Opaque | Geometry blocks even if its colour PNG contains low alpha. |
| Cutout / MASK | Level-zero bilinear numeric PNG alpha × interpolated vertex alpha × material opacity is compared with the authored cutoff. Covered slats/leaves block; holes transmit. |
| Blend / BLEND | Each physical straight crossing multiplies throughput by `1 − coverage`; it does not contribute diffuse surface bounce. |
| Genuinely open aperture | No pane triangle; throughput is one unless real trim, walls or other geometry obstruct the path. |
| Water | The matched surface transmits. Existing per-channel vertical receiver-depth extinction applies once to direct, indirect and fill. Opaque basin skirts remain blockers. |

Architecture repeats UVs; model sheets clamp. Same-distance hits on a triangulated
shared edge count as one Blend crossing; distinct layers multiply independently.
The sidecar is aligned with scene triangles and validates image storage, UVs and
coverage factors. Sparse Blend BVH flags avoid scanning opaque-only subtrees.
Cache-connectivity queries retain opaque/MASK obstruction semantics; transport
energy queries additionally apply Blend throughput. Opaque backfaces still block
but never supply front-face diffuse radiance.

This is neutral straight transmission, without refraction, Fresnel, coloured glass
absorption, scattering, caustics or arbitrary volume-path absorption. The tinted
glass PNG affects drawn colour and numeric opacity; it does not dye transmitted
light teal. Water's diagnostic `receiver_water_attenuation` reports depth at a
finite ray endpoint, not absorption integrated along that segment. A missing image
in a deliberately logical-only test table means unit coverage, never an inferred
glass exception; production compilation resolves images.

Fixture housing and decals retain the established exclusion from transport. An
analytic fixture's own luminous face must not shadow its source. Author opaque
trim and decoration as real geometry when they should cast static shadows.

## Charts, physical support and roofs

Positive spans allocate `ceil(span × density) + 1` inclusive endpoint samples.
The plan carries physical density separately from rounded atlas dimensions, so
a chart cut cannot resize or rotate its sampling footprint. The canonical frame
depends on the oriented world plane. Supported segments cross compatible
coplanar triangles and T-junctions, clipping at actual gaps, folds, material/normal
changes and opaque geometry. True half-plane clipping prevents a numerical
expansion from pushing diagonal corner samples through a touching wall.

Direct integration scouts four footprint corners and uses bounded 2×2 or 4×4
coverage integration when visibility differs. Every tap contributes; unsupported
extensions clip to real support rather than disappearing from the integral.
Diffuse filtering uses a physical pitch across compatible chart joins, preserving
hard normals, different reflectance and obstruction. Gutters still dilate only
their own chart. At a genuine page-family limit, small prop charts may occupy
proven free architecture rectangles under the existing MAXRECTS policy; density,
padding and total page budgets are preserved.

Omitted-height walls resolve the actual touching closed roof, including a centre
plane just outside the room or inside adjoining open air. Profiles split at each
owner transition and gable ridge, and each span uses its own roof at both endpoints.
Collision uses the same linear span conservatively. Real internal gaps stay gaps;
outer corner extensions are bounded by actual wall thickness and floating-point
rounding. Explicit-height rigid walls keep exposed horizontal tops, subtracting
only actual coplanar closed flat-roof/floor/wall coverage. A nearby roof owner alone
cannot erase those caps.

Cap lighting hints use actual height containment, then the complete wall's
established parent owner. A low corridor's two-dimensional footprint cannot steal
a cap above its ceiling. Continuity assertions compare a shared oriented surface;
intentional cap/vertical-face normal and directional-shade differences remain.

Author one pane per aperture and avoid coincident duplicate walls. Roof geometry
and wall footprints must actually touch for ceiling-following support. Intended
hard folds, bevels, material changes and contacts remain visible. Single-sample
screen silhouettes and thin alpha mip behavior belong to Stage 5; dynamic object
probe placement/contact belongs to Stage 4. Stage 6 owns the full dependency-aware
incremental/cache system; this stage retains existing package dependency checks.

## Diagnostic scope

The existing Stage 1 native selector and offline dump separate direct, indirect,
filtered, filled/stored irradiance, chart IDs, density, normals and exact caster
queries. Stage 3 adds sample-pitch and material/alpha metadata, finite-segment
throughput and receiver-depth water attenuation. Optional per-order gather counts
distinguish escaping paths, opaque/MASK hits, backfaces, absent visible cache
support, nearest fallback and zero radiance. These counts add instrumentation
cost; ordinary profiling uses the normal compiler without the dump environment.
They do not turn cache-empty percentages into a measured energy deficit.
