# Mannequin silhouette and stride — 0.14.0

The mannequin retains its faceted low-poly identity while its crown, jaw,
shoulder outline and toes have softer silhouettes. The 612 edited vertices do
not change the 2,412-triangle topology, fitted skin layout, 37-joint rig,
materials, node/rest transforms, weights, indices or any texture bytes. Every
appearance uses the same model; the existing Black tint is retained.

The six ordinary/carry/crowbar walk and ordinary/forward-arm/crowbar run clips
now share baked leg motion. A planted foot travels backward at the clip's
reference speed, cancelling forward actor travel during straight movement.
Recovery uses a continuous-velocity curve, a lower walking swing and restrained
heel/toe roll. Pelvis height rises through walking support to avoid a fixed
crouch. The named clip interfaces and durations are retained. Walking keeps its
1 m/s reference. Running uses a 3.2 m/s reference and 30% stance fraction,
replacing 2.2446 m/s; matched stance travel is approximately 0.65 m. At the
encounter's unchanged 3.75 m/s movement speed, this changes playback from
1.67 to 1.17 times the authored rate (about 295 to 207 steps per minute).
The original local upper-body channels preserve the zombie's forward arms, carry pose
and crowbar grip. The other 19 clips remain exact, including the authored
interactions, crouch and combat timing. No runtime source is changed.

## Reproduction and scope

The input is the encounter export SHA-256
`dcaa55ad8065de0653a4821507fd19cd568386cdfb3257ec233f6ef4ff9d7e54`.
It is reproducible with the existing `add_encounter_clips.py` and accepted R2
source `a27aa5b4fda530152ba888535e06b21442bb4c05c476ba0028435a4e05291051`.
Run the new refiner with Blender Python:

```sh
blender -b --python tools/entities/refine_mannequin.py -- encounter.glb refined.glb
python3 tools/entities/check_mannequin_refinement.py encounter.glb refined.glb
```

Both authoring and checking are explicit offline tools. They do not generate
texture imagery, bake a map, run in the application, or add ordinary Rust tests.
The original source binary is retained as the output's byte-identical BIN
prefix; replacement position, normal and animation arrays are appended. The
first-person arm's selected geometry, fingers and `carry_idle` data remain
exact, so only its exporter's input identity guard changes; its shipped assets
and historical provenance are unchanged.

The November 7 mannequin shadow-inventory entry is historical for its earlier
export. The current derivative has no cavity attenuation; it retains ordinary
lighting and the existing optional realtime shadow path. Bind floor origin,
height and lateral envelope are retained; depth decreases slightly. Native
floor contact and the renderer's subtle bounds-based grounding are separate
from self-shadowing.

## Verification

The isolated source passed strict all-workspace/all-target/all-feature Clippy
and `cargo build --locked --release --workspace --all-features`. The final
player, compiler and editor all embed the accepted mannequin ledger hash.
Catalog validation passed with 17 pre-existing placement warnings. The explicit
contract checker verifies all preserved data and loop boundaries. At 121 poses
per gait, walking sole clearance peaks at 55 mm instead of 75 mm; measured
flat-midstance forward drift is under 0.02 mm. Peak knee flexion falls from approximately 62 to 54 degrees walking and
104 to 85 degrees running. The complete authored root/pelvis trajectory is
checked on the written file: the root has zero translation and hip height
is recorded across both cycles, preventing stacked root bob. The faster run
uses a smooth reach limit during recovery; its minimum hip height improves
on the source run (0.777 m versus 0.746 m) rather than
inheriting the source root's extra lowering. Peak hip height is 0.853 m
running; walking spans 0.800–0.864 m.
These are offline straight,
flat-floor observations, not a claim of runtime world-foot locking.

The accepted final GLB SHA-256 is
`4042ec146859a9184b6f5799a611d049091c2c03f58211ccef2e12479a04de8a`.
Native evidence and production-build identities are recorded with the delivery
manifest. Windows remains deferred. Transitions,
turning and uneven floors retain the existing controller and blend behavior;
this asset-only change adds no runtime IK, foot locking or motion matching.

## Native comparison

Mac/Metal review uses actual native framebuffer captures at 1280×720. Each
matched gallery recording contains 240 encoded frames (8 seconds at 30 fps),
with 244 native draw receipts including still captures. The traces confirm
active walk/run cues and actual actor movement. Side-by-side videos place the
baseline on the left and the accepted refinement on the right. An encounter
review at the unchanged gameplay speed confirms the more readable run cadence,
forward zombie arms and upright robot tool grip. These recordings demonstrate
motion and appearance; they are not throughput benchmarks.

Art iteration used an isolated catalog pointing to an explicitly new scratch
model path, through the normal GLB loader. It did not alter or disable the
canonical accepted-export guard. The production build embeds the final
canonical ledger. Its canonical-path native smoke and actual-draw capture
checker passed; this separately confirms the approved model loads without
a scratch catalog path.

Full-lighting shadow review uses a private, enclosed 6×6 m fixture with one
placed mannequin and two existing fluorescent fixtures. Grounded default
lighting records one grounding subject; raising the actor 1 m records zero.
Grounded and raised-arms opt-in views record 8,680 posed ambient vertices and
4,824 triangles with no ambient errors. Native views confirm floor contact,
the raised control and no unexplained exterior patches in the raised-arm pose.
No new cavity/body attenuation is authored.

The larger 28-character gallery passes default Full lighting, but opt-in
self-shadowing reaches the existing device workgroup budget at 67,536 posed
triangles. That crowd case is a recorded limitation, not a passed opt-in check
or a new geometry-budget increase: the model still has 2,412 triangles. Its
earlier Static toggle views prove only option acceptance and no visual
corruption; the bounded Full fixture supplies the actual posed-shadow evidence.
These checks are visual/functional acceptance, not performance measurements.

Durable recipes, traces, source identities, comparison videos and production
receipts belong under `local/verification/mannequin-refinement-20261111/`;
intermediate art candidates are labeled separately from the final accepted hash.
