# 0.12.0 accepted asset integration

Connor resumed the queue after playtesting Beam. This stage uses
`0.12.0-staging`; the prior pause is superseded for asset integration and its
combined staging push/CI only. Connor subsequently approved the full corrected mannequin R2 animation set
(`yea nimation is approved`, Sentinel_9db6b95427708191bc91a85414bcbc68).
This follow-on adopts that exact asset. No main merge, release or subsequent
readability/crosshair/safety implementation is included.

## Installed inventory and custody

| Result | Models | Source and runtime location |
| --- | ---: | --- |
| Replaced | 32 Home | `assets/environments/home/props/` |
| Replaced | 5 shared | `assets/shared/props/`: cardboard box, ceiling diffuser, crate, EXIT and STOP signs |
| Replaced, material metadata adapted | 1 duck | `assets/environments/pool/props/rubber_duck.{glb,png}` |
| New | 15 Janitor / Supplies | `assets/janitor_closet/{models,textures}/`, `janitor:` catalogue IDs |
| Already adopted, exact hashes reverified | 4 | Drinks R2, snack machine, winter straight/corner railing **assemblies** |
| Replaced after motion approval | 1 mannequin | `assets/entities/mannequin/{model,textures}/`; both existing presets |
| Excluded | 1 candidate | Rejected newer Spoonerman; current in-game export retained |

The [per-model checklist](asset-integration-0.12.0-checklist.csv) records all 54
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
- Approved R2 runtime `libfile_093514de64988191b9b45f7bdd9ffbbe`; complete sources
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

Installed mannequin is now exact approved R2 SHA `6627807d…`. Spoonerman SHA
`e0aa64e6…`, both game-mode projects and Beam's black preset values remain unchanged. Rebuilt shared assets are
used through their existing IDs; neither mode's props or gameplay were edited.

## Validation and native evidence

Evidence root: `local/asset-integration-0.12.0/`. Runs use preserved **direct
executables**, repository assets and isolated state; no .app package is used.
Camera/item-pose diagnostic setup is labelled. E/F, pickup/drop/throw and mode
routes pass through ordinary SDL input. Native acceptance is Mac/Metal only.

- Strict all-target/all-feature Clippy and locked all-feature release build pass.
- All 139 Rust regressions pass (0.27 s execution); 369 Python tests pass with
  16 supported skips. Two tiny synthetic CPU regressions protect neutral gait
  selection and bounded pose/one-shot clocks; one tiny Python fixture protects
  declared loop semantics. No asset/GPU/map-bake dependency was added to CI.
- Catalogue validation: 610 entries, zero errors/warnings. Prop build check and
  texture check pass; 96 existing detailed texture warnings remain.
- Independent full asset audit: 381 models, 417 embedded PNGs, 580 standalone
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

## Approved mannequin R2 adoption

The installed GLB is byte-for-byte approved SHA-256
`6627807d4cddf7331cc7f96b03adb43f3686af71366bc8dc257f881b6ddf1156`:
2,412 triangles, 4,340 vertices, 37 joints and 19 clips. All 19 previous named
nodes, including 17 body joints, retain exact rest transforms. All six original
clip names remain; 20 finger descendants and 13 clips are added. The exact
1024² opaque fitted PNG is committed and matches the embedded image. Previous
accepted GLB/ledger values are preserved once in `Integration/previous-mannequin/`.
Existing catalogue envelopes, every physics profile and the black preset's
`[0.04,0.045,0.055]` tint are unchanged.

Read `FUTURE_INTEGRATION.md` before adoption. The loop-boundary checker now
honors explicit metadata `loop` booleans while preserving legacy fallbacks,
quaternion-sign-invariant closure and weight checks. Alphabetical carry clips
cannot displace ordinary neutral standing or walking. Zero-duration poses keep
a bounded zero clock; one-shots clamp once and loops wrap. Read-only animation
clock/root/socket evidence is allocated only for explicit native trace events.

`projects/mannequin-r2-review/` is an opt-in editable 27-actor preview: all 19
clips with explicit loop policies, paired door/chair/tabletop sequences,
ordinary neutral and .5/1.25 m/s walking plus reference-speed running. Door root
displacement stays baked against a fixed actor anchor. Existing hand and added
finger glow sockets exercise supported attachment, including socket precedence
over a nonzero fallback offset. No runtime IK, object grasp, door/lever
controller, metadata event dispatcher or navigation root-handoff system was
invented. Those contact/event tracks remain complete editable reference data;
this adoption does not claim native synchronized props or arbitrary crossfades.

Native R2 playback passes 89 checks: all named clips and policies, bounded pose
holds, continuing loops, clamped one-shot ends, ordinary standing/walk/run,
finite roots, single baked door displacement and actual hand/finger light
attachment. Six fresh mode replays pass: Beam win, beam/occlusion, double death
with immediate reset, upper-floor/door patrol; Closing Time win and item/reset
edges. Full native views preserve material/normal presentation and black tint.

The first gallery and Beam win attempts lost macOS window focus. Retries use
the existing SDL window-raise control; Beam's accepted win trace records nine
successful raises with actual input focus, real movement and all three gifts
returned. The accepted gallery ran through all endpoints. No .app wrapper or
security restriction was changed. The UI connector could not bind a raw binary;
its Terminal fallback was denied, so that UI route was abandoned. Existing
SDL controls completed validation. Closing's brief switch cue was present in
seven action-result snapshots but missed the first audit's capture window;
one additional early capture made the unchanged replay pass. Failed attempts
remain separately labelled.

Native R2 acceptance receipts are recorded under `r2/` and `r2-checks/` in the
evidence root. Approval of the motion set is the user's review, separate from
encoding checks and native compatibility. Rejected V1 files are excluded.

## Deliverables and remaining work

Canonical editable/source delivery:
`/Users/connordawkins/Downloads/Places-0.12.0-Asset-Integration-2026-10-18/`.
`Accepted/Home-Shared/` and `Accepted/Janitor-Supplies/` retain repository-shaped
asset folders, packed Blender/GIMP sources, previews and original checklists.
Duck includes the mirrored accepted runtime and immutable original source
delivery. `Accepted/Mannequin-R2/` retains complete motion sources and an
approval/adoption note; historical pending-review language in immutable original
source documents is superseded by that note.
No rejected Spoonerman or V1 mannequin is included. Canonical source trees were
moved here instead of creating extra extracted backups.

The final delivery also retains the editable preview, direct native payload,
validated packages, selected real native images, concise receipts and launchers.
Repository assets and host SDL3 remain required; this is not an installer.

The inspect viewport's missing dynamic-body preview and the old Pool harness's
symlink-root incompatibility remain actionable tooling gaps for the parent.
Existing unrelated editor/readability/safety gaps remain in the two mode reports.
Mannequin motion approval was received and acted on; no subsequent queue
task was started here. Asset rebuild issue #24 stays open for remaining work.

Phase 1 was committed/pushed as `17bd17854f3e59d9933a1ab76d3a8ce88540b17c`;
[CI 39104545890](https://github.com/csd113/Places/actions/runs/39104545890)
passed on that exact head, including the two completed games. Follow-on R2
publication and final native/cleanup receipts are recorded with the handoff.
