# Stage 2 — Colour and material foundations

2026-10-08.

The hero decodes colour into linear light once, retains HDR energy through lighting/material/reflection composition, then converts for display at presentation. Cream paint, upholstery, carpet and oak improve under the same lights, cameras, exposure constants and source artwork. No ambient or map-specific brightness correction was added.

| Before | Final |
| --- | --- |
| ![Before](../../images/art-style/original-room.png) | ![Final](../../images/art-style/stage2-final-room.png) |

## Implemented behavior and limits

CPU bounce reflectance excludes receiver illumination and follows model material/UV sampling. GPU colour sheets use sRGB resources; normals and masks remain numeric. Linear alpha-aware mips prevent hidden transparent RGB from contaminating visible levels; black/white averages to approximately byte 188 rather than 128. Float vertex colour and RGBA16F scene/emission/reflection resources preserve energy above 1.

Missing source normals retain flat geometric triangles, while authored model/pose normals use inverse transpose and preserve tangent handedness. Compact scalar roughness/metallic response and alpha classes agree across static, movable and character routes. Empty alpha classes no longer add phantom visible vertices. The implemented format/cache revisions and legacy readers are specified in [contracts](contracts.md).

Sofa receiver seams and the contact-free spawned chair remained at this milestone and were addressed by later stages. Native placement exposes positive uniform scale; rotated/nonuniform matrix and skin handling were deterministic test controls, not a claimed native nonuniform placement feature. Architectural normal maps are supported; glTF model normalTexture authoring is not. Single-sample silhouettes and tiny cutout mips remain qualified limits. The dark hall table is outside its authored direct-light range.

[Contracts](contracts.md) retain the implemented technical boundaries; [measured costs](performance.md) retain the dated work/resource methods and limits. The [seven-stage history](../../style-upgrade-20261007/art-style-history.md) records completed integration and distinguishes this milestone from later model-lighting work. Current reproduction and repository gates are in [Verification](../../VERIFICATION.md). Historical stage source/format numbers are not a claim about the current solver revision.

## Historical implementation and checks

Implementation [b34e1dd](https://github.com/csd113/Places/commit/b34e1dd6d8dcdf43280a35c9644bcb567905e492) passes formatting and strict debug/release Clippy. The local workspace command passes 2,042 library tests with 23 ignored, then exits 101 at three inherited stale-package discovery cases; it is not a complete passing run. Two real native round-trip tests verify all 256 sRGB byte values exactly and the HDR cube face convention. Four legacy-reader and four Python material checks pass. Two forced builds reproduce the package SHA, and six independent native view repeats match byte for byte. Moving/posed and nonuniform JSON placements are not claimed captured.
