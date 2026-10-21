# Places 0.11.0 prerelease

October 17, 2026. This source prerelease completes the approved 0.11.0 queue.
It builds on 0.10.0's exploration, physics, environments and Map Compiler GUI.
Maps remain optional external content; an empty installation opens the menu.

## Included work

- [Source ownership and cleanup](release-0.11.0.md#repository-reorganization-0110): reusable
  engine, game and Map Compiler modules, thin entry points, retained accepted
  assets and documented future editor boundaries.
- [Rust/dependency prerequisite](release-0.11.0.md#dependency-upgrade-0110): Rust 1.99.1 and
  reviewed dependency migrations with the existing native backend limits.
- [Renderer foundations](release-0.11.0.md#renderer-0110): surface/capture/resource lifetimes,
  same-map reload ownership, changed-only emission and cached visibility,
  bounded offline overlap diagnostics and the reviewed Office desk repair.
- [Shader corrections](release-0.11.0.md#shader-0110): small-scale normal transforms,
  fluorescence mask identity, raw-surface display copy and shared fragment work.
- [Close-up flashlight correction](release-0.11.0.md#flashlight-0110): remove the vertical
  source drop behind the confirmed floating-box appearance while retaining real
  cast shadows, lateral offset, beam settings and mannequin behavior.
- [Console and typed gameplay actions](release-0.11.0.md#developer-console-0110): 58 curated
  pickup IDs, safe reset/removal, trigger inspection, bounded world time and
  fixed stepping, model representation and third-person/session-local copies.
- [Sound and trigger foundations](release-0.11.0.md#sound-0110): bounded Kira/SDL3 playback,
  spatial sources, footsteps, doors, switches, props, licensed audio, swept and
  filtered trigger volumes, cleanup, persisted volume/mute and environment cues.
- [Fixture housing/emission audit](release-0.11.0.md#fixture-leaks-0110): close procedural
  mounting backs, route ordinary bodies through Full transport and restrict
  Home lamp/strip emission to intended luminous surfaces.
- [Source-aware caustics](release-0.11.0.md#caustics-0110): explicit source eligibility,
  independent coloured contributions, bounded visibility, submerged decals and
  authored tint, strength and gentle motion controls.
- [Loading/resource ownership](release-0.11.0.md#loading-0110): canonical package identity,
  bounded archive/image/model reads, shared decoding, cache accounting and
  transactional imports that preserve installed maps on failure.
- [Settings/input](release-0.11.0.md#settings-input-0110): partial settings repair, binding
  ownership, Console rebinding, fresh presses, focus/pause clock transitions and
  synced atomic persistence with retained invalid backups.
- [Entity/navigation correctness](release-0.11.0.md#entity-navigation-0110): sequence
  cancellation/publication, stopped-time ownership, live door/step constraints,
  nearest-point search, displayed-pose blends, route completion and bounded AI work.

The [retirement stage](release-0.11.0.md#rust-test-removal-0110) removed the old costly Rust
suite. Subsequent approved stages added narrowly justified, small CPU regressions;
the final suite has 67 passing tests, with no production-map baking dependency.
The changelog preserves the separate move/deletion accounting and stage history.

## Release gate and version evidence

Final implementation commit: `8ff9007021415128510c24e610fa5cb9f9ed4cf5`.
[Its exact-head CI](https://github.com/csd113/Places/actions/runs/38996822820)
passed every job/step: formatting, source-only locked target/feature checks,
strict debug/release Clippy, essential CPU tests, locked all-feature release
build, asset/generator checks, Python contracts and patch whitespace.

The final local implementation gate also passed all 67 CPU tests in 0.08 seconds
and 372 Python source tests with 17 explicit skips. The final entity campaign
passed 31 checks across 77 actions and 19 native captures, including mannequin,
roach shelter/return and Office/Home reload. Earlier feature campaigns remain
in their linked reports; they were not repeated as a new full-map release audit.

Release finalization changes documentation and issue completion records only.
Its exact committed CI outcome is recorded with the GitHub prerelease and in
`local/release-0.11.0/`; implementation validation is reused without rebuilding
the cleaned local target directory. No production or archived map is rebaked.

`Cargo.toml` and the root `places` lockfile entry are 0.11.0. Player menu/loading
UI, diagnostics and Map Compiler use `CARGO_PKG_VERSION`; macOS packaging derives
bundle versions from Cargo. The dependency-stage native empty-install menu
displayed `v0.11.0`, and final entity-stage native diagnostics report 0.11.0.
No source version bump is part of the subsequent 0.12.0 branch transition.

## Issue completion

The completed 0.11.0 enhancement/bug issues are #20 (layout), #16 (renderer),
#23 (flashlight), #22 (console), #12 (sound) and #19 (caustics). All were already
closed with completion reason before release finalization. Their remaining
planning/checklist text is reconciled with the linked stage evidence and limits.
Future or incomplete scopes remain open, including #21 packaging, #18 editor,
#17 Windows/Linux/release validation, #13 moving water and the broader UI,
player-model, visual-style and multiplayer requests. Session-local `/become`
does not establish completion of the broader player-model request.

## Delivery, compatibility and limits

The release follows the existing annotated numeric tag (`0.11.0`) and GitHub
**prerelease** source convention. There are no attached distributable binaries
or compiled maps. Use `git lfs pull` for tracked binary assets. Tested local
executables and raw evidence remain preserved outside `target`; they are not
represented as cross-platform release packages.

Compiler/version fingerprints can make earlier packages developer-stale while
remaining runtime-compatible with matching catalogs. Some stages changed source
and geometry inputs; compile selected authored sources explicitly when current
packages are required. Do not reset geometry or map revisions to hide currency.

Native coverage is Apple M2 Pro/macOS/Metal. Full Windows/Linux native validation
is deferred. The user listened to the native sound clip and answered
**“Listened — acceptable”**; physical output device unplug/replug remains untested.
Office wall-light side-spill remains unreproduced; the fixture audit establishes
closed delivered backs and corrects related demonstrated defects without
claiming that observation fixed. The earlier intermittent crash attribution
remains unresolved; bounded diagnostics are retained.

Performance findings are scene-specific: renderer comparisons include a 24.38%
small-drawable increase and a 1.53% close-door decrease. The shader campaign's
near-120-FPS plateau is inconclusive for throughput. Audio/flashlight variation
does not establish a speedup, and entity work reductions are not an FPS claim.
Metal allocation-byte counters remain unavailable, resource campaigns are bounded,
physical device loss was not induced, and ambiguous/posed geometry requires
review. Transient presentation/focus exclusions remain documented in stage reports.

After release, `0.12.0-staging` starts from released main and the fully merged
`0.11.0-staging` branch is retired. Editor, distribution overhaul, Luau and other
0.12.0 implementation await the user's separate go-ahead.

## Consolidated feature evidence

The release acceptance above is authoritative for 0.11.0. The dated feature
records below retain distinct implementation details, measurements, failed probes,
evidence paths and limitations. Their test counts, package fingerprints and runtime
observations describe their own checkpoints. Earlier scope restrictions, pauses
and proposed next steps are historical decisions, superseded by later completed
sections and the release acceptance; they are not instructions for current work.
Current contracts live in the authoring, asset, editor, installation and API guides.

- [Source-aware Pool caustics — 0.11.0](#caustics-0110)
- [Developer console pickup audit — 0.11.0](#console-pickup-audit-0110)
- [Rust and dependency prerequisite for 0.11.0](#dependency-upgrade-0110)
- [Developer console — 0.11.0 staging](#developer-console-0110)
- [Entity updates and navigation correctness — 0.11.0 staging](#entity-navigation-0110)
- [Fixture housing and emission audit — 0.11.0](#fixture-leaks-0110)
- [Flashlight close-up correction — 0.11.0 staging](#flashlight-0110)
- [Asset/map loading and resource ownership — 0.11.0 staging](#loading-0110)
- [Renderer foundations — 0.11.0 staging](#renderer-0110)
- [0.11.0 stage 1: repository and source responsibilities](#repository-reorganization-0110)
- [Rust automated-test removal — 0.11.0](#rust-test-removal-0110)
- [Settings and input correctness — 0.11.0 staging](#settings-input-0110)
- [Shader correctness and maintenance — 0.11.0 staging](#shader-0110)
- [Sound engine and content — 0.11.0 staging](#sound-0110)

<a id="caustics-0110"></a>
## Source-aware Pool caustics — 0.11.0

Issue [#19](https://github.com/csd113/Places/issues/19). Baseline:
`e8e729b192e836ff296292fd3497f854d2a2e26b`. This staging work does not merge
or release 0.11.0. The earlier Office side-spill observation remains
unreproduced; this stage does not claim to fix it.

Pool round/wall fixtures and explicit `sun` sources supply focused underwater
light. Fluorescent fixtures and `moon` sources cannot opt in. Other finite or
`directional` sources require explicit authored eligibility. RGB and brightness
never decide eligibility. Existing Beach/Aero sun and Winter/Lantern Hollow moon
sources now name those kinds in their source and generator.

Each eligible source retains its own direct basis from the existing compiled
lighting/visibility system. Runtime source state recomposes linear RGB without
rebaking diffuse light; excluded sources have no basis. Moving blockers reuse the
engine's existing caster resource and revision cache;
conservative source-to-cell bounds certificates cover the space between samples.
Source intensity controls preserve the shared power of wall-light apertures.
The ordinary fixture switch path refreshes source state too. These focused-light
radiance controls do not turn immutable prepared diffuse atlases into a runtime
colour/intensity solve.

World, model and decal receivers share world-XZ pattern coordinates and exact
water footprint/surface/bottom gates. A separate neutral material factor prevents
Static's baked mixed illumination from colouring eligible light a second time.
The blue lane decals now receive the same pattern as adjacent submerged tiles.
The committed numeric PNG is unchanged. There is no additional scene draw,
shadow-map pass, fluid simulation or runtime texture generation.

Per-water `caustics` authors multiplicative linear `tint`, `speed`, and `strength`.
Default speed is 1.35 times the previous drift; strength remains 0.24. Elapsed
simulation seconds drive motion. Speed changes retain phase, including freezing
at zero; quality-only reinstalls retain runtime controls and phase. Tint cannot
add a colour channel absent from the contributing source.

<a id="caustics-0110-bounds-and-approximation"></a>
### Bounds and approximation

The receiver lattice targets 0.15 m horizontal spacing and at most three height
cells. Each cell owns eight RGB/floor records, capped globally at 65,536 records
(1 MiB GPU storage); spacing grows if needed. Pool uses 51,432 records
(822,912 bytes). Source bases are sparse and capped at 4,194,304 records
(48 MiB record payload before allocator capacity). A source exceeding the budget
fails closed and is counted in diagnostics. Tiny minimum-resolution circular
volumes use a constant centre-column sample inside the exact shader footprint.

This is a bounded direct-light approximation, not refractive transport. Every
static and moving source-to-cell region must be clear before its eight
corner factors can contribute. Thin blockers between lattice points therefore
fail closed. Bounds certificates can darken extra space around solids and alpha
cutouts and sub-grid static details; shadow edges and floor transitions remain
limited by lattice spacing. Compiled submerged support tops raise receiver
queries onto model floors and steps. Cell-local records preserve clear cells
beside blocked neighbours; per-fragment floor guards prevent higher light
reaching buried receivers.
Moving submerged objects use eight cached source anchors, excluding their own
caster while retaining other blockers. Finite source water entry uses the source
centre. The full prepared diffuse solve keeps its existing quality and visibility
path. Captured reflected scenery retains its
ordinary authored initial state.

Dry scenes allocate only small empty bindings. With all eligible sources dark,
the shader returns before field/pattern fetches; the clock writes one zero-count
header on transition and then stops uploading. Stationary moving geometry
reuses its visibility results. Disabling source focus does not discard the
resident resource needed to re-enable it. An enabled eligible source whose shared
field is fully shadowed may retain the active header for a higher, self-excluded
instance receiver; zero shared RGB alone does not establish the dormant path.
An object crossing water-volume boundaries falls back to the shared field, so
its own conservative caster bounds can darken the submerged portion.

<a id="caustics-0110-native-acceptance-and-motion"></a>
### Native acceptance and motion

The final direct executable passes all 57 production Pool checks on Apple M2 Pro /
Metal at a physical 1280 × 720 drawable. Full/Static lighting and Low/High texture
transitions preserve source state, water controls and the held pattern. The
existing texture preset also selects its default filtering; Low/Medium/High
samplers are checked independently at startup.

Independent matched-pixel review confirms response on upper/middle treads, the
shallow shelf, circular bottom and actual submerged dynamic washer basket. For
example, 7,111/8,400 pixels in the tub-floor region change with focus; sampled
exterior corners, deck and wall stay identical. The basket's top response is
subtle (2–3 display-code values), while its vertical side remains unchanged.
All three focused/restored pairs match exactly, as do source-off/strength-zero
and the restored Full/High overview. Floor and blue lane respond continuously.

![Pool floor and blue lane receivers](../images/caustics-0.11.0/pool-focused.png)

[No eligible source](../images/caustics-0.11.0/pool-no-eligible.png) ·
[Steps and shallow shelf](../images/caustics-0.11.0/pool-steps.png) ·
[Circular floor and dry corners](../images/caustics-0.11.0/pool-circle.png) ·
[Submerged physical receiver](../images/caustics-0.11.0/pool-submerged-prop.png)

Eleven small source/occlusion controls pass in both Full and Static. A separate
open-ceiling copy of the actual Pool retains its water, props, steps, floor
regions and blue decals and passes all seven source combinations in both modes.
Pool-only and Sun-only add focus; fluorescent-only, Moon-only, no-source and
excluded-only frames are pixel-identical to disabled focus. Mixed energy equals
the eligible sum. The ordinary baked base remains identical across these focused
source controls. Native dim/recolour/tint actions respond and restore exactly;
excluded-source recolouring and brightening leave focus unchanged.

A 20 mm overhead strip blocks the sampled shadow completely (0/255 changed
pixels in Full and Static), while adjacent clear regions respond. A real movable
crate reduces aggregate field energy by 35.89%; its sampled floor shadow remains
identical to disabled focus (0/1,920 pixels), with neighbouring clear floor
responding (1,150/1,200). Moving it away restores the original field and floor.

[Sun-only Pool](../images/caustics-0.11.0/pool-sun-only.png) ·
[Coloured eligible sources](../images/caustics-0.11.0/mixed-coloured-sources.png) ·
[Thin sunlight blocker](../images/caustics-0.11.0/thin-sun-blocker.png)

Matched prior-stage and candidate clips use a fixed west view at
`[3.8, 1.6, 5.4]`, yaw 90°, pitch −42°, Full lighting, High textures/filtering,
and a physical 1280 × 720 Metal drawable. Each contains 120 genuine native
frames over four simulation seconds, encoded at 30 fps H.264/BT.709 with no
interpolation. Candidate speed 1 and 1.35 are also compared; the selected 1.35
retains gentle drift and readable tiles. Measured phase slopes are 1.000 and
1.350 per simulation second; 1.35 also holds at a 30 Hz simulation cadence.
These cadences are not claims about actual rendering FPS. Capture uses V-sync,
with bloom/reflections off; those settings differ from the October 14 reference
campaign, whose original report remains [Pool caustics](release-0.10.0.md#pool-caustics).
The original 240-frame clip was also inspected: it opens with a camera reframe
and includes a foreground rail, bloom and Full reflections. The current fixed
view omits that rail; both use High textures/filtering, Full lighting, FOV 60°,
Metal and 1280 × 720. Its original SHA-256 remains
`07ec4d8198b67d10f998617f617383a85dd6bbe39f24215b87bf2723b60b88b4`.
The previous build also differs in contrast and lighting, so the same-build
speed-1/speed-1.35 pair isolates motion more clearly. Its mean adjacent-frame
basin change rises by 1.343×, with no abrupt flash in the reviewed sequence.
This four-second inspection does not establish long-duration loop behaviour.
Capture timings are not throughput evidence.

<a id="caustics-0110-performance"></a>
### Performance

Direct executables on Apple M2 Pro / Metal, fixed west eye/view as above,
physical 1280 × 720, High filtering independent of texture residency, bloom on.
Full requests Full reflections; this view submits zero planar reflection passes.
Static commits the engine's Low/off lighting and reflections off in both builds.
V-sync and all frame caps are off (`swap_interval: 0`). Creation initially logs
Fifo, then the installed settings explicitly switch to **Immediate** before
timing. Full's 383–505 FPS observations exceed the display's reported 120 Hz,
establishing that those runs are not refresh-capped. No map compilation or our
image/video capture ran concurrently.

Full/High uses three alternating runs per variant, 180 warm-up + 600 measured
frames each. Medium/Low are single observations with the same counts. FPS is
total measured frames / total loop wall time; CPU frame and deduplicated hardware
GPU values below are medians of the per-run medians, in milliseconds. GPU elapsed
is the union of pass stage spans and includes scheduling stalls; CPU frame wall
time includes renderer/presentation backpressure.

| Lighting / textures | Average FPS before → after | CPU frame ms before → after | GPU elapsed ms before → after |
| --- | ---: | ---: | ---: |
| Full / High | 469.6 → 462.6 | 1.265 → 1.132 | 1.100 → 1.442 |
| Full / Medium | 473.4 → 474.4 | 1.359 → 1.394 | 0.835 → 0.795 |
| Full / Low | 483.1 → 492.0 | 1.513 → 1.496 | 0.852 → 0.790 |

Full/High GPU median cost increases 0.343 ms; p95 is 1.916 → 2.199 ms.
Its average FPS decreases 1.5%, within the observed run spread (baseline
434.5–505.3, candidate 454.9–468.1). These observations establish no FPS gain.
Same-build effect-disabled and eligible-sources-off controls average 463.9 and
436.2 FPS respectively, with overlapping but broad ranges (450.3–485.8 and
383.1–494.3). Both have identical 0.061 ms median CPU update cost; GPU medians
are 1.107 and 1.140 ms. All 13 source-off actions execute before measurement,
leaving all 17 apertures inactive. Dormant focus retains its resident field.

Static adds three longer alternating runs per variant (600 warm-up + 1,800
measured frames) after the first short comparison. Baseline/candidate/disabled
pooled average FPS is 181.8 / 195.6 / 210.9, but individual observations range
130–625 / 131–491 / 131–493. Later all three converge near 130 FPS despite
Immediate, while background graphics services are active. **Static throughput
comparison is inconclusive**; its pooled averages are not evidence of a gain.
Early GPU medians are 1.146 → 1.371 ms; late medians are 2.678 → 3.348 ms,
which cannot isolate active GPU cost from changed scheduling/pacing.

Scene submissions remain identical: 27 draws / 19,720 triangles in Full and
28 / 22,084 in Static, with no caustics pass. Raw per-frame CSVs, hardware sample
IDs, actual presentation transitions, commands and process identities are in
`local/caustics-0.11.0/perf/`; summaries are `perf-summary.json` and
`perf-review.json`. These are Mac observations; other native GPU platforms and
continuous moving-blocker throughput were not measured.

<a id="caustics-0110-verification-and-reproduction"></a>
### Verification and reproduction

Rust 1.99.1 / Cargo 1.99.1. The complete locked Rust gate passes: format,
all-target/all-feature check, strict debug and release Clippy, 42 CPU regressions
(0 failures/ignores, 0.01 seconds), and all-feature production executables.
Python source contracts pass 372 tests with four explicit skips. The focused CPU
regressions cover source eligibility
and metadata/legacy hydration; mixed RGB, initially dark activation, dimming,
recolouring and exclusion; small circles and wall aperture shares; independent
neutral material factors and versioned mesh/prop round trips; thin blockers,
submerged model support, narrow treads, clear neighbouring cells and a moving
receiver under a submerged platform. They prepare tiny
worlds without assets, full maps, transport baking or a native window.

Current records are PLLT4, PLMW3 and PLMP9. Legacy readers remain supported;
packaged authored identities restore legacy source eligibility. Neutral material
factors in old geometry default explicitly to white until the map is rebuilt.
Compiler identities include record revisions; acceptance must use current
verified packages, not an old bake.

Only Pool and the four maps with newly explicit Sun/Moon kinds were rebuilt;
each package passed `validate` and `verify --require-current` against the final
compiler. No prepared map is committed. Final executable SHA-256:
`394ee79bdac993d59bc7531292d5905a402848b959ad2c4bcab7899014e293c4`.
Compiler:
`299c322f6ad97df30cd62d6cf94965dca8e3552a51401e425146668501d02b5f`.
Production Pool package:
`9dcbe0bea5116f38105aa964157ca6b406431a003f921e9dfbd09bafa41ee127`.

`tools/bench/caustics_sources.py --author <local-directory>` writes disposable
source controls and a manifest of selected compile/verify commands. Its `--run`
mode accepts independent lighting and filtering settings; `--animation` supports
speed and simulation-cadence comparisons. Timing uses `tools/bench/frame_time.py`.
Raw commands, logs, original PNGs, native clips, hashes and exact pixel regions
remain outside `target` in `local/caustics-0.11.0/`, including
`curated-native-summary.json`, `manual-pixel-review.json` and
`manual-video-review/measurements.json`.

All native acceptance receipts pass. Asset validation passes with zero
warnings; texture checks retain 96 existing advisories. No PNG/model asset was
repainted or replaced. No archived map is in the refresh scope. Player swimming,
buoyancy redesign, moving water, editor features and broad compiler work remain
deferred; [#13](https://github.com/csd113/Places/issues/13) remains separate.

<a id="console-pickup-audit-0110"></a>
## Developer console pickup audit — 0.11.0

Audit date: October 16, 2026. Scope: the complete installed `assets/catalog.json`, its 370 model identities (366 unique GLBs; five streamer aliases share one file), and reusable physical overrides in `assets/gameplay.json`.

<a id="console-pickup-audit-0110-contract-and-outcome"></a>
### Contract and outcome

The explicit allowlist is `src/game/pickup_catalog.rs::ITEMS`: 58 reviewed stable catalog IDs and their display names, in lexicographic ID order. `find(id)` resolves an exact member. Command listing and give execution must use these same entries. Names, category, `solid`, file presence, and an existing physics profile do not grant eligibility. Aliases belong to command parsing and must resolve to an approved exact ID.

The audit parsed all 370 catalog models at their scene-node rest transforms with `tools/props/glb.py::read_glb`, measured their actual bounds, inspected rig/clip/morph declarations, and reviewed their documented use. Of the 58 approved choices, 57 are rigid; the lit cake is the one explicit animated-item exception, retaining its embedded candle clip. All approved actual extents are positive, at least 1 mm and at most 1.5 m per axis. All fit the existing pickup-reach calculation; the largest conservative model-bounds radius is 0.710141 m (the Beach crate).

Every approved entry has a strict-valid dynamic profile with `pickup: true`. This adds 42 profiles and preserves all 23 pre-existing gameplay metadata entries exactly by JSON value, including ground friction, existing bowling/cup/duck/drum tuning, and the fixed gutter compound. The gutter is deliberately excluded from the pickup allowlist. This audit changes no catalog visual entries, GLBs, PNGs, maps or baked packages.

<a id="console-pickup-audit-0110-complete-coverage"></a>
### Complete coverage

| Catalog population | Count | Decision |
| --- | ---: | --- |
| Reviewed pickup choices | 58 | Explicit allowlist below |
| Other model entries | 312 | Exact-ID exclusion ledger below |
| `decal` entries | 18 | Non-model assets; cannot be given as a pickup |
| `light` entries | 6 | Non-model assets; cannot be given as a pickup |
| `material` entries | 100 | Non-model assets; cannot be given as a pickup |
| `texture` entries | 99 | Non-model assets; cannot be given as a pickup |
| **Total catalog entries** | **593** | **Every entry accounted for** |

Non-model exclusions are all entries of the listed catalog types, not a partial sample. They comprise 100 material definitions, 99 PNG textures, six fixture-face light assets and 18 decals. They have no independent physical model. Trees/buildings/roads are never admitted because their dimensions happen to pass a generic bound.

<a id="console-pickup-audit-0110-reviewed-pickup-ledger"></a>
### Reviewed pickup ledger

Sizes below are measured occupied GLB spans in X × Y × Z metres, not guesses from catalog dimensions. Colliders are contact approximations; source artwork and model geometry remain intact. Explicit collider dimensions and coefficients are recorded in the sidecar. `new` means a profile was added by this audit; `existing` means its previous values were retained.

| Stable ID | Display name | Measured size (m) | Collider | Mass (kg) | Profile |
| --- | --- | --- | --- | ---: | --- |
| `arcade:bowling_ball_black` | Bowling Ball Black | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_burgundy` | Bowling Ball Burgundy | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_cobalt` | Bowling Ball Cobalt | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_green` | Bowling Ball Green | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_marbled` | Bowling Ball Marbled | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_purple` | Bowling Ball Purple | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_ball_red` | Bowling Ball Red | 0.216000 × 0.216000 × 0.216000 | sphere | 5 | existing |
| `arcade:bowling_pin` | Bowling Pin | 0.120000 × 0.380000 × 0.120000 | existing compound hulls | 1.5 | existing |
| `arcade:cat_plush` | Tuxedo Cat Plush | 0.192400 × 0.360000 × 0.157609 | cuboid | 0.12 | new |
| `arcade:litter_bottle_fallen` | Litter Bottle Fallen | 0.096000 × 0.096000 × 0.280000 | cuboid | 0.35 | new |
| `arcade:litter_can` | Litter Can | 0.066000 × 0.120000 × 0.065937 | cylinder | 0.018 | new |
| `arcade:litter_cup` | Litter Cup | 0.088871 × 0.140000 × 0.089991 | cylinder | 0.025 | existing |
| `arcade:litter_paper` | Litter Paper | 0.210000 × 0.009900 × 0.148000 | cuboid | 0.008 | new |
| `arcade:litter_wrapper` | Litter Wrapper | 0.120000 × 0.010200 × 0.085000 | cuboid | 0.004 | new |
| `arcade:party_balloon_weight` | Party Balloon Weight | 0.070000 × 0.054418 × 0.070000 | cylinder | 0.15 | new |
| `arcade:party_cake_lit` | Party Cake Lit | 0.360000 × 0.236000 × 0.360000 | compound plate/body cylinders | 1.4 | new |
| `arcade:party_cake_unlit` | Party Cake Unlit | 0.360000 × 0.218000 × 0.360000 | compound plate/body cylinders | 1.4 | new |
| `arcade:party_cup_balloons` | Party Cup Balloons | 0.085000 × 0.122000 × 0.085000 | cylinder | 0.025 | existing |
| `arcade:party_cup_confetti` | Party Cup Confetti | 0.085000 × 0.122000 × 0.085000 | cylinder | 0.025 | existing |
| `arcade:party_cup_geometric` | Party Cup Geometric | 0.085000 × 0.122000 × 0.085000 | cylinder | 0.025 | existing |
| `arcade:party_cup_stars` | Party Cup Stars | 0.085000 × 0.122000 × 0.085000 | cylinder | 0.025 | existing |
| `arcade:party_gift_arcade` | Party Gift Arcade | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_confetti` | Party Gift Confetti | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_green` | Party Gift Green | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_red` | Party Gift Red | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_stars` | Party Gift Stars | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_gift_stripes` | Party Gift Stripes | 0.303000 × 0.363415 × 0.303000 | compound box (ribbon/bow excluded) | 0.7 | new |
| `arcade:party_hat_confetti` | Party Hat Confetti | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_hat_dots` | Party Hat Dots | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_hat_shapes` | Party Hat Shapes | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_hat_stars` | Party Hat Stars | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_hat_stripes` | Party Hat Stripes | 0.150000 × 0.220000 × 0.150000 | convex_hull | 0.025 | new |
| `arcade:party_plate_balloons` | Party Plate Balloons | 0.220000 × 0.016000 × 0.220000 | cylinder | 0.025 | new |
| `arcade:party_plate_border` | Party Plate Border | 0.220000 × 0.016000 × 0.220000 | cylinder | 0.025 | new |
| `arcade:party_plate_confetti` | Party Plate Confetti | 0.220000 × 0.016000 × 0.220000 | cylinder | 0.025 | new |
| `arcade:party_plate_stars` | Party Plate Stars | 0.220000 × 0.016000 × 0.220000 | cylinder | 0.025 | new |
| `arcade:soda_bottle` | Soda Bottle | 0.096000 × 0.280000 × 0.096000 | cylinder | 0.35 | existing |
| `beach:crate` | Beach Crate | 0.820000 × 0.820000 × 0.820000 | cuboid | 12 | new |
| `core:cardboard_box` | Cardboard Box | 0.500000 × 0.500000 × 0.500000 | cuboid | 1.5 | new |
| `core:crate` | Crate | 0.600000 × 0.600000 × 0.600000 | cuboid | 8 | new |
| `core:plant` | Plant | 0.412423 × 0.985000 × 0.418366 | cuboid | 4 | new |
| `core:rubber_duck` | Rubber Duck | 0.098000 × 0.120000 × 0.145000 | cuboid | 0.065 | existing |
| `core:tv` | Television | 1.100000 × 0.700000 × 0.100000 | cuboid | 12 | new |
| `core:washer_drum` | Washer Drum | 0.420000 × 0.300000 × 0.420000 | cuboid | 6.4 | existing |
| `home:book_stack` | Two Stacked Books | 0.250000 × 0.055000 × 0.180000 | cuboid | 1.1 | new |
| `home:bowl` | Cereal Bowl | 0.150000 × 0.065000 × 0.150000 | cylinder | 0.25 | new |
| `home:crt_tv` | CRT Television | 0.550000 × 0.480000 × 0.460000 | cuboid | 9 | new |
| `home:cushion` | Checked Throw Cushion | 0.330000 × 0.320000 × 0.115000 | cuboid | 0.25 | new |
| `home:fork` | Table Fork | 0.026000 × 0.012000 × 0.196000 | cuboid | 0.045 | new |
| `home:kettle` | Home Kettle | 0.222500 × 0.201000 × 0.162000 | cuboid | 0.9 | new |
| `home:knife` | Table Knife | 0.021000 × 0.016000 × 0.215000 | cuboid | 0.065 | new |
| `home:landscape_frame` | Small Framed Landscape | 0.450000 × 0.500000 × 0.030000 | cuboid | 0.6 | new |
| `home:mug` | Ceramic Coffee Mug | 0.117000 × 0.090000 × 0.078000 | cuboid | 0.32 | new |
| `home:plant_table` | Small Potted Plant | 0.130000 × 0.280484 × 0.130000 | cuboid | 0.45 | new |
| `home:plate` | Dinner Plate | 0.220000 × 0.022000 × 0.220000 | cylinder | 0.35 | new |
| `home:spoon` | Table Spoon | 0.036000 × 0.020000 × 0.185000 | cuboid | 0.05 | new |
| `home:toaster` | Home Toaster | 0.252000 × 0.161000 × 0.168000 | cuboid | 1.2 | new |
| `pool:pool_potted_plant` | Pool Potted Plant | 0.588164 × 0.860127 × 0.561670 | cuboid | 8 | new |

The list includes every four party-cup prints, all four party-plate prints, five hats, six gifts, both cakes, seven bowling-ball finishes, the bowling pin, upright and fallen bottles, litter cup/can/paper/wrapper, tableware/cutlery, household loose objects, three portable potted plants, two wooden crates plus the cardboard box, the washer drum, rubber duck, and cat plush. The two-book stack stays one rigid assembly. Cushion/plush deformation, liquid motion in the mug, and leaf deformation are outside the existing rigid-item approximation.

Masses are deliberate gameplay approximations: 4–65 g for lightweight litter/cutlery, 25 g party paper, 0.7 kg boxed gifts, 1.4 kg cakes, 5 kg bowling balls, and up to 12 kg crates/televisions. Bow and flame details do not inflate their load-bearing colliders. For the horizontal fallen bottle, a cuboid follows its horizontal measured envelope instead of incorrectly using the local-Y upright cylinder.

Hollow/light items use explicit source-volume displacement where needed: paper 8 cm³, wrapper 4 cm³, empty can 20 cm³, paper plate 30 cm³ and paper hat 40 cm³. These values avoid treating economical solid contact envelopes as sealed displacement. The retained duck/drum water tuning remains intact. Every primitive profile was checked against density ≥10 kg/m³ and the existing 120 Hz buoyancy stiffness bound; all pass. Party hats use bounded nondegenerate nine-point cone hulls, and cake/gift compounds use only one or two ordinary primitives.

<a id="console-pickup-audit-0110-lit-cake-dependency-review"></a>
### Lit-cake dependency review

`arcade:party_cake_lit` retains the accepted GLB unchanged: 1,164 triangles, eight root scene nodes/meshes, no skin and no morph targets. Node 0 contains the cake, plate and seven candle shafts. Nodes 1–7 (`candle_flame_00` through `candle_flame_06`) contain the tiny separate flame geometry, already embedded inside this GLB. There is no separate catalog flame/candle model to create or spawn.

Its `candle_flicker` clip has seven LINEAR scale channels, targeting only the flame nodes. It never translates the cake body. Flame material slot 2, `party_tiny_candle_flame.002`, retains embedded emissive factor `[0.800000012, 0.439999998, 0.071999997]`. The cake spans `[-0.180000007, 0, -0.180000007]` to `[0.180000007, 0.235999994, 0.180000007]`; the unlit export stops at Y=0.217999995. `rigid_runtime_model_bounds` alone cannot accept the lit model because it has a clip. Runtime integration must explicitly use the supported animated-item/character transform path for this entry, follow the physics item in full orientation, and play `candle_flicker`.

Both cakes use the same two-part model-origin compound: a plate cylinder at `[0, 0.0035, 0]`, radius 0.18 and half-height 0.0035; and a cake-body cylinder at `[0, 0.0695, 0]`, radius 0.155 and half-height 0.0625. This preserves floor contact at Y=0 and body top Y=0.132 without inventing a broad collider around thin candle shafts/flames.

The reference Arcade placement (`party_cake_cake` in `levels/retro_arcade_demo.json`) also adds a separate `glow` component: colour `[1, 0.49, 0.16]`, intensity 0.28, range 0.8, offset `[0, 0.218, 0]`, enabled, no fade. That illumination is not contained in the GLB material. The console stage retains it through `pickup_catalog::runtime_glow` and the renderer's bounded attached-character light path, transformed with the physical cake. Failed light allocation rolls back the whole spawn; removal and suspension cleanly remove or hide its light. The embedded visible emissive flame material and clip remain unchanged.

<a id="console-pickup-audit-0110-exclusion-rationale"></a>
### Exclusion rationale

| Code | Count | Reason |
| --- | ---: | --- |
| B | 1 | Balloon and long string use dedicated attachment/animation behavior; measured height 1.53 m also exceeds give limit. |
| C | 12 | Character/creature rig with entity lifecycle; use the separate representation choices, not give pickup. |
| D | 13 | Draped cloth or hanging paper decoration; preserve its installation rather than make a stiff pickup shell. |
| E | 2 | Campfire scenery includes flames and map-owned illumination; not a loose carryable item. |
| F | 56 | Furniture or installed appliance/equipment assembly; outside loose handheld pickup scope. |
| L | 17 | Mounted or wired light fixture with map-owned illumination/attachment. |
| M | 15 | Installed fitting/sign/large artwork with a wall, ceiling, plumbing or decal contract. |
| S | 162 | Fixed architectural, road, railing, containment or surface module. |
| T | 34 | Terrain, rooted vegetation, environmental ice/snow/rock, or outdoor seating scenery. |

Some excluded objects fit the dimensional ceiling: chairs/tables, fixed monitors, mounted dispensers, light housings, rails, floor panels, individual icicles and snow caps are examples. Their installed or environmental use, rather than a broad dimension/category filter, keeps them out. Small framed landscape artwork is self-contained and portable; the large frame instead depends on its separate wall-art/decal composition. Portable potted plants are distinct from rooted trees, grass and shrub scenery.

The ledger records the maximum measured dimension to make size decisions inspectable. A value over 1.5 m is an additional rejection even if the reason code already excludes it. There are no unclassified model IDs.

| Excluded stable ID | Maximum measured dimension (m) | Reason |
| --- | ---: | --- |
| `arcade:ball_rack` | 1.100000 | F |
| `arcade:ball_return` | 1.810000 | F |
| `arcade:bench_three_seat_blue_red_blue` | 1.980000 | F |
| `arcade:bench_three_seat_red_blue_red` | 1.980000 | F |
| `arcade:blacklight_fixture` | 1.240000 | L |
| `arcade:cabinet_action` | 2.040000 | F |
| `arcade:cabinet_fighting` | 2.040000 | F |
| `arcade:cabinet_racing` | 2.040000 | F |
| `arcade:cabinet_scifi` | 2.040000 | F |
| `arcade:cabinet_shooter` | 2.040000 | F |
| `arcade:change_machine` | 1.460000 | F |
| `arcade:fixed_chair_blue` | 0.920000 | F |
| `arcade:fixed_chair_red` | 0.920000 | F |
| `arcade:gutter_segment` | 3.000000 | S |
| `arcade:lane_divider` | 3.000000 | S |
| `arcade:neon_bowl` | 1.820000 | L |
| `arcade:party_balloon` | 1.530000 | B |
| `arcade:party_cloth_rect_arcade` | 2.220000 | D |
| `arcade:party_cloth_rect_balloons` | 2.220000 | D |
| `arcade:party_cloth_rect_confetti` | 2.220000 | D |
| `arcade:party_cloth_rect_stars` | 2.220000 | D |
| `arcade:party_cloth_round_arcade` | 1.292000 | D |
| `arcade:party_cloth_round_balloons` | 1.292000 | D |
| `arcade:party_cloth_round_confetti` | 1.292000 | D |
| `arcade:party_cloth_round_stars` | 1.292000 | D |
| `arcade:party_streamer_blue` | 0.700120 | D |
| `arcade:party_streamer_orange` | 0.700120 | D |
| `arcade:party_streamer_pink` | 0.700120 | D |
| `arcade:party_streamer_purple` | 0.700120 | D |
| `arcade:party_streamer_yellow` | 0.700120 | D |
| `arcade:party_table_rect` | 2.100000 | F |
| `arcade:party_table_round` | 1.200000 | F |
| `arcade:pedestal_table` | 0.900000 | F |
| `arcade:pinball_comet` | 1.940000 | F |
| `arcade:pinball_cosmic` | 1.940000 | F |
| `arcade:restroom_grab_bar` | 0.930000 | M |
| `arcade:restroom_mirror_frame` | 0.968000 | M |
| `arcade:restroom_paper_holder` | 0.191000 | M |
| `arcade:restroom_sink` | 0.680000 | M |
| `arcade:restroom_soap_dispenser` | 0.240000 | M |
| `arcade:restroom_stall_divider` | 2.500000 | S |
| `arcade:restroom_stall_door` | 1.680000 | S |
| `arcade:restroom_stall_front` | 1.996000 | S |
| `arcade:restroom_stall_front_wide` | 1.996000 | S |
| `arcade:restroom_toilet` | 0.883000 | M |
| `arcade:score_monitor` | 1.120000 | F |
| `arcade:score_monitor_inactive` | 1.120000 | F |
| `arcade:vending_machine` | 2.030000 | F |
| `beach:bunting` | 4.381779 | S |
| `beach:coastal_rock` | 1.200000 | T |
| `beach:coastal_rock_wide` | 2.600000 | T |
| `beach:cove_foam` | 48.000000 | T |
| `beach:crab` | 0.832090 | C |
| `beach:dock` | 4.400000 | S |
| `beach:fish` | 0.638000 | C |
| `beach:grassy_bank` | 8.000000 | T |
| `beach:hut_brown` | 3.550000 | S |
| `beach:island` | 10.000000 | T |
| `beach:kiosk` | 3.640000 | S |
| `beach:lighthouse` | 7.700000 | S |
| `beach:lounge_chair` | 1.460459 | F |
| `beach:palm` | 6.000000 | T |
| `beach:palm_small` | 4.200000 | T |
| `beach:sand_patch` | 8.000000 | T |
| `beach:sea_arch` | 13.000000 | T |
| `beach:seagull` | 1.096000 | C |
| `beach:shoreline_foam` | 8.000000 | T |
| `beach:signpost` | 1.760000 | S |
| `beach:stilt_house_blue` | 6.080000 | S |
| `beach:town_arch` | 3.600000 | S |
| `beach:town_house_blue` | 5.450000 | S |
| `beach:town_house_coral` | 4.000000 | S |
| `beach:town_house_cream` | 5.400000 | S |
| `beach:town_parapet` | 2.400000 | S |
| `beach:town_shrub` | 1.300000 | T |
| `beach:town_stairs` | 4.200000 | S |
| `beach:town_terrace` | 4.030000 | S |
| `beach:town_terrace_base` | 4.000000 | S |
| `beach:umbrella` | 2.550000 | F |
| `carved-pumpkin` | 0.631126 | C |
| `core:armchair` | 0.900000 | F |
| `core:bed` | 2.000000 | F |
| `core:bookshelf` | 1.800000 | F |
| `core:cabinet` | 0.900000 | F |
| `core:ceiling_diffuser_01` | 0.600000 | M |
| `core:chair` | 0.900000 | F |
| `core:couch` | 2.000000 | F |
| `core:desk` | 1.600000 | F |
| `core:exit_sign` | 0.570000 | M |
| `core:fridge` | 1.800000 | F |
| `core:hot_tub` | 2.670000 | F |
| `core:lamp` | 1.500000 | L |
| `core:office_filing_cabinet_tall` | 1.320000 | F |
| `core:office_fluorescent_panel_housing` | 1.200000 | L |
| `core:office_noticeboard` | 1.000000 | M |
| `core:office_reception_desk` | 2.400000 | F |
| `core:office_snack_machine` | 1.900000 | F |
| `core:painting_frame_landscape_01` | 1.900000 | M |
| `core:pool_chair` | 0.850000 | F |
| `core:pool_curtain_corner` | 2.600000 | S |
| `core:pool_curtain_end` | 2.600000 | S |
| `core:pool_curtain_straight` | 2.600000 | S |
| `core:pool_guardrail_corner` | 1.050000 | S |
| `core:pool_guardrail_end` | 1.050000 | S |
| `core:pool_guardrail_straight` | 2.000000 | S |
| `core:pool_ladder` | 2.195000 | S |
| `core:pool_light_round_housing` | 0.500000 | L |
| `core:pool_light_wall_housing` | 0.400000 | L |
| `core:pool_table` | 0.800000 | F |
| `core:rug` | 2.000000 | S |
| `core:sink` | 1.100000 | F |
| `core:stop_sign` | 1.800000 | M |
| `core:stove` | 0.900000 | F |
| `core:table` | 1.400000 | F |
| `core:table_sideways` | 1.400000 | F |
| `core:vending_machine` | 1.900000 | F |
| `core:washing_machine` | 0.850000 | F |
| `core:water_cooler` | 1.100000 | F |
| `entity:cockroach` | 0.132125 | C |
| `frutiger_aero:accent_panel` | 2.200000 | S |
| `frutiger_aero:atrium_dome` | 12.000000 | S |
| `frutiger_aero:banner_atrium` | 2.400000 | S |
| `frutiger_aero:banner_corridor` | 2.400000 | S |
| `frutiger_aero:banner_reception` | 2.400000 | S |
| `frutiger_aero:bubble_sculpture` | 2.700000 | S |
| `frutiger_aero:ceiling_ring` | 1.300000 | S |
| `frutiger_aero:city_backdrop` | 23.850000 | S |
| `frutiger_aero:display_kiosk` | 2.400000 | S |
| `frutiger_aero:double_doors` | 3.200000 | S |
| `frutiger_aero:double_doors_open` | 3.200000 | S |
| `frutiger_aero:fountain_basin` | 3.200000 | S |
| `frutiger_aero:glass_canopy` | 3.600000 | S |
| `frutiger_aero:glass_partition` | 3.495000 | S |
| `frutiger_aero:green_backdrop` | 24.000000 | S |
| `frutiger_aero:light_pod` | 3.600000 | S |
| `frutiger_aero:lime_arch` | 3.600000 | S |
| `frutiger_aero:portal_spandrel` | 3.600000 | S |
| `frutiger_aero:reception_counter` | 4.500000 | S |
| `frutiger_aero:reception_soffit` | 5.200000 | S |
| `frutiger_aero:seating_pod` | 2.400000 | S |
| `frutiger_aero:tree_planter` | 3.008870 | S |
| `frutiger_aero:wall_bay` | 3.600000 | S |
| `frutiger_aero:wall_bay_tall` | 4.200000 | S |
| `home:armchair` | 0.925000 | F |
| `home:ball_light` | 0.799484 | L |
| `home:bookshelf` | 1.800000 | F |
| `home:cabinet_base` | 0.900000 | F |
| `home:cabinet_strip` | 0.550000 | L |
| `home:cabinet_wall` | 0.720000 | F |
| `home:coffee_table` | 1.400000 | F |
| `home:dining_chair` | 0.902000 | F |
| `home:dining_chair_refined` | 0.902000 | F |
| `home:dining_table` | 1.400000 | F |
| `home:door_casing` | 2.180000 | S |
| `home:floor_lamp` | 1.500000 | L |
| `home:outlet` | 0.120000 | M |
| `home:rug` | 2.000000 | S |
| `home:sink` | 1.089000 | F |
| `home:sofa` | 2.000000 | F |
| `home:stove` | 0.916000 | F |
| `home:tv_console` | 1.550000 | F |
| `home:wall_switch` | 0.120000 | M |
| `mannequin` | 1.723035 | C |
| `outdoor:boundary_ridge` | 20.000000 | T |
| `outdoor:bush_low` | 1.700000 | T |
| `outdoor:bush_round` | 1.550000 | T |
| `outdoor:campfire_static` | 1.300000 | E |
| `outdoor:collision_peg` | 0.060000 | S |
| `outdoor:concrete_step` | 1.400000 | S |
| `outdoor:fence_post` | 1.050000 | S |
| `outdoor:fence_two_rail` | 2.000000 | S |
| `outdoor:fence_two_rail_post` | 1.050000 | S |
| `outdoor:fence_two_rail_section` | 1.780000 | S |
| `outdoor:grass_patch_large` | 0.950000 | T |
| `outdoor:grass_patch_small` | 0.550000 | T |
| `outdoor:house_01_corner_trim` | 2.700000 | S |
| `outdoor:house_01_gable` | 3.000000 | S |
| `outdoor:house_01_porch_deck` | 3.000000 | S |
| `outdoor:house_01_porch_post` | 2.300000 | S |
| `outdoor:house_01_roof_ridge` | 3.400000 | S |
| `outdoor:house_01_roof_slope` | 3.400000 | S |
| `outdoor:house_01_wall_doorway` | 3.000000 | S |
| `outdoor:house_01_wall_solid` | 3.000000 | S |
| `outdoor:house_01_wall_window` | 3.000000 | S |
| `outdoor:house_02_corner_trim` | 2.700000 | S |
| `outdoor:house_02_gable` | 3.000000 | S |
| `outdoor:house_02_porch_deck` | 3.000000 | S |
| `outdoor:house_02_porch_post` | 2.300000 | S |
| `outdoor:house_02_roof_ridge` | 3.400000 | S |
| `outdoor:house_02_roof_slope` | 3.400000 | S |
| `outdoor:house_02_wall_doorway` | 3.000000 | S |
| `outdoor:house_02_wall_solid` | 3.000000 | S |
| `outdoor:house_02_wall_window` | 3.000000 | S |
| `outdoor:house_03_corner_trim` | 2.700000 | S |
| `outdoor:house_03_gable` | 3.000000 | S |
| `outdoor:house_03_porch_deck` | 3.000000 | S |
| `outdoor:house_03_porch_post` | 2.300000 | S |
| `outdoor:house_03_roof_ridge` | 3.400000 | S |
| `outdoor:house_03_roof_slope` | 3.400000 | S |
| `outdoor:house_03_wall_doorway` | 3.000000 | S |
| `outdoor:house_03_wall_solid` | 3.000000 | S |
| `outdoor:house_03_wall_window` | 3.000000 | S |
| `outdoor:house_04_corner_trim` | 2.700000 | S |
| `outdoor:house_04_gable` | 3.000000 | S |
| `outdoor:house_04_porch_deck` | 3.000000 | S |
| `outdoor:house_04_porch_post` | 2.300000 | S |
| `outdoor:house_04_roof_ridge` | 3.400000 | S |
| `outdoor:house_04_roof_slope` | 3.400000 | S |
| `outdoor:house_04_wall_doorway` | 3.000000 | S |
| `outdoor:house_04_wall_solid` | 3.000000 | S |
| `outdoor:house_04_wall_window` | 3.000000 | S |
| `outdoor:house_05_corner_trim` | 2.700000 | S |
| `outdoor:house_05_gable` | 3.000000 | S |
| `outdoor:house_05_porch_deck` | 3.000000 | S |
| `outdoor:house_05_porch_post` | 2.300000 | S |
| `outdoor:house_05_roof_ridge` | 3.400000 | S |
| `outdoor:house_05_roof_slope` | 3.400000 | S |
| `outdoor:house_05_wall_doorway` | 3.000000 | S |
| `outdoor:house_05_wall_solid` | 3.000000 | S |
| `outdoor:house_05_wall_window` | 3.000000 | S |
| `outdoor:house_corner_trim` | 2.700000 | S |
| `outdoor:house_roof_ridge` | 3.400000 | S |
| `outdoor:house_roof_slope` | 3.400000 | S |
| `outdoor:house_wall_doorway` | 3.000000 | S |
| `outdoor:house_wall_solid` | 3.000000 | S |
| `outdoor:house_wall_window` | 3.000000 | S |
| `outdoor:lamp_fence` | 0.420000 | L |
| `outdoor:lamp_stand` | 1.050000 | L |
| `outdoor:lamp_wall` | 0.520000 | L |
| `outdoor:masonry_pier` | 1.200000 | S |
| `outdoor:porch_canopy` | 3.400000 | S |
| `outdoor:porch_post` | 1.050000 | S |
| `outdoor:porch_railing_corner` | 0.910000 | S |
| `outdoor:porch_railing_end` | 1.050000 | S |
| `outdoor:porch_railing_straight` | 1.800000 | S |
| `outdoor:road_barrier` | 3.400000 | S |
| `outdoor:showcase_boulder` | 1.400000 | T |
| `outdoor:showcase_campfire` | 1.300000 | E |
| `outdoor:showcase_curb` | 6.000000 | S |
| `outdoor:showcase_curb_ramp` | 2.400000 | S |
| `outdoor:showcase_road_dash` | 1.500000 | S |
| `outdoor:showcase_road_gate` | 8.000000 | S |
| `outdoor:showcase_road_straight` | 8.000000 | S |
| `outdoor:showcase_road_unmarked` | 8.000000 | S |
| `outdoor:showcase_rock_face` | 5.000000 | T |
| `outdoor:showcase_rock_face_variant` | 4.800000 | T |
| `outdoor:showcase_sidewalk` | 6.000000 | S |
| `outdoor:showcase_sidewalk_corner` | 2.400000 | S |
| `outdoor:showcase_sidewalk_transition` | 2.400000 | S |
| `outdoor:showcase_stump_seat` | 0.700000 | T |
| `outdoor:streetlight` | 6.400000 | L |
| `outdoor:tree_01` | 6.400000 | T |
| `outdoor:tree_02` | 6.200000 | T |
| `outdoor:tree_03` | 6.800000 | T |
| `pool:pool_basin_tile_module` | 0.500000 | S |
| `pool:pool_bench` | 1.600000 | F |
| `pool:pool_ceiling_panel_module` | 0.600000 | S |
| `pool:pool_deck_tile_module` | 0.500000 | S |
| `pool:pool_drain` | 0.600000 | M |
| `pool:pool_entry_steps` | 1.200000 | S |
| `pool:pool_safety_sign` | 0.400000 | M |
| `pool:pool_service_door` | 2.100000 | S |
| `pool:pool_wall_tile_module` | 0.500000 | S |
| `pumpkin-skeleton` | 1.822028 | C |
| `rat` | 0.462770 | C |
| `sheet-ghost` | 1.619323 | C |
| `sheet-ghost-cat` | 0.593000 | C |
| `skeleton` | 1.722652 | C |
| `spooner-man` | 0.543769 | C |
| `winter:boulder_snow` | 1.400000 | T |
| `winter:door_hood_snow` | 1.940000 | S |
| `winter:drift_fence` | 1.800000 | T |
| `winter:drift_large` | 4.400000 | T |
| `winter:drift_medium` | 2.400000 | T |
| `winter:drift_small` | 1.200000 | T |
| `winter:drift_wall` | 3.000000 | T |
| `winter:entrance_frame` | 2.420000 | S |
| `winter:fence_post_snow` | 1.126000 | S |
| `winter:fence_two_rail_post_snow` | 1.127000 | S |
| `winter:fence_two_rail_section_snow` | 1.780000 | S |
| `winter:ice_fragment` | 1.100000 | T |
| `winter:icicle_cluster_mixed` | 1.200000 | S |
| `winter:icicle_cluster_sparse` | 1.200000 | S |
| `winter:icicle_long` | 0.650000 | S |
| `winter:icicle_medium` | 0.360000 | S |
| `winter:icicle_short` | 0.180000 | S |
| `winter:masonry_pier_snow` | 1.250000 | S |
| `winter:mound_irregular` | 2.100000 | T |
| `winter:railing_snow_corner` | 0.976000 | S |
| `winter:railing_snow_corner_assembly` | 1.124000 | S |
| `winter:railing_snow_end` | 1.126000 | S |
| `winter:railing_snow_straight` | 1.800000 | S |
| `winter:railing_snow_straight_assembly` | 1.800000 | S |
| `winter:rock_face_snow` | 5.160000 | T |
| `winter:snow_awning` | 2.600000 | S |
| `winter:snow_door_overhang` | 1.600000 | S |
| `winter:snow_ledge` | 1.200000 | S |
| `winter:snow_porch_edge` | 1.800000 | S |
| `winter:snow_post_cap` | 0.120000 | S |
| `winter:snow_rail_top` | 1.200000 | S |
| `winter:snow_roof_blanket` | 6.811420 | S |
| `winter:snow_roof_edge` | 2.400000 | S |
| `winter:snow_roof_slope` | 3.400000 | S |
| `winter:snow_stair_edge` | 0.800000 | S |
| `winter:stone_wall_snow` | 2.400000 | S |
| `winter:string_lights_long` | 6.650000 | L |
| `winter:string_lights_medium` | 4.050000 | L |
| `winter:string_lights_short` | 2.850000 | L |
| `winter:timber_lantern` | 3.200000 | L |
| `winter:tree_snow_01` | 6.800000 | T |
| `winter:tree_snow_02` | 6.800000 | T |
| `winter:window_frame` | 1.260000 | S |

<a id="console-pickup-audit-0110-verification-and-outstanding-native-evidence"></a>
### Verification and outstanding native evidence

- `python3 tools/assets/validate.py`: passed; 593 assets / 370 placeable; zero warnings.
- `python3 tools/props/build.py --check`: passed for all 370 model identities; unique family image residency 77.793 MiB (exact-PNG identity). Existing reviewed geometry/art-budget notes remain.
- Nonmutating complete GLB inventory: all 370 parsed; zero missing/failed models; 16 carry skin/animation declarations. Only the reviewed flame-animated cake enters this pickup list.
- Focused sidecar/list audit: 58 unique sorted exact IDs; every entry exists, has positive bounded measured geometry, strict-valid dynamic pickup metadata and a conservative radius within pickup reach; primitive water density/stiffness checks passed.
- `rustfmt --edition 2024 src/game/pickup_catalog.rs`: passed.

Full Cargo/Clippy/release gates and native execution belong to the integrating parent task; this subtask did not run a competing build or acquire the game window. No Rust tests were added: the work changes a reviewed data list and physical profiles, and the essential integration evidence is real give/list/pickup/drop behavior using the renderer and physics.

**Pending native acceptance:** give and pick up/drop at least a representative party cup, party plate, bottle, bowling ball, unlit cake and lit cake. The lit-cake check must show its original textured cake and embedded bright candle flames, active flicker, correct body orientation while carried/dropped, no orphaned static flame copy, and honest treatment of nearby glow/spill. Recheck the selected lightweight paper/hat profile in a water map. The outstanding item is execution evidence in the integrated runtime, not missing artwork or metadata.

Review identity: `assets/catalog.json` SHA-256 `020afb28566d36b5f096cd203826d281abc357aaf1b18cc83cbae3f851e82c62`.

<a id="dependency-upgrade-0110"></a>
## Rust and dependency prerequisite for 0.11.0

The staging build selects exact Rust 1.99.1. Cargo's `rust-version = "1.99"`
remains the MSRV. This host reuses its matching installed stable toolchain with
an override scoped to Places; the committed pin still controls CI/releases.

All fourteen direct dependencies were checked against the official crates.io
registry on 2026-10-16, excluding prereleases and yanked releases. Four resolved
direct versions changed: glam 0.29.3 → 0.34.1, serde_json 1.0.151 → 1.0.154,
sha2 0.10.9 → 0.11.1, and zip 8.6.0 → 9.0.3. Requirements for the other ten
now name their verified latest stable release where necessary. The lock refresh
selects thirty new/updated packages in total. The highest declared dependency
MSRV is 1.90. Older transitive glam, hashbrown, miniz_oxide and syn families
remain where upstream constraints require them; no patches or forced pins were
introduced.

The glam migration selects the corresponding right-handed camera functions,
retaining each original OpenGL or zero-to-one depth convention, projection
arguments, clip correction and probe Y flip. Upstream projection arithmetic can
round differently. SHA-2 output uses the existing lowercase, zero-padded hex
formatter, including streaming executable and physics-state identities. ZIP
retains the Deflate zlib-rs backend, compression level, timestamps, permissions
and bounded archive validation. Shader bytes, physics policy, executable names,
asset resolution, package versions and authored data are unchanged. No Rust
tests were added, removed or weakened. Independent source review found no issue.

Validation on macOS, Apple M2 Pro / Metal / SDL 3.4.18:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo check --locked --workspace --all-targets --all-features` | Passed; 29.49 s |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed; 20.52 s |
| Same Clippy gate with `--locked --release` | Passed; 35.26 s |
| `cargo test --workspace` | 2,336 library + 7 integration passed; 19 existing ignores |
| `cargo test --locked --workspace --all-features` | 2,343 library + 7 integration passed; 19 existing ignores |
| `cargo build --locked --release --bins` | Passed; 161.81 s |
| `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'` | 363 run; 17 initial skips. All 14 geometry cases then passed with the new executable, resolving 13 skips; 4 optional skips remain |
| `python3 tools/assets/validate.py` | 593 assets, 370 placeable models, 8 themes; zero warnings |
| Independent source/version/feature review and `git diff --check` | Passed |

Rust commands used `RUSTC_WRAPPER=` for the host's sandbox-incompatible wrapper.
No shipped map was baked. The native map-compiler GUI completed `test_room`;
`validate`, `verify --require-current`, and incremental `build` passed, with
`rebuilt: false`. All twelve non-provenance package records match the baseline
byte for byte. Only tool/build provenance differs; ZIP size changed by one byte.
The native player loaded the existing Office package and displayed the no-map
menu with v0.11.0. Both apps closed normally. No permission prompt blocked these
checks, and no signing or OS permission setting was changed.

The Office captures have the same 3,024 × 1,676 dimensions and 60 ready
presentations each. Visual inspection found no scene or lighting regression;
0.2621% of pixels differ (mean absolute channel difference 0.000676 on the
0–255 scale, maximum 42), so the images are not byte-identical. Glam's changed
projection arithmetic is a source of numerical rasterization differences.
Short runs recorded median loop time 8.334 → 8.357 ms, entry-to-ready
2.882 → 5.476 s, and request-to-ready 1.841 → 1.320 s. Window creation accounted
for 0.659 → 3.964 s of those startup times. Release build wall time was
161.81 s versus the earlier stage's 135.78 s. These single, uncontrolled samples
are recorded without claiming general startup/runtime/build performance parity.

A bounded 48-frame, twelve-case physics comparison retained repeatability and
worker-count parity. Five full-state hashes changed. Temporary diagnostic copies
reproduced both original release receipts and compared 17,472 snapshot pairs:
physical poses/rotations, velocities, mass/inertia, water fractions, contacts,
impulses and render positions are bit-identical. Only render quaternion
components differ, at most two ULP; maximum normalized orientation difference
is 0.0000134°. This isolates glam's changed ARM NEON quaternion interpolation
rounding from solver behavior. No production workaround or assertion change
was needed; diagnostic instrumentation is excluded from the committed source.

Binaries, bounded package, captures, command logs, registry receipts and numerical
proof remain outside `target` in `local/dependency-upgrade-0.11.0/`. The predecessor
harness CI run [38855035823](https://github.com/csd113/Places/actions/runs/38855035823)
completed successfully. The upgrade commit's CI status is recorded separately in
the handoff receipt. Linux/Windows native execution and a controlled performance
campaign were not run. Renderer work and Rust-test removal remain later stages.

Version sources: [Rust 1.99.1 release](https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/),
[glam 0.34.1](https://crates.io/crates/glam/0.34.1),
[SHA-2 0.11.1](https://crates.io/crates/sha2/0.11.1), and
[ZIP 9.0.3](https://crates.io/crates/zip/9.0.3).

<a id="developer-console-0110"></a>
## Developer console — 0.11.0 staging

This stage extends the opt-in native console and addresses issue #22, including
its subsequent `/become` scope. Enable **Settings → Controls → Developer Console**
and press backtick in play. Escape closes it. Typing pauses the world, releases
the mouse and suppresses gameplay input. Each command accepts `help`;
`/help <command>` works too. History and scroll controls retain their existing
bounds. Ordinary play gains no tutorial or throw text.

| Command | Contract |
| --- | --- |
| `/give list` | 58 explicitly reviewed pickup IDs/names, sorted by stable ID; scroll for the complete list. |
| `/give <id>` | Exact allowlist lookup; always dynamic, pickup-enabled and immediately movable. |
| `/respawn` | Validate the standing spawn against map geometry and live item bodies, safely release any held item, then restore only the player. |
| `/pos` | Eye/feet in metres; yaw/pitch in degrees and facing vector. +Y is up, yaw 0 faces −Z and yaw 90 faces +X. |
| `/remove` | Requires and removes the held physical item. |
| `/remove all` | Requires a held item; removes matching canonical catalog identities from session and authored movable physics props. Fixed decorations survive. |
| `/physics pause\|resume\|step` | Pause/resume rigid-body simulation, or immediately advance exactly one normal 1/120-second tick and stay paused. |
| `/time scale <0..4>` | Finite bounded world multiplier: 1 normal, 0 stopped. |
| `/trigger <id>` | Enter a real trigger volume through normal event bindings, conditions, cooldowns and one-shot rules. |
| `/trigger show [on\|off]` | Toggle up to 16 nearby trigger-volume IDs within 12 m and a 60-degree half-cone. |
| `/become list` | Curated pickup representations and an explicit portable-character list. |
| `/become <model>` | Select the visual and enter third person; `party_cup` and `spooner_man` are aliases. |
| `/become place` | Copy only the selected model's current feet/yaw transform into a session prop. |
| `/become phys on\|off` | Configure future copies: default off; on starts dormant until pickup, physical activation or a typed trigger/game-mode activation. |
| `/become reset` | Safely release a held item and restore the normal first-person visual. |

<a id="developer-console-0110-reusable-ownership-and-lifetime"></a>
### Reusable ownership and lifetime

`game::gameplay_actions` defines typed requests. `game::gameplay_runtime` applies
them against a `GameplayContext` containing live game, renderer, item-world and
authored-item owners. `Runtime::apply_action` needs no console or command string.
The console parser and output adapter translate requests and present results;
future game modes/trigger adapters can use the same backend, including keyed
`ActivatePhysics` without adding a textual command. No editor, multiplayer,
persistence format, audio stage or caustics stage is introduced.

The [complete catalog audit](release-0.11.0.md#console-pickup-audit-0110) records all 593
entries, measured eligible bounds, masses/colliders and exact exclusions. The
allowlist and execution share `pickup_catalog::ITEMS/find`; physical coefficients
remain in the existing `assets/gameplay.json` sidecar. Asset eligibility does not
change an authored instance's physics choice. Trees, buildings, fixed scenery,
non-model resources and unreviewed models cannot bypass the allowlist by ID.

At most 16 session props exist across give and representation copies, with a
bounded 256-key allocation lane per developer session. Failed allocation rolls
back physics and visual resources; deletion frees live-prop capacity. Removals
clean bodies/colliders, held/contact state, visuals and authored entity handles,
components, names and listeners. Generated/trimmed authored IDs use the same
resolver as the entity world. Reload restores original authored content; source
JSON and compiled packages are never edited. Map replacement or disabling the
console removes session visuals. Player-only respawn preserves world objects;
generic player teleports detach held control at the item's retained valid pose.
A full authored `reset_to_start` additionally clears session props/representation,
labels and clock controls; a separate generation prevents player-only recovery
from deleting them or erasing commands submitted after the reset.

The existing 16-mesh dynamic renderer cache remains bounded and shared with map
objects. At capacity it replaces only unused asset slots, preserving live
objects and explicitly registered handles. GPU resources and visibility caches
follow replacement. Sequentially giving/removing different models therefore
does not exhaust capacity merely from retained cache history.

<a id="developer-console-0110-simulation-clocks-and-collision"></a>
### Simulation clocks and collision

Player motion/look, input and held-object geometric control use the unscaled
clock. World physics consumes the multiplier once in a bounded fixed-step
accumulator, retaining a 120 Hz solver and at most 12 catch-up steps per frame.
Excess work is discarded/reported rather than enlarging integration intervals.
Doors, route/AI/timer logic, weather, world animation and light-response timers
use bounded scaled world time. The selected player's animation uses the player
clock. Opening the console or menus still pauses both input-driven updates.

Explicit physics pause overrides a positive time scale for rigid bodies only;
other world systems retain their configured world clock. Resume respects scale
zero. Single-step discards the fractional accumulator and publishes exactly one
fixed tick immediately, even while typing or at scale zero, then stays paused.
Held control remains swept and collision constrained without a solver tick.
Release/throw retains the valid release pose and bounded real-time velocity,
without inverse scaling or division by zero. At zero/pause the released object
stays frozen until world physics advances. Player sweeps also constrain stopped
item bodies, including empty-handed walking, falls and head contacts.

Noclip exit retains a clear airborne eye/yaw/pitch and resumes gravity. Exiting
inside geometry uses a revalidated entry/spawn or refuses safely. Genuine void
recovery retains its ordinary grace path rather than floor-snapping an otherwise
clear airborne pose.
The current, remembered and spawn candidates also pass live physical-item
capsule clearance. Console disabling follows the same typed exit policy; a
rejected exit retains noclip and its current pose.

<a id="developer-console-0110-representation-and-rendering"></a>
### Representation and rendering

Player control, collision capsule, identity and interaction eye are unchanged.
Visual models keep their authored metre scale unless their largest extent exceeds
2 m, when they are uniformly reduced. Measured bounds anchor the bottom and
horizontal centre to player feet; +Z model fronts map correctly to player yaw.
The camera boom adapts to visual size and sweeps a sphere against static
geometry, floor/ceiling, live doors and physics items. Game aiming remains at the
player eye. A very small model therefore has a low close camera while its player
collision body retains normal human dimensions.

Rigid and rigged/clip-only models use existing cached renderer paths. Runtime
character transforms support full quaternion physics orientation independently
of animation. Physical copies use validated dormant placement and ordinary
activation/recovery. Interact activates a dormant copy that is too large to
carry, through the same occlusion-checked physical target path; smaller copies
can be picked up normally. Physics-off copies are stationary visual props with no
player-control or physics body. Selection is transactional: failed model/resource
validation or obstructed release retains the old representation and held item.
Every dormant activation path checks the full player capsule before changing
body type. A copy still enclosing the player stays frozen until the player moves
clear; ordinary surface contact does not prevent activation.

The lit cake retains all seven embedded flame nodes, `candle_flicker`, emissive
material and the reference attached warm glow through a shared `GlowCue` profile.
The bounded light budget rejects an unallocatable copy and rolls it back. Light
keys disappear with their owning character; suspended physics characters hide
both their geometry and glow. Trigger labels reuse interaction cone/LOS queries
and the flashlight's shared prepared triangle transmittance, including floor,
ceiling and moving occluders, independently of flashlight power.
Displayed IDs share the command parser's 128-byte logical-ID limit. Their exact
text scales to fit and occupies separate, edge-clamped screen rows when nearby
anchors overlap; at most sixteen labels and eighteen layout rows are considered.

<a id="developer-console-0110-verification"></a>
### Verification

The seven essential CPU regressions use empty/minimal in-memory worlds, at most two
unit cubes and no GPU, asset loading or map bake. They protect exact-tick/remainder
isolation and numerical scale boundaries, frozen carry/release, empty-hand frozen
player collision, reset detachment, generated-ID authored deletion/reset, the
original airborne noclip bug and dormant activation around the player. They live
in owning `engine/tests` and `game/tests`
folders with actual module wiring; the broad deleted suite is not restored.
Native acceptance remains explicit developer tooling outside Cargo/CI.
The activation fixture uses one cube and narrow-phase queries without a solver tick.

October 16 final gates passed:

- `cargo fmt --all --check` and `git diff --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`, plus
  the locked release Clippy gate.
- `cargo test --workspace`: seven tests, no failures, under 0.01 s of test work.
- `cargo build --locked --release --workspace --all-features`: all four
  production executables, 2m20s on this machine.
- `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'`:
  372 discovered, four skipped, OK in 103.8 s. No map bake was required.
- Catalog validation: 593 entries, 370 placeables, zero warnings. Prop generator
  check passed. Texture check passed for 123 textures with 96 existing manifest
  coverage warnings; no texture imagery changed in this stage.

The final direct executable's SHA-256 is
`c3b080fce2eeb37f3c0a19f0770bd57424a7023e7ae09bb413a83fb00a35fb83`.
Every accepted session below used that same executable on Apple M2 Pro / Metal.
The independently compiled small fixture's SHA-256 is
`47e7dcd9de740e59f495d64c942cbadee6fa5bbdb37aba199a194871bdba7883`;
the final compiler's `verify --require-current` reports no differences.

| Explicit native campaign | Passed assertions | Main evidence |
| --- | ---: | --- |
| Controls | 73 | Help, invalid input, actual E pickup/removal, exact/generated authored identities, fixed cup preservation, triggers/visibility, copies, full reset and real reload. |
| Models | 58 | Lit cake/flames/glow, pickup/drop/throw at zero, 90-degree rigid orientations, contact, camera obstruction, physical Spooner-Man, large rig activation refusal and ordinary movement out before activation. |
| Clock | 82 | Ordinary 500 ms W input moves about 1.5 m at 0, 0.25, 1, 2, 4 and paused 4; the held duck follows, released bodies freeze, exact stepping and pause/resume precedence remain correct. |
| Complete pickup catalog, four chunks | 620 | All 58 IDs successfully create one active movable body and exact session identity; each disable cleans physics, geometry, character and session resources before the next model. |
| Recovery edge cases | 15 | Exact airborne pose retention followed by natural landing over clear floor/ramp/stairs/near-wall positions; solid ceiling and physical-prop fallback; genuine void exit retains pose before delayed recovery. |
| Existing Office demo | 10 | Cup and Spooner-Man representations, movement/animation at world scale zero, animated physics-off duplicate, first-person reset and safe respawn. |
| **Total** | **858** | All native processes exited successfully. |

Accepted native sessions used desktop fullscreen, actual logical 1512×949 /
drawable **3024×1898**, with no missed ready presentations. Earlier occluded
windowed runs and diagnostic placement/pose failures are retained separately and
excluded. The native campaign found and corrected the retained-mesh capacity
leak, overlapping trigger text, and the live-body noclip exit case. Independent
state/resource review has no outstanding findings.

The capture-free Office timing run used High quality/filtering, Full lightmaps
and reflections, bloom on, V-sync off, no frame limiter and actual **Immediate**
presentation. After 240 warmup frames, 3,600 measured frames averaged **195.1 FPS**
(mean loop 5.126 ms, median swap 0.045 ms). All 3,840 ready frames presented.
The active built-in display reports 120 Hz; throughput exceeded it and did not
plateau at refresh, so this sample is uncapped. This is one view on one machine,
with no before/after performance improvement claimed. Capture sessions are not
used for FPS comparisons.

Preserved logs, direct executables, packages, action scripts, raw captures and
`verification.json` live outside `target` in `local/developer-console-0.11.0/`.
Curated full-frame native images:

- [Readable local trigger IDs](../images/developer-console-0.11.0/trigger-labels.png).
- [Held lit cake](../images/developer-console-0.11.0/cake-held.png).
- [Physical Spooner-Man at a 90-degree orientation](../images/developer-console-0.11.0/spooner-tilted.png).
- [Third-person camera near a wall](../images/developer-console-0.11.0/spooner-wall-camera.png).
- [Cup representation in Office](../images/developer-console-0.11.0/office-cup.png).
- [Spooner-Man walking in Office at stopped world time](../images/developer-console-0.11.0/office-cat-walking.png).

To repeat the maintained native campaign, explicitly build the small fixture
with `places-compile build tests/fixtures/native/developer-console.level.json
--out <fixture.placesmap> --asset-root assets`, then run
`tools/bench/console_stage.py --binary <direct-executable> --map
<fixture.placesmap> --fullscreen --mode controls` (also `clock`, `models`, and
`catalog --chunk 0` through `3`). These operations remain outside ordinary CI.

Limits: native acceptance covers macOS/Metal. Copies are session-local, player
collision remains the normal human capsule, and characters without a dedicated
physics profile use a validated coarse bounds collider and 20 kg default mass.
No anatomy-specific collision, multiplayer, editor, audio or caustics work is
included. Exact-commit CI and final branch/issue closure are recorded on
[issue #22](https://github.com/csd113/Places/issues/22).

<a id="entity-navigation-0110"></a>
## Entity updates and navigation correctness — 0.11.0 staging

Final engine pass based on `bd81c9f5a1e02e7341a1c16e76f900a92484df03` on
the user's Mac. Scope is entity lifecycle/scheduling, navigation, animation
transitions and avoidable CPU work. The completed renderer, shader, physics,
audio, loading and settings audits were not repeated.

<a id="entity-navigation-0110-demonstrated-defects-and-corrections"></a>
### Demonstrated defects and corrections

- **Sequence ownership.** Taking all runs out of the world while executing
  actions hid them from cancellation and discarded newly started runs when the
  retained collection was restored. Runs now stay authoritative in the world;
  a monotonic revision distinguishes even a same-sequence replacement. Only
  runs present at tick start execute, so new starts retain a consistent next-tick
  boundary. Cancellation clears its controller without emitting completion.
- **Sequence publication.** Completion omitted its documented sequence-id key;
  sequence actions dropped reset and other report fields; move/face steps did
  not dirty frames, and dynamic turns omitted renderer commands. These now use
  the existing event/command contracts. Reset ends the tick. Successful sequence
  transforms also update AI pose, so perception, renderer frames and resumed
  locomotion do not jump back to an older position.
- **Stopped time and release.** Zero-time AI ticks could plan and consume
  waypoints, and the mover treated zero travel as arrival. Simulation phases
  now require positive finite time while queued player actions can still
  dispatch. Agents leave `Scripted` when sequence ownership ends. Flashlight
  holds retain the authored state, route and timers and continue facing the player.
- **Navigation topology.** Nearest-point search returned the first occupied
  Chebyshev ring even when a later axial cell was closer. Searches compare
  candidates until a geometric bound proves the result. A* diagonal corners
  and shortcuts now share live door passability; shortcuts retain home bounds
  and the baked step/slope rule instead of accepting a flat cliff as a slope.
- **Pose transitions.** The first explicit cue ignored its saved source pose,
  and interrupted fades sampled an old clip instead of the displayed blend.
  Transitions now retain the displayed node/morph pose. Paused, completed and
  settled scrub cues retain their pose revision, with explicit restart preserved.
  A route that reaches its endpoint publishes completion in that update.

<a id="entity-navigation-0110-maintainability-and-bounded-cpu-work"></a>
### Maintainability and bounded CPU work

`engine/entities/sequence_runtime.rs` owns the sequence dispatcher and its
movement primitives. It copies the active step rather than the full authored
definition each tick and resolves the definition index once per run. Existing
64-live-run/64-step budgets and authored wait rounding remain intact.

Trapped prey now respect the existing 0.75-second retry cadence. Perception
uses eight stable phases, hidden agents skip sight work, and sight checks reuse
the world's door collider slice. Settled flashlight holds publish no redundant
transform; cue changes still dirty the renderer handoff. Named clip lookup no
longer allocates a lowercase string on every lookup. Completed routes retain
their handoff, and fades avoid a duplicate pre-update frame rebuild.

The deterministic fixture demonstrates fourteen successive 50 ms trapped-prey
ticks with zero path queries, followed by a permitted retry. Only one of eight
agents checks perception in the initial 10 ms phase. Held-pose regressions verify
unchanged revision across repeated updates. These are bounded work reductions,
not measured FPS improvements; distant agents keep their authored autonomous
behavior rather than acquiring a new distance-based sleep policy.

<a id="entity-navigation-0110-regression-value-and-evidence"></a>
### Regression value and evidence

Thirteen focused regressions live in the owning `src/engine/tests/` tree: seven
sequence lifecycle/publication/handoff checks, three stopped-time/retry/topology
checks and three route/pose checks. Fixtures contain at most 25 navigation
cells, eight agents, two metadata props or one synthetic node with three
two-key clips. They load no shipped assets, bake no map and need no GPU/device.
The first twelve all fail against the preserved pre-pass implementation while
the previous engine checks pass. The additional handoff regression protects
the integration defect found during review. The final workspace passes all
67 CPU tests in 0.08 seconds, with zero failures or ignores.

`tools/bench/entity_navigation.py` is explicit direct-executable acceptance
tooling, outside ordinary CI. It uses existing Office/Home packages and retains
isolated settings, action scripts, snapshots, PNGs and native logs. All receipts,
baseline failures, executable hashes, final exact-commit CI/remote verification
and cleanup/custody records are preserved in `local/entity-navigation-0.11.0/`.

The complete `RUSTC_WRAPPER= sh tools/check-rust.sh` gate passes: formatting,
all-target/all-feature check, strict debug and release Clippy, workspace tests
and the locked all-feature production build. The four production executables
are preserved under `bin/` with SHA-256 receipts; the player hash is
`f22b267f90e7848474754e156baed5a993a94a60681c614b1bca3be22eef3b98`.
Python source contracts pass 372 tests with 17 explicit skips in 94.129 seconds.

Three successful direct native processes pass 31 assertions across 77 actions
and 19 captures. Snapshots confirm mannequin freeze/look/rest, its retained
moving clip, stopped-time player movement with unchanged agents, pause/resume,
physical roach refuge arrival, hidden countdown, moving visible return and a
fresh Office roster after Home/Office reload. The final roach debug sample is
idle near its original position, with no stuck time. Moving/resting mannequin,
retreating roach and both reloaded environments were visually inspected.

Native runs use Metal/Fifo at 1280 × 720 physical pixels, Full lighting and High
textures. Four transient ready-surface skips are recorded across the successful
clock/shelter processes; reload has none. Initial attempts lost OS focus, correctly
freezing simulation and making wall-time acceptance deadlines inconclusive.
Those traces remain preserved; explicit focus restoration completed the intended
simulation intervals. These are functional checks, not uncapped throughput
measurements or a new focus-system audit, and no FPS improvement is claimed.

<a id="entity-navigation-0110-stage-boundary-and-limits"></a>
### Stage boundary and limits

No map, texture, model, package format or compiler behavior was changed or baked.
Office's arms-forward moving/arms-down resting mannequin, flashlight hold/look
and roach physical shelter/hidden/return behavior remain authored as before.
Existing wait/substep rounding and same-clip replay identity are unchanged;
this pass does not promise network determinism or cross-platform native coverage.
No matching open issue covers this pass; future roadmap issues remain open.
No main merge, release, branch transition, editor, Luau, packaging overhaul or
0.12.0 work is part of this stage.

<a id="fixture-leaks-0110"></a>
## Fixture housing and emission audit — 0.11.0

The two Office shaft sconces do **not** have missing backplates. Their delivered
`core:pool_light_wall` model has a closed metal rear, correct normals and ordinary
non-emissive metal/gasket materials. Native front, side and wide views in the
real Office map show the closed body. Broad illumination in front of a sconce
and light received by metal in Static are distinct from escaping through its back.
No delivered Office or Pool fixture GLB, wall placement, source power or colour
was changed. Baseline: audio commit `55072c92f9558b0febe189a2217a9a5fec72bc90`.

The audit did find related defects in the procedural fixture path and two Home
models. Those are corrected in their implementation/authoring sources, rather
than through global lighting adjustments.

| Corrected scope | Cause and repair |
| --- | --- |
| Procedural Home, blacklight and Pool model fallbacks | Bare bodies shared the `Light` draw family with luminous faces and were excluded from Full transport. Bodies now cast opaque shadows and receive ordinary baked lighting; their authored paint shades remain albedo. Fitted luminous endpoint sheets remain separate. |
| Home flush mount | Open mounting back; source at the mount, 70 mm above the diffuser; square source samples reached the rim. Close the ten-sided back and use the actual diffuser plane with an inscribed aperture. Move the center boss behind that plane, matching its documented design and avoiding loss of the central Full source sample. |
| Blacklight batten | Already closed, but its body did not occlude and its source sat 46 mm above the lens. Retain the body and fitted 8:1 PNG; align source extent and plane to the 1.12 × 0.14 m lens. |
| Pool round/wall procedural fallbacks | Add the missing mounting backs. Recess the round fallback face to the shared 17 mm source plane so its new cap does not coincide with the diffuser. Delivered GLBs remain unchanged. |
| `home:floor_lamp` | The first registered material was luminous and also owned the already-built base/stem. Register ordinary `lamp_body` before those 80 triangles; preserve the 144 shade triangles and their emission. |
| `home:cabinet_strip` | The same registration error made its housing luminous. Register ordinary `strip_housing`; replace two exposed underside triangles with eight aperture-border triangles around the unchanged lens, removing a 0.02754 m² coplanar overlap. Final model: 18 ordinary + 12 luminous triangles. |

The earlier fluorescent troffer repair is retained: its opaque backplate,
brushed-metal material, recessed fitted diffuser, UV orientation and source
contract are unchanged. Geometry revision 16 invalidates stale prepared meshes
and lighting for the procedural changes. No solver tolerance, atlas density,
ambient setting, exposure, shadow policy or authored source intensity was reduced.
No PNG was created, resized or repainted.

The complete material scan covered 366 distinct GLBs, including 42 luminous
models. A separate topology/normal/role audit covered 21 lamp and fixture GLBs:
Office and both Pool housings; core lamp/exit sign; Home ball/floor/strip lights;
Outdoors wall/fence/stand/street lights; Aero pod/ring/soffit; Arcade blacklight
and neon; all three Winter string spans and the timber lantern. All six built-in
fixture IDs and their procedural implementations were inspected. No additional
missing mounting backs or bad authored normals were found. Touching assembly
seams and intentionally buried faces were distinguished from exposed overlap.

Office's `shaft_lower` and `shaft_middle` remain at X=35.86, Y=2.1/5.7,
Z=2/16, yaw 270°, with 20 mm rear wall clearance. Independent triangle queries
using Full's actual 3×3 stencil (18 taps across two strips) blocked all **1,386**
sampled mounting-wall rays on ordinary metal and left all **450** sampled front
rays clear. Source clearance exceeds the actual endpoint tolerance. This rejects
the missing-backplate hypothesis for those placements; it does not establish
exhaustive visibility at every possible receiver or arbitrary yaw.

Independent review checked the shared routing, source planes, outward cap
winding, nondegenerate geometry, quad budgets and unchanged delivered fixtures.
It also independently decoded both Home exports. Floor-lamp triangle attributes,
ordering, UVs, colours and PNG bytes are unchanged. The strip's other five housing
faces and complete lens remain unchanged; its new perimeter preserves the original
affine underside UV mapping (maximum numeric error 2.81×10⁻⁸), closes the exterior
without T-junctions and has zero exposed housing/lens overlap. Scratch regeneration
is deterministic and matches both shipped outputs.

Native acceptance uses the direct executable through SDL3/wgpu Metal at a real
1600 × 900 drawable. Isolated compact controls cover all six fixture IDs, lit
and switched-off states, front/side/detached rear views, and a moving flashlight.
Separate asset roots exercise the model fallbacks. Production transport queries
now block six Home/blacklight ceiling-side witnesses on actual housing triangles
while leaving both central forward-source rays clear; all eight witnesses were
unblocked before. Sampled lamp-shade and strip-lens image regions match exactly
before/after, independently of their now-dark ordinary bodies.

| Genuine native evidence | Before → after |
| --- | --- |
| Home drum side / closed rear | [side before](../images/fixture-leaks/home-side-before.png) → [side after](../images/fixture-leaks/home-side-after.png); [rear before](../images/fixture-leaks/home-back-before.png) → [rear after](../images/fixture-leaks/home-back-after.png) |
| Floor lamp, unchanged shade | [before](../images/fixture-leaks/lamp-before.png) → [after](../images/fixture-leaks/lamp-after.png) |
| Cabinet strip, unchanged lens | [before](../images/fixture-leaks/strip-before.png) → [after](../images/fixture-leaks/strip-after.png) |
| Actual Office sconce, unchanged closed metal body | [side audit](../images/fixture-leaks/office-side.png) |

Passed production checks: `sh tools/check-rust.sh` (format, locked check, strict
debug/release all-target/all-feature Clippy, 26 existing CPU tests and locked
all-feature release build); asset validation, texture and prop checks; Office
and Arcade generator currency; 372 Python source tests (four explicit native
skips); patch whitespace. No new Rust tests were added. Texture checks retain
96 existing manifest advisories; there are no new validation failures.

The explicit installed-map refresh scope is Home, Arcade, Office, Winter, Aero
and Lantern Hollow: the Home fixture is reused by the latter three, while Office
is the unchanged model control. Each build is followed by package validation and
`verify --require-current`; completed outcomes and exact hashes are retained in
the local `package-builds.json` receipt. Pool and Beach have no affected active
fixture path and are not refreshed. Compiled packages remain untracked.

Static retains its existing coarse visibility model without fixture self-occlusion;
its ordinary body receiver colours are now lit, but this pass does not turn Static
into Full transport. Reflective captures retain their authored initial state.
The geometry audit also recorded pre-existing enclosed Outdoors light anchors as
a separate self-occlusion limitation; they are not a missing-back/side-leak defect
and were left unchanged. No Linux/Windows native or FPS claim is made.

Raw source/asset proofs, exact camera and switch recipes, original/current
executables, package hashes, logs and genuine captures are preserved outside
`target` in `local/fixture-leaks-0.11.0/`. No archived map was baked. No matching
open issue was found, so no issue was closed. Caustics remains a separate stage.

<a id="flashlight-0110"></a>
## Flashlight close-up correction — 0.11.0 staging

Resolves [issue #23](https://github.com/csd113/Places/issues/23), starting from
`a76236ad301bcd021c036742c6981a1eafc8f53e` on `0.11.0-staging`.

<a id="flashlight-0110-confirmed-cause-and-correction"></a>
### Confirmed cause and correction

The user confirmed the freestanding Office cabinet capture below as the reported
“floating shadow box.” Crouching and looking upward makes the silhouette look
like a black extension above the prop. Arcade's bathroom stall partitions, with
the room fixtures switched off and flashlight on, show the same upper-edge effect.

The source sat **18 cm below the camera in view space**. Its real shadow therefore
projected upward onto distant receivers. Native albedo/distance diagnostics locate
the confirmed box on a wall about 9–10 m away, behind the cabinet about 2–3 m away;
there is no floating box mesh or proxy caster. A separate near-wall control's
38-pixel band matches analytic source/prop/wall projection within one pixel.
Removing raster depth bias leaves that band intact. Disabling visibility only
for a diagnostic removes real cast shadows, distinguishing them from the separate
close-range soft cone edge. These controls do not establish a depth-space,
normal-transform or shadow-acne fault for the reported case.

The correction removes **only the vertical source drop** in the existing gameplay
attachment. It retains 14 cm right / 6 cm forward placement, six-metre aim
convergence and geometric source retraction. The lateral baseline retains visible
cast shadows while the above-top silhouette disappears in the confirmed views.
Both rendered lighting and mannequin observations consume this same source.

The cone, colour, intensity, range, falloff, shadow-map resolution, four PCF
comparisons, depth/normal bias, material response and contact lighting are
unchanged. Vertical beam placement and near-surface illumination intentionally
change with the corrected source. No map, runtime texture or model changed.

<a id="flashlight-0110-matched-native-evidence"></a>
### Matched native evidence

Apple M2 Pro / Metal, `Bgra8UnormSrgb`, High textures / 16× filtering, Full lighting
and lightmaps, Full reflections, bloom on, FOV 60°. Still captures are unchanged
1512×838 native frames, with identical camera, simulation and fixture states.

| Case | Before | After |
| --- | --- | --- |
| Office `basement_hide_1`, camera `(13, 0.8, 12.3)`, yaw 35°, pitch +12° | [Native PNG](../images/flashlight-0.11.0/office-before.png) | [Native PNG](../images/flashlight-0.11.0/office-after.png) |
| Arcade `restroom_divider_0`, camera `(-5.3, 0.8, 1.5)`, yaw 180°, pitch +30°, bathroom lights off | [Native PNG](../images/flashlight-0.11.0/stall-before.png) | [Native PNG](../images/flashlight-0.11.0/stall-after.png) |

Independent sampling of the confirmed upper-box region finds 3,704/3,750 dark
pixels before and zero after. A separate 551-pixel lateral-shadow region remains
black in both images. Final native images match the reviewed source-height
control byte-for-byte.

Office acceptance includes farther/nearer freestanding cabinets, standing and
crouched views, oblique angles, flat/curved props and the dark Arcade stall tops.
Two genuine native 11-second / 330-frame videos at 1280×720 / 30 FPS record normal
C crouch transitions, approach/retreat, continuous look input and flashlight
OFF/ON. Each run presents all 660 source frames without misses; all nine captured
camera/simulation states match exactly. Recording deliberately paces source
frames and is excluded from FPS measurements.

Existing native acceptance passes F/held-key handling, actual Controls rebinding
to R, console/menu/focus ownership, texture and Static/Full transitions and reload;
mannequin freeze, gaze, dim-edge hold, release and wall/floor/cabinet occlusion;
held/dropped objects and posed models. A narrower physics control captures a lit,
unheld duck twice after an impulse: its position and rotation change, with
nonzero linear and angular velocities at both captures. The earlier generic
`moving-lit` image had already settled and is not the moving-at-capture proof.
The ordinary default-feature release player separately loads and captures the
confirmed Office view successfully.

All players execute the native binary directly with explicit working directory,
assets and isolated state. No app packaging, wrapper or OS security/permission
change was used. Initial runs with unavailable surfaces are excluded. One Arcade
zero-bias control missed the flashlight input after focus loss and is also
excluded; it is not evidence of self-shadowing. Accepted final captures have
actual flashlight state witnesses and no missed ready presentations.

<a id="flashlight-0110-performance-and-resources"></a>
### Performance and resources

Three interleaved runs per side use the same Office view, 120 warmup plus 600
measured frames, fixed 1/60 simulation, V-sync off, swap interval zero, no frame
limiter and actual **Immediate / 1512×838** presentation. Each has 720 ready
presentations and zero misses. This campaign exceeds display refresh pacing;
the earlier shader-stage approximately 120 FPS plateau is not used as evidence.

| Metric | Before | After |
| --- | ---: | ---: |
| Pooled average FPS | 377.54 | 381.13 |
| Individual run average FPS range | 374.17–384.39 | 376.91–389.71 |

Average is measured frames divided by their total loop wall time. The **+0.95%**
difference lies within run-to-run variation; no throughput improvement is
established. Camera/settings/packages and submitted geometry match.

Separate native controls perform eight OFF/ON toggles per executable with all
720 ready frames presented. Late counts on both sides retain 39 textures, 61
render pipelines, 23 samplers and 700 bind groups. Small transient buffer/view
counts vary; no monotonic retained-resource growth or fatal state is observed.
The existing single 1024² depth target remains 4 MiB and is reused across toggles.
Metal allocation-byte counters are unavailable, so no memory-saving claim is made.

<a id="flashlight-0110-verification-and-limits"></a>
### Verification and limits

Strict `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo build --locked --release --workspace --all-features`, ordinary release
player build, formatting, whitespace and 372 Python source checks pass (four
optional skips). No Rust tests were added: scalar offset assertions would mirror
the implementation; the actual regression requires the retained native views.
No production map was baked. Independent review covers source, exact confirmed
pixels/receivers, motion, input/AI/object witnesses and measurement receipts.

Coverage is Apple M2 Pro / Metal only, with bounded native runs. This corrects the
confirmed source-placement appearance; it does not claim to solve every possible
shadow artifact or remove physically legitimate cast shadows on distant surfaces.
Reproduction drivers, immutable executables, full native media, hashes, check
logs and independent review are retained outside `target` under
`local/flashlight-0.11.0/`.

<a id="loading-0110"></a>
## Asset/map loading and resource ownership — 0.11.0 staging

Based on `f11e2819c92ad96488946f0005a4156f380c6621`, on the user's Mac.
This pass follows the completed renderer, shader, sound, caustics and physics
work. It keeps the existing engine/game/compiler boundaries and package format.

<a id="loading-0110-demonstrated-problems-and-corrections"></a>
### Demonstrated problems and corrections

- **Inconsistent package snapshots.** The preparation key, semantic world and
  variant each opened the path independently. A replacement could combine
  records from different manifests or pair retained gameplay semantics with a
  newly installed lighting variant. Loaded semantics now carry their canonical
  manifest identity. Every variant open and cache reuse checks that identity
  before caching or installation. A changed retained package gives an actionable
  reload error. Generation cancellation, bounded queues and the previous world
  remain owned by the existing preparation worker/frame loop.
  Native acceptance then exposed a recovery handoff defect: the subsequent
  explicit reload could stage new records under the renderer's rolled-back
  configuration. Staging now republishes the player's current graphics
  selection, and a successful world commit clears the previous failure hold.
- **Untrusted allocation hints.** A 10-byte prop header or 66-byte mesh header
  could reserve 1,048,576 in-memory collection entries before checking payload
  length. Readers now prove minimum encoded widths before reservation and charge
  each mesh range against the remaining aggregate geometry budget. Manifest
  lengths must match the archive index; actual decoded lengths must match too.
  Blob names and manifest SHA-256 declarations must agree.
- **Unsafe import replacement.** Direct copying could truncate the destination
  through a same-file alias or leave an installed package partly overwritten.
  Import now uses an exclusively created sibling, bounded copying, flush/sync,
  validation of the staged archive/semantics and atomic publication. Failure
  cleans only the owned temporary and preserves the destination. Its 1,088 MiB
  transport ceiling permits the existing 1 GiB decompressed budget plus ZIP
  overhead. Selected prepared records still receive their normal load-time
  decoding/validation; import does not decode every quality variant.
- **Repeated image work and late budgets.** Warm prefetch decoded unchanged PNGs
  before discovering the cache already owned them. It now skips those decodes
  and stages at most 64 images / 256 MiB decoded. Material resolution returns a
  named error immediately after its existing 1 GiB budget is crossed, rather
  than decoding the remaining table. Fixture sheets share the budget, with
  shared Arcs charged once, and unused shared-family faces skip resolution.
  Failed material resolution discards cache-only pixels; successful and failed
  level loads trim optional CPU image retention to 256 entries / 256 MiB.
  Pack path aliases now resolve to the stored image's identity, sharing one
  decode and budget charge. Active worlds keep independent image Arcs.
- **Malformed asset inputs.** Regular-file reads bound PNGs at 32 MiB, GLBs at
  64 MiB, catalog JSON at 8 MiB and gameplay JSON at its existing 1 MiB limit.
  Read growth is checked. GLB parsing borrows its BIN payload and validates exact
  framing, unique JSON/BIN chunks, one local buffer and declared buffer length.
  Existing geometry ceilings and animation row checks run before component
  allocation; morph resolution reuses the known vertex count.
- **Undercharged retained worlds.** The prepared-cache estimate counted only
  collision walls and eight navigation bytes per cell. It now includes floor,
  ceiling, water and ladder samplers, navigation class/portal/mask/region
  allocations, probe points/chains and spare vector capacity. The existing
  three-entry / 192 MiB cache can evict earlier; active oversized worlds still
  use their independent lease and weak reuse identity.

Installed-level discovery, image ownership and import now live in
`engine/loader/manager.rs`, preserving `loader::LevelManager`. No new executor,
asset framework, dependency, runtime texture imagery or editor module was added.
Physics sampler helpers only count memory; simulation behavior is unchanged.

<a id="loading-0110-verification"></a>
### Verification

Final verification receipts are retained outside `target` under
`local/loading-0.11.0/`, including commands, gate logs and source compatibility
checks. The source scan inspected all 366 GLBs and 578 PNGs: zero compatibility
findings, largest GLB 2,619,232 bytes, largest PNG 3,326,060 bytes, catalog 310,967
bytes. Asset validation reports 593 assets, 370 placeable, eight themes and zero
warnings. No production artwork or authored map changed.

Nine new CPU regressions use tiny headers, in-memory ZIP/GLB/PNG records,
temporary import files and a four-byte material budget. They protect the exact
allocation, snapshot, alias and failed-publication bugs above; they require no
map bake, native device or shipped asset tree. Native lifecycle checks remain
explicit developer operations outside automatic CI and Rust tests.

The full `RUSTC_WRAPPER= sh tools/check-rust.sh` gate passes: formatting,
all-target/all-feature check, strict debug and release Clippy, `cargo test
--workspace` (51 passed, zero failures/ignores, 0.06 seconds), and the locked
all-feature production build. The nine added regressions account for the
increase from 42. Python source contracts pass 372 tests with four existing
skips in 99.893 seconds. One initial material-budget regression exposed the
real pack alias duplication; its sharing assertion remained unchanged after
the fix. Initial and final logs are preserved separately.

All eleven selected native lifecycle cases pass against the final direct
executable on Apple M2 Pro / Metal in 138.272 seconds. They exercise CPU-ready
staged-upload cancellation, resize/cancel/retry, in-flight reuse, shutdown during
preparation, latest-generation supersession, in-place replacement, retained
lighting refusal and explicit recovery, corrupt-package retry, repeated visits,
graphics-only gameplay preservation and diagnostic material fallback. Twenty
scripted actions have maximum observed enqueue-to-handler latency 8.14 ms;
small-fixture staged disposal measured 0.217 ms. These observations are not
large-level latency bounds. All 141 RSS samples succeeded.

The initial native run passed ten cases and exposed the graphics handoff defect
above; the complete selection was rerun after fixing it. The final receipts are
`native-final/result.json`, per-case logs/traces and `native-gate-final.log`.
The initial `native/` directory remains intact. These small fixture bundles use
the silent audio fallback; production audio ownership is checked separately.

The existing Office and Pool packages then completed eight native commits,
including a deliberately corrupted disposable Pool copy, named load refusal,
restoration and successful retry. Both final captures were inspected. Native
audio initialized with 63 shared clips / 22,065,040 decoded bytes and zero
errors. Each captured world had one live ambience voice; shutdown cleared both
active and pending voices. Package geometry cache hits followed the first load
of each map. Accounted prepared records stayed at 27,988,086 bytes for Office
and 11,723,120 for Pool. Each repeated world texture table stayed at 33,554,424
GPU bytes; later visits uploaded zero world textures and had no missing/fallback
draws. These counters cover the named resource families, not all GPU memory.

Functional runs used actual Fifo presentation at a physical 3024 × 1676 drawable,
with V-sync enabled. No FPS comparison or throughput improvement is claimed.
The first production run's commit RSS ranged from 276.4 to 387.6 MiB and included
framebuffer readback; its short rise alone cannot establish a leak or a plateau.
Raw package hashes, captures, RSS samples, traces and acceptance checks are in
`production/`. A separate repeat control omits framebuffer readback.

That readback-free control completed 31 alternating Office/Pool commits with
29 prepared-cache hits, zero load/GPU failures and 99 successful coarse RSS
samples. Both prepared-record charges and each world's texture residency stayed
constant; every visit after initial uploads reported zero world-texture uploads.
After the first ten commits, commit RSS ranged 355.45–376.41 MiB; the final Office
commit was 363.14 MiB. These are short observations with allocator/driver
residency, not a proof against slower leaks. Its receipts are in
`production-repeat/summary.json` and the adjacent log/trace/memory files.

All four production executables are preserved under `local/loading-0.11.0/bin/`
with SHA-256 receipts. The final game hash is
`68ced53a537eb208e1a2972315cc9fbb67df19b2062097597ed1b7ed569a6df6`.
No shipped map was baked; the production runs used the existing verified
Office/Pool packages. Exact pushed-commit CI and final cleanup/custody receipts
are retained in the same local evidence directory.

<a id="loading-0110-limits-and-stage-boundary"></a>
### Limits and stage boundary

The budgets account retained payloads and bounded decoding; they are not a
single absolute process/GPU memory ceiling. Displayed/staged worlds, independent
model/animation data, decoder scratch, audio, GPU allocations and driver caches
have their existing owners and can coexist. No new aggregate animation-keyframe
policy was invented. Portable path checks reject ordinary special files before
open, but do not claim protection against arbitrary external filesystem races.
Cancellation is cooperative between package preparation phases; a single
bounded decode is not preempted mid-record.

Native evidence is macOS/Metal only. Windows/Linux acceptance and broader import
UX/distribution work remain with issues #17 and #21, which this pass does not
close. No accompanying loading-only issue was open. No main merge, new branch
or release is part of this stage; settings/input, entity/navigation, editor,
map-format reset, broad compiler redesign and flowing water remain deferred.

<a id="renderer-0110"></a>
## Renderer foundations — 0.11.0 staging

The targeted implementation, independent source review and native acceptance
campaign are complete on Apple M2 Pro/Metal. This report records mixed throughput
results, bounded resource observations, the structural Office repair and explicit
limits. Final staging CI is checked at the exact pushed SHA before issue #16 is
closed; completion evidence is linked from that issue.

<a id="renderer-0110-implemented-corrections"></a>
### Implemented corrections

The evidence supported targeted changes rather than a renderer rewrite.

- Retire pending surface images, saved capture frame and HUD replay whenever
  drawable/world resources change or the device becomes fatal. Previously the
  saved frame was never invalidated despite its documented lifetime contract.
- After one Outdated acquisition retry, retain Lost recreation and Validation
  fatal handling. Recreated surfaces refresh colour, alpha and present modes;
  incompatible formats fail before configuration. The retained SDL window clone
  keeps raw handles alive through surface destruction.
- Replace mutable renderer scene state on explicit same-ID reloads, including
  authored presentation, door sources, runtime models, visibility, weather and
  effects. Graphics transitions inherit playback only when explicitly requested.
  Stop staging, pose/environment, material and readback work after device loss.
- Write changed runtime emission scales into existing material uniforms instead
  of recreating all dynamic geometry/materials. Stable object identities protect
  removed/reused slots; unchanged values do not write again.
- Cache dynamic/character UV visibility anchors using exact source descriptors,
  receiver transform/bounds/identity, immutable transport identity and movable
  occluder revision. Installation replaces the owning GPU cache. Borrow water
  volumes during floating motion and reuse each computed dynamic transform.
- Classify immutable world passes once on upload. Gather translucent families
  into one stable global sort and visit a selected prop/dynamic entry directly.
  Shader bytes, depth precision/conversion, rasterization, material alpha,
  shadow sampling, water and lighting quality were not changed.

`visual-diagnostics` enables wgpu counters. Opt-in resource JSON records live
HAL objects and observed peaks at lifecycle boundaries/every sixty frames, plus
partial texture/atlas/post texel budgets. Metal does not implement HAL allocation
byte counters: those fields are null, not zero. Cache references can share GPU
allocations and must not be summed as total VRAM. The native launcher now records
actual app RSS samples at one-second intervals, excluding wrapper memory.

<a id="renderer-0110-offline-geometry-foundations-and-repair"></a>
### Offline geometry foundations and repair

The architecture checker now uses conservative spatial candidates with a
2,000,000-visit work cap instead of quantized plane bins/quadratic bucket scans.
An exhausted budget reports `geometry-check-incomplete`. Local plane checks use
all triangle corners. Tiny regressions cover remote origins, old normal-bin
boundaries and counted active-pair work.

The shared Python triangle kernel retains stable node/mesh/primitive/triangle
addresses, full vertex records, material/alpha information and explicit instance
matrices. It reports exact, near-coplanar and opposite-facing coverage. Every
visited pair consumes the explicit budget; incomplete audits have no repair plan.
Generic deletion requires exact equally oriented OPAQUE faces with identical
records/material in the same source primitive. Near planes, different materials,
seams, opposite sheets and placement clashes stay review-only. Skins, animation,
morph targets/default weights require pose review. Mutators reject unknown,
additional or trailing GLB chunks before editing and reverify hashes, records and
an empty second duplicate plan.

`--prop-instances` exports the engine's actual floor/yaw/scale/span/shear matrices.
`--audit-instances` records source/model identities and considers distinct fixed
GLB instances. Unresolved material override alpha is `UNKNOWN_OVERRIDE`.
Architecture/prop contacts, fixtures, doors and live poses are outside this
static-instance report. No per-frame pair scan or editor UI was added.

Depth review verified the native `Depth32Float` target, 0.1–100 m scene clip
range, the shared OpenGL projection/frustum convention and its single wgpu
`[-1,1]`→`[0,1]` depth correction. Ideal float-bin spacing is about 0.2 μm at the
0.575 m desk close-up, 0.6 μm at 1 m and 60 μm at 10 m; these analytic values
exclude hardware interpolation error. The diagnosed two opaque materials occupy
the same plane, with no intended depth separation. More depth precision would
not establish their intended ownership. Existing explicit decal layering/bias
remains separate from opaque model repairs; no global bias or clip-plane change
was added. Authored layered ribbons, paper backs and mounting contacts remain
review findings, not automatic deletion targets.

The Office `top_reception` desk had 18 confirmed same-facing wood/paint perimeter
triangle overlaps. The explicit reviewed recipe retains the existing opaque
paint band and clips only covered wood at its top plane. It moves no surface.
The GLB changes from 256 to 262 triangles; bounds, material dictionaries, four
embedded PNG payloads and other primitives remain exact. The repaired SHA-256 is
`f8a4f4198dcebabfb25519d96acdbcf8a6d352096381e7e85c032b5ffe6351cc`.
The targeted overlaps fall from 18 to zero even at a 1e-8 m² report threshold.
Five tiny foot/internal same-facing contacts remain review findings. Original
bytes and the hash-bound inspectable plan are preserved outside `target`.
A single fresh Office package build is used for candidate validation. Its
validated bytes are installed into the ignored `assets/levels/office_demo.placesmap`
for normal local play and pass `verify --require-current`. The exact original
compiled package was not retained: the baseline levels directory was a symlink
to live packages when the replacement was installed. The original authored desk
GLB, before captures, original package hash and byte-comparison receipts remain
preserved. The owned baseline overlay is now isolated and omits Office rather
than presenting replacement bytes as the original; unchanged maps remain linked.

Current Arcade/Office model audit: 87 static assets completed, five pose-dependent
assets excluded. Most complete at two million visits; the Office snack machine
needed an explicit twenty-million cap (12,174,410 visits, 18.113 seconds). No
unreviewed generic deletion was applied. The Office transformed-instance audit
completed at 11,965,190 visits with 35 opposite-facing contacts/five exclusions.
Arcade remains **incomplete at 100,000,000 visits** (45.263 seconds), with eight
observed opposite-facing paper-holder/divider mounting contacts and 32 live/pose
exclusions. Its partial results do not establish a clean map. All 34 existing
base-placement duplicate warnings are intentional balloon bouquets with distinct
rig offsets, sizes, colours and phases; no placement was removed.

<a id="renderer-0110-native-performance-comparison"></a>
### Native performance comparison

Hardware: Apple M2 Pro/Metal, Retina 2×, High textures/filtering, Full lightmaps
and reflections, bloom on, FOV 60. Both executables use the prerequisite Rust
1.99.1/dependency baseline. The before executable is from `e29adae`; the after
executable contains the reviewed `5d9983e` implementation. Actual presentation
switches to Immediate, settings say VSync off, and each BENCH_SUMMARY records
swap_interval 0. No engine FPS cap is active.

Each side has three separate runs per view, 120 ready-frame warm-up and 600
measured frames at a fixed 1/60 simulation step. FPS uses real wall time:
`total measured frames / sum(loop_ms) × 1000`. The table pools 1,800 frames per
side/view. All twelve paired runs match settings, camera, physical drawable,
package hash, movement script, fixed step and per-frame draw calls, visible
indices and reflection-pass counts; all report 720 ready presentations and zero
missed-ready frames. CPU/GPU diagnostic trace flags are absent on both sides.
No owned build/bake ran during timing. Separate 3,600-frame preconditioning runs
completed on each executable; permission waits and their lost-wrapper recovery
are excluded from comparisons. Every comparison still performs its own identical
120-frame warm-up. Thermal checks before/after report no recorded thermal or
performance warning; no CPU power status is available. These are sequential
batches, not interleaved trials or temperature-controlled measurements.

| View / drawable | Before average FPS | After average FPS | Change |
| --- | ---: | ---: | ---: |
| Bathroom lateral, 1512×838 | 378.63 | 470.95 | +24.38% |
| Bathroom lateral, 3024×1676 | 217.68 | 216.94 | -0.34% |
| Close closed door, 3024×1898 fullscreen | 130.43 | 128.44 | -1.53% |
| Arcade cabinets, 3024×1898 fullscreen | 227.55 | 227.73 | +0.08% |

The small-resolution lateral result is an observed 24.38% throughput increase;
its per-run before/after ranges are 371.59–390.24 and 463.04–480.21 FPS. The
larger lateral and cabinet views are close in throughput. The close-door view is
**1.53% slower**, consistently across its three runs; this is reported as a small
tradeoff, not erased as “no regression.” There is no evidence of a shader speedup.
A useful maintenance guardrail from this hardware baseline is to investigate
losses greater than 5% of average FPS under these same conditions; it is explicit
developer tooling, not a full-map CI requirement. This campaign's maximum
measured decrease is below that guardrail.

| View | Update mean ms, before→after | Render mean ms, before→after | GPU elapsed ms (median of run medians), before→after |
| --- | ---: | ---: | ---: |
| Bathroom lateral, 1512×838 | 0.902→0.872 | 1.576→1.086 | 1.891→1.885 |
| Bathroom lateral, 3024×1676 | 1.017→1.026 | 3.412→3.417 | 5.018→5.023 |
| Close closed door, 3024×1898 fullscreen | 1.233→1.265 | 6.233→6.299 | 8.091→8.099 |
| Arcade cabinets, 3024×1898 fullscreen | 1.120→1.118 | 3.082→3.093 | 4.807→4.806 |

Render time includes encoding, submission and waiting, rather than pure CPU
execution. GPU elapsed is the union of pass-stage spans, including scheduling
stalls; overlapping pass spans must not be added as independent active costs.
The low-resolution improvement with nearly unchanged GPU elapsed is consistent
with reduced CPU-side submission work. Source changes demonstrably remove
repeated classification, sorting, scanning, UV-anchor construction and resource
recreation; this sequential experiment does not isolate each contribution.
Close-door GPU elapsed remains about 8.1 ms at 233 base scene draws versus
about 4.8 ms at 98 cabinet-view draws. This comparison combines geometry and
fragment work; it does not isolate their costs. Changing the lateral drawable
from 3024×1676 to 1512×838 reduces GPU
elapsed from about 5.0 to 1.9 ms. Lighting/material work and covered pixels remain
relevant, and no effect or scene content was reduced to improve the result.

The historical near-20-FPS case was not reproduced. Earlier
[doorway](release-0.10.0.md#performance-0100) and [multicore](release-0.10.0.md#multicore-0100) reports already
document fragment-visibility/lighting fixes included in this baseline; their
gains are not attributed to this stage. The early 119.59→217.66 FPS / +82%
lateral comparison is **superseded**: repeated before samples also reach 217.68
FPS. Earlier instrumented cabinet timing and incorrect wrapper-RSS receipts are
retained as diagnostic history and excluded from the accepted table.

Per-run detailed distributions, sampled app RSS, load state, binary/package
hashes and native presentation identity remain in receipts. The largest sampled
single-view RSS peak was 1,428.3 MiB before and 1,053.4 MiB after; sampling and
transient allocations vary, so these maxima are not a claimed memory saving.
Longer map-transition memory evidence is separate below.

Genuine matched door, Bowling Alley and Pool basin captures are PNG-byte and
pixel identical to baseline at 1512×838, with matching package/settings, camera,
draw counts and triangles. The fresh Office close-up has a continuous counter
edge and no visible holes or material discontinuities. Its 8,930 changed pixels
(0.705%) differ by at most two RGB levels; visible triangles increase by six as
expected. Structural overlap removal is established offline; a static capture
alone does not prove temporal flicker removal. Office package validation and
current-source verification passed; collision, navigation and semantics are
byte-identical to the original package.

The Pool control activates two bodies. The initial nearby trace contains 1,822
physics frames, 254 held frames, finite pose/render-pose components and no
recovery, but its camera obscured the duck. A corrected dry-deck run removed
`PLACES_CAMERA` (which deliberately pins render angles despite scripted aim).
Its genuine free/held/thrown/moving images clearly show the duck on the deck,
close to the player while held, then farther away after throwing, with model
lighting and material faces intact. The 1,601-frame trace has 296 held frames,
finite transforms and zero recovery. Group reset leaves two dormant bodies;
reactivation resumes them. This is reset/reactivation evidence, not body-removal
proof. Map transitions separately replace the Pool world and its resources.
Transparent water, caustics, opaque rails/deck and the moving duck were inspected.
The empty-asset native launch renders the expected no-map menu and exits normally.

<a id="renderer-0110-native-resource-and-transition-acceptance"></a>
### Native resource and transition acceptance

The diagnostic executable completed 40,000 measured frames and exited normally.
Window creation through shutdown took 261.24 seconds; its 777.54-second launcher
elapsed time includes a 516.05-second Documents approval wait and is not a
performance measurement. Scripted transitions cover Arcade quality low/high,
lighting/static-to-full changes, Arcade↔Pool twice, explicit same-ID Arcade
reload, Office, return to Arcade, and 1280×720→1512×838 drawable changes.
The lighting action labelled `static` selects the implementation's lower-lighting
mode, disabling lightmaps/reflections, before `full` restores them. Both real SDL
minimize/restore calls were issued at configured frames with no warning, and
rendering continued. The harness does not emit an observed OS minimized-state
receipt, so that state is not independently proven. Resize/drawable transitions
and fresh in-engine captures are recorded. True physical device loss remains
policy-tested rather than induced.

There are 974 sampled resource records and no fatal state. At the final 185 frame
samples (frames 30,000–41,040), textures remain 142, bind groups 1,311, render
pipelines 56, samplers 22 and the persistent cache 116 entries. Buffers vary
677–682 and texture views 158–160 during settling. Transition
observed peaks of 8,587 buffers and 179 textures return to those settled ranges;
pending upload/staging state clears. Counts do not show accumulating map resources
in this campaign. Sampled actual-app RSS peaks at 2,487.7 MiB during transitions
then returns to 893.2–897.1 MiB in the final minute (median 894.5 MiB, last
895.2 MiB). That bounded observation is not proof of zero long-term growth.
Metal allocation bytes remain unavailable/null, sampled peaks are not exact
high-water marks, and partial texture/atlas/post budgets cannot be summed.

The existing default, diagnostics, baseline and final-candidate launches continued
after their Documents prompts resolved. When executor reconnections lost a
wrapper, completed preconditioning output was retained, native process exit was
checked, and only unstarted timing commands were resumed. The stable
`io.github.csd113.places.validation` identity was used throughout. No permission
was reset, identity spoofed, access broadened, denial bypassed or pending launch
duplicated. No native or build job remains after acceptance.

<a id="renderer-0110-verification-and-limits"></a>
### Verification and limits

| Gate | Result |
| --- | --- |
| Prerequisite exact-SHA CI 38857268227 at e29adae | Success |
| Format check | Passed |
| Strict workspace/all-target/all-feature Clippy | Passed, debug and release |
| `cargo test --workspace` | 2,340 library + 7 integration passed; 19 existing ignores |
| Locked all-feature workspace tests | 2,347 library + 7 integration passed; 19 existing ignores |
| Final source Python suite | 372 run, passed; four optional skips |
| Final focused offline geometry suite | Nine passed in ~0.005 s |
| Asset validation | 593 assets / 370 placeable / 8 themes; zero warnings |
| Existing explicit native headless GPU resource controls | Both passed (quality toggles and entity map reloads); no production map bake |
| Release tools + diagnostics build | Passed; binaries preserved outside target |
| Independent complete source/asset/test review | No unresolved source findings |
| Default candidate captures, no-map menu, matched timing | All 24 comparison runs exited 0 and matched per-frame geometry |
| Diagnostics native transition/resource memory | 40,000 measured frames, 261.24 native seconds, exit 0; bounded settled census/RSS observed |
| Visible moving/held/thrown prop and reset/reactivation | Genuine captures and finite pose trace inspected independently |

All existing Rust tests remain. Four new owning-subsystem CPU tests cover UV
cache invalidation, retry policy, replacement capabilities and bounded geometric
candidates; their fixtures are scalar lists/two or three triangles, with no GPU,
production asset or map bake dependency. Nine Python cases protect exact repair,
alpha/material/UV distinctions, transforms, conservative candidates, budgets,
morph declarations, chunk safety and affine clipping. Existing default tests were
not expanded into full-map or performance campaigns.

Physical device loss was not induced, OS minimized-state receipt is unavailable,
and memory observations cover this bounded campaign. Linux/Windows physical-GPU
native execution is unverified. Ambiguous overlaps and posed geometry require review;
the broader editor workflow stays in 0.12.0. Audio, test removal, a water rewrite
and the dedicated flashlight close-up investigation (#23) are outside this stage.
No flashlight shader/shadow behavior was changed. Its retained 1024² depth map,
four PCF taps and material alpha coverage were audited; projected bounds shadows
still have an eight-subject budget and an approximate silhouette. These are
relevant context for the queued close-up fake-shadow investigation (#23), not
claims that investigation is complete.

Evidence, original/repaired desk plans, audit receipts, command logs, captures,
source hashes and runnable binaries are under `local/renderer-0.11.0/`, outside
`target`. Default candidate player SHA-256:
`7b24a1e00f3107350530115455c9e800bb180da5dcd08aa827415b285e94c92b`.

<a id="repository-reorganization-0110"></a>
## 0.11.0 stage 1: repository and source responsibilities

The existing crate now has three explicit owners: `engine`, `game` and
`map_compiler`. Application entry points are thin. Engine production code has
no dependency on game UI or Map Compiler orchestration. No editor, renderer
rewrite, dependency or workspace was introduced. See [ARCHITECTURE.md](../ARCHITECTURE.md)
for the dependency direction and future editor contract.

The game owns the player loop, input policy, settings, menus, console and HUD.
The engine owns reusable scene/package records, asset resolution, rendering,
physics, entity simulation, targeting and counters. The Map Compiler owns
source preparation, offline navigation, preprocessing, validation, lighting,
package publication and its retained CLI/GUI. Shared progress observers and
locomotion values remove reverse imports; static collision compilation no longer
constructs the game's live entity world. Duplicate bounds math and asset-profile
validation now have one implementation each.

Repository cleanup removed superseded constructors, authoring scripts, an unused
codec experiment and a retired capture script. Useful report/asset documentation
was consolidated. The used package repacker remains a tool with a thin entry
point. Branding has one byte-identical PNG source. Export staging now includes
the runtime `gameplay.json` that it previously omitted. The itemized paths,
destinations, reasons and final rename-aware patch totals are in
[CHANGELOG.md](../../CHANGELOG.md).

All existing Rust tests remain for the separately ordered post-renderer test
cleanup. No Rust audit file was added. Root audit modules remain test-only;
their names refer to geometry/lighting regressions, not production subsystems.
Existing opt-in compiler/renderer diagnostics retain active consumers.

Validation on macOS/Metal:

| Command or check | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo check --locked --workspace --all-targets --all-features` | Passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo clippy --locked --release --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo test --workspace` | 2,336 library + 7 integration checks passed; 19 existing ignores |
| `cargo test --workspace --all-features` | 2,343 library + 7 integration checks passed; 19 existing ignores |
| `cargo build --locked --release --bins` | Passed; 135.78 s wall time |
| `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'` | 361 run, 4 existing skips, no failures; 121.093 s |
| Asset validation, accepted-model check, Home validator, entity clip boundaries | Passed; 593 assets, 370 placeable models, 8 themes; zero asset warnings |
| Independent source/path/diff review | Passed; include paths resolve, 10 WGSL files unchanged, Rust test inventory preserved |

Rust commands used `RUSTC_WRAPPER=` to bypass the host's sandbox-incompatible
sccache wrapper. Ordinary source checks performed no whole-map bake. Python Zoo
tests retain their assertions with tiny static/animated fixtures; one obsolete
constructor-only Python test was removed.

Native acceptance used preserved 0.10.0 and final release binaries. Empty-install
startup displayed the menu and v0.11.0, created normal state and required no map.
The Office capture at the same pose/settings was exactly equal across all
2,560 × 1,440 pixels, with 60 ready presentations and no missed ready frames.
Recorded entry-to-first-scene times were 2.761 s before and 2.787 s after.
These short capture runs are not a controlled frame-time campaign; activation
delays affected the launch waiter, and no broad runtime or cold-build performance
claim is made. Linux and Windows native execution were not exercised.

The real Map Compiler GUI completed the bounded `test_room.json` fixture,
cancelled another attempt, then completed a retry. Actual receiver counts were
visible during progress. The 1,946,140-byte package passed `validate`,
`verify --require-current`, and an incremental `build` returned `rebuilt: false`.
All twelve non-provenance archive records matched the 0.10.0 fixture byte for
byte. Differences were the tool identity, version and their manifest hashes;
authored/package format semantics were unchanged. No shipped map was rebaked.

Ignored cleanup removed 394 redundant/regenerable files (20,001,488 bytes),
including four hash-verified asset duplicates. Unique art, editable sources,
settings, deliveries and release evidence were retained. Logs, comparison
receipts, the bounded package and final binaries are preserved outside `target`
under `local/stage1-0.11.0/`; this is local evidence, not a release payload.

Stage 2 can work within `engine/render` and the existing simulation/presentation
handoff. GPU ownership, synchronization, z-fighting, sound and editor behavior
remain separate work. This stage does not publish 0.11.0 or merge staging to main.

<a id="rust-test-removal-0110"></a>
## Rust automated-test removal — 0.11.0

<a id="rust-test-removal-0110-scope-and-production-preservation"></a>
### Scope and production preservation

Starting commit: `502b37c84839d8815a2706234b42b58e8b22d7a7`, on
`0.11.0-staging`. The approved stage removes all existing project-owned Rust
unit/integration/executable doctests and test-only audit/support infrastructure,
including tests introduced by the renderer and shader stages. No old suite was
rerun before deletion; no map was baked.

The 2,377 `#[test]` functions and every `cfg(test)` item are gone. A source fence
inventory finds only JSON/text documentation examples, with no executable Rust
doctests. No dependency existed solely under `[dev-dependencies]`, and no explicit
`[[test]]` or custom harness declaration existed. Cargo's ordinary discovery
remains available for future justified tests; no blanket `test = false` or
`doctest = false` hides new tests.

Thirteen root audit/support modules, 65 owner test files and three integration
files contain no production implementation. All mixed production files retain
their non-test items; extraction and moves were unnecessary. Removed helpers are
previously test-gated CPU shader mirrors, assertions, small fixture constructors,
renderer controls and test-only accessors/reexports. Three test-serial worker
branches are removed without changing production worker budgets; cross-platform
performance helpers retain their OS-specific compilation guards.

Runtime/source validation, package hashes and record-size/path bounds, production
assertions, loading/cancellation/error paths, assets, compiler binaries, geometry
checker/repair planner, native renderer probes, benchmark binaries and production
`engine::lighting::transport::probe_audit` remain. The latter stays byte-identical,
including safe diagnostic file publication. Python validation and developer tools,
third-party dependency code, and shared `tests/fixtures/levels` / `native` inputs
remain; only the ten Rust-exclusive unit JSON fixtures are removed.

The shared Rust gate/CI replace the obsolete test stage with an all-feature
production executable build. The former test-only optimization profile and
ignored Rust GPU invocation in the full desktop script are removed. Active docs
now distinguish retained tooling/runtime checks from retired Rust-only assertions;
a build is not artwork or native-render acceptance. Historical reports retain
their dated test evidence.

`AGENTS.md` is now tracked so the updated future-test policy is durable, including
owning-folder discovery and no ordinary-CI full-map bakes. It and verification
guidance require FPS comparisons to disable V-sync/frame caps, record actual
presentation/drawable settings, and label refresh-rate plateaus inconclusive.
This cleanup makes no new performance comparison.

<a id="rust-test-removal-0110-accounting"></a>
### Accounting

No source moves, merges or production extractions occur in this stage.
The changelog enumerates every genuinely deleted file. Git text-line accounting
below separates whole-file deletion from edits within surviving Rust files.

| Path | Status | Added lines | Removed lines |
| --- | --- | ---: | ---: |
| `src/architecture_audit.rs` | deleted | 0 | 3,244 |
| `src/engine/ai/mod.rs` | test items removed | 0 | 3 |
| `src/engine/ai/tests.rs` | deleted | 0 | 1,376 |
| `src/engine/assets.rs` | test items removed | 0 | 122 |
| `src/engine/assets/tests.rs` | deleted | 0 | 1,016 |
| `src/engine/collision.rs` | test items removed | 0 | 3 |
| `src/engine/collision/movement.rs` | test items removed | 0 | 65 |
| `src/engine/collision/tests.rs` | deleted | 0 | 883 |
| `src/engine/collision_index.rs` | test items removed | 0 | 200 |
| `src/engine/diagnostics.rs` | test items removed | 0 | 58 |
| `src/engine/door.rs` | test items removed | 0 | 148 |
| `src/engine/entities/balloon.rs` | test items removed | 0 | 77 |
| `src/engine/entities/components.rs` | test items removed | 0 | 382 |
| `src/engine/entities/events.rs` | test items removed | 0 | 403 |
| `src/engine/entities/id.rs` | test items removed | 0 | 73 |
| `src/engine/entities/mod.rs` | test items removed | 0 | 2,043 |
| `src/engine/entities/sequences.rs` | test items removed | 0 | 99 |
| `src/engine/entities/spawn.rs` | test items removed | 0 | 85 |
| `src/engine/entities/timers.rs` | test items removed | 0 | 261 |
| `src/engine/entity.rs` | test items removed | 0 | 285 |
| `src/engine/font.rs` | test items removed | 0 | 3 |
| `src/engine/font/tests.rs` | deleted | 0 | 64 |
| `src/engine/gltf.rs` | test items removed | 0 | 3 |
| `src/engine/gltf/tests.rs` | deleted | 0 | 2,563 |
| `src/engine/interact.rs` | test items removed | 0 | 440 |
| `src/engine/level.rs` | test items removed | 0 | 7 |
| `src/engine/level/tests.rs` | deleted | 0 | 3,461 |
| `src/engine/level/tests/wall_ceiling.rs` | deleted | 0 | 656 |
| `src/engine/lighting.rs` | test items removed | 0 | 3 |
| `src/engine/lighting/bake.rs` | test items removed | 0 | 355 |
| `src/engine/lighting/directional.rs` | test items removed | 0 | 3 |
| `src/engine/lighting/directional/tests.rs` | deleted | 0 | 123 |
| `src/engine/lighting/light.rs` | test items removed | 0 | 3 |
| `src/engine/lighting/light/tests.rs` | deleted | 0 | 681 |
| `src/engine/lighting/lightmap/atlas.rs` | test items removed | 0 | 57 |
| `src/engine/lighting/lightmap/mod.rs` | test items removed | 0 | 3 |
| `src/engine/lighting/lightmap/tests.rs` | deleted | 0 | 2,143 |
| `src/engine/lighting/occlusion.rs` | test items removed | 0 | 1,116 |
| `src/engine/lighting/probes.rs` | test items removed | 0 | 502 |
| `src/engine/lighting/tests.rs` | deleted | 0 | 2,718 |
| `src/engine/lighting/transport.rs` | test items removed | 0 | 18 |
| `src/engine/lighting/transport/coverage.rs` | test items removed | 0 | 4 |
| `src/engine/lighting/transport/diagnostics.rs` | test items removed | 0 | 222 |
| `src/engine/lighting/transport/tests.rs` | deleted | 0 | 1,805 |
| `src/engine/lighting/transport/tests/continuity.rs` | deleted | 0 | 572 |
| `src/engine/lighting/transport/tests/fill.rs` | deleted | 0 | 269 |
| `src/engine/lighting/transport/tests/model_lighting.rs` | deleted | 0 | 392 |
| `src/engine/lighting/transport/tests/performance.rs` | deleted | 0 | 880 |
| `src/engine/lighting/transport/tests/probes.rs` | deleted | 0 | 805 |
| `src/engine/lighting/transport/tests/quality.rs` | deleted | 0 | 1,264 |
| `src/engine/lighting/visibility.rs` | test items removed | 0 | 1,140 |
| `src/engine/loader.rs` | test items removed | 0 | 28 |
| `src/engine/loader/tests.rs` | deleted | 0 | 5,914 |
| `src/engine/logging.rs` | test items removed | 0 | 49 |
| `src/engine/materials.rs` | test items removed | 0 | 3 |
| `src/engine/materials/builtin.rs` | test items removed | 0 | 48 |
| `src/engine/materials/color.rs` | test items removed | 0 | 23 |
| `src/engine/materials/emission.rs` | test items removed | 0 | 3 |
| `src/engine/materials/emission/tests.rs` | deleted | 0 | 47 |
| `src/engine/materials/fluorescence.rs` | test items removed | 0 | 32 |
| `src/engine/materials/image.rs` | test items removed | 1 | 95 |
| `src/engine/materials/image/tests.rs` | deleted | 0 | 274 |
| `src/engine/materials/reflection.rs` | test items removed | 0 | 66 |
| `src/engine/materials/response.rs` | test items removed | 0 | 3 |
| `src/engine/materials/response/tests.rs` | deleted | 0 | 134 |
| `src/engine/materials/tests.rs` | deleted | 0 | 1,676 |
| `src/engine/nav/mod.rs` | test items removed | 0 | 3 |
| `src/engine/nav/query.rs` | test items removed | 0 | 37 |
| `src/engine/nav/tests.rs` | deleted | 0 | 1,210 |
| `src/engine/package/collision.rs` | test items removed | 0 | 255 |
| `src/engine/package/compact_lightmaps.rs` | test items removed | 0 | 187 |
| `src/engine/package/lightmaps.rs` | test items removed | 0 | 115 |
| `src/engine/package/manifest.rs` | test items removed | 0 | 44 |
| `src/engine/package/mod.rs` | test items removed | 0 | 3 |
| `src/engine/package/navigation.rs` | test items removed | 0 | 99 |
| `src/engine/package/props.rs` | test items removed | 0 | 1,117 |
| `src/engine/package/shared_props.rs` | test items removed | 0 | 278 |
| `src/engine/package/tests.rs` | deleted | 0 | 1,718 |
| `src/engine/package/world.rs` | test items removed | 0 | 210 |
| `src/engine/package/world/dependency_tests.rs` | deleted | 0 | 396 |
| `src/engine/package/world/irradiance_tests.rs` | deleted | 0 | 159 |
| `src/engine/perf.rs` | test items removed | 0 | 3 |
| `src/engine/perf/platform.rs` | test items removed | 2 | 29 |
| `src/engine/perf/tests.rs` | deleted | 0 | 170 |
| `src/engine/physics/floor_tests.rs` | deleted | 0 | 133 |
| `src/engine/physics/hull_tests.rs` | deleted | 0 | 296 |
| `src/engine/physics/mod.rs` | test items removed | 0 | 12 |
| `src/engine/physics/recovery.rs` | test items removed | 0 | 4 |
| `src/engine/physics/recovery_tests.rs` | deleted | 0 | 500 |
| `src/engine/physics/tests.rs` | deleted | 0 | 1,885 |
| `src/engine/physics/water_tests.rs` | deleted | 0 | 478 |
| `src/engine/physics/world.rs` | test items removed | 0 | 23 |
| `src/engine/props.rs` | test items removed | 0 | 3 |
| `src/engine/props/tests.rs` | deleted | 0 | 662 |
| `src/engine/quality.rs` | test items removed | 0 | 3 |
| `src/engine/quality/tests.rs` | deleted | 0 | 817 |
| `src/engine/render.rs` | test items removed | 0 | 19 |
| `src/engine/render/boundary_tests.rs` | deleted | 0 | 261 |
| `src/engine/render/common/animation.rs` | test items removed | 0 | 146 |
| `src/engine/render/common/api.rs` | test items removed | 4 | 599 |
| `src/engine/render/common/architecture.rs` | test items removed | 0 | 45 |
| `src/engine/render/common/atmosphere.rs` | test items removed | 0 | 380 |
| `src/engine/render/common/blacklight.rs` | test items removed | 0 | 141 |
| `src/engine/render/common/character.rs` | test items removed | 0 | 1,905 |
| `src/engine/render/common/doors.rs` | test items removed | 0 | 450 |
| `src/engine/render/common/dynamic.rs` | test items removed | 0 | 1,426 |
| `src/engine/render/common/dynamic_lights.rs` | test items removed | 0 | 251 |
| `src/engine/render/common/dynamic_visibility.rs` | test items removed | 0 | 376 |
| `src/engine/render/common/dynamic_visibility/tests/receiver_cache.rs` | deleted | 0 | 83 |
| `src/engine/render/common/effects.rs` | test items removed | 0 | 852 |
| `src/engine/render/common/fixtures.rs` | test items removed | 0 | 314 |
| `src/engine/render/common/framebuffer.rs` | test items removed | 0 | 97 |
| `src/engine/render/common/light_transport.rs` | test items removed | 0 | 1,502 |
| `src/engine/render/common/mesh.rs` | test items removed | 0 | 41 |
| `src/engine/render/common/mod.rs` | test items removed | 0 | 56 |
| `src/engine/render/common/postprocess.rs` | test items removed | 0 | 102 |
| `src/engine/render/common/probe_filter.rs` | test items removed | 0 | 206 |
| `src/engine/render/common/prop_chart_tests.rs` | deleted | 0 | 723 |
| `src/engine/render/common/props.rs` | test items removed | 0 | 449 |
| `src/engine/render/common/reflections.rs` | test items removed | 0 | 300 |
| `src/engine/render/common/snow.rs` | test items removed | 0 | 602 |
| `src/engine/render/common/storm.rs` | test items removed | 0 | 89 |
| `src/engine/render/common/tests/character_normals.rs` | deleted | 0 | 62 |
| `src/engine/render/common/view.rs` | test items removed | 0 | 67 |
| `src/engine/render/common/water.rs` | test items removed | 0 | 100 |
| `src/engine/render/tests.rs` | deleted | 0 | 10,325 |
| `src/engine/render/wgpu/caustics.rs` | test items removed | 0 | 160 |
| `src/engine/render/wgpu/character.rs` | test items removed | 0 | 504 |
| `src/engine/render/wgpu/decals.rs` | test items removed | 0 | 363 |
| `src/engine/render/wgpu/diagnostics.rs` | test items removed | 0 | 183 |
| `src/engine/render/wgpu/dynamic.rs` | test items removed | 0 | 17 |
| `src/engine/render/wgpu/effects.rs` | test items removed | 0 | 80 |
| `src/engine/render/wgpu/entity_lighting_tests.rs` | deleted | 0 | 1,517 |
| `src/engine/render/wgpu/environment.rs` | test items removed | 0 | 48 |
| `src/engine/render/wgpu/gpu_timing.rs` | test items removed | 0 | 118 |
| `src/engine/render/wgpu/lightmap.rs` | test items removed | 0 | 491 |
| `src/engine/render/wgpu/low_lighting_tests.rs` | deleted | 0 | 315 |
| `src/engine/render/wgpu/material.rs` | test items removed | 0 | 447 |
| `src/engine/render/wgpu/postprocess.rs` | test items removed | 0 | 386 |
| `src/engine/render/wgpu/props.rs` | test items removed | 0 | 176 |
| `src/engine/render/wgpu/reflections.rs` | test items removed | 0 | 609 |
| `src/engine/render/wgpu/renderer.rs` | test items removed | 0 | 630 |
| `src/engine/render/wgpu/sky.rs` | test items removed | 0 | 19 |
| `src/engine/render/wgpu/surface.rs` | test items removed | 0 | 197 |
| `src/engine/render/wgpu/tests/postprocess.rs` | deleted | 0 | 27 |
| `src/engine/render/wgpu/tests/props.rs` | deleted | 0 | 46 |
| `src/engine/render/wgpu/tests/surface_lifecycle.rs` | deleted | 0 | 60 |
| `src/engine/render/wgpu/tests/world_transforms.rs` | deleted | 0 | 57 |
| `src/engine/render/wgpu/texture.rs` | test items removed | 0 | 954 |
| `src/engine/render/wgpu/ui.rs` | test items removed | 0 | 221 |
| `src/engine/render/wgpu/uv_receivers.rs` | test items removed | 0 | 189 |
| `src/engine/render/wgpu/world.rs` | test items removed | 0 | 2,775 |
| `src/engine/runtime_work.rs` | test items removed | 1 | 24 |
| `src/engine/spatial.rs` | test items removed | 0 | 3 |
| `src/engine/spatial/tests.rs` | deleted | 0 | 587 |
| `src/engine/weather.rs` | test items removed | 0 | 316 |
| `src/fog_void_audit.rs` | deleted | 0 | 208 |
| `src/game/app/actions.rs` | test items removed | 0 | 36 |
| `src/game/app/flashlight.rs` | test items removed | 0 | 25 |
| `src/game/app/item_input.rs` | test items removed | 0 | 73 |
| `src/game/app/item_runtime.rs` | test items removed | 0 | 43 |
| `src/game/app/loading.rs` | test items removed | 0 | 450 |
| `src/game/app/loading/cache_tests.rs` | deleted | 0 | 404 |
| `src/game/app/mod.rs` | test items removed | 0 | 35 |
| `src/game/bench.rs` | test items removed | 0 | 3 |
| `src/game/bench/lighting_sequence.rs` | test items removed | 0 | 93 |
| `src/game/bench/tests.rs` | deleted | 0 | 311 |
| `src/game/console/mod.rs` | test items removed | 0 | 3 |
| `src/game/console/tests.rs` | deleted | 0 | 209 |
| `src/game/display.rs` | test items removed | 0 | 122 |
| `src/game/input.rs` | test items removed | 0 | 14 |
| `src/game/input/tests.rs` | deleted | 0 | 593 |
| `src/game/mod.rs` | test items removed | 0 | 7 |
| `src/game/perf_overlay.rs` | test items removed | 0 | 105 |
| `src/game/settings.rs` | test items removed | 0 | 7 |
| `src/game/settings/low_lighting_tests.rs` | deleted | 0 | 156 |
| `src/game/settings/tests.rs` | deleted | 0 | 1,429 |
| `src/game/tests.rs` | deleted | 0 | 10,004 |
| `src/game/tests/art_style.rs` | deleted | 0 | 200 |
| `src/game/tests/ice.rs` | deleted | 0 | 260 |
| `src/game/tests/movement_diagnostics.rs` | deleted | 0 | 383 |
| `src/game/tests/movement_performance.rs` | deleted | 0 | 83 |
| `src/game/tests/movement_regression.rs` | deleted | 0 | 726 |
| `src/game/tests/recovery.rs` | deleted | 0 | 149 |
| `src/game/tests/water_posture.rs` | deleted | 0 | 253 |
| `src/game/tests/winter.rs` | deleted | 0 | 130 |
| `src/game/ui.rs` | test items removed | 0 | 963 |
| `src/game/ui/menu.rs` | test items removed | 0 | 234 |
| `src/game/ui/pointer.rs` | test items removed | 0 | 75 |
| `src/lib.rs` | test items removed | 0 | 25 |
| `src/lighting_audit.rs` | deleted | 0 | 1,242 |
| `src/lighting_audit_cases.rs` | deleted | 0 | 2,102 |
| `src/lighting_isolation.rs` | deleted | 0 | 494 |
| `src/lighting_partition_audit.rs` | deleted | 0 | 624 |
| `src/lighting_repair_regression.rs` | deleted | 0 | 784 |
| `src/lighting_vertical_audit.rs` | deleted | 0 | 577 |
| `src/map_compiler/application.rs` | test items removed | 0 | 50 |
| `src/map_compiler/diagnostics.rs` | test items removed | 0 | 199 |
| `src/map_compiler/geometry_check.rs` | test items removed | 0 | 1,251 |
| `src/map_compiler/gui.rs` | test items removed | 0 | 45 |
| `src/map_compiler/mod.rs` | test items removed | 0 | 169 |
| `src/map_compiler/probes.rs` | test items removed | 0 | 309 |
| `src/map_compiler/progress.rs` | test items removed | 0 | 138 |
| `src/map_compiler/storage.rs` | test items removed | 0 | 43 |
| `src/map_compiler/tests/geometry_overlap.rs` | deleted | 0 | 53 |
| `src/map_compiler/tests/material_transport.rs` | deleted | 0 | 43 |
| `src/map_compiler/tests/performance.rs` | deleted | 0 | 652 |
| `src/showcase_audit.rs` | deleted | 0 | 118 |
| `src/static_prop_lighting_tests.rs` | deleted | 0 | 1,382 |
| `src/surface_audit.rs` | deleted | 0 | 1,260 |
| `src/test_support.rs` | deleted | 0 | 433 |
| `src/zoo_audit.rs` | deleted | 0 | 195 |
| `tests/command_line.rs` | deleted | 0 | 48 |
| `tests/fixtures/unit/architecture_contacts.json` | deleted | 0 | 232 |
| `tests/fixtures/unit/controller_contacts.json` | deleted | 0 | 3,319 |
| `tests/fixtures/unit/encounter_lifecycle.json` | deleted | 0 | 267 |
| `tests/fixtures/unit/map_independent_preparation.json` | deleted | 0 | 23 |
| `tests/fixtures/unit/navigation_stairs.json` | deleted | 0 | 259 |
| `tests/fixtures/unit/navigation_steps.json` | deleted | 0 | 560 |
| `tests/fixtures/unit/renderer_material_contract.json` | deleted | 0 | 29 |
| `tests/fixtures/unit/renderer_weather_contract.json` | deleted | 0 | 24 |
| `tests/fixtures/unit/shore_contacts.json` | deleted | 0 | 67 |
| `tests/fixtures/unit/winter_contacts.json` | deleted | 0 | 76 |
| `tests/list_levels.rs` | deleted | 0 | 364 |
| `tests/macos_cpu_port.rs` | deleted | 0 | 70 |

<a id="rust-test-removal-0110-acceptance"></a>
### Acceptance

- Rust 1.99.1 and Cargo 1.99.1 match the repository pin. The sandbox cannot use
  the configured sccache wrapper; validation sets `RUSTC_WRAPPER=` without
  changing repository settings or compiler/lint policy.
- Formatting, locked all-target/all-feature check, and strict debug/release
  all-target/all-feature Clippy pass. All-feature production release build passes;
  the default-feature release build also passes.
- Python source contracts: 372 run, 17 optional/native skips, exit 0. No Python
  validation infrastructure was removed. Asset validator: 593 assets, 370
  placeables, eight themes, zero warnings, exit 0.
- Independent reviewer normalized every one of the 134 mixed Rust files against
  the starting commit, stripping test-gated items, comments/whitespace, and the
  documented test-only branches. Zero unexpected non-test code differences;
  production probe-audit diagnostics remain byte-identical.
- Optional strict rustdoc generation still fails on 57 pre-existing broken,
  private or redundant intra-doc link locations. Every reported source line is
  present unchanged at the starting commit. Links made stale by this removal
  were fixed. No lint is suppressed; rustdoc is outside the production gate.

Machine receipts, logs, source inventories, per-file accounting and independent
review are retained under `local/rust-test-removal-0.11.0/`, outside `target`.
The direct native Metal smoke loads the existing Arcade package, realizes 76
models with zero failures, draws its world/props/characters and captures the
expected door/materials/interaction HUD. The accepted run exits 0, records 180
ready presentations and zero ready-state misses at 2560×1440, and requests then
reports Immediate presentation with V-sync disabled. This is functional smoke
coverage, with no throughput/performance claim. An initial 30-frame pilot also
exited 0 and rendered correctly but its wrapper rejected three startup
presentation misses (27 ready presentations); that failed pilot is retained and
not counted as the accepted run. Neither run shows a fatal, panic, GPU validation
error or failed model. No `.app` wrapper, security setting change or map bake was
used. Native coverage is M2 Pro/Metal only.

The production compiler validates the installed Arcade archive (`off`/`full`,
19 entries, 95 dependencies); the player CLI discovers all eight installed maps.
No matching open cleanup issue was found; unrelated queued issues remain open.

Exact-commit CI, remote-ref verification, preserved executable hashes and final
`cargo clean` receipts are retained with the commit identity in the local handoff
and delivery response. This stage stops before queued flashlight work.

<a id="settings-input-0110"></a>
## Settings and input correctness — 0.11.0 staging

Based on `68601b3b40a77f62ab8fcb046c55eefe1d9fa14f` on the user's Mac,
following the completed renderer, shader, physics, sound and loading passes.
The existing settings model, Controls page and native event loop remain the
owners; no UI redesign, input framework or new dependency was introduced.

<a id="settings-input-0110-confirmed-defects-and-corrections"></a>
### Confirmed defects and corrections

- **Partial-file data loss.** Missing mandatory settings or early binding fields
  invalidated the entire JSON object, discarding otherwise valid preferences.
  Missing fields now receive their normal defaults. Explicit graphics choices
  remain independent: low texture quality can retain Full lighting. A false
  `PLACES_NO_BLOOM` switch no longer forces a saved-Off bloom preference on.
  Control/FOV sanitization also replaces nonfinite values before clamping.
- **Unreachable and conflicting keys.** Validation accepted unknown key names
  and reserved `KP_MINUS`, while native SDL events emitted `KEYPAD -`. A shared
  native-name resolver canonicalizes aliases and rejects unreachable names.
  Pause, overlay and diagnostic keys retain shell ownership. Repair reserves
  valid configured keys before assigning unused defaults, and retains explicit
  unbound entries across reloads rather than stealing another action's key.
- **Console ownership.** The console mixed a fixed physical Grave scancode with
  logical gameplay bindings. Console is now an ordinary configurable logical
  binding, defaulting to backtick, using the same conflict validation as
  Interact and Flashlight. Rebinding it releases its previous key. The existing
  Controls row count derives from the binding list so its Back row stays reachable.
- **Repeat capture.** Controls accepted OS repeat events as the next binding,
  allowing a held menu activation key to bind itself. Gameplay, menu navigation,
  console toggles and rebinding now share one fresh-press helper. Text editing
  retains its normal repeat behavior; short gameplay taps retain their press edge.
- **Focus and paused clocks.** Clearing held keys did not stop airborne player
  motion, rigid bodies or visual updates during focus loss. The preserved
  baseline fell from 9.6 m to 5.09 m while reporting no focus. One activity gate
  now stops player/world, physics, weather and visual clocks for focus loss,
  menus, console and loading, with timing reset on focus changes. World time
  scaling remains separate from player motion and geometric carry control.
- **Persistence publication.** Direct writes could truncate the previous file
  before a save completed; a second malformed file replaced the previous invalid
  backup. Saves now write/sync an exclusively created sibling before rename.
  Failure removes only the owned temporary. Malformed files retain separately
  numbered backups using non-replacing publication.

<a id="settings-input-0110-regression-value-and-verification"></a>
### Regression value and verification

Three additional CPU regressions live in the owning `src/game/tests/` tree:
one protects partial-file/save failure and successive backup data loss; one
protects native key identity, binding repair and unbound/console ownership; one
protects the shared press/repeat/tap/reset contract. They use tiny JSON,
temporary settings files and SDL event values, with no native device, compiled
map or shipped asset dependency. Native lifecycle acceptance remains explicit
developer tooling, outside Rust tests and ordinary CI.

Validation receipts, initial failures, the preserved baseline reproduction,
action scripts, settings, native traces/captures, executable hashes and final
exact-commit CI/cleanup/custody receipts are kept
outside `target` in `local/settings-input-0.11.0/`.

The opt-in `tools/bench/settings_input.py` launches a direct executable against
an existing developer-console fixture. It covers repeat/rebind, independent
graphics choices, flashlight/interact edges, console and pause capture, saved
binding reload, focus transitions and stopped-world player/carry control. It
never builds or bakes. The native key action's optional `repeat` flag allows the
same handler path to receive explicit OS-repeat cases.

The complete `RUSTC_WRAPPER= sh tools/check-rust.sh` gate passes: formatting,
all-target/all-feature check, strict debug and release Clippy, `cargo test
--workspace` (54 passed, zero failed/ignored, 0.06 seconds), and the locked
all-feature production build. Python source contracts pass 372 tests with
17 explicit skips in 95.525 seconds (`PLACES_SKIP_SMOKE=1`). The new regressions
increase the previous CPU count from 51 to 54.

The final direct executable completes all 18 native assertions across three
processes, 139 scripted actions and 17 captures. The existing Controls layout
and carried-cup captures were visually inspected. Low textures retain actual
Full renderer lighting, and false `PLACES_NO_BLOOM=0` retains saved bloom-Off.
Rebound controls persist into the next process. Pause/resume retains an exact
stationary position after a key release in the menu. Focus loss retains the
9.558 m airborne height after the pre-loss simulation interval; the baseline
continued falling. At world scale zero the player and held cup each move about
0.600 m together while the solver's fixed-step count remains zero. All three
native processes reach `shutdown_complete`. These runs report actual Fifo
presentation at 1280 × 720 physical pixels, with a 480 × 270 Low scene target.

All four production executables and SHA-256 receipts are preserved under
`local/settings-input-0.11.0/bin/`; the player SHA-256 is
`c2e0d07c892a1339fb95629be46ec2b5a13733aaaaae133c14d89c97896ce90a`.

<a id="settings-input-0110-limits-and-stage-boundary"></a>
### Limits and stage boundary

Native verification is macOS/Metal only. Scripted focus events exercise the real
focus handler and SDL capture reconciliation; they do not certify OS focus
switching on every desktop. Functional runs retain V-sync, so no FPS comparison
or throughput improvement is claimed. Staged settings writes protect against
failed publication; file synchronization alone is not a guarantee against every
filesystem/power-loss failure. Backup/staging slots are bounded and their
publication failures retain diagnostics rather than promising unlimited recovery.

No shipped map or asset was changed or baked. No accompanying settings/input
issue is open; broader UI, distribution and platform issues remain open. Main
merge, new branches, releases, entity/navigation, editor, packaging, map-format
reset, broad compiler changes and flowing water remain outside this pass.

Final custody is recorded in `local/settings-input-0.11.0/CUSTODY.md`: preserved
executables are verified before cleanup, all build/native users exit before
`cargo clean`, and checkout/index/build/native-window custody is explicitly
released when the exact pushed-commit CI and cleanup receipts are complete.

<a id="shader-0110"></a>
## Shader correctness and maintenance — 0.11.0 staging

Shader-stage baseline is `6011d92ab1cf6d7710ef631672109d3f55e90e6f`.
The completed renderer stage is not included in this stage's performance gains.
Implementation and acceptance below are separate from test removal, flashlight
close-up issue #23, console #22, sound #12 and caustics #19.

<a id="shader-0110-implemented-corrections"></a>
### Implemented corrections

- **Normal transforms:** a small absolute determinant was treated as singular.
  A valid rotated model or animated joint at scale `1e-5` consequently received
  identity or zero normals. One shared inverse-transpose helper preserves the
  exact ordinary path and rescales the finite linear matrix before exceptional
  inversion. Placement, skinning and diagnostic snapshots now agree; each keeps
  its existing singular fallback. Tiny retained skin-weight totals also work.
- **Fluorescence bindings:** untextured static models with equal local mask
  index zero could reuse another model's material/mask. A named material identity
  includes the validated resolved fluorescence resource slot. Missing or invalid
  local indices cannot borrow the next model's image. Identical masked models in
  separate spatial batches may retain separate material bind groups, consistent
  with the existing base/emission slot policy; cached image allocations still share.
- **Raw presentation:** non-sRGB window fallback decoded already encoded display
  RGB without a hardware re-encode. The final pipeline selects the existing raw
  copy for raw surfaces. An explicit feature-only native probe exercises actual
  production copies on the player's device: both raw RGBA/BGRA controls change
  `[55,13,133,255]` to `[128,64,191,255]`; both sRGB controls retain the latter
  exactly. This reproduces the former entry selection, not an old executable's
  physical raw window. Normal Metal windows use sRGB.
- **Fragment maintenance:** prepare authored/geometric normals once and reuse
  them for material lighting and caustic selection. Derivatives remain outside
  conditional normal preparation; visible-side material normals and original
  geometric receiver orientation stay distinct. Exact uniform zero-strength
  gates skip sheen powers/reflection samples; every nonzero response is retained.
- Correct stale float vertex-colour, HDR target, uniform-size, alpha, reflection
  readback and static/runtime lighting contracts. Direct benchmark execution now
  records the actual child PID/RSS and has no app packaging or automatic retry.

No textures, map/package contents, resolution, sample counts, material response,
light/fog intensity, depth/alpha policy or enabled effects were reduced.

<a id="shader-0110-inventory-and-profile"></a>
### Inventory and profile

All ten production WGSL files were inspected: six vertex and 25 fragment entry
points, with no authored compute entry point. World concatenates storm, attached
lights, world and caustics helpers; decals shares storm/attached-light definitions.
Sky, effects, post and UI are independent modules; diagnostics is feature-only
world source injection. Existing backend query operations create internal compute
pipelines; those are not omitted authored lighting shaders.

World has five binding groups: camera; base sheet; material/normal/emission/UV;
environment/lightmaps/reflections/caustics; attached lights/flashlight visibility.
The 88-byte vertex stride, 80-byte camera, 112-byte material, 6,240-byte environment,
400-byte attached-light record and storage/node strides agree with host layouts.
Lightmaps, scene, reflection, emission and blur remain linear HDR; colour textures
sample sRGB, masks/normals stay numeric, HUD/presented values stay encoded display
RGB. Review covered Low/Full gates, spatial entity lighting/UV response, shadow
coverage, reflected transforms, fog, sampling/LOD, storage bounds and invalid data.
Detailed entry/binding/layout inventories and source hashes are preserved locally.

A Metal System Trace attached to the directly launched baseline identifies scene
fragment work as the dominant measured GPU pass cost in the close-door view: a stable five-second window contains
600 scene encoders, approximately 3,948.74 ms of fragment intervals and 168.70 ms
of vertex intervals. Scene fragment work is about 6.58 ms per encoder versus
0.28 ms vertex work. Planar fragment work is about 0.36 ms, emission 0.16 ms,
resolve 0.21 ms and the final copy 0.13 ms. Channels overlap: these are not additive
frame costs. The trace separates geometry from fragment and reflection/post work;
it does not isolate overdraw, texture stalls and arithmetic within fragments.
No claim assigns all pixel cost to lighting arithmetic. The normal consolidation
is a bounded removal of demonstrably repeated work, not a shader rewrite claim.

<a id="shader-0110-native-acceptance"></a>
### Native acceptance

An immutable before bundle contains the accepted default player, source archive,
all ancillary assets and real copied Arcade/Office/Pool packages. Its 963-file
SHA-256 manifest was reverified; there are no symlinks to mutable assets. The
accepted repaired Office package is the shader baseline. Default and diagnostic
candidate binaries are retained separately outside `target`.

Every native player was executed directly with explicit repository working
directory, assets and isolated state. No `.app` or LaunchServices wrapper, TCC
reset, security workaround or identity change was used. The sandbox exposed no displays; normal
escalation enabled native Metal. Instruments' initial direct launch of the baseline
(PID 37985) requested Documents access at 08:31 local time; its 15-second trace
ended with the target still awaiting permission and contains no target GPU work.
The user's Allow response is logged at 08:58, after that target had exited; no
duplicate or resumed old launch is claimed. That trace is excluded. Profiler
attachment to an already directly launched player succeeded.

Arcade door, Office counter/normal view, Pool transparency/caustics basin, independent
Low lighting and fixed-simulation Office flashlight captures are PNG-byte and
pixel identical before/after at 1512×838. Package, settings, camera, simulation,
drawable and geometry match. A wall-clock flashlight pilot reached different
mannequin poses and was excluded; fixed input timing resolves that confound.
Rare normal-scale/mask identity fixes have targeted CPU evidence rather than a
claim that these unchanged shipped views demonstrate their boundary cases.

A genuine four-second native Pool video records 120 source frames at 1280×720,
30 FPS (ready frames 1–239, stride two). Sampled frames and free/held/thrown/moving
captures show the intact duck and water. All 294 physics records are finite,
30 are held, and recovery count stays zero. Reset leaves two dormant bodies and
reactivation resumes them; this is not body-removal proof or every-frame QA.

The 63.40-second diagnostic transition campaign covers texture quality, lower
lighting, Arcade↔Pool, same-ID reload, Office and resize. Several rapid Full and
Low-lighting requests were superseded before their own install; those demonstrate
cancellation, not completed state transitions. There were 61 ready-state
nonpresentations in a roughly 1.1-second transient gap and one loading miss;
rendering recovered, with no logged fatal/validation failure. The gap is not
attributed to a confirmed shader defect or described as zero skipped frames.
Across 390 resource records there is no fatal state. Late periodic frame samples 6000–6600
retain 142 textures, 1,311 bind groups, 56 render pipelines, 22 samplers and 116
persistent cache entries; buffers vary 677–682 and views 158–160, staging clears.
This is bounded non-accumulation evidence, not a long-term leak guarantee. RSS
samples varied during concurrent build/test work and are not a savings or settled
memory claim; Metal HAL allocation-byte fields remain unavailable. A subsequent
23.15-second isolated control captures fully installed independent Low lighting,
Full lighting, Static lightmaps, Full lightmaps and explicit same-map reload.
All five actual capture states match their intended settings and have zero
ready-state misses; this does not erase the longer campaign's transient gap.

<a id="shader-0110-matched-throughput"></a>
### Matched throughput

Both default executables ran directly, High textures/filtering, Full lightmaps
and reflections, bloom, FOV60, VSync off/Immediate, swap interval zero and no
engine FPS cap. Three interleaved 600-frame runs per side/view follow identical
120-ready-frame warmups and fixed 1/60 simulation. Pair order reverses in trial
two; two seconds between process exits/launches allows desktop state to settle.
All 24 accepted runs exit zero with 720 ready presentations and no ready misses.
Binary/package hashes, settings, camera/drawable, movement and per-frame geometry
match. CSV player samples use global-frame cadence, so their extrema differ by
sampling phase after variable load lengths. Reconstructing each sampled position
against the same ready-world movement schedule gives maximum Z error 1.24e-6 m
for all twelve lateral runs; this is recorded separately from sampled extrema.

| View / physical drawable | Before average FPS | After average FPS | Change |
| --- | ---: | ---: | ---: |
| Close closed door, 3024×1898 | 119.99 | 120.00 | +0.00% |
| Arcade cabinets, 3024×1898 | 119.98 | 119.99 | +0.01% |
| Bathroom lateral, 3024×1676 | 120.02 | 120.03 | +0.01% |
| Bathroom lateral, 1512×838 | 120.02 | 120.07 | +0.05% |

Average FPS is total measured frames divided by total loop wall time, pooling
1,800 frames per side/view. **No measurable FPS improvement is established.**
Despite uncapped Immediate settings, this direct-launch environment is paced near
120 FPS across substantially different GPU workloads; its cause is not isolated.
Do not compare this ceiling with historical wrapped renderer-stage throughput or
attribute those earlier gains to shaders. Internal GPU spans vary with scheduling
and are not substituted for a throughput gain. A separate single candidate Metal
trace has scene-fragment mean about 6.40 ms versus baseline 6.58 ms; that bounded
instrumented observation is insufficient for a general GPU speedup claim.

An initial candidate fullscreen attempt returned 3024×1676 instead of 3024×1898
and was rejected, including its unmatched pair. No smaller drawable is counted
as a shader improvement. The accepted rerun enforces each requested drawable.
Pre-campaign thermal/performance queries were unavailable; the post-campaign
query reports no recorded warnings. Temperatures were not controlled.
Permission waits, trace runs, builds and other native controls are excluded from
the accepted timing campaign.

<a id="shader-0110-verification-and-limits"></a>
### Verification and limits

Strict workspace/all-target/all-feature Clippy passed in debug and release.
Default workspace tests pass: 2,344 library plus seven integration, 19 existing
ignores. All-feature tests pass: 2,351 library plus seven integration, same ignores.
Python source contracts: 372 run, four optional skips. Four new owning-subsystem
CPU tests protect distinct transform, skinning, mask and raw-format boundaries;
they require no GPU, asset decode or map bake. Existing tests remain intact.
Independent source, actual native visual/probe/moving-prop, timing, motion-witness
and resource review found no unresolved correctness issue. The reviewer
independently recomputed the timing averages, geometry matches and sampled motion.

Native platform coverage is Apple M2 Pro/Metal only. Raw fallback was exercised as
an offscreen format control, not a physical non-sRGB display. No forced physical
device loss or other-platform native execution is claimed. Flashlight/contact
shadow equations and quality remain unchanged; issue #23 is not resolved here.
Evidence and queue notes are under `local/shader-0.11.0/`.

<a id="sound-0110"></a>
## Sound engine and content — 0.11.0 staging

Implementation for [issue #12](https://github.com/csd113/Places/issues/12), based on
`9e1bb32068c14311341d9cd20e3833fba86c5bee`, on `0.11.0-staging`. The editor remains
deferred. This stage does not change the renderer, light placement or caustics.

<a id="sound-0110-ownership-and-bounded-work"></a>
### Ownership and bounded work

`engine::audio` owns a [Kira 0.12.5](https://docs.rs/kira/latest/kira/) mixer
through a small [SDL3 playback backend](https://wiki.libsdl.org/SDL3/SDL_OpenAudioDeviceStream).
Kira remains the maintained mixing solution; using the existing SDL3 device layer
avoids CPAL's additional device-management thread and its disconnect panic path.
The backend retains the [logical default output device](https://wiki.libsdl.org/SDL3/SDL_OpenAudioDevice),
so SDL handles device changes. Initialization failure produces a silent owner, and browser builds keep
the same silent API. No microphone or recording device is opened.

The catalog admits at most 64 cues, 128 file references, 16 MiB per WAV, 30 seconds
per clip and 64 MiB decoded samples. All selected sounds are decoded and shared
before gameplay. Forty-eight mixer voices and a combined 48 live/pending source
budget bound playback; stopped-loop replacement can retry without unbounded
queues. One-shots drop on overload. Map replacement clears pending starts and
stops live voices; shutdown closes the SDL stream before freeing callback data.
Ordinary playback performs no file I/O or decoding. The callback generates PCM
into fixed scratch storage; SDL owns its own buffering and device thread.

A nonblocking atomic gate excludes Kira's resource-publication and retirement
boundaries, preventing an upstream retired-queue interleaving identified during
independent review. Contention never waits: existing audio continues processing,
loop starts retry within the budget and one-shots may drop. No Tokio runtime or
second preparation pool was added. Explicit benchmark capture alone enables a
bounded output PCM tap, drained and written by the game thread.

`game::app::audio_runtime` selects material footsteps, switch feedback, actual door
motion, selected prop interactions and physical impact cues. Physics exposes
bounded contact onsets and retained substep paths; it does not own playback.
Impact minimum speed/impulse, per-item cooldown and a global token budget suppress
resting contact chatter. Player footsteps follow walked distance, with teleport,
climbing, swimming and airborne guards. Spatial sources follow live physical
body centres. Stereo pan and smooth distance falloff are used; HRTF, acoustic
occlusion and reverb are outside this stage.

The listener remains at the controlled player's ears in third person and after
`/become`. Menus, console, loading and focus loss pause both clock domains.
World-domain speed follows `/time scale` exactly, including zero; real-domain
player actions and authored ventilation retain normal speed at zero world time.
Player movement and held-object control retain their existing unscaled rules.
Master, effects and ambience volume plus mute persist under Settings → Audio.
A deliberately stopped loop fades for 12 ms; that short retiring tail can finish
across a subsequent reset/mute boundary.

<a id="sound-0110-shared-authoring-and-sounds"></a>
### Shared authoring and sounds

Audio components/actions use the existing typed entity infrastructure. Trigger
filters select player, any actor, or an exact item asset/instance and held state;
selectors combine. Per-actor and aggregate occupancy support enter/exit, once,
repeat and cooldown bindings. Swept player and physical substep paths catch thin
crossings, including bounced paths and carried items. Accepted events defer
across the per-frame budget rather than discarding cleanup exits. Full-queue
membership rollback retries final exits. Synchronous event descendants preserve
the chain-depth limit across deferred frames.

The library contains **24 cues, 63 real WAV files, 5.26 MiB on disk and 21.04 MiB
decoded**. Selected CC0 recordings come from Kenney, rubberduck, OwlishMedia,
TinyWorlds, Peludo, Longor, isaiah658 and csaszi. Exact primary declarations,
source/archive/member hashes, import edits and output hashes are recorded in
[the asset guide](../assets/AUDIO.md), `assets/audio/provenance.json`,
`authoring.json` and retained notices. The duck uses the actual public HQ MP3
preview, rather than claiming access to the login-only original WAV.

Office has a quiet upper-floor HVAC region and tappable mug; Pool has hall water
ambience; Aero's outdoor region has birds; Home has a tappable mug. Generator
integration preserves these additions. Prepared maps require recompilation.
The small native fixtures stay outside the shipped lineup and automatic CI.
Office audio evidence uses a verified current Static package; the small native
fixture also exercises Full playback. Full Office lighting rebaking is outside
this audio acceptance campaign.

<a id="sound-0110-validation-and-evidence"></a>
### Validation and evidence

Local formatting, locked all-target/all-feature check, strict debug and release
Clippy, the all-feature release workspace build and all 26 Rust tests pass.
Nineteen focused audio/trigger/contact regressions add about 0.01 s CPU runtime:
they protect allocation and path limits, callback resource contention, source
cleanup, trigger overflow/chain depth, contact noise and true swept crossings.
They do not bake maps. The existing Python campaign passes 372 tests with four
expected skips; asset, audio, prop, texture and maintained-generator checks pass.
Two independent final reviewers found no remaining confirmed implementation
blocker after the retirement race and event-chain fixes.

Eight direct-executable native campaigns pass on Apple M2 Pro / Metal:

| Campaign | Observed outcome |
| --- | --- |
| Controls | Cooldown/rearmed switches, open/close/reopen door, selected prop cue, exact held instance, type/held rejection, once-only thin crossing |
| Movement | Real W movement selects wood, carpet, tile/hard and gravel; footsteps continue at zero world time and stop in menus |
| Lifecycle | Actual distance attenuation and turn-reversed pan; world/real clock policy, menu/focus pause, `/become` listener invariance, explicit stop and reload |
| Settings | Ordinary keyboard navigation changes all three volumes, toggles mute repeatedly, produces exact PCM silence while muted and preserves choices after reload |
| Unavailable audio | Invalid process-local SDL audio driver reaches the initialization-error fallback; the game continues with zero voices/output frames |
| Voice overload | Ten start/replace/stop cycles of 60 requests retain at most 48 voices; 1,008 accepted starts, 252 overload refusals, zero errors and stable cache |
| Physical contact | Ordinary E pickup/release produces one contact start; the body sleeps and no further sound starts during three seconds at rest |
| Map changes | Ten alternating map changes clear every old source and restart the next; decoded memory stays exactly 22,065,040 bytes and process RSS settles near 335 MiB |

Native captures contain finite, nonzero **48 kHz stereo output PCM**, with zero
capture drops in accepted runs. These observations establish actual callback
output and accepted playback; they do not establish perceived sound quality.
Human listening was a separate acceptance step because this execution
environment cannot listen to its audio input. During final release acceptance,
the user listened to the native clip and answered **“Listened — acceptable”**.
That confirms listening acceptance without claiming physical unplug/replug or
cross-platform device validation. The [fixture clip](../videos/sound-0.11.0-fixture-native.mp4)
contains 360 real native frames and the playback callback's PCM, with no
microphone or substituted soundtrack. Per-frame presentation timestamps follow
the recorded output-audio clock (maximum mux quantization error 0.21 ms).
The [Office clip](../videos/sound-0.11.0-office-native.mp4) contains 300 native
frames with authored ventilation and actual carpet footsteps, using the verified
Static package. Its mux timestamp error is below 0.17 ms.
The [Audio settings capture](../images/sound-0.11.0/audio-muted.png) retains the
unchanged native frame. Capture runs are intentionally paced and excluded from
performance measurements.

The tested all-feature game executable has SHA-256
`2088fc1fe237718556503eaad7d216bbc02830fbf82e4150c1baaa9b17cdb043`.
Raw traces, output WAVs, plans, gate logs, package identities and compact receipts
remain in `local/sound-0.11.0/`, outside Cargo's build directory. The final game
and compiler are preserved once under its `bin/` directory.

<a id="sound-0110-average-fps"></a>
### Average FPS

A controlled audio-off/on comparison uses the same final executable, current
Office Static package, stationary camera `(21, 1.6, 10)`, High textures/16×
filtering, Static/low lighting, reflections Off, bloom On and 60° FOV. Each run
warms up 240 frames and measures 3,600 frames; order is off/on/on/off. V-sync and
all frame caps are disabled. The actual mode is **Immediate**, with **1920×1080
physical drawable pixels**. More than 81% of measured frames finish faster than
the display's 120 Hz interval, and sustained throughput exceeds refresh: these
runs are uncapped. No video/PCM capture or competing build runs during timing.

| Playback | First average FPS | Second average FPS | Pooled average FPS |
| --- | ---: | ---: | ---: |
| Disabled | 198.6 | 199.7 | **199.2** |
| Enabled, authored HVAC loop | 195.1 | 207.2 | **201.0** |

The difference lies within run variation; no throughput improvement is claimed.
This measures controlled playback cost in the current build. Earlier pre-stage
and Full-lighting diagnostics showed display pacing and are **inconclusive**;
they are excluded from the matched result. A whole-stage Full-lighting throughput
comparison is therefore not established.

Developer teleports and player-only respawn retain the established trigger
baseline policy: no enter/exit events are fabricated, and existing cue state
is retained. Audition region transitions with normal movement; full reset/load
restores authored emitter state.

Unknown elevated prop/architectural support surfaces currently use the hard-floor
footstep fallback; authored floors, regions, ramps and stairs select their
material. Device disappearance/reconnection follows SDL's logical-device
behavior; physical unplug/replug has not been performed on this host. An initialization
failure stays silent for that session; restart after restoring assets/device
availability. Web audio
is intentionally silent for this initial desktop stage.
