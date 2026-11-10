# Upright crowbars and five-companion encounter — 0.14.0

The first-person crowbar now rests nearly upright near the lower-right edge.
The closed robot hand rolls with its crowbar, keeping the shaft upright without
rotating it through the fingers. Zombies use the mannequin's existing
`run_arms_forward` and `pose_arms_forward` clips.

`encounter.companion_count` accepts 1–5 and defaults to one. The shipped
`projects/encounter-lab/` is unchanged. Each robot has an independent identity,
health, targeting and hand attachment. Zombies choose the nearest living robot
or player; robots prioritize their own pursuers. Toggle and respawn retire every
member and attachment. The shared ragdoll limit is 26, enough for twenty zombies,
five robots and the player. Health remains map opt-in.

Group packages require `encounter-groups-v1`. The preceding compiler correctly
rejects these packages as unsupported. The new source field and its placement
contract are documented in `MAP_AUTHORING_GUIDE.md`.

## Validation

Final checks passed:

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo build --locked --release --workspace --all-features`
- `cargo test --workspace`: 589 passed, zero failed or ignored.
- Formatting, diff whitespace checks, package builds and current-package checks.
- Mannequin skinning/contact sweep and clip-boundary checks.

One new small in-memory regression covers five distinct spawn positions,
individual zombie target identity, retargeting after a robot dies, tool slots,
complete group cleanup and count bounds. Existing single-companion priority and
repeated-toggle tests remain. The existing ragdoll-budget test now exercises all
26 slots; no full map is baked by Rust tests.

The asset comparison preserves all 19 original animations, geometry, rig, UVs,
materials, embedded images, existing accessors and 6,903,536 original binary
bytes. Only the previously appended crowbar clips change. No texture imagery
was generated. The accepted mannequin hash is
`fe72580b3dd2cd5ec6459a316677cb1488f113a1ef20b97205d92a12c34a3df9`.

Native Mac/Metal captures inspect the robot grip, arms-forward zombie movement,
first-person idle/swing and looking down at 45°, 60° and 110° FOV. The initial
upright view exposed the forearm end; the final view turns the forearm toward the
eye and keeps that end below frame. Holstering leaves the body hidden. The
original lab's native checkpoint contains exactly one companion.

An explicit native lifecycle run uses five robots, three full twenty-zombie
cohorts, three E respawns and 32 rapid group-toggle requests in one process.
Diagnostic lethal health actions create all 26 simultaneous ragdolls in each
round. Every respawn and the final toggle checkpoint restore the same entity,
component, AI, renderer and solver ownership as the warmed baseline: one player
body, seven colliders, no joints, no ragdolls and no bone-pose caches.

Cleared-state RSS was 352.69, 354.20 and 332.80 MiB. These short samples show no
sustained rise, not proof against every leak. Across the three corpse checkpoints,
maximum joint-anchor error was 8.40 mm and maximum contact penetration was
3.88 mm. These are sampled values, not continuous maxima. Forced deaths in this
run test capacity/cleanup; the separate video uses ordinary combat.

## Evidence and recording

Evidence is outside the build directory, under the ignored local directory
`local/verification/upright-group-20261110/`. It includes source fixtures, packages,
input recipes, native traces, screenshots, process receipts, asset reports and
the final build/test logs. The original single-companion project is untouched.

The video map copies the lab, selects five robots, and centers their starting
point at `(16, 21)` for framing. Both original corner zones still spawn ten
zombies independently, one per second. Health, damage, speed and regeneration
settings are unchanged. Its overview camera is a developer recording view, not
a spectating feature. The recipe activates the authored companion/start buttons;
it injects no damage, healing, health-rule changes or ragdoll commands.

Recording streams actual native framebuffer pixels to H.264 at 1920 × 1080,
30 fps, without audio. The existing diagnostic clock uses 1/60-second simulation
steps and records every second frame, so playback follows simulation time even
when readback/encoding makes capture slower than real time. These are correctness
and presentation checks, not FPS comparisons.

The delivered `five-robots-vs-twenty-zombies.mp4` retains the continuous opening
23 seconds of `video-close/`'s 37.7-second recording, trimming only idle aftermath
without re-encoding. All twenty zombies are dead by the checkpoint at encoded
18 seconds, and all five robots survive this run. The earlier wider recording
also remains in `video/`; four robots survived that separate encounter. No result
was imposed. Encoder frame counts match the recorded native frames, and the
delivered H.264 file decodes successfully. Opening, combat and ending frames were
visually inspected.

Windows validation remains deferred. Group size is source configuration; no new
group-size editor control, multiplayer, waves or spectating was added. Existing
crowd avoidance remains basic, and robots can converge closely on one target.
