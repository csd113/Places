# 0.12.0 accepted asset integration

Connor resumed the queue after playtesting Beam. This stage uses
`0.12.0-staging`; the prior pause is superseded for asset integration and its
combined staging push/CI only. No main merge, release, mannequin adoption or
subsequent readability/crosshair/safety implementation is authorized here.

## Installed inventory and custody

| Result | Models | Source and runtime location |
| --- | ---: | --- |
| Replaced | 32 Home | `assets/environments/home/props/` |
| Replaced | 5 shared | `assets/shared/props/`: cardboard box, ceiling diffuser, crate, EXIT and STOP signs |
| Replaced, material metadata adapted | 1 duck | `assets/environments/pool/props/rubber_duck.{glb,png}` |
| New | 15 Janitor / Supplies | `assets/janitor_closet/{models,textures}/`, `janitor:` catalogue IDs |
| Already adopted, exact hashes reverified | 4 | Drinks R2, snack machine, winter straight/corner railing **assemblies** |
| Deferred | 1 candidate | Corrected mannequin R2; full motion approval remains pending |
| Excluded | 1 candidate | Rejected newer Spoonerman; current in-game export retained |

The [per-model checklist](asset-integration-0.12.0-checklist.csv) records all 53
changed/new runtime paths, installed/source hashes, bounds, triangles, texture
edges and editable-source locations. `assets/completed-models.json` now pins
230 accepted exports; all pins match actual files. Catalogue ownership prevents
older builders overwriting these replacements. There are 381 runtime GLBs,
610 catalogue entries and 387 placeable presets. The extra blue folded-towel
preset shares one GLB/UV/image with the ivory towel.

Library custody:

- Home/shared runtime `libfile_041b64ffa2448191952df17fd332ef93`; complete sources
  `libfile_19a78b518c1c819196089bfd37cc8c42`.
- Duck `libfile_5d61aadb123c8191ad3950cec8b22074`.
- Janitor runtime `libfile_87decad8b3f0819189b06c6edb374bd5`; complete sources
  `libfile_4d8ff85c56648191b3cdc96b6715a167`.
- Gated R2 runtime `libfile_093514de64988191b9b45f7bdd9ffbbe`; complete sources
  `libfile_480213b105c08191afc68cef05459d66`; checksum record
  `libfile_4b21a61721708191b698cd110022687c`.

All were retrieved with the supported Library transfer flow. No cloud path was
treated as a Mac path. The complete Home checksum list verifies 251 files; duck
7; R2 runtime 26 and complete sources 286. Janitor's 15 runtime hashes and
standalone PNG equivalents match its delivery and the independent asset audit.
Original archive hashes/Library identities remain in the local custody receipts.

## Contracts preserved

All 37 Home/shared exports retain exact decoded previous bounds. Fitted PNGs,
emission roles and source pivots are retained. The switch's named
`switch → lever_pivot → lever` hierarchy, local translations, LINEAR `toggle`
samples and .35-second duration match exactly; exported node order may differ.
Existing level placements and collision dimensions are unchanged.

The requested `51e672fc…` duck checksum identifies the **raw GLB**, as the
earlier audit recorded; its ZIP is `e57c3bb9…`. The repeatable hash-guarded
`tools/props/adapt_rubber_duck.py` removes only optional
`KHR_materials_specular` metadata. It reproduces installed SHA-256
`a3e306329179900549b38168bf1e8299d24a3aea18a35723b0016dce7d8c6518`.
BIN bytes, geometry, normals, UVs, embedded image and supported core PBR remain
exact. Bounds remain .098 × .12 × .145 m; the real 128² PNG is retained at the
existing texture path. Mass .065 kg, .00065 m³ displacement and cuboid profile,
and the Pool showcase's scale/mass/profile are unchanged. No analytic bobbing
replaces physical water response.

Janitor models use the ordinary supported GLB/PNG/catalogue architecture.
`assets/janitor_closet/README.md` documents shelf contacts, source pivots and
economical item colliders. `assets/gameplay.json` holds reusable profiles:
fixed open rack/sink compounds, movable cart/bucket without pickup, and portable
supplies with pickup. Sign hull points are relative to the model bounds centre;
compound offsets are relative to the model origin. Bucket/cart supports reach
their actual floor datum. The cart catalogue depth conservatively covers its
asymmetric rear handle about the retained origin (.660001 m envelope versus
.561 m occupied depth); it is not a model scale change. The four denser janitor
assemblies and duck have explicit per-export art-budget reviews, below the
unchanged 6,000-triangle ordinary engine ceiling.

`projects/janitor-supplies/` has a static editable layout and an explicit
`janitor_physics` companion. Both show the complete coherent collection. The
static layout keeps every model visible in the existing inspect viewport;
the physics level demonstrates profiles, pickup and reset. The native editor
selected/placed the blue towel preset, saved and reopened the layout. No new
rendering, physics or interaction system was added.

Installed mannequin SHA `462d42f3…`, Spoonerman SHA `e0aa64e6…`, both game-mode
projects and Beam's black preset remain unchanged. Rebuilt shared assets are
used through their existing IDs; neither mode's props or gameplay were edited.

## Validation and native evidence

Evidence root: `local/asset-integration-0.12.0/`. Runs use preserved **direct
executables**, repository assets and isolated state; no .app package is used.
Camera/item-pose diagnostic setup is labelled. E/F, pickup/drop/throw and mode
routes pass through ordinary SDL input. Native acceptance is Mac/Metal only.

- Strict all-target/all-feature Clippy and locked all-feature release build pass.
- All 137 existing Rust regressions pass (0.27 s execution); 368 Python tests
  pass with 16 supported skips. No new tests or CI map-bake dependency added.
- Catalogue validation: 610 entries, zero errors/warnings. Prop build check and
  texture check pass; 96 existing detailed texture warnings remain.
- Independent full asset audit: 381 models, 417 embedded PNGs, 579 standalone
  PNGs, zero errors. All installed entity clip-boundary checks pass.
- Native closet: materials/UV presentation, grounding, slate-blue shared-mesh
  tint, open sink/rack/cart construction, dark/light/flashlight response,
  actual pickup/drop, floor and rack/crate contact, sign/bucket floor support,
  dormant startup and exact group reset. Accepted physics receipt has eight
  passing checks and no recovered/suspended body.
- Native Home: existing showcase living/kitchen/dining/light views inspected.
- Native Pool: float/settle, dry/wet pickup/drop/throw and dormant reset pass
  existing water assertions (10/19/12 checks including no recovery).
- Fresh Static/Full exports prepared explicitly for the closet, Home, Pool,
  Beam and Closing Time. No shipped source or historical final delivery changed.
- Beam win/reveal/occlusion/death/immediate-reset and Closing Time win/item/reset
  edge replays all pass; five receipts are indexed in `checks/mode-acceptance.json`.
  All five packages pass compiler validation. Preview manifest and both level
  sources recover byte-for-byte through `import-project`. Native switch rest,
  toggled and restored endpoints were inspected under the actual flashlight.

Original failed attempts remain separate. The old Pool launcher creates asset
symlinks rejected by the current regular-file/root checks. The same input plans
and assertions passed using the real repository asset root; security was not
weakened. Its dry throw needed a longer carry-settle interval and downward aim
after turning. A diagnostic sign pose was correctly refused while the player
overlapped its dormant starting body; moving the diagnostic camera/player clear
resolved it. These are tooling/setup findings, not concealed successful runs.

No FPS comparison is made. Captures, physics traces and concurrent explicit
bakes are not throughput evidence.

## Mannequin R2 adoption gate and plan

R2 raw GLB SHA `6627807d…` is preserved but **not installed**. Offline inspection
finds 2,412 triangles, 4,340 vertices, 37 joints, 19 clips and no geometry/UV/
normal/skin/animation errors. All 19 currently named nodes (including the 17
original body joints) retain exact rest transforms; the six original clip names
are present, with 13 additions. This does not establish motion approval.

After approval, read the delivered `docs/FUTURE_INTEGRATION.md`, preserve the
complete rig/clip set, and first fix the legacy clip-boundary checker to honor
declared `loop:false` instead of its hardcoded one-shot exclusions. Retain loop
closure and quaternion-sign-invariant checks. Pose holds and looped locomotion
must remain distinct from door/handling/sit/stand one-shots. Current loader reads
loop/kind/reference speed; delivered event/root-motion/contact tracks are not a
runtime prop/door controller or IK implementation. Do not invent either for
ordinary NPC adoption. Validate original gait/AI and Beam black/reveal/reset
contracts before any future switch, then separately scope optional authored
interaction/root/prop synchronization. Rejected V1 files are never candidates.

## Deliverables and remaining work

Canonical editable/source delivery:
`/Users/connordawkins/Downloads/Places-0.12.0-Asset-Integration-2026-10-18/`.
`Accepted/Home-Shared/` and `Accepted/Janitor-Supplies/` retain repository-shaped
asset folders, packed Blender/GIMP sources, previews and original checklists.
Duck includes the mirrored accepted runtime and immutable original source
delivery. `Deferred/Mannequin-R2/NOT-ADOPTED.txt` makes the approval gate explicit.
No rejected Spoonerman or V1 mannequin is included. Canonical source trees were
moved here instead of creating extra extracted backups.

The final delivery also retains the editable preview, direct native payload,
validated packages, selected real native images, concise receipts and launchers.
Repository assets and host SDL3 remain required; this is not an installer.

The inspect viewport's missing dynamic-body preview and the old Pool harness's
symlink-root incompatibility remain actionable tooling gaps for the parent.
Existing unrelated editor/readability/safety gaps remain in the two mode reports.
Full mannequin motion approval is the sole adoption gate; no subsequent queue
task was started here. Asset rebuild issue #24 stays open for remaining work.

Final native acceptance passes. Staging publication/CI and cleanup receipt will
be appended after the preserved delivery is checked.
