# Stage 3 — Continuous surfaces and explicit transport

2026-10-08.

The compiler keeps real surfaces continuous across construction cuts while preserving actual blockers, hard geometry and source falloff. Transport distinguishes physical receiver support, direct light, gathered sky/diffuse interreflection and stored fill.

| Before | Final |
| --- | --- |
| ![Before](../../images/art-style/stage3-before-contact.png) | ![Final](../../images/art-style/stage3-final-contact.png) |

## Implemented behavior and limits

World-supported receiver sampling removes the sofa's chart-oriented discontinuity without treating every dark band as erroneous. Matched direct/indirect/filter/fill and caster findings remain in [diagnostics](diagnostics.md); indirect includes sky and diffuse interreflection, with no inferred per-emitter bounce or standalone AO layer.

Finite-segment visibility accounts for real opaque, cutout and blend geometry. Transmission is neutral and straight; no refraction or coloured absorption is claimed. Transparent apertures, water, foliage and closed gable coverage use actual source geometry. The ghost-collider warning was investigated geometrically: reviewed tail occupancy is inside existing return-wall solids with actual mesh support, not permission to accept unsupported collider centres.

The dark resin table remains beyond its source range, and central dark samples coincide with a penetrating plant. Invalid source/asset control runs and failed intermediate candidates were not accepted as final proof. Dynamic-chair field/contact followed in Stage 4; silhouette/tiny-alpha limitations remain separate. Three inherited local stale discovery packages were later reconciled by the completed Stage 7 inventory gate.

[Contracts](contracts.md) retain the implemented technical boundaries; [measured costs](performance.md) retain the dated work/resource methods and limits. The [seven-stage history](../../style-upgrade-20261007/art-style-history.md) records completed integration and distinguishes this milestone from later model-lighting work. Current reproduction and repository gates are in [Verification](../../VERIFICATION.md). Historical stage source/format numbers are not a claim about the current solver revision.

## Real gable coverage

| Before | Final |
| --- | --- |
| ![Before](../../images/art-style/stage3-before-annex.png) | ![Final](../../images/art-style/stage3-final-annex.png) |

## Historical implementation and checks

Implementation [b937724](https://github.com/csd113/Places/commit/b9377244249fa21fa12975ae8d93297e41111ebd) passes formatting, locked source checks and strict debug/release Clippy. The completed local workspace command passes 2,072 library tests with 23 ignored, then exits 101 at the same three inherited stale-package discovery cases. A preceding interrupted run remains incomplete; its three observed library failures are repaired and absent from the final run. Normal/instrumented final packages agree exactly. Seven of nine final native repeats are byte-identical; hall/corner changes are limited to upper lintels, at most 2/255 per channel, as qualified in the diagnostic findings.
