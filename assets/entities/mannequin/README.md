# Concrete mannequin

The maintained GLB has 2,969 vertices, 1,376 triangles, 23 joints and one
embedded opaque concrete PNG. Its unchanged bind bounds are
`[-0.210, 0, -0.1572]` to `[0.210, 1.720, 0.1572]` metres, with +Y up,
+Z forward and floor contact at the origin.

| Clip | Duration | Kind | Reference speed |
| --- | --- | --- | --- |
| `pose_stand` | hold | pose | — |
| `pose_arms_up` | hold | pose | — |
| `pose_arms_forward` | hold | pose | — |
| `walk` | 1.10 s | walk | 1.0 m/s |
| `run` | 0.68 s | run | 2.244582043 m/s |
| `run_arms_forward` | 0.68 s | run | 2.244582043 m/s |

`run_arms_forward` combines the existing run's root, pelvis and leg channels
with the actual `pose_arms_forward` upper-body hold. The legs and pelvis keep
the running gait and vertical bob; both arms stay straight forward without
swinging. All six clips loop, and `pose_stand` provides the arms-down idle.
Choose the clips explicitly with AI `move_clip` and `idle_clip`.

Re-export the derived clip without changing the maintained model:

```sh
python3 tools/entities/build_mannequin.py --add-run-arms-forward
python3 tools/entities/build_mannequin.py --check-run-arms-forward
python3 tools/entities/validate_entities.py \
  --glb assets/entities/mannequin/model/mannequin.glb \
  --workers 1 --samples-per-second 120
```

The maintenance export reuses existing animation accessors. It preserves the
entire BIN chunk, all existing clips and metadata, geometry, weights, inverse
binds, UVs, materials and embedded PNG bytes. It is idempotent; `--source`
and `--out` can export from a separate source GLB into scratch.
The check flag regenerates in disposable temporary storage, compares exact
bytes, and rejects a missing or stale derived clip without modifying the GLB.

The default legacy constructor does not reproduce the committed model's
later geometry refinements or walk/run clips. Review its output only in
scratch with `--out /tmp/mannequin-constructor.glb`.

Offline validation samples every clip at 120 Hz. The derived run stays at
least 1.28 mm above the floor, has 81 contact frames among 83 sampled frames,
and stays below 1.333× triangle-edge stretch. Native playback is checked
separately when the clip is integrated into a level.
