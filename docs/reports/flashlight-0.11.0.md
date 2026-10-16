# Flashlight close-up correction — 0.11.0 staging

Resolves [issue #23](https://github.com/csd113/Places/issues/23), starting from
`a76236ad301bcd021c036742c6981a1eafc8f53e` on `0.11.0-staging`.

## Confirmed cause and correction

The user confirmed the freestanding Office cabinet capture below as the reported
“floating shadow box.” Crouching and looking upward makes the silhouette look
like a black extension above the prop. Arcade's bathroom stall partitions, with
the room fixtures switched off and flashlight on, show the same upper-edge effect.

The source sat **18 cm below the camera in view space**. Its real shadow therefore
projected upward onto distant receivers. Native albedo/distance diagnostics locate
the confirmed box on a wall about 9–10 m away, behind the cabinet about 2–3 m away;
there is no floating box mesh or proxy caster. A separate near-wall control's
38-pixel band matches analytic source/prop/wall projection within one pixel.
Removing raster depth bias leaves that band intact. Disabling visibility only
for a diagnostic removes real cast shadows, distinguishing them from the separate
close-range soft cone edge. These controls do not establish a depth-space,
normal-transform or shadow-acne fault for the reported case.

The correction removes **only the vertical source drop** in the existing gameplay
attachment. It retains 14 cm right / 6 cm forward placement, six-metre aim
convergence and geometric source retraction. The lateral baseline retains visible
cast shadows while the above-top silhouette disappears in the confirmed views.
Both rendered lighting and mannequin observations consume this same source.

The cone, colour, intensity, range, falloff, shadow-map resolution, four PCF
comparisons, depth/normal bias, material response and contact lighting are
unchanged. Vertical beam placement and near-surface illumination intentionally
change with the corrected source. No map, runtime texture or model changed.

## Matched native evidence

Apple M2 Pro / Metal, `Bgra8UnormSrgb`, High textures / 16× filtering, Full lighting
and lightmaps, Full reflections, bloom on, FOV 60°. Still captures are unchanged
1512×838 native frames, with identical camera, simulation and fixture states.

| Case | Before | After |
| --- | --- | --- |
| Office `basement_hide_1`, camera `(13, 0.8, 12.3)`, yaw 35°, pitch +12° | [Native PNG](../images/flashlight-0.11.0/office-before.png) | [Native PNG](../images/flashlight-0.11.0/office-after.png) |
| Arcade `restroom_divider_0`, camera `(-5.3, 0.8, 1.5)`, yaw 180°, pitch +30°, bathroom lights off | [Native PNG](../images/flashlight-0.11.0/stall-before.png) | [Native PNG](../images/flashlight-0.11.0/stall-after.png) |

Independent sampling of the confirmed upper-box region finds 3,704/3,750 dark
pixels before and zero after. A separate 551-pixel lateral-shadow region remains
black in both images. Final native images match the reviewed source-height
control byte-for-byte.

Office acceptance includes farther/nearer freestanding cabinets, standing and
crouched views, oblique angles, flat/curved props and the dark Arcade stall tops.
Two genuine native 11-second / 330-frame videos at 1280×720 / 30 FPS record normal
C crouch transitions, approach/retreat, continuous look input and flashlight
OFF/ON. Each run presents all 660 source frames without misses; all nine captured
camera/simulation states match exactly. Recording deliberately paces source
frames and is excluded from FPS measurements.

Existing native acceptance passes F/held-key handling, actual Controls rebinding
to R, console/menu/focus ownership, texture and Static/Full transitions and reload;
mannequin freeze, gaze, dim-edge hold, release and wall/floor/cabinet occlusion;
held/dropped objects and posed models. A narrower physics control captures a lit,
unheld duck twice after an impulse: its position and rotation change, with
nonzero linear and angular velocities at both captures. The earlier generic
`moving-lit` image had already settled and is not the moving-at-capture proof.
The ordinary default-feature release player separately loads and captures the
confirmed Office view successfully.

All players execute the native binary directly with explicit working directory,
assets and isolated state. No app packaging, wrapper or OS security/permission
change was used. Initial runs with unavailable surfaces are excluded. One Arcade
zero-bias control missed the flashlight input after focus loss and is also
excluded; it is not evidence of self-shadowing. Accepted final captures have
actual flashlight state witnesses and no missed ready presentations.

## Performance and resources

Three interleaved runs per side use the same Office view, 120 warmup plus 600
measured frames, fixed 1/60 simulation, V-sync off, swap interval zero, no frame
limiter and actual **Immediate / 1512×838** presentation. Each has 720 ready
presentations and zero misses. This campaign exceeds display refresh pacing;
the earlier shader-stage approximately 120 FPS plateau is not used as evidence.

| Metric | Before | After |
| --- | ---: | ---: |
| Pooled average FPS | 377.54 | 381.13 |
| Individual run average FPS range | 374.17–384.39 | 376.91–389.71 |

Average is measured frames divided by their total loop wall time. The **+0.95%**
difference lies within run-to-run variation; no throughput improvement is
established. Camera/settings/packages and submitted geometry match.

Separate native controls perform eight OFF/ON toggles per executable with all
720 ready frames presented. Late counts on both sides retain 39 textures, 61
render pipelines, 23 samplers and 700 bind groups. Small transient buffer/view
counts vary; no monotonic retained-resource growth or fatal state is observed.
The existing single 1024² depth target remains 4 MiB and is reused across toggles.
Metal allocation-byte counters are unavailable, so no memory-saving claim is made.

## Verification and limits

Strict `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo build --locked --release --workspace --all-features`, ordinary release
player build, formatting, whitespace and 372 Python source checks pass (four
optional skips). No Rust tests were added: scalar offset assertions would mirror
the implementation; the actual regression requires the retained native views.
No production map was baked. Independent review covers source, exact confirmed
pixels/receivers, motion, input/AI/object witnesses and measurement receipts.

Coverage is Apple M2 Pro / Metal only, with bounded native runs. This corrects the
confirmed source-placement appearance; it does not claim to solve every possible
shadow artifact or remove physically legitimate cast shadows on distant surfaces.
Reproduction drivers, immutable executables, full native media, hashes, check
logs and independent review are retained outside `target` under
`local/flashlight-0.11.0/`.
