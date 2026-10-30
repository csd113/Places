# Spawn and test here — Windows acceptance, 0.13.1-dev

Verified on October 30, 2026, from base `98baef6c09c90578f0eb76262c6c31d224be6d92`.

## Result and controls

The **Here** button beside Play is available in Simple and Advanced mode. It offers the actual editor camera or an exact visible floor click. Cmd/Ctrl Shift P starts from the camera. Esc cancels floor picking. Ordinary Play retains its authored-spawn behavior.

A typed, ephemeral intent captures the project root, level ID, eye, yaw and clamped pitch before asynchronous export. Document edits/reloads, cancellation and failures discard it. Import receives no pose override; only the player receives `PLACES_PLAY_HERE`. Normal Play removes inherited spawn overrides. Play-here checkpoints/settings use a separate `play-here` profile within the existing package/level test profile.

Picking uses the actual architecture triangles, rendered face orientation, model occlusion and exact initial door leaf colliders. Standing placement reuses the game floor and full-body clearance machinery and validates again against the installed runtime, including physical props. It never searches for a distant recovery point or substitutes a world plane. The map's saved player spawn, selection and undo/redo are not used to store the temporary pose. The usual save-before-export behavior remains.

The override is consumed at fresh startup. Explicit checkpoint Resume, graphics reloads and later transitions retain their own position rules. Malformed/nonfinite legacy spawn tuples are rejected as a whole instead of changing arity by dropping a component.

## Verification

All final Rust gates exited 0:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace` — 396 passed, using the installed asset root and `RUST_TEST_THREADS=1`.
- `cargo clippy --locked --release --workspace --all-targets --all-features -- -D warnings`
- `cargo build --locked --release --workspace --all-features`

Four small regressions protect strict numeric parsing, full standing/storey placement, and source/save/history/level identity preservation. They use in-memory geometry and tiny starter documents, with no map baking. The isolated target needed an explicit asset root for existing asset-dependent tests. An existing timed Luau quest test failed once under parallel contention; the complete serial suite passed. No lint policy or test budget was weakened.

Eleven native acceptance cases passed against the final executables. Representative runtime eye positions:

| Case | Observed result |
| --- | --- |
| Camera captured before export; camera then moved | `[1, 1.6, 0]`, yaw 35°, pitch −12°; orbit target was elsewhere |
| Repeat camera launch | `[2, 1.6, −3]`, yaw 75°, pitch −8° |
| Normal Play after both | Authored `[0, 1.6, 0]`, yaw/pitch 0° |
| Exact floor click | `[1, 1.6, 0]`; selection, raw source and history state retained |
| Elevated / recessed floor | `[14, 3.6, 5]` / `[25, 2.1, 5]` |
| Stair tread / ramp hull support | `[5.75, 3.2, 2.2]` / approximately `[1.75, 3.75996, 2]` |
| Void, occluded floor, closed door, tight header, solid prop | Clear refusal; no queued launch |
| Picker/build cancellation and missing player executable | Intent cleared; ordinary Play remained independent |
| Save, move, explicit Resume | Restored `[2, 1.6, −2]`, yaw 80°, pitch −10°, rather than replaying the startup pose |
| Graphics reload | Same restored pose, with Low quality actually applied |
| Level transition | Destination authored `[6.9, 1.6, 3.5]`, yaw 270°, pitch 0° |

The copied fixture sources remained byte-identical across the campaign, after enabling gameplay saving through the editor for the checkpoint fixture. The normal profile gained no checkpoint; the saved checkpoint exists only in the play-here profile and its checksum and player payload were verified.

A further direct Windows keyboard run used ordinary player mode, without benchmark actions. It opened Play from the camera, produced a native 1920×1080 renderer capture at the expected pose, and returned normally. Editor captures at 1440×816 verify the readable Here control and both mode dialogs. Hardware: NVIDIA GeForce RTX 3070 Laptop GPU, DX12. Native processes ran at Normal priority; compiler children and background Rust gates ran at BelowNormal. Presentation was Fifo; no throughput/FPS gain is claimed.

Evidence is retained outside the repository and target under `task-6/spawn-here/`: `native-final/acceptance.json`, per-case editor snapshots/PNGs, player CSVs, runtime snapshots, ordinary keyboard-run capture, gate logs/results, `source-build-receipt.json` and binary hashes. The shared loading trace can interleave editor and player diagnostic rows; acceptance requires valid runtime anchors and an independent player CSV. The ordinary keyboard run used separate traces and has no malformed rows. Windows Graphics Capture timed out, so visual inspection used the native renderer PNGs; native keyboard and window lifecycle actions used Computer Use.

## Preparation and packaging impact

No engine/compiler preparation input changed. The all-features Windows compiler code identity remains:

`29d6fc16a6277f3025251f3b8d44969f1067effdde4f46ff06a71002339478c9`

This feature introduces **zero prepared map/project package invalidations**. All four retained repository package candidates remain byte-identical; their previous currency state is unchanged. Only copied acceptance fixtures were exported, outside ordinary Rust tests/CI. No shipped-map campaign was rebaked.

Final executable SHA-256 values:

- Player: `b1f358240ba961d2f4080d89e072e4786b13146ef2056ebf821db5b8b89aa340`
- Editor: `889ebde9abfe920ee920e2ac32cdb4dccdaf82b66ac5a0804f1e68415d04f150`
- Compiler: `29cac81b10b7738604da16f6ded6a8858a8e3fd8767203d8c3c69b66bd92e0a6`

The unrelated generated menu remains preserved and outside this feature commit: SHA-256 `93b92d7d6caaa9a7fc1a1bec2bb0805c17cccd25f584c730ef882ffbbcf9a27a`. Prior packaging archives, snapshots and pending packaging operations were not changed or retried. The existing Windows distribution archive predates this feature; distributing it requires a separately authorized packaging refresh of the new executables, without new map baking for this feature.

## Practical limits

Camera starts require standing clearance and an eye within one walking step of standing height over an actual floor. Use floor picking when the orbit camera is high above the scene. Authored movable-prop bounds are reserved conservatively before launch, followed by the native physics clearance check. Mac native validation remains deferred. No iGPU, security or power configuration was changed.
