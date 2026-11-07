# Tall Guest native diagnostics

This explicit developer executable is excluded from ordinary tests and never
bakes a shipping map. It uses the production importer, animator, posed collision
owner and entity controller. Build with the same features as the native player:

```text
cargo build --locked --release --all-features --bin places-tall-guest-pose-check
places-tall-guest-pose-check <new-output-directory>
places-tall-guest-pose-check --state-replay <fixture.json> <new-report.json>
places-tall-guest-pose-check --impact-replay <fixture.json> <six-contacts.json> <new-report.json>
```

Set `PLACES_ASSET_ROOT` to the canonical assets directory. Inputs must be regular
files, each at most 64 KiB; outputs must not already exist. The pose bank writes
the nine-clip legacy bank (5,609 actual skinned frames), all 130 retained node
transforms (127 skin joints), bounds and clip/blend/aim
metadata (about 330 MiB vertex data). The original 30-joint generation took about half
a second; that timing is historical. A separate
read-only BVH review took 92 seconds. It includes 45 narrow isolated direct-aim
candidates and a rejected wide positive control. Sampled crossing-free poses do
not approve production direct head overlays; production remains disabled.

State replay protects original evidence time/identity, full physical pose,
private restore, lifecycle and cosmetic voice epoch/sequence/generation fences.
It compares 120 continuation ticks and 19 semantic/negative checks (0.36 s here).
Impact replay protects causal-release bounce grouping: six recorded physical
contact descriptors in half a simulation second must give one incident, landing
and pursuit start (0.13 s here). It does not produce six solver collisions.

These narrow tools cover bugs beyond file-format checks without introducing
rendering, full map baking or audio-device costs into CI. GPU rendering, real
solver motion/contact, device PCM and performance require separately recorded
native game runs. Human listening/comprehension and arbitrary-geometry fit are
not established by these tools. See `docs/reports/tall-guest-integration.md` for
the reviewed scope and limitations.

The v0.3 export also supplies `capture_prey` and `swallow_prey`; the legacy bank
above does not exercise the new action. See `docs/reports/tall-guest-swallow.md`
for the exact profile, source-coordinate normal/topology contract and separate
live-controller/native evidence. Do not infer whole-prey acceptance from the
legacy pose bank or extend its old timing numbers to the new rig.
