# Runtime interaction recipes

Use these inputs with the actual authored project and an explicit preserved
package/binary. They are setup definitions, not proof of successful interaction.
Inspect the native snapshots and captures before accepting frame timings.

`home-books.json` uses the clear side spawn `3.6,1.6,10.7,270` and view
`270,-25` from `views.json`. It places the two books on the bed, crouch-jumps
onto the 0.55 m bed, expands to standing and retreats. The old rear-wall spawn
`1.9,1.6,11.8,0` has insufficient full-body clearance in the current map and
must not be reused. A standing jump alone falls short of the bed.

Run wall-clock input actions with real simulation delta and a long enough
wall-clock recording. A fixed-frame recording can finish its measured frames
before the later input actions occur. For example, on a Windows drawable whose
physical size matches the requested window:

```sh
python tools/bench/frame_time.py --binary PATH_TO_PLACES --package PATH_TO_ENVIRONMENTS --out NEW_RUN_DIRECTORY --level home --spawn=3.6,1.6,10.7,270 --camera=270,-25 --actions tools/bench/performance/home-books.json --seconds 7 --warmup 0 --real-delta --size 1280x720 --expected-drawable 1280x720
```

V-sync and frame limiters must be disabled for FPS claims. Verify actual
presentation, physical drawable and uncapped pacing in the receipt; historical
logical-size metadata does not establish the current backing scale. A result
near refresh rate remains inconclusive without a workload/pacing control.

The frame-time harness rejects an escaped player with no clear recovery spawn.
This guard does not establish that every camera is useful or that a planned
route succeeded. Native functional validation remains separate from timing.
See the [Windows qualification report](../../../docs/reports/runtime-qualification-0.13.1-windows.md).
