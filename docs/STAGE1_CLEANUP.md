# Stage 1 cleanup inventory

Scope: desktop repository cleanup, preserving the current OpenGL/GLES2 renderer,
content and gameplay. No renderer migration is implemented here.

## Tool inventory

| Disposition | Area | Evidence / purpose |
|---|---|---|
| Keep | `tools/package.sh` | Current flat desktop and macOS bundle packaging; compiled-build tests validate both payload layouts. |
| Keep | `tools/assets/validate.py` | Catalog, references, shipped levels and fixture validation. |
| Keep | `tools/props/` | `build.py` imports `parts`, mesh, palette and GLB modules; preview/texture modules support model generation and editor thumbnails. Entity wrapper remains the documented entry point. |
| Keep | `tools/textures/` | `build.py` imports all art manifests; seam repair is independently used by the texture validation workflow. Shipped higher-resolution artwork is preserved by dimension guards. |
| Keep | `tools/levels/build_fixture_levels.py` | Generates current regression fixture levels consumed by tests and benchmark tools. |
| Keep | `tools/bench/` executable tools | Desktop timing, GL captures, lightmap reports, image comparison and hole checks; independent of retired deployment. The two capture scripts cover complementary broad diagnostic and frozen canonical view sets. |
| Keep | `tests/*.py`, `level-editor/` | Package/runtime integration tests and browser editor with its own Node test suite. Editor format limitations remain documented. |
| Remove | Retired platform deployment directory | Device manifest and README have no role in desktop packaging. |
| Remove | `tools/bench/notes/` | Historical measurements and superseded per-change commands; retained in Git history. Current tool usage lives in the benchmark README. |
| Remove | Root resolved-design-decisions document | Aspirational hardware-era requirements; current contracts are in the asset/map guides and implementation. |
| Keep | `CHANGELOG.md`, `docs/renderer-baseline/` | Explicit historical records and frozen visual evidence, not current interfaces. Original names inside these records are intentional. |

Added `tools/verify.sh` for the official command gate and
`tools/bench/compare_baseline.py` for strict comparison of canonical captures.
No unresolved tools remain. There are no tracked CI jobs, cross-compilation
configuration files, alternate build systems or generated debug screenshots.
Documentation screenshots, canonical renderer captures and editor thumbnails
are intentional assets. Local caches are ignored.

## Dependencies

All seven direct crates are used: glam (geometry), glow (OpenGL), png (image
I/O), sdl2 (desktop window/input/context), serde + serde_json (content/settings),
and zip (level packs). ZIP now enables only `deflate-flate2-zlib-rs`, dropping
its unused alternative Zopfli writer and its exclusive dependencies. No crate
was upgraded. No application feature flags or target-specific dependencies
exist. Transitive duplicate exceptions are explained in `clippy.toml`.

## Names and behavior

The crate/binary is `places`; environment switches are `PLACES_*`; the desktop
bundle and X11 identity are `io.github.csd113.places`. Editor globals use Places
and browser storage uses `places.*`. Old environment/storage aliases and the unused startup-log alias are not
retained. Existing portable `settings.json`, `cache/lightmaps`, assets and level
paths are unchanged. GLB generator metadata and fixture author labels are
renamed without changing geometry or imagery.

Device-specific GPU probes are removed; generic Linux devfreq/DRM and macOS
CPU telemetry remain. Rendering profiles, context fallback, shaders, authored
reference canvas and all visual constants remain unchanged. Formatting debt
is repaired; the soft-shadow unregistered-site branch is extracted verbatim
into a helper to satisfy the function-length lint after formatting.

## Verification

Use [VERIFICATION.md](VERIFICATION.md) and `sh tools/verify.sh` for the complete
current gate. Stage 0's immutable document records historical formatting and
tooling issues; it does not exempt current code from this gate. Dormant demo
emission entries and the editor's limited schema support remain content/tool
limitations, outside this behavior-preserving cleanup.

Historical author credits in two lighting fixtures and the descriptive adjective
“liminal” in art descriptions are retained; neither is an application interface.

## Verification results (2026-09-23)

All final checks passed on the macOS development host. Logs and generated
captures are local under `target/stage1/` and are not committed.

| Command | Result |
|---|---|
| `sh tools/verify.sh` | Exit 0; executes every command in VERIFICATION.md's main gate. |
| `cargo fmt --all --check` | Pass; baseline formatting debt fixed. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` | Pass. |
| `cargo test --workspace --all-features` | 839 passed, 0 failed, 3 intentionally ignored. |
| `python3 tools/assets/validate.py` | Pass, 0 warnings. |
| `python3 tools/textures/build.py --check` | 45 sheets pass; 35 documented soft-budget warnings. |
| `python3 tools/props/build.py --check` | 33 props pass. |
| `python3 -m unittest tests.test_package` | 42 passed. |
| `(cd level-editor && npm test)` | 144 passed. |
| `cargo build --release` | Pass. |
| `python3 -m unittest tests.test_compiled_build` | 8 passed, none skipped. |
| `python3 tools/props/build.py`; `python3 tools/textures/build.py`; `python3 tools/levels/build_fixture_levels.py` | Pass twice; second run has identical hashes for all generated outputs. |
| `python3 tools/props/generate_spooner_man.py` | Pass; output hashes unchanged. |
| `PLACES_CAPTURE_DIR="$PWD/target/stage1/final" PLACES_BASELINE_STATE="$PWD/target/stage1/final-state" sh tools/bench/capture_baseline_views.sh` | Places Demo runs through existing GL renderer; 25 Full + 25 Low captures, all identical to tracked Stage 0. |
| `python3 tools/bench/compare_baseline.py target/stage1/final` | 50 byte-identical PNGs. Equal, changed and missing-image cases also checked with temporary fixtures. |
| `python3 tools/bench/check_holes.py target/stage1/after/high/*.png` | All 25 views pass, 0% near-black. |
| `python3 tools/bench/bench_local.py --label stage1 --repeat 1 --frames 10 --warmup 2 --noswap` | Pass; telemetry JSON produced. |
| `python3 tools/bench/lightmap_report.py --label stage1 --shots demo_pool --env PLACES_VERBOSE=1` | Pass; capture and populated lighting metrics. |
| `python3 tools/bench/visual_check.py --baseline "$PWD/target/release/places" --current "$PWD/target/release/places" --strict` | Tool smoke passes all 11 views with zero differing pixels; independent canonical comparison above proves Stage 0 parity. |
| `sh tools/package.sh "$PWD/target/stage1/package"` | Flat desktop and macOS bundle produced; packaged demo capture exits 0. |
| `git diff --check` | Pass. |

An early validation run overlapped in-place texture regeneration and read a
partially written PNG. The final full gate ran after generation completed and
passed. No persistent asset failure remains. All 33 GLB binary chunks are
identical to HEAD, with only JSON generator metadata renamed; no PNG changed.
Shipped/fixture level JSON is identical except for four author-label renames.
The repository-wide scan included binary assets and found no obsolete brand
or device strings outside the two explicitly historical record areas.
