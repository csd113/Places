# Native model-lighting regression replays

These are the accepted production camera, player and animation controls for the
[model-lighting correction](../../../../docs/model-lighting-root-cause-and-fix.md).
They use real Lantern Hollow skeletons and kitchen models in Demo, Hallows and
`levels/home_showcase.json`; no replacement models or synthetic lighting scene.

Build current packages before captures. The compiler records its executable
identity; keep the same compiler while preparing/verifying packages. A final
player rebuild refreshes its embedded Demo archive. See
[desktop verification](../../../../docs/VERIFICATION.md) for the complete gate.

```sh
cargo build --release --bins --features visual-diagnostics
target/release/places-compile build assets/levels/places_demo.json --out assets/levels/places_demo.placesmap
target/release/places-compile build assets/levels/lantern_hollow.json --out assets/levels/lantern_hollow.placesmap
target/release/places-compile build levels/home_showcase.json --out levels/home_showcase.placesmap
cargo build --release --bin places --features visual-diagnostics
```

The capture tool refuses to overwrite existing images. Sequence capture paths
start at `debug-maps/model-lighting-replay/`; copy a sequence and change only those
paths to a fresh local output directory for each new run. Do not change cameras,
frames, delta, FOV, quality, exposure or poses when making a matched comparison.
Native execution requires an unlocked desktop session and a working GPU adapter.

## Static cupboards, sink, refrigerator and corridor

```sh
python3 tools/bench/capture_art_style_hero.py --manifest tests/fixtures/native/model-lighting/demo-kitchen-corridor.json --out debug-maps/model-lighting-replay/demo-high --quality high --diagnostic final --fixed-delta 0.016666667 --capture-frame 60
python3 tools/bench/capture_art_style_hero.py --manifest tests/fixtures/native/model-lighting/hallows-kitchen.json --out debug-maps/model-lighting-replay/hallows-kitchen-high --quality high --diagnostic final --fixed-delta 0.016666667 --capture-frame 60
python3 tools/bench/capture_art_style_hero.py --manifest tests/fixtures/native/model-lighting/home-showcase-cabinet-sink.json --out debug-maps/model-lighting-replay/showcase-cabinets-high --quality high --diagnostic final --fixed-delta 0.016666667 --capture-frame 60
python3 tools/bench/capture_art_style_hero.py --manifest tests/fixtures/native/model-lighting/home-showcase-refrigerator.json --out debug-maps/model-lighting-replay/showcase-fridge-high --quality high --diagnostic final --fixed-delta 0.016666667 --capture-frame 60
```

Repeat at `--quality medium` and `low` with new outputs. `--diagnostic baked-light`,
`world-normal` and `albedo` isolate components; final composition remains the
visual acceptance gate. The normal player can omit `--diagnostic` for ordinary
composition. `demo-movable-skinned.json` and `demo-night.json` cover the actual
animated/skinned cat, movable washer/door and nighttime environment.

## Original held-pose skeleton approach and retreat

The target is `campfire_skeleton_1`, position `[-16.7009,0,-31.7478]`, yaw
`-150.0229`, authored constant `pose_sit_chair`. `camera-only.json` leaves the
player fixed and moves the render eye through 12/6/3/1/0.8/0.7/0.6/0.5/0.4 m,
then retreats. `player-only.json` fixes the close render eye and moves the player;
`orientation-only.json` rotates the eye without translation. Capture receipts
record actual camera/player coordinates, quality, frame and simulation clock.

```sh
python3 tools/bench/capture_art_style_hero.py --manifest tests/fixtures/native/model-lighting/hallows-skeleton.json --lighting-sequence tests/fixtures/native/model-lighting/camera-only.json --frames 2100 --fixed-delta 0.016666667 --move-script forward@999-1000 --entity-light-trace --diagnostic final --quality high --out debug-maps/model-lighting-replay/camera-run
```

Substitute `player-only.json` (2100 frames) or `orientation-only.json` (420).
Repeat with `--diagnostic world-normal` using a sequence copy with fresh capture
paths. The distant held-control script prevents ordinary automatic camera motion
without moving the player during these runs. The ordinary controller/collision
approach uses `hallows-crouch.json`, `real-crouch.json`, 720 frames and
`--move-script crouch@0-14,forward@1-5.2,backward@6.2-10.0`. Its closest actual
collision-limited separation is approximately 0.600017 m; a requested position
must never be reported as an achieved position without the capture receipt.

## Quality restoration

```sh
python3 tools/bench/capture_art_style_hero.py --manifest tests/fixtures/native/model-lighting/hallows-quality.json --lighting-sequence tests/fixtures/native/model-lighting/hallows-quality-loop.json --frames 1140 --fixed-delta 0.016666667 --move-script forward@999-1000 --entity-light-trace --diagnostic final --quality low --quality-cycle 180:medium,420:high,660:low,900:high --out debug-maps/model-lighting-replay/hallows-quality-run
```

For the Demo kitchen, use `demo-quality.json` and `demo-quality-loop.json`.
Same-frame High-only controls at frames 600/1080 and Low-only controls at 120/840
must match their corresponding restored phases. The capture tool records the
real selected quality and image-bound diagnostic receipt, rather than assuming
an environment variable was honored. Resource telemetry is checked separately;
restored Low can retain an inactive CPU visibility cache.
