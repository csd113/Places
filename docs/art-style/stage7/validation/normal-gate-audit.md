# Stage 7 independent normal gate audit

The reviewed `tools/verify.sh` now follows the necessary source/package order:
normal release C1; asset and generator checks; recursive normal package migration;
normal C2/player embedding rebuild with compiler SHA comparison; the shared locked
Rust gate; complete Python discovery; explicit atlas and Low-lighting GPU tests.
This is an implementation audit, not a completed gate result. Primary owns the
gate process, target and binary/package publication.

The Python command is:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -p 'test_*.py'
```

Do not set `PLACES_SKIP_SMOKE` or `PLACES_SKIP_PACKAGING`; missing display/binary,
platform/display-lock skips remain stated execution limitations. Discovery includes
the prior partial module list and GLB materials, office/outdoor/winter assets,
weather, environment/schema controls and Stage 7 helper contracts. The Model Zoo
generator `--check` already runs in `tests.test_package`. The initial audit found
the capacity fixture generator check missing; primary added it before map
compilation in the saved gate. The already-running long gate started before that
line was added, so its coverage is supplemented by primary's successful explicit
[current-generator receipt](../execution/capacity-generator-current.json) and
[raw log](../execution/capacity-generator-current.log). Its original
48-model witness now survives catalogue growth without reducing the maintained
5,312 props, 2,760 explicit solid placements, 120 lights or 18 routes. Independent
tests require exact serialized fixture bytes, preserve the witness under reordered/
expanded catalogues and refuse removal/type/model loss of a witness ID. Actual
collision pressure remains required by the unchanged Rust scale test. No new
validator exclusion is needed.

`tests.test_compiled_build` contains nine native/portable execution tests plus its
platform test. They retain embedded-demo startup, malformed settings/custom
sources, unknown asset degradation, no player-side static preparation, saved
settings, unrelated working-directory packaging and complete probe samplers.
`tests.test_wgpu_bootstrap` contains twenty-five native tests plus its platform
test, including CPU/GPU cancellation, resize/retry, preparation reuse, shutdown
joins, latest-generation requests, in-flight same-path replacement, failure
preservation/retry, latest quality during preparation, camera/entity preservation,
repeated-visit cache reuse with fresh spawn, real world replacement and texture/
material response/budgets. Its action traces require matching GPU-ready and actual
scene-presented identities. The existing loading suite supplies bounded RSS
samples and repeated cache use; it does not establish GPU memory or gameplay FPS.

The whole Rust gate includes the positive capacity/count and widened material
index contracts. Useful focused filters for their receipts are:

```sh
cargo test --locked --lib --all-features the_2026_raised_caps_accept_content_past_their_former_boundary
cargo test --locked --lib --all-features the_beyond_former_limits_fixture_carries_content_the_old_caps_refused
cargo test --locked --lib --all-features the_mesh_material_budget_mirrors_the_level_budget
cargo test --locked --lib --all-features a_mesh_material_past_the_former_u16_boundary_round_trips
```

These are already covered by the full gate and need no redundant build. The
optional beyond-former-limits package-reader test returns when its optional archive
is absent, so that return must not become a compiled/native acceptance claim. The
synthetic `cap:*` fixture remains an explicit CPU boundary witness, while
`capacity_dense` stays in normal compiled/native source coverage.

Additional focused ignored GPU resource/readback checks may run serially after
primary releases the target/native allocation:

```sh
cargo test --locked --lib --all-features entity_resources_restore_after_quality_cycles_and_map_reloads -- --ignored --test-threads=1
cargo test --locked --lib --all-features directional_entity_irradiance_reaches_real_rendered_pixels -- --ignored --test-threads=1
cargo test --locked --lib --all-features a_packaged_probe_chain_uploads_and_reads_back_every_level -- --ignored --test-threads=1
cargo test --locked --lib --all-features the_cube_round_trip_matches_the_reference_face_convention -- --ignored --test-threads=1
cargo test --locked --lib --all-features the_srgb_sample_round_trip_is_measured_on_this_adapter -- --ignored --test-threads=1
cargo test --locked --lib --all-features odd_triangle_indices_upload_without_changing_draw_counts -- --ignored --test-threads=1
```

Require each filter to execute one real test. Do not blanket-run all ignored
diagnostics: compiled environment/probe pixel campaigns have prior Stage 6 evidence
and additional capture-directory prerequisites; developer measurements also write
unrelated files. The normal gate already executes both ignored Low-lighting tests.

The small executor consumes the completed package campaign and exact manifest set:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tools/bench/native_regression_maps.py \
  --campaign COMPLETED_PACKAGE_CAMPAIGN --binary FROZEN_NORMAL_BINARY \
  --out NEW_NATIVE_DIRECTORY
PYTHONDONTWRITEBYTECODE=1 python3 tools/bench/native_regression_maps.py \
  --campaign COMPLETED_PACKAGE_CAMPAIGN --binary FROZEN_NORMAL_BINARY \
  --geometry --out NEW_GEOMETRY_DIRECTORY
```

The first invokes the existing capture tool for one genuine normal High image for
each of 48 supported source paths. The second retains 49 real CPU checker reports,
adding the intentionally loader-invalid control with named curved-invalid errors.
The negative geometry fixtures must keep their exact intended confirmed-check
names and required warning names; ordinary confirmed errors remain failures.
Failures preserve their logs and continue across independent cases. A frozen
binary/catalogue/capture-tool change stops the campaign. No source, package or
camera is repaired by the executor.

Normal/feature final parity uses the six exact environment manifests, package,
camera, drawable and settings at the same capture-frame request. Inspect actual
pixel differences and qualify weather/animation timing. Static comparison images
may use exact SHA equality; an animated mismatch cannot simply become a claimed
match. The feature build's `final` receipt must show `diagnostic_identity_post:
false`, normal exposure/grade/bloom and unsuppressed sky/decals/effects. The live
quality checker requires genuine nonzero capture-submission indices/draws plus
requested/applied/resident agreement. Add `--package ACTUAL_PACKAGE` to require
actual atlas inventory, reflection gate and entity-field availability to match the
selected prepared variant. Its receipt retains any named package lightmap fallback.
Pass `--entity-light-trace` to all twelve thematic capture runs and
`--chair-sequence ACTUAL_SEQUENCE` to their checker. Capture-only entity-spatial
blocks precede the corresponding visual capture receipt; a unique actual scripted
chair identity/position plus its uploaded spatial payload must survive every
endpoint, including Low/Medium with Full prepared lighting data and High filtering.
The checker cannot infer the chair's on-screen visibility from those bindings.
Capture submissions are real encoded offscreen work and remain distinct from
surface presentation and physical display cadence.
