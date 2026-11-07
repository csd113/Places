# Model shadows: current authoring convention

This is the practical default until a better measured realtime method is
available, not a claim that precomputation is the only possible solution.
[Renderer contracts](../RENDERER.md#76-shadows) remain authoritative for behavior.
The [dated catalog review](../reports/model-shadows-0.13.1-mac.md) and
[per-model inventory](shadow-inventory-2026-11-07.json) record evidence and gaps.

## Choose the correct path

| Model/use | Default and justified action |
| --- | --- |
| Opaque animated character | Ordinary environment/material lighting, animation, world/rigid visibility and independent flashlight casting. No realtime body self-shadow by default. Do not treat that intentional absence as a defective asset. |
| Reviewed deep interior cavity | Fixed mesh-attached attenuation can be acceptable inside a mouth. The current exception is the exact Tall Guest export only. Review opening/closing, extreme poses and lighting directions before adding another exception. |
| Exterior/body | Prefer no self-shadow to a dark patch that looks glued on while the light or pose changes. Do not mass-bake bodies or silently replace approved geometry/materials to manufacture a shadow. |
| Static opaque prop/architecture | Keep the existing offline triangle/alpha transport and prepared receiver lighting. Charted and dense indexed/probe-lit routes differ; do not promise identical self-shading at every quality. No character cavity bake is needed. |
| Rigid moving/physics/held prop, door, elevator | Keep actual rigid transforms, ordinary receiving-light visibility, independent flashlight casting and eligible floor grounding. Its own rigid object is excluded from the CPU receiver-visibility query; this is not posed-character realtime self-shadowing. |
| BLEND ghost, glass, water, floating/mounted part, overlay/effect | Apply actual alpha, support and mounting semantics. An absent opaque floor blob can be intentional. Mixed opaque/glass models and MASK models are eligible through their non-BLEND parts; a wholly BLEND ghost is not. |
| Trigger, audio, light, timer, spawn/sequence/volume record | No physical opaque body means no body self-shadow or grounding requirement. A visible fixture housing is a separate model. |

Keep the realtime implementation available under Graphics > Advanced:
`Real character self shadows (WARNING major performance hit)`.
`real_character_self_shadows` defaults to false, including older configs. Enabling
restores original character albedo/material response and eligible current-pose
opaque self-visibility. It is not a switch for static prop shadows or floor
grounding. BLEND/MASK do not use the opaque posed self-visibility query. Preserve
live toggling, saved preference and quality/reflection roundtrips.

## Grounding is not a silhouette cast shadow

The shared floor approximation is a subtle bounds-based diffuse footprint,
capped at 22% total loss and eight relevant footprints. It is not a full room-light
mesh silhouette, per-limb contact AO, a tabletop receiver, or a water-surface
shadow. It uses the highest authored walkable floor below the bounds top at the
bounds center, including supported slope/step offsets. Separation fades out at
0.5 m. Downward residual light uses centered grounding, not a projection metres
away. Normal animated actors use transformed bind bounds; ragdolls and balloons
have their specific current-bounds routes. Flashlight shadows are independent.

Author a sensible contact origin, real dimensions, normals, alpha and closed
physical geometry where appropriate. Check actual placement, not just the model
origin: a hanging sign, airborne fish or held object need not darken the floor.
Do not darken transparent regions or force blobs onto mounted parts. Generated
doors' frame and leaf are separate dynamic models; validate open/closed/rotated
transforms and glass separately, with a clear swing and correctly fitted opening.

## Replacement and acceptance checklist

1. Record the GLB hash, material/primitive order, rig, clips, sockets and bounds.
   Update the dated inventory or explicitly mark it historical when these change.
2. Do not reuse a cavity array merely because path or vertex count matches.
   Tall Guest's accepted GLB, generator hash guard, loader identity checks,
   primitive/vertex mapping, `cavity_visibility.bin` and its JSON provenance are
   one contract. Regenerate/review that contract after a relevant export change;
   never just loosen the hash check. Keep exterior values at unity and preserve
   original albedo/materials in realtime mode. See
   `tools/entities/bake_tall_guest_cavity.py` and the sidecar's reproduction recipe.
3. Inspect front/back and important cavities at useful scale. Exercise relevant
   idle/walk/open-mouth/extreme poses, attachments and hiding/culling/re-entry.
   Check default and opt-in under changed light direction, lights off, flashlight,
   Static/Full lighting and relevant reflection/bind-group paths.
4. Check grounded, moved/rotated, raised/held, absent/restored and real support
   placement. Use sparse controls so the eight-footprint budget is not mistaken
   for a missing asset shadow. Corroborate subtle RGB footprints with diagnostics
   and empty/raised controls. A model thumbnail does not prove installed placement.
5. Benchmark representative distinct risks, not every asset. Use matched camera,
   pose and quality, actual uncapped presentation, V-sync and limiters off, drawable
   identity, repeated opposite-order runs, means and tails. Preserve any visual
   tradeoff and cost, not just the fastest FPS. Keep expensive native/bake campaigns
   outside ordinary Rust tests; add a narrow regression only for an actual defect.

Current Tall Guest data covers 624 inner-mouth vertices (562 attenuated, 62 at
unity), with all 4,514 exterior/body vertices at unity. No other character received a new cavity/body bake
in the catalog review. Its earlier close view improved 96.7 to 424.9 FPS with the
approved default; this is a specific Mac workload, not a universal asset promise.
The review's smaller opaque rat also loses substantial throughput with realtime
self-shadowing. Neither polygon count alone nor an attractive realtime still is
enough to choose the default.
