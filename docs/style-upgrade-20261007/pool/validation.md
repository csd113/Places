# Pool final validation

The authoritative result is the stable final content, with current packages,
rather than intermediate authoring runs. Raw command logs, CSVs and hashes are
retained under `debug-maps/pool-style-20261007/evidence/validation`.

| Check | Final outcome |
| --- | --- |
| `python3 tools/assets/validate.py` | PASS: 302 assets, 169 placeables, five themes, zero warnings. |
| `python3 -m unittest tests.test_package.PoolContentTests tests.test_package.ShippedLevelTests.test_the_model_zoo_is_current_with_its_generator` | PASS: seven tests, including new Pool material/prop registration and unchanged Zoo generation. |
| `python3 tools/props/build.py --check` | PASS: file-backed prop builders and catalog coverage. |
| `python3 tools/textures/build.py --check` | PASS: PNG presence, dimensions/decoding and catalog. Expected soft-budget warnings for authored 1024 sources remain. |
| Seam checks for deck, basin, wall, ceiling, coping, band and metal | PASS: all seven periodic surface PNGs. |
| Read-only asset integrity audit | PASS: zero errors. Pool subset, UVs, topology, alpha and hashes are in `model-audit.json`. |
| `places --check-geometry --level assets/levels/places_demo.json` | PASS: zero errors, two pre-existing non-Pool warnings; no new Pool warning. See `geometry.json` / `geometry.txt`. |
| `places-compile build assets/levels/places_demo.json --workers 8 --force` | PASS: all three variants; 98,762,970 bytes; 223.951 seconds. |
| `places-compile build assets/levels/movement_test.json --workers 8` | PASS: directly dependent Pool surface refresh; 42,370,590 bytes; 44.891 seconds; source unchanged. |
| `places-compile build assets/levels/model_zoo.json --workers 8` | PASS: three Pool additions and remade Pool asset refresh; 84,702,370 bytes; 234.337 seconds. |
| `places-compile verify <source> --package <package> --require-current` | PASS for all three affected shipped packages. Package SHA-256 and compiler/lighting identities are in `costs.json`. |
| ZIP CRC checks for those three packages | PASS. |
| `RUSTC_WRAPPER= cargo build --release --bins` | PASS with the final embedded demo; 1 minute 36 seconds. |
| `cargo fmt --all --check` | PASS. |
| `RUSTC_WRAPPER= cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS: exit zero, 9.40 seconds. No lint suppression or lowered configuration. |
| `RUSTC_WRAPPER= cargo test --workspace` | Library PASS: 2,021 passed, zero failed, 23 ignored (329.61 s). Compiler/command-line/CPU-port targets and doc tests PASS. Full command exits 101 only because three quiet-discovery integration cases encounter preserved stale historical drop-ins; details below. |
| Native High renderer captures | PASS: thirteen matched pairs, same camera/settings and Metal adapter; zero missing world textures, zero failed model loads. |
| Native Pool traversal | PASS: submerged stairs, ladder and hot-tub exits finish on dry deck at eye y=.1 m. See `traversal.json`. |
| Scope preservation | PASS: all non-Pool catalog entries, water, ladders, rooms, doors, routes, sequences, volumes, effects and spawn unchanged. Existing Zoo entries unchanged. |
| Patch whitespace | PASS. |

`RUSTC_WRAPPER=` disables the host's unavailable sccache service; it changes
no Rust flags or gate. The final debug/all-features Clippy invocation is the
repository-required command. CI additionally runs its configured release
Clippy and locked checks; the pushed exact-SHA result is reported at handoff.

The geometry warnings are the existing Office trim sliver near
`(1.215, .09, .162)` and Outdoor porch layer at `(13.5, .24, -90.95)`.
They were not repaired or suppressed by this Pool pass. The compiler leaves
33 sub-texel demo sliver quads on the engine's existing vertex-lit fallback,
instead of allocating pathological lightmap charts.

An initial workspace run overlapped intermediate map authoring and found
stale embedded packages and casing/coping intersections. A later stable run
passed 2,019 tests and found two decal issues: the wall sign crossing the band
and the expected external-sheet list missing the new lane. Both invalidated
assertions now pass individually after fixing the content/list; the repeated final library gate passes against the corrected package. No failing assertion
was weakened. The tile repeat expectation now reflects the authored 20 cm
wall tile; the normal-map explanation names the actual Pool casing material.

Historical drop-in packages that reference changed Pool artwork (including
`levels/geometry_intentional.placesmap` and `levels/level0_pit.placesmap`) are
now stale against the working asset root. The final `tests/list_levels.rs`
quiet-discovery cases (`repository_layout_lists_only_packages`,
`another_working_directory_lists_the_same_packages`,
`packaged_layout_lists_only_packages`) fail on their expected stale-dependency
warning, although the current shipped packages load. These optional untracked
drop-ins are absent from a clean CI checkout. Their files and historical
evidence were preserved; neither tests nor warnings were weakened, and no
unrelated map repair/rebake was performed. This local workspace limitation is
reported at handoff alongside the exact pushed CI status.
Winter, Home, Outdoors, Office, Hallows and entity source/assets are untouched.

The wide native counters are a proportional cost check: +17 draw calls,
+3.8% loaded vertices, +10.67 MiB world texture residency, unchanged texture
edge limits and lightmap page count. The 45-frame diagnostic samples are not
a controlled performance benchmark; no stable FPS improvement is claimed.

Sleep protection is task-owned `caffeinate -di`, with both
`PreventUserIdleSystemSleep` and `PreventUserIdleDisplaySleep` assertions.
It is explicitly left active for the next serial owner. No persistent power
or security setting was altered. Current PID/custody is recorded in the local
retained evidence and the task handoff, avoiding a stale committed PID.
