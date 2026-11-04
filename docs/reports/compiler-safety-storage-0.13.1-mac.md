# Compiler publication, reuse and compact delivery — 2026-11-04

Validated on the audit's Apple M2 Pro Mac, from `2e503223ca0a9c263ecf4f837b81af1af14e54a3`,
in an isolated checkout. Maintained maps, assets and other audit checkouts were
unchanged. Builds and measurements held `/tmp/places-audit-validation.lock`;
the performance owner's four native probes ran during an explicit quiet handoff.

## Changes

- Shared publication guards require a `.placesmap` destination, reject editable
  input aliases and foreign map/project destinations, and recheck source content,
  destination identity and canonical paths immediately before atomic publication.
  Cancellation and concurrent edits retain the previous output or external work.
- Member identity includes its source, attached scripts and package release,
  independently of project menu metadata. Navigation reuse verifies declared
  records and a key covering physical authoring inputs, compiled collision,
  scripts and compiler identity. Component/script changes conservatively
  invalidate navigation. Worker counts stay out of archive metadata; optional
  cache diagnostics cannot exceed the compiler metadata record limit.
- `build --compact` and `export-project --compact` publish binary records with
  Zstandard-19 directly, using bounded level-9 prop layout selection. Bootstrap
  JSON stays readable; development defaults remain Zstandard-6. The existing
  guarded `optimize-storage --compact` migration remains available.

## Measured tradeoffs

Single quiet wall-time samples, using the same nine-member Low/Static collection
as the audit. The one-worker controls also set `PLACES_TOOL_WORKERS=1`.

| Complete project operation | Audit | Fixed compiler |
| --- | ---: | ---: |
| Unchanged export, one worker | 2.767 s | 2.811 s |
| Display metadata refresh, one worker | 17.274 s | 3.439 s |
| Fresh fast export, 12 workers | — | 32.306 s |
| Compact refresh from prepared fast output, 12 workers | — | 32.427 s |

The metadata refresh is about 80% faster in these samples. All nine members and
their **100 compressed records** were reused byte for byte while project metadata
and recoverable sources refreshed. Changes included name, author, description,
thumbnail reference and a member menu title. This is a scoped export result.

| Delivery profile | Package bytes | Warm decode median, three reads |
| --- | ---: | ---: |
| Fast | 35,833,520 | 94.9 ms |
| Compact | 30,357,179 | 108.8 ms |

Compact saves **5,476,341 bytes (15.28%)** and uses more encoding time. Its refresh
peaked at 337,575,936 bytes RSS. These encode/decode samples support an explicit
delivery option; they are not player-load, FPS or worst-case memory guarantees.

## Preservation and verification

- Same/equivalent paths, hard links, symlinks and project inputs are protected.
  Actual CLI cancellation returned 130; edits to source or output during a bake
  returned errors while preserving the prior package or external bytes.
- Agent, obstacle, collision and geometry edits missed navigation reuse. Agent
  and obstacle edits reused lighting. Every resulting incremental archive was
  byte-identical to its forced Static reference. Serial/12-worker Full fixture
  archives were also byte-identical.
- All nine decoded collection worlds match the original audit package. Fast and
  compact outputs retain exact ordinary records, reconstructed props and all
  stored switch states; the Full fixture also retains exact decoded HDR planes.
  Source recovery is byte-exact. Native validation passed with 385 intact
  collection dependencies, nine fixture dependencies and no warnings.
- Existing compact storage migration also passed with exact decoded records and
  unchanged provenance. No lighting precision, transport or rendering setting
  changed. Full preservation uses the small fixture, not a fresh Full collection.
- Final Clippy passed for all targets/features; all **458 tests passed**; the
  locked release workspace/all-features build passed in **5m37s**. Formatting and
  patch checks passed. Three narrow regression tests require no assets or GPU.

Retained evidence is in the Mac `task-14/verification` directory (`results.json`,
`comparison.json`, validators, decoded oracles, sources and packages), alongside
the campaign scripts and acceptance logs. Compiler SHA-256:
`1100e26ca2a71c838928d56d971d83a030988a2eb0f1d1d4caab4794e1b9d2b7`.
One optional timing-wrapper attempt failed sandbox resource-stat access after
successful publication; it is marked excluded. Accepted one-worker controls use
wall-clock timing without that wrapper.
