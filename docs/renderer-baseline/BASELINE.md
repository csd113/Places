# Historical renderer baseline

This is the frozen reference state of the former Places renderer, captured
before the wgpu renderer existed. It records what that renderer built, tested
and drew at the baseline commit, and it is the comparison ground truth the
captures that followed were measured against. Nothing here was cleaned up,
improved or re-authored: the screenshots are that renderer's own output,
captured through the game's documented one-frame capture path.

The capture-time environment names in this record (the `LIMINAL_*` switches and
the crate name `liminal-rust`) belong to the preserved implementation at the
`renderer-gles2-reference` tag; see
[RENDERER_REFERENCE.md](../RENDERER_REFERENCE.md). They are kept verbatim so
the record reproduces.

## 1. Repository baseline

| field | value |
| --- | --- |
| branch | `main` |
| base commit | `634503fd85f5cffc0ad88bdfc20492cdad2bb543` |
| working tree at capture time | clean (the baseline additions in this directory are commits on top of the base commit) |
| date | 2026-09-23 (macOS local; capture logs are timestamped 2026-09-24 UTC) |
| crate | `liminal-rust` 0.6.0, window title "Places" |
| renderer | OpenGL through `glow` 0.16; the context is requested as OpenGL ES 2.0 and falls back to the desktop compatibility profile (2.1) when the ES request is refused; no multisampling, no dynamic shadow maps |
| windowing / platform | SDL2 0.38 (window, input, GL context, swap interval) |
| lighting | baked, not realtime: per-texel lightmap atlas for static geometry, vertex-lit fallback, static-prop occlusion, static reflection probes and at most one half-resolution planar mirror |
| post-processing | offscreen colour+depth target resolved to the drawable: bloom driven by emission, tone shoulder, distance fog, subtle grade; the UI draws afterwards at the drawable resolution |
| default level | `places_demo` — "Places Demo", the only bundled level |
| build configuration | release profile: `opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`, `panic = "abort"` |
| toolchain | rustc/cargo 1.98.1 (`stable-aarch64-apple-darwin`); crate declares `rust-version = "1.91"`, edition 2024 |
| host | macOS 27.0 (build 26A428), Apple Silicon (arm64), Retina display at a 2.0x backing scale |
| Python / Node used for tooling | Python 3.13.5; Node v22.14.0 |

Quality profiles at this commit:

* **Full** (default): shipped texture resolutions, all optional per-pixel work
  (normal map, sheen, reflections), offscreen scene target at the drawable size.
* **Low**: the same assets and level, each texture box-filtered once at level
  load (surfaces 256, prop sheets 128, emissive masks 128), the optional surface
  response left out, and the 3D scene rendered no wider than the historical
  480 px reference width. Fog, frame and emission are identical.
* Both bake lightmaps; the profile selects the atlas density. Bloom and
  reflections are independent player settings, on in both captures.

## 2. Validation results

Run from the repository root on the commit above. Full logs were kept under
`target/migration-baseline/logs/` in the capture-time working tree (not
committed; the commands below are the authoritative reproduction).

| Check | Command | Result |
| --- | --- | --- |
| formatting | `cargo fmt --all --check` | **FAIL — pre-existing** (53 diffs across 10 files, all from the recent lighting/shadow work; see §6.1; `cargo clippy` and the release build are clean, and the baseline capture did not reformat anything) |
| lints | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS (exit 0, no warnings) |
| Rust tests | `cargo test --workspace --all-features` | PASS — 842 tests: 839 passed, 0 failed, 3 ignored (157.9 s) |
| release build | `cargo build --release` | PASS (exit 0, no warnings) |
| catalog / level validation | `python3 tools/assets/validate.py` | PASS — 118 assets (33 placeable), 3 themes, 0 warnings |
| texture budget | `python3 tools/textures/build.py --check` | PASS — 45 sheets, 0 errors, 35 "over preferred 256" warnings (the intentionally 1024x1024 Office/Pool/Home art; documented policy) |
| prop budget | `python3 tools/props/build.py --check` | PASS — 33 props, all GLBs parse and fit the decoded-memory budget |
| texture regeneration determinism | `python3 tools/textures/build.py` | PASS — exit 0, no file changed (`git status` clean afterwards) |
| fixture-level regeneration determinism | `python3 tools/levels/build_fixture_levels.py` | PASS — exit 0, no fixture JSON changed |
| prop regeneration determinism | `python3 tools/props/build.py` | PASS — exit 0, every GLB, thumbnail and `assets/prop_proxies.json` byte-identical |
| package gate | `python3 -m unittest tests.test_package` | PASS — 42 tests |
| compiled-build smoke tests | `python3 -m unittest tests.test_compiled_build` | PASS — see §2.1 |
| level editor | `cd level-editor && npm test` | PASS — 144 tests, 0 failures |
| runtime smoke / A-B | `target/migration-baseline/run-runtime-validation.sh` | PASS — see §2.2 |
| benchmark / capture tools | `tools/bench/bench_local.py`, `check_holes.py`, `visual_check.py`, `lightmap_report.py` | PASS — see §2.3 |

### 2.1 Compiled-build smoke tests

`tests/test_compiled_build.py` runs the actual release executable from scratch
directories outside the repository. All eight cases passed:

* a packaged build (binary + `assets/`) boots from an unrelated directory,
  resolves its payload from its own path and writes first-run state next to the
  payload, not in the working directory;
* an empty install creates `levels/`, `import/`, `cache/` and `settings.json`
  and boots the embedded Places Demo with no asset tree at all;
* a restart loads the saved configuration;
* a malformed `settings.json` is preserved as `settings.json.invalid` and
  replaced by defaults;
* malformed custom levels are skipped by name with a reason;
* unknown materials/props/fixtures degrade with diagnostics and still render;
* Places Demo is always selectable by id;
* the level-load reflection-probe bake draws with complete samplers (no
  `GLD_TEXTURE_INDEX_2D` / "texture unloadable" driver warnings).

### 2.2 Runtime checks

Every run below used the release binary with a pinned scratch state root and
the one-frame capture path (`LIMINAL_BENCH=1 LIMINAL_BENCH_NOSWAP=1
LIMINAL_CAPTURE=...`); exit codes, output and PNGs are in the capture-time
working tree under `target/migration-baseline/`.

* two consecutive launches of the same view produce **byte-identical** PNGs;
* a cold lightmap bake and a warm cache hit for the same view produce
  **byte-identical** PNGs;
* the whole 50-image canonical set regenerated twice is **byte-identical**
  (the run hashes are in `target/migration-baseline/logs/41-…` and `42-…`);
* `LIMINAL_NO_LIGHTMAPS=1`, `LIMINAL_NO_REFLECTIONS=1` and `LIMINAL_NO_BLOOM=1`
  each produce a different frame, i.e. each stage is actually contributing
  (the lightmap control changes ~51 % of pixels, the planar reflection ~3.7 %
  inside the wet-deck rectangle, the bloom control ~1.4 % around the emissive
  fixture);
* Full and Low differ in the expected direction (Low is softer, with the
  surface-response term absent);
* a second window size (960x540 logical) renders at the matching drawable size
  (1920x1080) with the same content in the same aspect;
* `LIMINAL_NO_OFFSCREEN=1` still boots and renders (documented diagnostic path);
* normal runs are quiet, no run printed `panicked`, `[materials]` or a GL
  texture warning, and every run exited 0.

### 2.3 Benchmark / capture tooling

All four Python bench tools were run after the tooling relocation:

* `bench_local.py` (`--label baseline-smoke --repeat 1 --quality full`): one
  120-frame run of Places Demo reports `render_mean_ms` 0.566,
  `loop_median_ms` 1.592, 150 draw calls, 31 067 vertices and writes
  `target/agent-work/bench/baseline-smoke.json`;
* `check_holes.py docs/renderer-baseline/high/*.png`: all 25 High views report
  0.0 % near-black pixels (exit 0);
* `visual_check.py` comparing the build against itself (absolute binary paths
  required, see §6.1): 0 differing pixels in all 11 shots, i.e. the capture and
  decode path is deterministic over the demo and the regression fixtures;
* `lightmap_report.py`: runs, captures all 15 shots and writes `report.json`;
  its measured fields populate when it is given `--env LIMINAL_VERBOSE=1`
  (see §6.1).

## 3. Python tooling organization

The repository keeps all first-party Python development/build/asset/validation
tooling under `tools/`. The baseline capture work audited every tracked Python
file and moved the one file that was still outside a purpose directory.

| | count |
| --- | --- |
| tracked `*.py` files before the relocation | 34 |
| tracked `*.py` files after the relocation | 34 (one moved, none added or removed) |
| under `tools/` | 32 |
| outside `tools/` | 2 (both test suites, documented below) |

Moved file:

| old path | new path | why |
| --- | --- | --- |
| `tools/generate_spooner_man.py` | `tools/props/generate_spooner_man.py` | a prop generator entry point belonged with the `tools/props/` toolkit rather than at the root of `tools/`; the wrapper now lives beside the `build.py` it drives |

References updated for the move:

* `tools/props/generate_spooner_man.py` — its own docstring command and its
  module-relative import path (it now imports `build` from its own directory);
* `docs/ASSET_SPECIFICATION.md` (§8.6 and the regeneration command list);
* `assets/entities/spooner-man/README.md` (the regeneration command).

The historical `CHANGELOG.md` entries that name the old path are intentionally
left untouched: a changelog records what a past release did, and rewriting
history is not part of a relocation.

Python files intentionally left outside `tools/`:

| path | justification |
| --- | --- |
| `tests/test_package.py` | a test suite, not tooling. `tests/` is the repository's canonical test directory (the README, `ASSET_SPECIFICATION.md` and `MAP_AUTHORING_GUIDE.md` all document `python3 -m unittest tests.test_package` / `python3 tests/test_package.py`), and moving it would change the module identity and discovery the commands rely on. It performs no asset generation or developer pipeline work. |
| `tests/test_compiled_build.py` | the compiled-release smoke suite, same reasoning; documented as `python3 -m unittest tests.test_compiled_build`. It is the repository-level test of the shipped executable, not a utility. |

No other Python is tracked anywhere in the repository (no CI configuration
exists; the only shell scripts are `tools/package.sh`,
`tools/bench/capture_views.sh` and `tools/bench/capture_baseline_views.sh`,
none of which contains Python). The
`python3 -m http.server` line in `level-editor/README.md` invokes the Python
standard library to serve static files for the browser editor; it is not a
first-party tool.

Validation of the relocation:

* the moved wrapper runs from the repository root and from an unrelated
  working directory (`python3 tools/props/generate_spooner_man.py`), prints the
  same budget report and leaves the tree clean;
* `sha256` of `assets/entities/spooner-man/model/spooner-man.glb` and
  `level-editor/assets/thumbs/spooner-man.png` are identical before and after
  the move plus re-run;
* the whole asset pipeline was re-run after the move — catalog validation,
  texture `--check` and regeneration, prop `--check` and full regeneration,
  fixture-level regeneration and the package test suite — with no output
  change.

## 4. Renderer features confirmed

Confirmed visually in the captures (view ids in §5) and through the toggles in
§2.2:

* **base textures** — Office wallpaper/carpet/suspended ceiling, Pool tile and
  ceiling, Home wallpaper/paint/hardwood/carpet/tile, sign and decal sheets
  (all views);
* **material shine / surface response** — the two pool notice boards
  (`plastic_panel`, `panels_east`), present in High and absent in Low;
* **normal maps** — `plastic_panel` and `panels_east` carry tangent-space
  normal maps; the High/Low pair isolates the term;
* **baked lighting** — warm Office fluorescents (`reception`, `office`), cool
  Pool downlights and wall luminaires (`pool_wide`, `pool_entry`), the red
  stair hall (`stair_mid`), warm Home rounds (`home_arch_entry`,
  `home_balcony_east`);
* **lightmap atlas** — per-texel pools on floors, ceilings and walls; the
  `LIMINAL_NO_LIGHTMAPS=1` control renders the vertex-lit fallback and differs;
* **baked shadows / occlusion** — prop contact darkening and blocked pools
  (`drum`, `home_kitchen`), the under-balcony dark wedge
  (`home_under_balcony`), stair-hall containment (`stair_mid`);
* **reflections** — planar mirror on the wet pool deck
  (`wet_deck_shallow`) and a baked reflection probe on the brushed-metal board
  (`panels_east`); `LIMINAL_NO_REFLECTIONS=1` removes both;
* **decals** — the NO DIVING cut-out sign on the pool wall and basin floor
  (`pool_wide`, `pool_north`, `pool_basin_from_deck`) and the hazard stripes
  (`pool_steps`, `pool_entry`);
* **transparency** — dirty/clear/tinted glass window panes between Office and
  Pool, seen from both sides (`office_win_close`, `pool_win_from_pool`,
  `stair_mid`);
* **props / GLB models** — Office desks, chairs, cabinets, cooler and vending
  machine (`reception`, `office`); Pool patio set, ladder, curtain and
  guardrail modules (`pool_wide`); Home kitchen run, beds, seating, rugs and
  plants (`home_kitchen`, `home_balcony_east`);
* **dynamic-object path** — the turning washing-machine drum (`drum`), drawn
  outside the static batches;
* **multi-floor / vertical geometry** — stair hall and pool basin
  (`stair_mid`, `pool_basin_from_deck`), Office looking down into the Pool
  (`office_win_close`), the two-storey Home with balcony, staircase, gable
  ceiling and knee walls (`home_arch_entry`, `home_main_west`, `home_ceiling`);
* **emission and post-processing** — emissive fixture faces (the Office
  panel's diffuser), the bloom/resolve stage (`LIMINAL_NO_BLOOM=1` differs on
  ~1.4 % of pixels around it), fog in the long corridor views. The demo's two
  `animated_emissions` sign materials are declared but placed on no geometry at
  this commit, so the animated-sign feature is not visible in the captures
  (see §6.1.7);
* **quality profiles** — the complete set exists in both High
  (`high/`) and Low (`low/`) with identical cameras.

## 5. Visual benchmark manifest

Conventions for every capture in `high/` and `low/`:

| setting | value |
| --- | --- |
| level | `places_demo` |
| resolution | 1280x720 drawable (a 640x360 logical window at the 2.0x backing scale; `LIMINAL_VERBOSE=1` prints `logical 640x360 | drawable 1280x720 pixels | backing scale 2.0x`) |
| frame | the first rendered frame of a fresh process (`LIMINAL_CAPTURE`, default `LIMINAL_CAPTURE_FRAME=1`) |
| field of view | 60 degrees (shipped default) |
| filtering | linear (shipped default) |
| bloom / reflections / lightmaps | on (shipped defaults) |
| vsync | off, no swap (`LIMINAL_BENCH_NOSWAP=1`; presentation only, does not change pixels) |
| quality | `high/` = Full, `low/` = Low (`LIMINAL_QUALITY`) |
| UI | none (the game boots straight into the level) |

`spawn` is `LIMINAL_SPAWN` (`x,z[,yaw]`, or `x,y,z,yaw` for an explicit eye
height) and `camera` is the absolute `LIMINAL_CAMERA` `yaw,pitch` override.
Yaw 0 faces −Z (north on the level plans), yaw 90 faces +X (east), yaw 180
faces +Z, yaw 270 faces −X; positive pitch looks up. Where no camera override
is listed, the spawn yaw is the camera.

| # | view | area | spawn | camera | exercises |
| --- | --- | --- | --- | --- | --- |
| 1 | `reception` | Office reception | `2.0,5.6,74` | spawn yaw 74 | overview: wallpaper/carpet/ceiling textures, fixture light pools, desk/chair/cabinet props, doorway |
| 2 | `office` | Office workroom | `9.5,3.5,90` | 90 | multi-prop room, linoleum patch, tinted window, several light pools |
| 3 | `office_win_close` | Office → Pool window | `3.1,4.8,0` | 180,-10 | dirty-glass transparency, the pool seen through the pane, two lighting temperatures |
| 4 | `pool_win_from_pool` | Pool → Office window | `3.1,9.4,0` | 0,10 | glazing from below, clear glass, vertical Office/Pool relationship |
| 5 | `pool_wide` | Pool deck | `14.0,13.0,45` | spawn yaw 45 | overview: NO DIVING wall decal, guardrails, curtain modules, ladder, round and wall luminaires |
| 6 | `pool_north` | Pool north side | `12.0,14.0,0` | 0 | wall decal, guardrails, office windows, tiled surfaces |
| 7 | `pool_entry` | Pool east walkway | `21.0,8.6,270` | 270,-10 | floor decal, wall luminaire, baked occlusion under the guardrail, corridor doorway |
| 8 | `wet_deck_shallow` | Pool wet deck | `20.6,-0.1,10.8,0` | 0,-25 | planar mirror reflection on the wet deck, steps, tile |
| 9 | `pool_basin_from_deck` | Pool basin | `10.5,10.6,0` | 0,-26 | recessed basin, NO DIVING floor decal, lightmapped basin floor |
| 10 | `pool_steps` | Pool → corridor steps | `22.0,11.5,0` | 90,-10 | raised landing transition, hazard-stripe decal, metal notice board |
| 11 | `plastic_panel` | Pool notice boards | `6.0,10.5,0` | spawn yaw 0 | moulded-plastic albedo + normal map, metal frame, tile |
| 12 | `panels_east` | Pool east notice board | `23.6,8.2,90` | spawn yaw 90 | brushed-metal albedo + normal map + reflection probe |
| 13 | `stair_mid` | Red stair hall | `21.5,3.2,180` | 180,-14 | coloured-light isolation, stair elevation change, pool through a window and a passage, glazing |
| 14 | `corridor_mid` | Quiet corridor | `30.5,13.0,90` | 90,-4 | damp carpet + stained ceiling, chain of light pools, long fog sightline |
| 15 | `drum` | Corridor washing machine | `28.4,13.6,0` | 0,-22 | dynamic-object path (turning drum), contact occlusion |
| 16 | `home_approach` | Corridor → Home archway | `50.8,12.6,90` | spawn yaw 90 | corridor continuity, archway geometry, the Home beyond |
| 17 | `home_arch_entry` | Home entrance | `54.4,12.6,90` | 90,-2 | two-storey volume, staircase, guardrail, arch reveal, mixed materials |
| 18 | `home_stairs_low` | Home staircase | `54.0,13.6,90` | 90,-10 | stair geometry (eight risers), handrail, wood/paint materials |
| 19 | `home_main_south` | Home main room → balcony | `62.4,4.8,180` | 180,-2 | balcony/void multi-floor, under-balcony occlusion, couch/TV props |
| 20 | `home_main_north` | Home living area | `61.0,9.4,0` | 0,-2 | many material types at once, rug/couch/table/bookshelf props, warm downlights |
| 21 | `home_main_west` | Home two-storey overview | `63.4,7.0,250` | 250,-2 | upper storey and railing, knee wall, gable volume, desk props |
| 22 | `home_kitchen` | Home kitchen | `60.6,6.6,300` | 300,-3 | kitchen GLB run (cabinets, sink, stove, fridge), tile/hardwood threshold, knee wall |
| 23 | `home_balcony_east` | Home balcony bedroom | `59.6,12.5,90` | 90,-6 | upper-floor props (bed, rug, bookshelf, lamp), handrail, raised floor |
| 24 | `home_ceiling` | Home gable ceiling | `61.0,7.0,180` | 180,38 | gable ceiling profile, ceiling fixture pools, upper-storey soffit |
| 25 | `home_under_balcony` | Home under-balcony | `62.6,13.6,0` | 0,-2 | shadow-heavy area, baked occlusion, dark-frame composition |

Storage:

* `docs/renderer-baseline/high/` — 25 PNGs, Full profile;
* `docs/renderer-baseline/low/` — 25 PNGs, Low profile, same 25 cameras;
* each directory carries `manifest.txt`: one `name<TAB>level<TAB>spawn<TAB>
  camera<TAB>quality` row per capture;
* total committed size is about 19.1 MiB (High 15.6 MiB, Low 3.5 MiB); the
  Low frames compress much better because that profile draws the scene at the
  480 px reference width and upscales it;
* nothing is resized, colour-corrected or re-encoded: the PNGs are exactly what
  the game's capture path wrote.

Reproduce (repository root, release binary built):

```sh
# Capture the current renderer in the same 25 views, both profiles.
# Always set PLACES_CAPTURE_DIR: the script's default target is this frozen
# directory, which must never be overwritten.
PLACES_CAPTURE_DIR="$PWD/target/agent-work/baseline-compare" \
    sh tools/bench/capture_baseline_views.sh
PLACES_QUALITY=low \
PLACES_CAPTURE_DIR="$PWD/target/agent-work/baseline-compare-low" \
    sh tools/bench/capture_baseline_views.sh # one profile

# Compare the captures against this frozen set, per view.
python3 tools/bench/compare_captures.py \
    target/agent-work/baseline-compare docs/renderer-baseline

# A shell/hole sanity check on the committed reference set.
python3 tools/bench/check_holes.py docs/renderer-baseline/high/*.png
```

The frozen images themselves reproduce only from the preserved implementation:
build the `renderer-gles2-reference` tag worktree and capture there with its own
scripts (see [RENDERER_REFERENCE.md](../RENDERER_REFERENCE.md)). Against that
checkout's committed asset tree, `tools/bench/compare_baseline.py` reports
50/50 byte-identical images.

`compare_captures.py` decodes two capture sets and reports per-view differing
pixel counts, the affected fraction and the worst channel delta per shot. The
canonical sets in `high/` and `low/` are paired by filename with the captured
output; treat any pixel difference against this baseline as either a known item
below or the bounded difference class documented in
[docs/RENDERER.md](../RENDERER.md) §15.

## 6. Known baseline imperfections

These are present at the baseline commit. They are the preserved
implementation's own known imperfections, listed so a later change is not
blamed for them; the baseline capture did not change them.

### 6.1 Repository / tooling

1. **`cargo fmt --all --check` fails.** There are 53 formatting diffs across 10
   files (`src/game/tests.rs`, `src/lighting/bake.rs`,
   `src/lighting/bake/range_audit.rs`, `src/lighting/lightmap/atlas.rs`,
   `src/lighting/lightmap/tests.rs`, `src/lighting/occlusion.rs`,
   `src/lighting/visibility.rs`, `src/quality/tests.rs`, `src/render/api.rs`,
   `src/render/tests.rs`), nearly all line-wrapping choices in the recent
   lighting/shadow work. `cargo clippy` and the release build are clean.
   Reformatting is a mechanical repository-cleanup change deliberately left to
   the later repository cleanup.
2. **`tools/textures/build.py --check` prints 35 "over preferred 256"
   warnings.** The Office, Pool and Home surface sheets and the NO DIVING sign
   are intentionally authored at 1024x1024; the preferred size is a soft
   budget.
3. **The benchmark walkthrough view table in `tools/bench/capture_views.sh`
   predates the Home wing of Places Demo.** Entries named `sign_corridor`,
   `unmade_world` and `unmade_back` now frame plain walls/corridor, and the
   README's demo route still describes an unmade-world ending. The canonical
   set in this directory uses the current areas instead; the stale table was
   left as it was.
4. **No CI configuration exists**; all automated enforcement is manual
   commands, as `docs/ASSET_SPECIFICATION.md` §24 states.
5. **`visual_check.py` needs absolute binary paths.** Its captures run with the
   staged package as their working directory, so the relative paths in
   `tools/bench/README.md`'s example (`target/release/liminal-rust`) raise
   `FileNotFoundError`; passing absolute paths works (used for §2.3). The
   example was not corrected during the capture.
6. **`lightmap_report.py` does not enable its own telemetry.** The
   `[level]`/`[lighting]`/`[lightmaps]`/`[spatial]` numbers it parses are only
   printed when `LIMINAL_VERBOSE=1`; run without `--env LIMINAL_VERBOSE=1` the
   report's metric fields are empty (captures still succeed, exit 0). The
   README example does not mention the switch; it was left as it was.
7. **Places Demo declares two `animated_emissions` for materials no geometry
   uses.** `core:glass_sign_lit_01` and `core:glass_sign_flicker_01` appear only
   in the level's `animated_emissions` block, so the backlit-sign animation is
   dormant at this commit, and `tools/assets/validate.py` validates the
   animation schema without flagging an emission whose material is unused.
   This is content drift from the Home-wing rework, not a renderer defect; it is
   listed so a later renderer does not chase a sign that never draws.

### 6.2 Renderer

1. **No anti-aliasing.** The GL context requests no multisample buffer, so
   geometry edges are hard-aliased at the drawable resolution (visible on the
   staircase, railings and window frames). This is the baseline look.
2. **The Low profile is visibly softer.** It draws the 3D scene at no more
   than 480 px wide and upscales to the drawable, and it drops the
   normal-map/sheen surface response. Same assets and ids, lower per-pixel
   work — by design.
3. **Reflections are static and approximate.** Probes are baked once per level
   load at 64 texels per face and the planar mirror is half resolution and
   limited to one plane per frame; both are weighted by authored sheen, so a
   low-shine surface shows little or nothing. They are not scene reflections.
4. **Lighting is baked.** There are no realtime lights and no dynamic shadow
   maps; dynamic objects are lit by a single probe of the static bake with no
   shadows or self-occlusion.
5. **Colour space is deliberately linear-in-display-space.** There is no
   sRGB/gamma handling; textures, tints and lighting constants were calibrated
   together in the current space (the current renderer documents the same
   display-space policy in [RENDERER.md](../RENDERER.md) §3).
6. **Lightmap resolution is finite.** The atlas is 2 pages / ~1.1M chart
   texels for the demo at Full; small-scale light detail is interpolated, and
   a bake that cannot fit its page budget falls back to the historical
   vertex-lit path.
7. **Decals cannot cross a floor or ceiling height change, and a gable
   ceiling takes no decals** (current level-format limitation).
8. **No per-object alpha on GLB props, no refraction and no transmission into
   the light bake**; transparency is material-level (`blend`/`cutout`) on
   surfaces and windows.
9. **The `LIMINAL_NO_OFFSCREEN=1` diagnostic path is not pixel-identical** to
   the normal path (it skips the planar pass, probe binds, bloom and resolve);
   it exists for benchmark A/B runs only.

## 7. Scope of this baseline

The baseline capture added only: this document and its screenshots, the
reproduction script
`tools/bench/capture_baseline_views.sh`, the documentation references to it,
and the relocation of `tools/generate_spooner_man.py` into `tools/props/` with
its command references in the asset specification, the entity README and its
own docstring updated. No renderer code, shader, asset, level, lighting
constant or dependency was changed, and no wgpu code was added.
