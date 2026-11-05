# Python cleanup and baseline map checks — 0.13.1

Starting revision: `124f6c85003a74d6cfefbd123f3ffa3db0a6ab11`, verified as the
remote `0.13.1-dev` tip before work. Changes were made in a separate clean clone.

## Retired files

The following 18 tracked Python files were removed after checking imports,
scripts, source tests, CI and current authoring documentation. Python source
count changes from 289 to 272, including the new 34-line weather helper.
The [starting tree](https://github.com/csd113/Places/tree/124f6c85003a74d6cfefbd123f3ffa3db0a6ab11)
preserves all retired recipes and exact historical camera tables. Dated reports,
captures, asset masters, completed exports and archived map sources are retained.

| Reason | Exact paths |
| --- | --- |
| Frozen Demo camera journals and their dedicated runner | `tools/bench/capture_home.py`, `tools/bench/capture_pool.py`, `tools/bench/capture_outdoors.py`, `tools/bench/capture_demo_views.py` |
| Frozen Winter style journal | `tools/bench/capture_winter_style.py` |
| Bundle launcher prohibited by the current direct-executable validation workflow, plus its two dedicated identity tests | `tools/bench/native_app.py`, `tests/test_native_app.py` |
| Scratch-only environment reconstructions superseded by saved editor projects | `tools/levels/build_beach.py`, `tools/levels/build_frutiger_aero.py`, `tools/levels/build_lantern_hollow.py`, `tools/levels/build_office.py`, `tools/levels/build_retro_arcade.py`, `tools/levels/build_winter.py` |
| Completed bootstrap migrations, old dressing recipes and their private restroom helper | `tools/authoring/environment_home_pool.py`, `tools/levels/migrate_office_collection.py`, `tools/levels/refine_home.py`, `tools/levels/refine_pool.py`, `tools/levels/arcade_restroom.py` |

Winter's severe-control derivation still has live consumers. Its unchanged
algorithm and configuration path now live in `tools/levels/winter_weather.py`;
weather source/package tests and explicit native acceptance import it. Winter
assembly tests retain their independent cottage-mount witness without importing
a retired map builder. No gameplay test was removed to resolve the four failures.

Additional tools were inspected and retained: reusable shore and party helpers
have focused consumers and documented authoring value; railing, soundscape and
outdoor-dressing helpers are shared. `build_outdoor_route.py` and
`build_model_zoo.py` still support explicit fixtures and existing source controls.
Asset/model/texture authoring, production validation, package/security checks,
current collection acceptance and specialized native diagnostics remain intact.

Current ownership and commands were corrected in the map, environment, audio,
offline-tooling and benchmark guides. Dated release links to removed builders
now pin their historical revision. CI did not invoke any retired entry point,
so its existing source checks and full-map-baking prohibition remain unchanged.

## Four baseline findings

* The collection travel check now verifies elevator destinations, concrete
  arrival cabin IDs, local landings and reciprocal links across all nine sources.
* Lantern Hollow's Jun/quest check retains the signed Hub return safeguard using
  its actual elevator stop. Its perimeter check compares occupied intervals on
  every side, including the existing four-metre east lift surround, rather than
  requiring exactly four fence segments. Subdividing stock is allowed; missing
  stock and unintended gaps fail.
* Arcade's existing doorway/route check remains intact. Both body/cap pairs at
  the south elevator opening intruded 0.15 m from each jamb. The left pair now
  ends at X=6.05; the right pair begins at X=7.55. Only six numeric source values
  changed. Baseboards have no collision under the format contract; this fixes
  visual trim overlap and does not establish a player-blockage bug.

## Verification

The original full source suite reproduced exactly four failures (373 tests,
16 skips). The focused updated source checks passed (42 tests, one explicit
package skip). Mutation controls reject missing/wrong/one-way elevator routes,
missing fence stock, unintended perimeter gaps and the original trim intrusion.

Final checks passed:

* `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'`:
  371 tests, 16 explicit skips, 148.6 seconds. Only the two tests dedicated to the
  removed bundle launcher were retired; all gameplay/geometry checks remain.
* `python3 tools/assets/validate.py`: no errors; the same 17 existing wall/room
  warnings as the starting revision's performance-task record.
* `python3 tools/levels/compile.py --list-json`, native Winter acceptance `--help`,
  and AST parsing/import inspection: both curated distributions remain listed,
  the helper's native consumer imports successfully, and no retired imports remain.
* `cargo clippy --workspace --all-targets --all-features -- -D warnings` and
  `cargo build --locked --release --workspace --all-features`: passed with cached
  dependencies in offline mode, using a dedicated target directory and disabling
  the sandbox-incompatible `sccache` wrapper. Release build: 6m 05s.
* Release compiler `check-project projects/environments --asset-root assets`:
  all sources valid, no conversion or source writes. Production Luau guard:
  11 bounded refusal cases and one finished-project check passed without baking.
* Two source-derived doorway controls compiled as Static in about 0.1 seconds
  each, with no warnings. They copy the exact Arcade wall and four trim records
  into one neutral lit room, without props or an elevator. Direct native player
  captures at identical camera/settings show the former 15 cm trim projections
  and their removal at both jambs. Capture receipts verify matching binary,
  loaded package identity and 1280×720 drawable. The first sandboxed player exited
  with no available display; the desktop runs exited successfully.

The focused controls establish visual trim clearance, not a new full-map
traversal or performance claim. No shipped map or complete collection was baked;
compiled distributions require explicit re-export before shipment. Local evidence
and matching player/compiler binaries are retained outside the source checkout.
All owned build/native processes exited, the reproducible target directory was
removed after preserving verified binaries, and the shared validation lock was
explicitly released at `2026-11-05T00:24:07Z`.
