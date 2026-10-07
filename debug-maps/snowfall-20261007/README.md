# Snowfall native review

The maintained source is `assets/levels/winter.json`; `weather: {kind: snow}`
selects the engine's reusable gentle defaults. This campaign uses the actual
SDL3/Metal player and held movement controls, including doorway crossings,
turning, strafing, dark sky and pale ground at all three quality levels.

Build the current release and compile Winter before running:

```sh
env RUSTC_WRAPPER= CARGO_INCREMENTAL=0 cargo build --release
./target/release/places-compile build assets/levels/winter.json --workers 12
python3 tools/bench/capture_snowfall.py --out target/snowfall-evidence/captures
python3 tools/bench/capture_snowfall.py --out target/snowfall-evidence/perf \
  --mode perf --views square
python3 tools/bench/capture_snowfall.py --out target/snowfall-evidence/cycle \
  --mode cycle --qualities high --views door-out
./target/release/places-compile build \
  debug-maps/snowfall-20261007/sources/snowfall_contrast.json \
  --out levels/snowfall_contrast.placesmap --workers 12
python3 tools/bench/capture_snowfall.py --out target/snowfall-evidence/contrast \
  --level snowfall_contrast --views square,pale-ground
```

`evidence/` is an ignored durable snapshot outside `target/`. It keeps the
original generated PNG master, final sprite, matching source/catalog/assets,
playable package, pinned player/compiler, native captures, telemetry and logs.
See `evidence/launch.py` for the matching frozen launch and `docs/reports/light-snowfall.md`
for measured results and limits. Existing debug playgrounds and external Office
and consolidation archives are independent and remain intact.

Weather telemetry measures complete CPU billboard sync (including steam/shared
queue write), seeds evaluated, flakes submitted and roof suppression. Quad
coverage includes alpha-zero texels and opaque-depth rejection: it is an upper
estimate, not a GPU fragment counter. Performance A/B uses the same binary and
package with `PLACES_BENCH_WEATHER_OFF=1`, waits for GPU completion, and disables
VSync. It records 600 frames after 120 warm-up frames in alternating on/off order.
The CPU seed/group/vertex storage never grows after level load; wgpu's own
queue staging and engine-wide allocations are outside that narrower claim.
