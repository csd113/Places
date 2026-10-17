# Places 0.11.0 prerelease

October 17, 2026. This source prerelease completes the approved 0.11.0 queue.
It builds on 0.10.0's exploration, physics, environments and Map Compiler GUI.
Maps remain optional external content; an empty installation opens the menu.

## Included work

- [Source ownership and cleanup](repository-reorganization-0.11.0.md): reusable
  engine, game and Map Compiler modules, thin entry points, retained accepted
  assets and documented future editor boundaries.
- [Rust/dependency prerequisite](dependency-upgrade-0.11.0.md): Rust 1.99.1 and
  reviewed dependency migrations with the existing native backend limits.
- [Renderer foundations](renderer-0.11.0.md): surface/capture/resource lifetimes,
  same-map reload ownership, changed-only emission and cached visibility,
  bounded offline overlap diagnostics and the reviewed Office desk repair.
- [Shader corrections](shader-0.11.0.md): small-scale normal transforms,
  fluorescence mask identity, raw-surface display copy and shared fragment work.
- [Close-up flashlight correction](flashlight-0.11.0.md): remove the vertical
  source drop behind the confirmed floating-box appearance while retaining real
  cast shadows, lateral offset, beam settings and mannequin behavior.
- [Console and typed gameplay actions](developer-console-0.11.0.md): 58 curated
  pickup IDs, safe reset/removal, trigger inspection, bounded world time and
  fixed stepping, model representation and third-person/session-local copies.
- [Sound and trigger foundations](sound-0.11.0.md): bounded Kira/SDL3 playback,
  spatial sources, footsteps, doors, switches, props, licensed audio, swept and
  filtered trigger volumes, cleanup, persisted volume/mute and environment cues.
- [Fixture housing/emission audit](fixture-leaks-0.11.0.md): close procedural
  mounting backs, route ordinary bodies through Full transport and restrict
  Home lamp/strip emission to intended luminous surfaces.
- [Source-aware caustics](caustics-0.11.0.md): explicit source eligibility,
  independent coloured contributions, bounded visibility, submerged decals and
  authored tint, strength and gentle motion controls.
- [Loading/resource ownership](loading-0.11.0.md): canonical package identity,
  bounded archive/image/model reads, shared decoding, cache accounting and
  transactional imports that preserve installed maps on failure.
- [Settings/input](settings-input-0.11.0.md): partial settings repair, binding
  ownership, Console rebinding, fresh presses, focus/pause clock transitions and
  synced atomic persistence with retained invalid backups.
- [Entity/navigation correctness](entity-navigation-0.11.0.md): sequence
  cancellation/publication, stopped-time ownership, live door/step constraints,
  nearest-point search, displayed-pose blends, route completion and bounded AI work.

The [retirement stage](rust-test-removal-0.11.0.md) removed the old costly Rust
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
