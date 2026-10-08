# Dependency plan for Stages 2–7

This is Stage 1's recommended allocation of the observed gaps. Later stage leads
must read the actual numbered request and preserve these dependencies; this table
does not prescribe an algorithm, sample count, atlas density or budget increase.
All views below refer to the unchanged [hero manifest](hero-manifest.json).

| Stage / gap | Evidence, likely cause and confidence | Expected benefit; risk/cost | Reproducible verification |
| --- | --- | --- | --- |
| **2 — color and energy contract** | Current `render/wgpu/texture.rs::TextureSemantic::format` uses raw UNORM; mips average raw channels. `render/common/light_transport.rs` samples the same albedo for bounce reflectance. `world.wgsl::surface_light` compresses HDR light before albedo; `postprocess.rs` and `reflections.rs` use RGBA8. Hero lamp/fixture detail and gray cream fabric are visible symptoms, not proof of a gamma error. **High** confidence in the contract; **medium** in the share of the visual gap. | Predictable color, bright source detail and a more coherent warm/cool balance. Palette/bounce/emission/alpha changes can affect every theme. Medium integration cost; measure target memory and GPU work before format changes. | Neutral and colored existing swatches at unit/fractional illumination; bright textured source ramp; alpha overlap; matched room/window. Preserve UI, fixture artwork and all seven immutable concepts. Document the chosen color/albedo/output contract, then update fingerprints and guides. |
| **3 — illumination distribution and reconstruction** | Hero window stays dark under ambient .22 and a supported moon; Low is more readable. Demo sky ambient is 0 and has no global source. `chart_receives_fill` excludes props; `SurfaceFilter::across_edge` excludes prop boundaries. Historical `lighting-quality-pass.md` retains refrigerator discrepancy .0129529 linear RGB; current hero sofa shows diagonal gradients. **High** source confidence; **medium** attribution of each visible gradient. | Softer plausible broad illumination and coherent model faces without erasing recess/contact shadows. Costs include rays, packing, field storage and bake time; existing demo Full already binds the eight-page policy. | Export direct/indirect/filtered/fill contributions, real charts and emitter/caster geometry for hero room/contact/window. Test real allocator dimensions under arbitrary segmentation, not only manually matching grids. Separate missing authored energy from processing loss. Retain sealed dark spaces and supported recovery behavior. |
| **4 — openings, receiver geometry and shadows** | No new physical hero opening crack or corner leak is established. Source has shared solid-slice ownership, floor boundary and gable-eave handling. Production grids use `ceil(length*density)` despite inclusive endpoints. Direct coverage refines detected changes only; thin casters may be missed. Prop LOD drops for a transformed AABB axis >3 m. **High** limits/coverage confidence; **unconfirmed** new hero leak. | Stable openings/junctions, smooth legitimate shading, retained handles/feet/caster shadows. Geometry edits risk collision/navigation, chart identity and size-dependent behavior. Medium cost, tightly coupled to Stage 3. | Reuse `lighting_quality` window/strip/corner/thin-caster/rotated/scaled fixtures selectively, plus hero corner/window/contact. Compare geometric depth/ownership and direct visibility before assigning a lighting cause. Preserve real occluders within centimetres of receivers. Test across the actual model LOD boundary, room heights and doorway floor interfaces. |
| **5 — material and entity consistency** | Same GLB chairs at equal scale/yaw differ in baked gradients and grounding; nearby probes are valid and nonzero. Runtime `light_transport.rs::entity_lighting` samples one bounds centre; fill policies differ. `gltf.rs::resolve_material` lacks imported response controls; static props use `GpuMaterial::plain_with_alpha`. World sheen is view/baked-energy driven. **High** capability/policy confidence; pair position differences remain a confound. | Cloth stays matte, timber/ceramic/plastic/metal separate, spawned objects inhabit the same lighting. New controls affect albedo/bake compatibility, meshes, uniforms and animation/fades. Medium–high cost; avoid asset-specific branches or indiscriminate glossy defaults. | Hero entities/hall/contact; a matched-location static/dynamic control through ordinary authoring, plus one existing skinned actor. Compare camera rotation, source changes and passage crossing. Preserve genuine static shadows, valid probe labels, real darkness, opacity/fade and interactions. Measure dynamic mesh/object/upload costs. |
| **5, after color/material interfaces — alpha and reflection correctness** | Static GLB BLEND is forced opaque (`wgpu/props.rs`), character MASK becomes opaque (`wgpu/character.rs`), emissive coverage and sorting differ between families. Sky is absent from `bake_reflection_probes`/`encode_planar_capture` though main scene draws it. **High** source confidence; hero clear pane already transmits correctly. | Consistent glazing/foliage/glow and exterior sky in ice/water reflections. Risks include sorting/draw costs, ghost identity, bloom and probe switching. Fix the sky omission separately from spatial-reflection sophistication. | Same OPAQUE/MASK/BLEND asset on static/dynamic/skinned paths, overlapping glass/ghost/steam from both directions, blended emissive coverage samples. Use a small reflective outdoor plane with visible sky; move across probe-selection boundaries. Preserve Pool water and Winter ice/weather. |
| **6 — atmosphere, image stability and art tuning** | Ordinary world fog differs from decals/effects/emissive passes; regional ordinary density is sampled at the endpoint, whereas storm sightlines already integrate shelter. Global fog is fixed; no edge AA path exists. Outdoor concept has readable blue depth; hero night forms remain dark. **High** source confidence, **medium** need for each new visual capability. | Coherent air/depth, cleaner moving silhouettes and restrained source halos. GPU/transparency costs and intentional Winter whiteout are risks. Tune scene scale/density only after light/material contracts stabilize. | Wall+decal at distance, fog volume between camera and an outside object, effects/emission in the same air; short native camera motions on existing rails/foliage. Hero window/room plus one calm/severe Winter pair. Preserve texture detail and supported weather; never correct captures externally. |
| **7 — authored content, reference migration and final validation** | Asset galleries retain Home's oversized vault/blank walls, straight repeated Outdoor placement and Winter's repeated angular trees/roof load. These are composition/geometry issues. Three local packages are genuinely stale; documentation contracts disagree with source. **High** evidence confidence. | Theme-wide coherence and a trustworthy normal workflow. Highest content breadth; avoid repeatedly rebaking every map in earlier stages. Migration must preserve custom debug maps, old evidence and reusable launchers. | Resolve every ledger row with exact source/package/reference checks; rebake changed maps intentionally; matched native High gallery for all themes, appropriate Medium/Low and Winter severe checks. Full desktop/platform gates, clean-source CI, and a foreground scene-performance campaign. Recheck reference hashes and all compatibility limits before queue cleanup. |

Required dependency edges: the color/albedo contract precedes transport tuning
and material response; receiver/caster geometry and actual sampling evidence
precede seam/shadow changes; probe/material interfaces precede entity parity;
alpha/energy behavior precedes bloom/fog calibration; all settled contracts
precede broad Stage 7 migrations. Stages 3–5 must agree interfaces before any
bounded parallel writers. Independent audit conclusions do not authorize later
stages to overlap repository or target ownership.

## Practical visual acceptance

Use the normal High native player and the fixed views. Review concept art beside
the whole scene rather than comparing only isolated texture samples.

- Broad light should make the furnished room legible, with a distinct local warm
  practical pool. Pale walls, cloth and fixture faces should retain useful detail
  and color. Brighter sources should not simply turn more of the scene white.
- A coherent flat face should not reveal chart/triangle boundaries. Real joints,
  folded facets, tight recesses and contact shadows must remain visible. Preserve
  the current closed corner and transmitting opening behavior.
- Window reveals, sill, glazing and background should agree geometrically and
  in lighting; no unexplained rim, aperture light leak or floor hole. The garden
  should read as shapes in depth without flattening night into daylight.
- Feet, tabletop objects and fittings should meet their supports and appear
  grounded. A moving or placed entity should look plausible beside its static
  counterpart, without a brightness jump at room/probe boundaries.
- Carpet/cloth, oak, paint, tile, resin and metal should retain a restrained
  stylized identity under the same view. Glass should keep its coverage, and
  reflected sky should agree with the visible exterior.
- Fog, decals, effects and source halos should share the same atmosphere.
  Severe Winter should keep its deliberate shelter-aware whiteout.
- Repeated unchanged captures should remain comparable. Stage 1 achieved exact
  byte equality in three views; later deliberate changes need matched settings,
  transparent provenance and a visual explanation rather than an arbitrary
  image-difference quality score.
- Performance acceptance must use actual submitted scene frames. Keep bake,
  memory, atlas/texture storage and draw complexity alongside visual evidence;
  Stage 1's background loop numbers do not establish an FPS budget.

No bounce count, moment/SH order, probe spacing, tone curve, atlas density,
triangle cap or new quality threshold is selected by this plan. Existing hard
contracts stay enforced until measured evidence justifies a scoped change.
