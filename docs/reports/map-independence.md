# Map independence — October 12, 2026

Places builds, packages and starts with zero installed maps. Authored sources,
assets and existing ignored packages are preserved; no map is embedded or
fabricated as a fallback. Version remains 0.8.0 on `arcade-env`.

## Coupling removed

- Packaging previously required the compiler, current packages for every curated
  source, and `levels/lineup.txt`. The default `tools/package.sh OUTPUT` now exports
  the player and runtime assets with empty map folders. `--maps DIRECTORY`
  explicitly includes any supported external packages after archive/dependency
  validation and ordinary player discovery. Failed validation leaves the previous
  export intact. No source names or map compilation are involved.
- Cargo source distributions exclude `levels/` and `assets/levels/`. There is no
  project build script that fetches or bakes a map.
- Startup and worker requests no longer require the official demo or retry it.
  An empty installation goes straight to the usable menu. An explicit missing
  `PLACES_LEVEL` leaves the menu with a warning; otherwise the first discovered
  package can provide the background. Geometry authoring commands require an
  explicit source. Dynamic washer parts follow placed content in any map.
- Production-source includes, directory scans and large navigation/content
  campaigns were removed from normal Rust tests. Ten bounded unit fixtures total
  99,424 bytes. Contact, door, water, navigation, encounter, weather, material,
  codec/security and numerical-boundary assertions remain; redundant authored
  layout promises stay with source checks and deliberate developer campaigns.
  No new ignore marker or feature gate hides required coverage.

Explore rescans on opening. Import rescans both writable folders and continues
past invalid candidates, preserving successful imports. Missing, corrupt or stale
packages fail cleanly. Errors use two bounded lines clear of Back and the keyboard
legend, with full diagnostics in the log. Catalog and dependency hashes, package
format checks and Full atlas quality requirements remain enforced.

## Verification

A disposable source copy physically omitted both production map directories;
original files were never renamed or removed. Existing Rust dependency caches were
reused. Runtime assets and state were explicitly pinned for its fixture tests.

| Command / check | Result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed in normal and map-free source trees |
| Release Clippy with the same targets/features | Passed |
| `cargo test --workspace` | 2,232 library + 8 integration tests passed; 18 existing manual native diagnostics |
| Map-free `cargo test --offline --workspace --all-features` | 2,238 library + 8 integration tests passed; library execution about 31 seconds |
| Map-free offline check of all targets/features and release build of both binaries | Passed without fetching or baking production maps |
| `cargo package --offline --allow-dirty` | Archive verified by compilation; actual archive contains neither production map directory nor any `.placesmap` |
| `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'` | 352 tests passed, 4 explicit native/export/weather suites skipped |
| `PLACES_TEST_EXPORT=1 python3 -m unittest tests.test_packaging` | All 6 passed, including real export/sibling preservation |
| `python3 tools/assets/validate.py` | 555 catalog assets, 336 placeable; zero errors/warnings |
| Default and explicit external-map exports | Both flat and macOS layouts passed; default had zero maps, explicit export one arbitrarily named Arcade package |

Hosted Rust verification now runs in a temporary checkout with optional map
content omitted. Authored-source/asset checks run separately. Neither normal
Cargo commands nor hosted CI compile/bake the shipped lineup. The existing
`tools/build-maps.sh`, `tools/levels/compile.py` and full desktop gate remain
explicit authoring operations.

Native SDL3/wgpu Metal acceptance used the isolated map-free application bundle.
The menu and empty Explore instructions rendered normally; empty import, Settings,
saved Low textures with Full lighting, restart and menu Quit worked. After startup,
one corrupt package and current Arcade were placed in its private import folder.
The valid map appeared despite the bounded error, loaded and supported walking,
pause/settings/resume and normal Quit. Changing textures Low → Medium retained
Full lightmaps/reflections and the moved player position. The first package decode
took 2.19 seconds; recorded lighting/atlas preparation was zero. No production
package was rebuilt and no full lightmap bake was run for this task.

## Remaining content work

Only the local Arcade package matches both current source and catalog bytes. The
other nine local packages have older catalog hashes and are correctly rejected by
discovery until deliberately rebuilt. Aero's source bytes match its package, but
its catalog provenance is stale. Model Zoo also has stale source bytes and a
recorded Full `page overflow` with no atlas. These are retained content/compiler
follow-through items; quality and overflow errors were not weakened. Compiler
executable freshness is separate from runtime format/dependency compatibility.

No authored level, PNG/GLB, shader, lighting/UV/material/entity rendering contract, README
promotion or release metadata was changed. Preexisting untracked audits remain
untouched. Exact publication/CI and final scoped cleanup are recorded in the
handoff; disposable builds, exports, fixtures and raw traces are removed afterward.
