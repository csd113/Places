# Stage 4 lighting contracts — 2026-10-08 UTC

This extends Stage 2 linear HDR and Stage 3 geometry/transport contracts. The
original hero, PNGs, concepts, material factors, exposure, sky, transport presets
and atlas budgets remain unchanged. Additive actor and fixture controls keep
`art_style_hero` and use the same compiler/material/renderer paths.

## Energy and sources

PLPF v3 preserves the combined probe field. An aligned selected-direct sidecar
stores incident linear RGB means and signed first moments for stable always-on
source IDs. Sampling uses the same accepted weights for combined and selected
coefficients and subtracts before directional reconstruction. Global direct,
unselected local direct, gathered diffuse/sky and authored fill remain in the
residual. Selected finite sources are evaluated once at actual entity fragments,
using authored source shapes, range, falloff, strength and world normals. Each
point/line/rectangle tap retains its own interpolated visibility and cosine.
No albedo, gamma or exposure enters probe storage; draw-time material and the
existing tone mapping apply once.

Eight source slots retain stable identities. Validated switchable fixtures reserve
slots because their direct energy is absent from base probes; their on/off state
invalidates entity payload caches. Always-on sources retain baked bounced energy.
Switchable entity response adds live direct only; switched diffuse bounce is
still absent from the base field. Static switch groups retain their prepared
layers. This difference in inputs must be identified in comparisons.

Legacy PLPF v2 retains its combined centre sample and does not add selected direct.
Readers reject malformed lengths, labels, source IDs, energy and residual moments.
Trusted constructed fields are validated before publication; per-frame sampling
is not a recovery mechanism for malformed packages.

## Spatial validity and transforms

Eight support corners are transformed into world positions. Each local axis is
inset from actual bind bounds by min(1 cm, 2% of its extent), retaining at least
96% of the axis and clearing assembly stop liners. The same support bounds are
uploaded for shader interpolation; exterior fragments clamp to their edge. Actual
mesh, collision, culling and contact bounds remain unchanged. This prevents lit
closed doors from taking every sample inside a surrounding frame.
Means/moments use compact normalized Shepard interpolation over the existing
1.5 m lattice support, with labelled air and real segment visibility. Connected
air does not allow a probe ray through short opaque dividers or furniture.
Missing corners can reuse compatible visible centre/corner support; unreachable
corners remain zero. Valid darkness stays zero. No ambient lift or model-specific
brightness correction is introduced. Shader interpolation uses local bounds,
while direct response uses transformed fragment position and oriented world normal.
Pose normals retain the established skin and inverse-transpose/normal handling.
Animation beyond bind bounds clamps this spatial fit; it is a bounded approximation.

## Casters and grounding

One shared finite-placement/animator/128-character claim plan controls native
spawning, voxel exclusion and retained bind-batch caster metadata. Fully claimed
moving actors do not remain immutable bind-pose casters or self-block probe rays.
Partial/overflow model groups retain existing static fallback behavior. PLMP v5
adds one strict boolean byte per batch; v3/v4 remain readable with conservative
caster flag true. Collision and existing model/vertex charges are preserved.

The player constructs an immutable exact triangle/alpha visibility scene once
from prepared geometry, plus cached model-local ray resources for every object
admitted by the existing 64-object dynamic scene. Current inverse transforms,
world bounds and object identity provide moving occlusion with self exclusion.
All transforms advance before visibility synchronization and receiver refresh.
Stationary receivers invalidate when a neighboring caster moves; unchanged
frames reuse their payloads. No runtime transport solve or per-frame BVH build
is performed. Stage 3 PNG opaque, cutout and Blend semantics are retained in
finite direct tap visibility; static water retains its existing attenuation.

Up to eight opaque movable subjects provide lightweight soft floor footprints.
Actual transformed bind bounds, authored local floor and dominant incoming direction
control the projection; conservative character culling padding is excluded.
Suspended subjects fade, entity bodies do not receive their own footprint, and
aggregate diffuse removal is capped at 22%. This is approximate grounding, not
mesh-silhouette shadow maps or general animated occlusion. It adds no shadow draw
or texture. Skinned bodies use these bounds footprints rather than full posed ray casters.
Static prepared atlases do not recompute their indirect field when a door moves.
Any remaining acceptance failures belong in this stage report, never in the
unrelated Stage 7 map ledger.

## Live resources and diagnostics

Requested texture/lighting quality changes no longer mutate resident material
and frame gates before staged resources commit. Preparation captures the complete
graphics request, including reflections; commit installs that captured request.
The next request remains pending. Texture quality and filtering retain independent
settings and resource keys. Spatial cache identity includes field, exact scene,
model transform, moving-caster revision and active selected/switchable source state.

The environment uniform is 5632 bytes: eight residual/visibility anchors, eight
finite sources and eight contact subjects extend the existing layout. Offset tests
pin CPU/WGSL agreement. New opt-in native views show actual valid/invalid probe
positions, accepted neighborhood links, entity direct and residual response.
Markers are bounded to 2048 lattice slots (recorded stride), depth-test normally,
and use the real white fallback PNG. They are feature-only geometry, not artwork.
Capture logs distinguish the legacy combined centre trace from actual uploaded
spatial anchors, per-tap visibility, source descriptors and binding state.

The bounded native JSON sequence executes ordinary spawn/move/despawn/fixture APIs
and writes genuine framebuffer PNGs at ready frames. It never overwrites captures.
It is inert outside the benchmark. Quality scripts use normal Settings setters
and complete regular staged package installs without restarting the player.
