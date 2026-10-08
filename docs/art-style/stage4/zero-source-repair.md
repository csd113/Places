# Final-audit correction — 2026-10-08 UTC

The intermediate publication `c6b1efe85a87a969ae7c2405e284f9ea5ceada96`
passed [exact-SHA CI](https://github.com/csd113/Places/actions/runs/37799238665)
with 2,103 library tests and every integration test. A subsequent compiler/runtime
contract review found an essential coverage gap before Stage 4 ownership release.
The original implementation and all its native captures, logs and runnable bundles
remain immutable; this correction appends the final contract and its evidence.

The solver emitted `local_direct = None` when there were no selected always-on
sources. That selected PLPF v2 and the legacy center-sampling path. A switch-only
scene therefore lost the live entity source path, even though runtime tests with a
constructed v3 empty-source field passed. The v3 writer rejected that constructed
field and its reader collapsed a zero source count to `None`.

New solves now always emit PLPF v3. Empty source IDs carry aligned zero selected
means/moments for every probe. The reader preserves that presence marker, and both
codecs reject truncation or nonzero direct energy without a source. Genuine legacy
v2 remains readable and retains its historical path. Solver revision 15 invalidates
cached revision-14 fields. Runtime production shaders and accounting need no change.

Distinct specialists own compiler/codec generation and runtime/package loading
coverage. Tests exercise dark, sky-only and switch-only solves; compiler air labels;
serialization and package decoding; reserved-source on/off/on accounting; and
unchanged residuals and cache identity. The lead owns the validation jobs and Git.

The accepted hero has selected always-on sources in all five packages. Its retained
visual/movement/quality and GPU campaign remains the rendering evidence. Refreshed
packages must preserve the previous payload bytes while changing bake identity;
new binaries/packages and immutable runnable snapshots receive their own provenance.
No new visual or GPU campaign is required for this codec/generation correction.

Validation and final publication receipts are appended after their commands finish.

The first focused test compile caught private-module imports in the new compiler
fixture and ambiguous empty-vector assertions. The lead routed the fixture through
the existing `cfg(test)` render facade and used explicit `is_empty` assertions.
The failed compile log is retained in `repair-validation/`; production behavior
was unchanged by these test integration corrections.

The repaired focused compiler/codec/runtime/package tests passed. Strict Clippy
then caught test-only binding shadowing, one unsuffixed label, and its required
typed empty-array assertion style. Those causes are corrected without suppressions.
`repair-validation-v2/` retains the successful focused checks and failed lint log.
`repair-validation-v3/` then passed strict debug/release Clippy. Its workspace run
caught the architecture guard scanning a fixture bake in `src/package/world.rs`.
The owned runner/cargo/test processes were stopped before editing. The new fixture
now lives in a separate `cfg(test)` module; the player guard remains unchanged.
The final complete command sequence is recorded separately in `repair-validation-v4/`.

## Local validation of the frozen repair

[Final execution receipts](repair-validation-v4/execution.json) and
[summary](repair-validation-v4/summary.json) record nine Python tests, format, all
focused solver/codec/compiler/package/runtime tests, the architecture guards,
and strict debug/release all-target/all-feature Clippy passing.
`cargo test --workspace` passes **2,102 library tests**, with zero library failures
and 23 ignored tests. It exits 101 only at the three unchanged local package
discovery failures already assigned to Stage 7; full local green is not claimed.
The remaining macOS integration target is run explicitly and passes. Clean-source
exact-final-SHA CI remains the publication gate.

The first package refresh could not create the required Metal reflection adapter
inside the sandbox. Its error and empty attempt are preserved in
[repair-packages-sandbox-attempt](repair-packages-sandbox-attempt/failure.json).
The same hero-only build runs through reviewed native execution; no renderer
feature, assertion, reflection capture or validation is disabled.

## Compatible hero refresh

[Build provenance](repair-build-provenance.json) records 215 frozen Rust/WGSL/
Cargo/build inputs and separate successful normal/diagnostic release commands.
[Build and current-package verification](repair-packages/execution-and-payload-equality.json)
passes for the original, entities, switchable, movement and comparison controls.
[Full archive-member equality](repair-packages/full-member-equality.json) confirms
all shared members are byte-identical, including geometry, lighting, irradiance,
atlas textures, collision/navigation and reflection cubemaps. Only solver/compiler
fingerprints, lightmap `content_key`, and their content-addressed metadata entries
change. Original hero package size is 5,189,440 bytes, one byte above the prior
package due to metadata compression. This is not a runtime or bake-speed claim.
