# Prompt 7 blizzard review

Winter remains calm by default. This separate source copies its complete scene
and opts into severity 1, five-meter visibility and [8,3] m/s wind. The same
1,400 seeds occupy a six-meter nearby volume. Fog performs distance occlusion.

```sh
env RUSTC_WRAPPER= CARGO_INCREMENTAL=0 cargo build --release
./target/release/places-compile build assets/levels/winter.json --workers 12
./target/release/places-compile build debug-maps/blizzard-20261007/sources/blizzard_review.json --out levels/blizzard_review.placesmap --workers 12
python3 tools/bench/capture_snowfall.py --out debug-maps/blizzard-20261007/evidence/calm
python3 tools/bench/capture_snowfall.py --level blizzard_review --out debug-maps/blizzard-20261007/evidence/severe
python3 tools/bench/capture_snowfall.py --level blizzard_review --mode perf --views square --out debug-maps/blizzard-20261007/evidence/perf
```

`evidence/` retains matching source/packages/assets, pinned executables/SDL,
launch instructions, raw native telemetry, captures and verification logs outside
`target/`. Its `launch.py` starts either calm Winter or severe Blizzard Review
with isolated state. See `docs/reports/blizzard-whiteout.md` for findings and
limits. Existing snowfall, movement, compiler and external Office/consolidation
archives remain intact. Shared build artifacts are retained for queued tasks.
