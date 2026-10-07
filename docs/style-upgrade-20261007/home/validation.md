# Home final validation

Raw logs, CSVs, compiler identities and task-owned process status are retained
under `debug-maps/home-style-20261007/evidence/validation`. The final captured
content is the result; intermediate packages and screenshots remain reviewable
in the separate candidate snapshots.

| Check | Outcome |
| --- | --- |
| Catalog validation | PASS: 332 assets, 189 placeables, five themes, zero warnings. |
| Focused Home content and Zoo-generation Python tests | PASS: four tests. |
| Prop builder `--check` | PASS: 189 models; 49,335 triangles; 36,708 KiB decoded texture budget, below 64 MiB. |
| Texture builder `--check` | PASS: 78 sheets. Existing soft-budget/unmanifested Winter warnings remain; no failing PNG. Normal Home builds load committed sources. |
| Periodic seams | PASS: all ten remade/new Home surface PNGs. |
| Master/native pipeline | PASS: all 24 pairs exactly match 1024² → 256² Lanczos downsampling. |
| Home GLB/PNG audit | PASS: 31 families, 9,200 triangles total, largest 704; no degenerate/nonmanifold/inconsistent-winding triangles. Embedded pixels match the native standalone PNGs; UVs within 0..1 with floating-point tolerance. |
| Native geometry validation | PASS: zero errors, the same two existing non-Home warnings. |
| Four affected bundled packages | Final identities/size/CRC and compiler timing are recorded in `costs.json`; verification requires current source/catalog/dependencies. |
| Release binary build | Final result recorded below. |
| `cargo fmt --all --check` | PASS. |
| `RUSTC_WRAPPER= cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS: final exit zero, 15.99 seconds; no suppression or configuration weakening. |
| `RUSTC_WRAPPER= cargo test --workspace` | Final result recorded below. |
| Native High captures | Twelve matched before/after views and a single-pendant on/off pair with all other lights active; native Metal renderer, zero failed models/missing world textures. Manifests retain camera settings, renderer identity and PNG hashes. |
| Scope preservation | Non-Home asset bytes/catalog entries and immutable Home sheets identical. Demo routes/entities/interactions and original room dimensions/openings intact. Existing non-Home Zoo placements intact. Only the Home prop/material/light/dressing fields change. |
| Patch whitespace | PASS. |

`RUSTC_WRAPPER=` avoids the host's unavailable sccache service without changing
compiler flags. Two Cargo build workers and four test threads bound resource use. A local
incremental-cache linker failure was resolved by setting `CARGO_INCREMENTAL=0`,
preserving the old cache; no source or lint workaround was needed. CI runs the
complete locked workspace selection and release Clippy on the pushed revision.

Release build: PASS, exit zero, 95.99 seconds. Final demo preparation takes
236.16 seconds for off/medium/full variants; no compiler warnings.

Workspace tests: the required `cargo test --workspace` run reached 2,016
passing library tests, 23 ignored and five failures before stopping. The failures are repaired and checked in focused reruns: the Home sink selector,
600 mm sink's countertop alignment, lamp saturation, the unchanged CPU
fallback geometry limit, and bundled-level discovery after refreshing the
direct Lantern Hollow cabinet dependency. Five Home showcase cases also
pass, for ten passing focused Rust checks. The full authored scene retains its below/above contrast and a direct
pendant-on/off gain near the orb, with all other sources active. Supplemental
source-isolation checks prevent new room fill from substituting for the
pendant; they retain warm output, upward self-exemption, directional sign
contribution and High source gain. The paired native screenshots show the
small table lift in bright authored room fill. Neither lighting thresholds nor
geometry limits change. The preceding complete run is preserved in
`pre-regression-workspace-tests.log`; focused logs retain the repaired results.

The user requested proportionate checks without repeating unaffected test
campaigns. The full local selection is therefore not repeatedly rerun after
those focused repairs. The exact pushed revision's clean-checkout CI provides
the final complete workspace gate; its outcome is reported in the handoff.
The known local historical-map discovery compatibility described by Pool is
preserved and deferred, rather than deleting archives or rebaking unrelated
maps.

The pre-existing geometry warnings are the Office trim sliver near
`(1.215, .09, .162)` and Outdoor porch layer at `(13.5, .24, -90.95)`.
No unrelated repair or suppression is introduced. The new backsplash uses an
ordinary supported wall chart; additional thin-box caps/nosings were removed
after native review. The profiled casing sits proud of the old frame to avoid
coincident faces. Prepared lighting still gives the small cabinet recesses
coarser/stronger shadow gradients than the sheet; no shared-renderer change
or fabricated screenshot hides that difference.

Movement Test's JSON remains byte-identical. Its package is refreshed because
the developer's current-package fingerprint includes the full catalog.
Lantern Hollow's source and Hallows artwork remain byte-identical; its package
requires a direct Home cabinet dependency refresh to remain discoverable.
The original package is retained in the before archive. No Hallows design
work is introduced. The other historical packages and older evidence are
retained. The local
quiet-discovery limitation already documented in
[Pool validation](../pool/validation.md) is not repaired by rebaking unrelated
maps or weakening the assertions.

The exact preceding Pool commit's [CI run 37672078580](https://github.com/csd113/Places/actions/runs/37672078580)
completed successfully on `476dfb4e603da2dbaaf78a1016243ca20f4e1858`.
The Home handoff reports its own exact pushed SHA, remote verification and CI.

All task-owned build/capture jobs finish before checkout/target release. The
transferred `caffeinate -di` process remains active with both idle-system and
idle-display sleep assertions for Outdoors. No persistent power/security
setting is changed. Process identities and final custody verification stay in
local evidence rather than baking a transient PID into the journal.
