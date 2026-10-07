# Final winter certification review

Prompt 9 audits the reusable winter toolkit and both calm and severe Winter.
The report and compact results are in `docs/reports/winter-final-audit.md` and
`docs/reports/winter-final-audit-evidence/`. The ignored `evidence/` archive
retains full validation logs, native images/CSV traces, matching sources,
packages, assets, player/compiler/SDL binaries and preservation manifests.

Run from the repository root with an unlocked macOS console:

```sh
caffeinate -di sh tools/verify.sh
target/release/places-compile build debug-maps/blizzard-20261007/sources/blizzard_review.json --out levels/blizzard_review.placesmap --workers 12
caffeinate -di python3 tools/bench/capture_winter_integration.py --out target/winter-audit-review
caffeinate -di python3 tools/bench/capture_winter_integration.py --out target/winter-audit-lower --qualities medium,low --views square,lodge,pond,forest-rest,string-cottage,interior
caffeinate -di python3 tools/bench/capture_snowfall.py --mode cycle --qualities high --views door-out --out target/winter-audit-cycle-calm
caffeinate -di python3 tools/bench/capture_snowfall.py --level blizzard_review --mode cycle --qualities high --views door-out --out target/winter-audit-cycle-severe
caffeinate -di python3 tools/bench/capture_snowfall.py --mode perf --views square --out target/winter-audit-perf-calm
caffeinate -di python3 tools/bench/capture_snowfall.py --level blizzard_review --mode perf --views square --out target/winter-audit-perf-severe
```

The frozen audit launcher needs no `target` directory or compilation:

```sh
python3 debug-maps/winter-audit-20261007/evidence/launch.py winter
python3 debug-maps/winter-audit-20261007/evidence/launch.py blizzard_review --quality high
```

Previous snowfall, blizzard, winter-integration, movement and compiler review
archives retain their original launchers and payloads. Historical evidence
recovered from `target` before cleanup lives under
`evidence/recovered-target/`, with original relative paths and file hashes in
`evidence/recovered-target-manifest.json`. Paths embedded in historical logs
still name their original locations; the frozen launchers above are the
supported runnable review paths. Persistent energy and lock settings are unchanged.
