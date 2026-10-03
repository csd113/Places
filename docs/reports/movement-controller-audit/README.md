# Movement audit evidence

The engineering report is [../movement-controller-audit.md](../movement-controller-audit.md). `validation-summary.json` records final outcomes; `evidence-manifest.json` records hashes.

`baseline-regressions.log.gz` contains the six failing pre-repair regressions. `roof-edge-before.log.gz` records the further adversarial roof-edge failure before its repair. `controller-gable-final-tests.log.gz` contains the final focused regressions and timestep matrices. The gzip-compressed command logs preserve the complete final native verification, workspace tests, strict Clippy, formatting, and checking output.

`before-controller/` preserves the original controller/query source at the released lighting baseline, before movement edits. That baseline is also the separate lighting commit `665bfae2f665d912784d6802b8bd404d81dd6c11`.

`native-before/` and `native-after/` contain the Pit reproduction before repair and all 82 final route captures. Logs are gzip-compressed without altering their contents. The ordinary engine input scripts, spawns and native exit results are in each `manifest.json`. The CSV columns are rendered frame, eye X/Y/Z, yaw and pitch. Sampling is every five rendered frames; frame rates differ between runs, so these samples are not uniform-time trajectories. Initial menu/boot samples may have zero coordinates and must be excluded from movement comparisons. Unit tests provide the deterministic per-timestep numerical invariants; native recordings provide separate real SDL/Metal validation.

`native-low-ceiling-before.*` and `native-low-ceiling-after.*` preserve the matched whole-disc ceiling edge reproduction. The recorded standing head is eye Y + 0.2 m.

`performance-before.json` and `performance-exact-final.json` contain all seven release samples per scenario, in microseconds per frame. `build-hashes.json` identifies the exact original executables. Locally preserved runnable copies, their local SDL dependency, assets, map sources and compiled packages are in [the playground](../../../debug-maps/movement-audit-20261003/README.md). Saved copies are locally signed with a relative SDL path; the playground records their separate hashes.

The two `.original.gz` lighting logs are byte-exact originals of the only saved log files whose terminal blank lines required normalization for the publication whitespace check. Lighting code and assets were not altered by that normalization.
