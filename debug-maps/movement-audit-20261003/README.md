# Places movement audit playground

36 saved maps: 31 distinct controller/performance fixtures, three sweep/contact scenes, The Pit, and the movement QA stations. Each source and compiled package is listed below. These maps are separate from production levels.

From the repository root:

```sh
python3 debug-maps/movement-audit-20261003/launch.py
python3 debug-maps/movement-audit-20261003/launch.py movement_audit_qa_reference
python3 debug-maps/movement-audit-20261003/launch.py level0_pit
```

The launcher uses the saved native arm64 Mac build, saved SDL library, saved assets, and a separate runtime directory. It opens the requested map directly with WASD, arrow-key look, Space jump, C crouch, E interact, and Escape pause. Lightmaps are off because these diagnostic packages contain the off variant. Set Lightmaps to Off again if you change that setting in this collection’s isolated settings.

`bin/`, `asset-root/`, `runtime/`, and `captures/` are locally preserved and Git-ignored; they are outside `target` and temporary directories. The before build is `bin/places-before`. Both executables use `bin/lib/libSDL3.0.dylib`, plus standard macOS frameworks. `preserved-builds.json` and `preserved-assets.json` record the saved hashes. Copies of licenses are in `licenses/`. On another checkout, rebuild the current game with `cargo build --release`; the launcher falls back to that build and the repository assets if the saved local copies are absent.

`sources/` contains authoring JSON; `packages/` contains playable compiled maps; `metadata/` and `manifest.json` contain initial eye positions and each map’s purpose. `fixture-code/` preserves the exact deterministic Rust fixtures. `references/` retains the original QA map identity and package, alongside the uniquely named copy that avoids bundled-map precedence.

Fixtures that inject velocity or a specific grounded state in Rust preserve their geometry and initial position here; those injected test conditions are documented in the saved test code. Interactive exploration uses normal movement. The 45-degree door scene uses the standard authored door/frame; the unit fixture isolates the bare leaf. Several maps deliberately start airborne or overlapping to exercise spawn recovery.

Rebuild and validate packages with `python3 debug-maps/movement-audit-20261003/build.py`. Native load/capture verification is `python3 debug-maps/movement-audit-20261003/validate_native.py`; it writes `native-validation.json` and `captures/`. Compile/probe capture and native validation require an available macOS desktop and Metal device. The final checked results are in `build-validation.json` and `native-validation.json`.

The Pit comparison uses the recorded balcony spawn and the ordinary input script:

```sh
python3 debug-maps/movement-audit-20261003/launch.py level0_pit --script "jump@0-0.06,forward@0-0.58" --seconds 1.5 --capture debug-maps/movement-audit-20261003/captures/pit-after.png
python3 debug-maps/movement-audit-20261003/launch.py level0_pit --binary debug-maps/movement-audit-20261003/bin/places-before --script "jump@0-0.06,forward@0-0.58" --seconds 1.5 --capture debug-maps/movement-audit-20261003/captures/pit-before.png
```

The low-ceiling edge reproduction is `movement_audit_aa5c8e0f5f3f`, with `--script "jump@0-0.06" --seconds 0.4`. `--yaw` overrides the suggested viewing direction. Map IDs preserve parameter variants even when their test names match.

| Map ID | Purpose | Geometry parameters |
| --- | --- | --- |
| level0_pit | Preserved level0 pit reference map | rooms 2.7,2.7,3.2,2.7,6.0,6.0,6.0,6.0,2.2,2.2,2.2,2.2,2.2,2.2,2.2,2.2,2.2,2.2,2.2,2.2,17.0,17.0,17.0,17.0,17.0 m; 43 boxes; Y 0..2.7, 0..2.7, 0..2.7, 0..2.7 |
| movement_audit_17c50b94530c | stacked storey ceiling below feet cannot teleport a jump | rooms 2.4,3 m; 0 boxes |
| movement_audit_1bf2c6791b49 | steps require ground and full head clearance | rooms 8 m; 2 boxes; Y 0..0.4, 3..3.01 |
| movement_audit_258e7ddd25a2 | a room ceiling is support from above and never a downward clamp | rooms 2.4 m; 0 boxes |
| movement_audit_2f1424a900dc | ceilings stop upward velocity preserve lateral motion and then fall at every rate | rooms 2.05 m; 0 boxes |
| movement_audit_3abd34b6f63e | landing on a room roof edge uses the whole body disc | rooms 2.4,8 m; 0 boxes |
| movement_audit_420608eb6e55 | spawn across a gabled roof uses separate bounded recovery | rooms 2.4 m; 0 boxes |
| movement_audit_49489c743dfc | thin floor and overhead slab cannot be tunnelled through | rooms 8 m; 1 boxes; Y 2..2.0005 |
| movement_audit_5488abc5b606 | ceilings stop upward velocity preserve lateral motion and then fall at every rate | rooms 1.8004999 m; 0 boxes |
| movement_audit_5933da10473f | ledge departure and fall distance are equivalent across frame rates | rooms 20 m; 0 boxes |
| movement_audit_5e49ec819009 | grounded floor region steps never embed the body in their rims | rooms 4.0 m; 0 boxes |
| movement_audit_66326a50cde0 | real solid step at maximum height is traversable | rooms 8 m; 1 boxes; Y 0..0.4 |
| movement_audit_6f20140b35d2 | a floating room floor blocks both a jump from below and side entry | rooms 8,4 m; 0 boxes |
| movement_audit_7685e3f65809 | floor two walls and ceiling corner remain nonpenetrating | rooms 8 m; 3 boxes; Y 0..5, 0..5, 2.05..2.055 |
| movement_audit_80d3a944bbaf | measure controller hot path | rooms 4.0 m; 1 boxes; Y 0..3 |
| movement_audit_845dfea385f9 | airborne side contact never steps or becomes ground and slides | rooms 8 m; 1 boxes; Y 0..4 |
| movement_audit_8b0ee9957100 | steps require ground and full head clearance | rooms 8 m; 2 boxes; Y 0..0.41, 2..2.01 |
| movement_audit_9f8610ed7167 | falling body edge cannot enter a prop when center misses top | rooms 8 m; 1 boxes; Y 0..0.9 |
| movement_audit_a0a48bfabc94 | spawn overlap recovery is clear bounded and deterministic | rooms 8 m; 1 boxes; Y 0..1 |
| movement_audit_a5d6e03e7ba5 | air spawn requires actual support instead of an xz floor | rooms 8 m; 0 boxes |
| movement_audit_a731e3c57d74 | steps require ground and full head clearance | rooms 8 m; 2 boxes; Y 0..0.41, 3..3.01 |
| movement_audit_aa5c8e0f5f3f | low ceiling covers the body disc at its edge | rooms 2.05,8 m; 0 boxes |
| movement_audit_b0f5ad049c0d | ceilings stop upward velocity preserve lateral motion and then fall at every rate | rooms 2.78 m; 0 boxes |
| movement_audit_b4d7f82904ca | side entry into a low room ceiling is blocked without a vertical teleport | rooms 2.05,8 m; 0 boxes |
| movement_audit_ba5f0e0ddf5c | measure controller hot path | rooms 4.0 m; 4001 boxes; Y 0..3, 0..3, 0..3, 0..3 |
| movement_audit_c021e6f5ced9 | thin floor and overhead slab cannot be tunnelled through | rooms 8 m; 1 boxes; Y 2..2.0005 |
| movement_audit_c53f8cd3e4ca | repeated jump cycles preserve apex and land at every frame pattern | rooms 8 m; 0 boxes |
| movement_audit_c6eed58f83a3 | steps require ground and full head clearance | rooms 8 m; 2 boxes; Y 0..0.4, 2..2.01 |
| movement_audit_corner | Independent 90 degree contact planes | rooms 5 m; 2 boxes; Y 0..4, 0..4 |
| movement_audit_d13514f38a15 | ledge departure and fall distance are equivalent across frame rates | rooms 8 m; 1 boxes; Y 0..1.5 |
| movement_audit_e3b660c958ce | ceiling impact is found when the jump apex is between clear endpoints | rooms 8 m; 1 boxes; Y 2.8..2.801 |
| movement_audit_f0498cfb8d5d | narrow passage crouch and stand clearance remain physical | rooms 8 m; 3 boxes; Y 1.1..1.15, 0..3, 0..3 |
| movement_audit_fffd85d16796 | a floating room floor blocks both a jump from below and side entry | rooms 8,4 m; 0 boxes |
| movement_audit_qa_reference | Preserved movement test reference map | rooms 8,4,1.85,8,8,8,8,8 m; 46 boxes; Y 0..3, 0..3, 0..3, 0..3 |
| movement_audit_rotated_door | 45 degree / 1 mm door leaf | rooms 5 m; 0 boxes |
| movement_audit_thin_wall | 50 m sweep / 0.1 mm wall | rooms 5 m; 1 boxes; Y 0..4 |
