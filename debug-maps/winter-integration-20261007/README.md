# Winter integration review

Prompt 8's polished calm Winter and the same scene in Prompt 7's severe mode.
The current severe source is synchronized by `tools/levels/build_winter.py`.
The older Prompt 7 frozen collection remains unchanged.

```sh
python3 tools/levels/build_winter.py --check
target/release/places-compile build assets/levels/winter.json --workers 12
target/release/places-compile build debug-maps/blizzard-20261007/sources/blizzard_review.json --out levels/blizzard_review.placesmap --workers 12
python3 tools/bench/capture_winter_integration.py --out target/winter-integration-review
python3 tools/bench/capture_winter_integration.py --out target/winter-integration-review-lower --qualities medium,low --views square,lodge,pond,forest-rest,string-cottage,interior
```

Keep the macOS console unlocked for presentation tests and frame profiling.
The initial full gate recorded eleven locked-console skips; these commands
rerun the native suite and collect final completed-frame on/off/off/on timings
after other CPU/GPU work has stopped:

```sh
env -u PLACES_ASSET_ROOT RUSTC_WRAPPER= CARGO_INCREMENTAL=0 python3 -m unittest -v tests.test_wgpu_bootstrap
python3 tools/bench/capture_snowfall.py --mode perf --views square --out target/winter-integration-perf-calm
python3 tools/bench/capture_snowfall.py --level blizzard_review --mode perf --views square --out target/winter-integration-perf-severe
```

The ignored `evidence/` collection preserves raw before/after captures,
movement/weather/frame telemetry, validation logs, exact source/package copies,
matching player/compiler/SDL binaries and dependency assets outside `target`.
Use its frozen launcher independently of later repository or compiler edits:

```sh
python3 debug-maps/winter-integration-20261007/evidence/launch.py winter
python3 debug-maps/winter-integration-20261007/evidence/launch.py blizzard_review --quality high
```

See `docs/reports/winter-integration.md` for the art changes, native evidence,
performance observations, validations and remaining implementation limits.
Shared target artifacts and all earlier debug collections are retained for the
queued certification pass. This review does not execute Prompt 9.
