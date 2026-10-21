# Places 0.12.0 prerelease

October 18, 2026. This release brings the native editor, editable project
distribution and recovery, the nine-map Quiet Collection, and the separate
Closing Time, It Only Exists in the Beam and Relay Annex campaigns together.
It is a prerelease with known defects, not a bug-free or cross-platform release.

## Release contents and version

- The native editor supports protected save/reopen, geometry and gameplay
  authoring, bounded safety review, project-local sandboxed Luau, structured
  compiler jobs, cancellation, complete project export/recovery and isolated play.
  See [editor controls](../EDITOR.md) and the linked stage reports there.
- Quiet Collection connects eight environments through a hub, retaining editable
  sources, embedded menu artwork, Home's handoff quest and Pool's annex. The eight
  superseded standalone environment distributions are retired. See the
  [collection handoff](release-0.12.0.md#environment-collection-0120-retirement).
- The separate campaigns remain independent. Shadow-only entities, layered
  navigation, readable editor text and the five-dot crosshair are included.
- Installed resources resolve from the executable; writable data/settings/cache
  use the documented user paths. macOS exports include relocated SDL3, licenses,
  sibling editor/compiler binaries, ad-hoc signatures and minimum-OS metadata.
  See [installation](../INSTALLATION.md) and the
  [distribution acceptance report](release-0.12.0.md#macos-distribution-0120).

Cargo metadata and the root lockfile package are **0.12.0**. The player menu,
loading UI and diagnostics use `CARGO_PKG_VERSION`; the compiler reports
`places-compile 0.12.0`. Packaging derives both bundle version fields from Cargo.
The release tag is **`v0.12.0`**, as explicitly requested. Earlier releases used
annotated numeric tags; this retains annotated tags and GitHub prerelease status.

The downloadable binary payload is **arm64, macOS 27.0 or newer**, validated on
macOS 27.0.1 / Apple M2 Pro / Metal. The bundled SDL3 sets the minimum OS.
It is ad-hoc signed, without Developer ID signing, notarization or a Gatekeeper
acceptance claim. Windows and Linux binaries are not published or represented
as tested. Tracked source assets still require Git LFS in a source checkout.

## Package compatibility

The collection package is 449,857,977 bytes, SHA-256
`9e78fbe3f6a2cf00f19f410da76271f1acb6c6b14d91b2e5ba0dd8682e532ee8`.
The three accepted current-catalog compatibility exports are included with their
original bytes and provenance:

| Campaign | Bytes | SHA-256 |
| --- | ---: | --- |
| Closing Time | 34,116,781 | `0712c4b188a43c500d498dc61549d87e34b23a6e069fd8da026b8de778eac0a4` |
| It Only Exists in the Beam | 6,314,015 | `93ecf5e64996111a0480fbf5838de33164175551bb94f48c7628cc220f14027c` |
| Relay Annex | 24,434,167 | `50912ac052ea79a1f7a0f25f2115d96fe40856632ca9424077e6c2d46cad2c60` |

Runtime compatibility, catalog identity, bounded package validation and dependency
integrity are required. No validation bypass, patched provenance or enlarged
input limit is used. The final export must discover all **14 members**: nine
collection maps, Closing Time, Beam and three Relay Annex maps. Explore groups
the collection into its entry row; member count differs from menu row count.

Compiler currentness is stricter than runtime compatibility: executable hashes
are part of build fingerprints. These retained packages were built by earlier
accepted compilers and are not claimed current under the version-bumped compiler.
No shipped map was rebaked for this wrap-up. Exact source recovery and prior
end-to-end acceptance remain documented in the
[campaign compatibility report](release-0.12.0.md#environment-0120-compatible-campaigns) and
[native collection report](release-0.12.0.md#environment-collection-native-qa).

## Final validation

The final local gate passed:

- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo`.
- `cargo test --workspace --all-features`: 192 tests, zero failures; no shipped-map bake.
- `cargo build --locked --release --workspace --all-features`: production executables built in 3m 15s.
- `PLACES_SKIP_SMOKE=1 python3 -m unittest tests.test_package`: 50 focused package/source contracts.
- `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'`: 370 tests, three expected skips, zero failures (98.269s). The first run caught the version bump against the unfinished changelog heading; finalizing the dated heading fixed it.
- `python3 tools/assets/validate.py`: 620 assets, zero warnings.
- `python3 tools/verify-luau-release.py --project projects/closing-time --project projects/beam --project projects/relay-annex --project projects/environments --report local/release-0.12.0/luau-release.json`: 11 bounded release refusals and four finished-project checks, no map bake.
- Real `tools/package.sh --with-editor` exports, with and without maps; all four packages validated against the exported catalog and every declared member discovered.
- Direct native flat and bundle menus, visible `v0.12.0`, bundle version/minimum-OS metadata, strict app signature and relocated SDL3 loading without Homebrew runtime libraries.
- Four independent mode launchers and native editor edit → save → process exit → reopen → build → imported sibling-player play → return to editing. The saved chair transform survives reopen; the compiler builds that edited project.
- `tools/verify-installation.py`: all 27 checks passed, including map-free startup, imported play/restart, arbitrary working directory, non-ASCII paths, read-only installation, storage failures, corrupt/conflicting imports and settings/save-file custody.
- `tools/authoring/environment_acceptance.py run` plus `environment_audit.py`: 75 assertions passed on the final exported player and unchanged bundled collection, including ordinary hub → Home → hub traversal and no immediate bounce.
- Final ZIP extraction into a separate non-ASCII path: all file hashes matched,
  strict bundle signature/version/minimum-OS checks passed, all 14 members were
  rediscovered, and all four launchers entered gameplay after ordinary input.
- `git diff --check`.

Raw logs, native captures, package receipts and preserved deliverables live
outside `target/` in `local/release-0.12.0/`. Hosted CI for the final commit is
linked in the GitHub prerelease. Ordinary CI and Rust tests do not bake shipped
maps. No new regression tests or dependencies were introduced in this wrap-up.

Native captures were visually inspected. The default menu/campaign checks used
3024×1676 physical pixels and Fifo presentation; the hub route used a 960×540
logical window. These are correctness checks, with no FPS improvement claim.
Campaign launch checks supplement, rather than repeat, the prior full campaign
acceptance. The known defects below remain unresolved.

The published `Places-0.12.0-macos-arm64.zip` is **687,722,141 bytes**, SHA-256
`cb29aacf74a6b24e1272548f424c0f88bcd71b1ea065aaae4e4f9f3769b1851c`.
It contains `Places.app` with the editor/compiler and all four packages, five
separate `.command` launchers and a launch/limitations README. Seven source
release executables, both export layouts, map copies and the final archive are
preserved outside `target/`. The archive's local extraction is validated;
download quarantine/Gatekeeper acceptance is not claimed.

## Launching the binary distribution

After extracting the macOS archive, keep the launchers beside `Places.app`:

```sh
./Quiet-Collection.command
./Closing-Time.command
./It-Only-Exists-in-the-Beam.command
./Relay-Annex.command
./Editor.command "/absolute/path/to/project"
./Places.app/Contents/MacOS/places
./Places.app/Contents/MacOS/places --paths
./Places.app/Contents/MacOS/places --import "/absolute/path/example.placesmap"
```

The app opens its ordinary menu. Launchers select bundled campaign entry IDs
through the existing `PLACES_LEVEL` setting and use the same installed player.
They do not depend on a source checkout, shell working directory or target tree.
Settings, imported packages and editor projects persist. `saves/` reserves a data
location; gameplay pose, quest state and moved objects remain session-local.
There is no gameplay save/load format or command.

## Known defects and deferred work

These user-reported issues remain unresolved in this prerelease. The release
smokes do not establish their absence or fixes; exact causes remain unconfirmed.

- [Engine #27](https://github.com/csd113/Places/issues/27): severe slowdowns in
  Home when handling couch cushions or jumping onto books on the bed, and near
  Pool's supply/janitor closet; frozen props may fail to activate on contact and
  trap the player. No new FPS improvement is claimed.
- [Engine #27](https://github.com/csd113/Places/issues/27): Relay Annex's second
  part has reported stair-jumping floor fall-through; sloped ramps and joins also
  require investigation. A successful launch does not validate every traversal.
- [Game #28](https://github.com/csd113/Places/issues/28): raw “release blocked”
  feedback can appear in the player HUD; Relay Annex NPCs reportedly mimic the
  player's walking animation. Neither is represented as fixed.
- [Game #28](https://github.com/csd113/Places/issues/28): dropping the required
  package into Relay Annex's first-section well can make it inaccessible and
  soft-lock progression. No reliable quest-item recovery is claimed; a broad
  restart is not a demonstrated repair of the underlying defect.
- [Editor #29](https://github.com/csd113/Places/issues/29): extreme z-fighting on
  Relay Annex's **second-room ceiling**, including the unresolved authoring/export
  validation gap. This does not identify a second map or floor as its cause.
- [UI #5](https://github.com/csd113/Places/issues/5): long error messages have
  reportedly run off-screen; wrapping, viewport bounds and reachable controls
  remain unfinished. This report has not been independently reproduced.
- Earlier #27/#29 samples also retain slow Arcade/fog-overlap views, perceptible
  editor preview refresh and a map-switch regression. These dated observations
  are not a fresh universal baseline or a demonstrated memory leak; no new
  performance fix is claimed here.
- Sound polish and the tiny always-packaged live main-menu background remain
  unfinished in #28, coordinated with #11/#13/#21. The current collection can
  provide menu scenery; an empty installation has no dedicated live scene.
- #27–#30 retain the wider performance, resource-budget and package-size audit.
  Editor axis-label colour matching and direct viewport dragging remain in #29.
  Custom runtime asset importers and broader editor safety/repair coverage are
  incomplete. Existing safety diagnostics are bounded, not exhaustive proof.

No 0.13.0 implementation is part of this release wrap-up. These defects do not
prevent basic installation, menu startup, supported package discovery or the
validated editor roundtrip, but several can impair gameplay or progression.

## Issue accounting and custody

#18 and #21 remain open: the delivered editor/macOS path is substantial, but
their full requested scope includes outstanding native Windows/Linux acceptance
and broader editor/safety requirements. #17 remains open for its platform/final
release gates. The 0.13.0 backlog and partial/deferred issues are not closed by
this release; completed 0.11.0 sound-engine work in #12 remains closed.

The initial staging checkout was clean at `13a3b69b92f1b7949687e5871d051fdcf8eee725`.
The collection retirement commit `877c7bef5f3e232fcc07f9fe97e180460a292de4`
and all preceding staging history are preserved. Release finalization changes
only Cargo metadata/lockfile, changelog, README, installation guidance and this
report. No accepted source map, asset or historical package is overwritten.

## Consolidated feature evidence

The release acceptance above is authoritative for 0.12.0. The dated feature
records below retain distinct implementation details, measurements, failed probes,
evidence paths and limitations. Their test counts, package fingerprints and runtime
observations describe their own checkpoints. Earlier scope restrictions, pauses
and proposed next steps are historical decisions, superseded by later completed
sections and the release acceptance; they are not instructions for current work.
Current contracts live in the authoring, asset, editor, installation and API guides.

- [0.12.0 accepted asset integration](#asset-integration-0120)
- [Relay Annex editor follow-up](#editor-0120-annex-followup)
- [It Only Exists in the Beam — three native authoring passes](#editor-0120-beam)
- [Closing Time — native editor authoring and playable mode](#editor-0120-closing-time)
- [Closing Time / Beam implementation and safety pass](#editor-0120-gaps-safety)
- [Layered navigation and native editor inspection](#editor-0120-navigation)
- [After navigation: authored modes, assets and safety review](#editor-0120-next-handoff)
- [Relay Annex — feature-test campaign, 0.12.0](#editor-0120-relay-annex)
- [Shadow-only entities: 0.12.0](#editor-0120-shadow-only)
- [Stage 7 handoff: complete project export/import](#editor-0120-stage7-handoff)
- [Basic editor: 0.12.0 task3](#editor-0120-task3)
- [Geometry and safety tools: 0.12.0 task4](#editor-0120-task4)
- [Stage 5: gameplay and environment authoring](#editor-0120-task5)
- [Stage 6: bounded Luau and native editor integration](#editor-0120-task6)
- [Stage 7: complete project export, recovery and distribution round trips](#editor-0120-task7)
- [Editor legibility and five-dot crosshair — 0.12.0](#editor-legibility-crosshair-0120)
- [Accepted campaign compatibility copies](#environment-0120-compatible-campaigns)
- [Environment collection: legacy import audit](#environment-0120-import-audit)
- [Quiet Collection distribution handoff — 0.12.0 staging](#environment-collection-0120-retirement)
- [Places / The Quiet Collection — 0.12.0 staging](#environment-collection-0120)
- [Environment collection native QA](#environment-collection-native-qa)
- [Office collection migration](#environment-collection-office)
- [Essential macOS distribution and first run — October 18, 2026](#macos-distribution-0120)

<a id="asset-integration-0120"></a>
## 0.12.0 accepted asset integration

Connor resumed the queue after playtesting Beam. This stage uses
`0.12.0-staging`; the prior pause is superseded for asset integration and its
combined staging push/CI only. Connor subsequently approved the full corrected mannequin R2 animation set
(`yea nimation is approved`, Sentinel_9db6b95427708191bc91a85414bcbc68).
This follow-on adopts that exact asset. No main merge, release or subsequent
readability/crosshair/safety implementation is included.

<a id="asset-integration-0120-installed-inventory-and-custody"></a>
### Installed inventory and custody

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

<a id="asset-integration-0120-contracts-preserved"></a>
### Contracts preserved

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

<a id="asset-integration-0120-validation-and-native-evidence"></a>
### Validation and native evidence

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

<a id="asset-integration-0120-approved-mannequin-r2-adoption"></a>
### Approved mannequin R2 adoption

The installed GLB is byte-for-byte approved SHA-256
`6627807d4cddf7331cc7f96b03adb43f3686af71366bc8dc257f881b6ddf1156`:
2,412 triangles, 4,340 vertices, 37 joints and 19 clips. All 17 original body joints retain exact names and rest transforms. The rig
wrapper remains unchanged. The identity render-mesh node is renamed from
`mannequin` to `mannequin_skin`; existing maps do not target that old mesh node
as a socket. Its rest placement remains identical. All six original
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
edges. High and Low native flashlight/ambient views preserve material/normal
presentation and black tint. The native editor selected R2 through its visible
asset picker, placed it at metre scale, saved and reopened the placement with
all routes retained; six editor persistence checks pass. All six delivered
packages validate, and the R2 project sources recover byte-for-byte. The exact
final direct binaries are preserved outside `target`; after confirming no active
target users, `cargo clean` removed 5.5 GiB. A post-clean direct import/render
smoke passes all 27 review actors without recreating target.

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

<a id="asset-integration-0120-deliverables-and-remaining-work"></a>
### Deliverables and remaining work

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
passed on that exact head, including the two completed games. Approved R2 was
committed/pushed as `13bc4a0d959ce25afe6322f2fa2dfb2c066ec941`;
[CI 39107602119](https://github.com/csd113/Places/actions/runs/39107602119)
also passed on that exact head, including production build, declared-clip checks,
asset generators and Python contracts. Final native/editor/material, source-path,
post-clean and preserved-payload receipts are indexed in the delivery handoff.
The final documentation receipt adds no code or assets and skips a redundant CI
run. No main merge or release was performed.

<a id="editor-0120-annex-followup"></a>
## Relay Annex editor follow-up

Work on `0.12.0-staging` from `888e6118`. Accepted campaign sources are inputs;
verification works on an isolated project copy under `local/editor-annex-0.12.0`.
No environment hub, distribution rollout, audio stage, merge or release is included.

<a id="editor-0120-annex-followup-implementation-and-verification-checklist"></a>
### Implementation and verification checklist

| Gap | Implementation target | Verification target | Status |
| --- | --- | --- | --- |
| R1 | Complete ramp creation draft before admission | Invalid initial camera target; valid complete pose; cancel/undo | Implemented; native verification below |
| R2 | Staircase and guardrail forms; existing room landing tools | Create/edit, physical connection, save/reopen | Implemented; native verification below |
| R3 | Selected-floor local overrides with explicit ownership | Stacked floors retain independent materials/heights | Implemented; native verification below |
| R4 | Height-aware legacy route support and authoring | Upper floor move/blocked edge; route preview | Implemented; native verification below |
| R5 | Physical item contact and release diagnostics | Dormant contact and blocked held movement | Implemented; native verification below |
| R6 | Water forms and vertical slice | Basin and exit ramp coverage; save/reopen/play | Implemented; native verification below |
| R7 | Outdoor ceiling, sky, weather, directional source forms | Intake environment preview and roundtrip | Implemented; native verification below |
| R8 | Item centre/sleep/receipt debugging | Actual live receiver membership/state | Implemented; native verification below |
| R9 | Preserve selected-map cache manifest check | Tower graphics cache hit and circuits | Inherited fix retained; native quality replay below |
| R10 | Structural-plane/shaft diagnostic classification | Tower candidates remain reviewable; no blanket ignores | Implemented; native verification below |
| R11 | Bounded explicit per-map display metadata | Friendly inactive titles without loading world semantics | Implemented; native verification below |
| R12 | Guidance wrapping and authoring preview | Long prose, narrow windows, keyboard focus | Implemented; native verification below |

Operation measurements cover open/switch, scene selection/search, transform,
inspector edits, history, safety scan/fix, save/reopen and build/export progress.
CPU document timings and native handler/preview timings are distinct. No paced
native capture is used as uncapped throughput evidence.

<a id="editor-0120-annex-followup-current-implementation"></a>
### Current implementation

The grouped Space and environment sheet edits complete existing engine records.
Ramp creation enters a draft before admission. Staircase/rail forms work alongside
the existing owned room/landing tools. Selected-floor overrides split only the
chosen room, retain the ceiling plane, preserve unknown source fields and add
short boundary faces for a height change. Existing ambiguous global patches,
regions and intersecting slopes are refused explicitly; their legacy semantics
remain intact. Explicit GLOBAL patch/region forms expose that effect and allow
removal before creating the local override. A native probe refused the ambiguous
override, deleted the patch through its form, applied the local section and
restored all original geometry with undo. Every Apply uses the same validated
undo/source path.

Legacy routes sample support near the actor's current feet in both admission and
runtime movement. They cannot transfer silently to a lower storey. Their paged
forms retain ordered steps and show a sampled path preview. AI and route ownership
remain exclusive; AI-only beam response must be removed explicitly before choosing
no AI. The native refusal attempts are retained, followed by a successful UI-authored
9.6 m route and actual 5.00→5.48→5.29 m movement at that height.

Safety measures real dormant rigid-shape penetration against item-world scenery,
including explicit fixed compounds. It excludes resting contacts within 2 mm and
offers no automatic item relocation. A 5 cm native placement probe reports exactly
0.0500 m penetration, then undo restores it. Play warns after a blocked release or
sustained held-motion refusal. The opt-in `/trigger show` overlay draws actual item
centres/states and receiver bands under the existing filter/receipt policy. Green
means eligible occupancy, separately from script conditions and completion.

Water previews include X/Z elevation slices through actual room/ramp/stair support.
Exterior forms expose open ceilings, installed sky references, snow/weather and
directional illuminators. Sky/weather/light previews temporarily use authored
lighting; cancel restores Inspect. Native screenshot review caught and corrected a
last-row/navigation overlap. Retina/antialiased text, existing icons and crosshair
are retained; no visual asset or catalog file changed.

201 measured opposite floor/ceiling pairs in Tower are now explicit
`shared-storey-plane` notes. Fourteen perimeter and three single-height flood
candidates remain visible for manual connected-shaft review. Other reversed/model
candidates also remain; no blanket ignore or topology repair was introduced.
Placement-ignore metadata and the existing automatic-correction protection gate
remain unchanged.

Per-map `display_name` is bounded explicit project metadata. Inactive menu titles
read the project index and retain ID fallback, without reading inactive semantics.
The existing corrupt-inactive-payload regression now also checks this title.
Guidance wraps words/long tokens into six-row pages under the unchanged six-line,
72-character scripting limit. Page Up/Page Down browse; Return/Escape dismiss.
Native Beam exposed the first pagination version consuming Return; the corrected
key policy has a focused regression and the failed native attempt is retained.

<a id="editor-0120-annex-followup-performance-evidence"></a>
### Performance evidence

`editor-profile` is an explicit operation probe, outside automatic tests and CI
map baking. On the unchanged three-map source, open averages 3.24 ms before, 3.27 ms in the first after sample and 2.01 ms
in the final warm repeat. Transform+undo is 2.69→2.71 ms and redo+undo
2.60→3.00 ms in the final repeat. Filesystem/CPU timing variance limits these samples. Cached object indexing removes
per-frame enumeration/allocation, and dirty-state comparison moves to accepted
edits/history changes. The CPU dirty check falls from about 18.7 microseconds to
a cached Boolean read. These small measurements do not imply a game FPS gain.

Matched native recipes perform three pose/undo/redo cycles, inspector editing,
15 map switches, safety and save. Selection handler averages 0.163→0.098 ms;
search 0.117→0.073 ms. Preview refresh remains about 200–250 ms. Tower switching
is 218→229 ms and Intake 219→251 ms with current-scene model retention; no overall
latency speedup is claimed. The single coalesced preview worker keeps one active
and one latest pending request. Optional texture retention is capped and model
retention follows current scene dependencies. Peak RSS is 430.8→430.5 MiB in these
repeated flows; this is not a meaningful measured memory reduction.

The edited three-map native export takes 90.63 s after cancellation. A subsequent
warm final Build+Play/return cycle takes 9.16 s including 6.5 s of actual player actions.
The UI remains responsive, reports progress, imports the complete project and
reaps the player before returning to a focused editor. Native latency runs use
ordinary paced editor presentation and are distinct from uncapped FPS evidence.

Final native preview wait averages (before → after, ms) are open 402→410,
transform preview 212→212, transform apply 196→193, inspector 206→203,
undo 202→200, redo 201→199, Discharge switch 181→200, and Safety 150→167.
Save handler time is 3.85→3.75 ms. A matched deliberate-duplicate correction
probe has mean preview waits 218.0→218.6 ms and apply 172.6→173.6 ms;
preview/cancel/apply/undo preserve exact source. These are operation samples,
not statistical latency guarantees. Final compiler identity forced a cold
Build+Play rebuild (93.58 s including play); the subsequent warm return is 9.16 s.

Final unrecorded High/Full static scenes average **204.4 FPS Intake, 224.2 Tower,
202.4 Discharge**, using 1,200 warmup and 1,200 measured frames per scene. Logs
confirm actual Immediate presentation after startup Fifo, 1280×720 physical
drawable, V-sync and all frame caps off. Each mean exceeds the current 120 Hz
display; 80–83% of loop intervals are shorter than one refresh. Earlier samples
vary substantially, so this is no game throughput improvement claim.


<a id="editor-0120-annex-followup-verification-and-delivery"></a>
### Verification and delivery

Final receipts, source custody, per-gap assertions, Library identities, exact-head
CI, binary hashes and post-clean smoke are recorded in
`local/editor-annex-0.12.0/handoff.json`. All work stays on `0.12.0-staging`.
No accepted campaign source, asset, menu/distribution rollout, audio stage,
environment hub, main merge, release or branch deletion is included.

Native authoring verification uses visible controls in an isolated copy, with
focused-window document checkpoints and framebuffer captures. It is scripted
input, not a claim of manual mouse work. Complete source recovery preserves all
ten declared files exactly (manifest, three levels, three Luau scripts and three
thumbnails). The exercise retains a UI-authored upper-floor legacy route and a
water-opacity edit; accepted campaign sources and assets remain unchanged.

The final 960×544 logical-window exercise covers all twelve grouped controls,
invalid numeric input, keyboard focus, global/local ownership, preview/cancel,
apply/undo and the correction/ignore cycle. Ignoring the exact duplicate wall
removes its fix control; clearing that ignore restores correction eligibility.
Repeated corrections and undo return the original source. Build cancellation
retains the previous published package byte-for-byte. The first duplicate probe
used different scalar representations and correctly offered no exact-duplicate
fix; failed automation recipes are retained.

The three added owning-component CPU regressions protect local-floor ownership
and source roundtrips, upper-storey route continuity without falling to lower
support, and complete guidance wrapping with established Return dismissal.
Existing package and stacked-physics tests extend title and contact assertions.
They use small deterministic fixtures, take no GPU/assets or map bake, and the
full 170-test Rust suite takes about 0.35 s. Strict debug and release Clippy,
locked all-feature release build, formatting and patch whitespace pass. All 369
Python tooling tests pass with three supported skips; 610 assets validate with
zero warnings. Explicit adversarial package checks pass 49 cases and Luau release
checks retain eleven bounded refusals.

Remaining boundaries are deliberate: local-floor conversion refuses ambiguous
same-plane carriers, intersecting slopes and gabled ceilings; advanced Luau
logic and separate navigation AI remain available. Contact diagnostics concern
the authored rigid pose, not every animation/door/held trajectory. Connected
shaft candidates require contextual review, and the checker makes no exhaustive
closure proof. Validation here is native macOS/Metal; broader cross-platform
editor issue #18, menu issue #6 and general detail-panel issue #5 remain open.
No matching narrow issue was fully completed beyond those broader scopes.

<a id="editor-0120-beam"></a>
## It Only Exists in the Beam — three native authoring passes

Implemented on `0.12.0-staging`, preserving Closing Time's local handoff
`71ac4f88b43d8ab3fe5aa2604c3e8a6137f49978`. Local commits and validation only:
no push, GitHub CI, main merge or release. Closing Time deliverables are unchanged.
Native acceptance is **Mac only**, on the Apple M2 Pro/Metal backend.

<a id="editor-0120-beam-paused-after-delivery"></a>
### Delivery checkpoint

The temporary pause for playtesting ended before the later accepted integration.
The historical direct launcher remains `./local/verification/0.12.0-beam/try-player.sh`,
using `payload-reset-verified`, matching repository assets, Full lighting and
isolated manual state. It remains runnable after target cleanup; it is not an installer.

<a id="editor-0120-beam-playable-delivery"></a>
### Playable delivery

Editable project: `projects/beam/`. Final complete-project distribution:
`local/verification/0.12.0-beam/It-Only-Exists-in-the-Beam.placesmap`.
The verification folder preserves immutable `payload-final/` direct player,
editor and compiler, plus the final `payload-reset-verified/` rebuild with
explicit pose diagnostics, launch scripts, source snapshots, real input recipes,
framebuffer captures, post-mixer audio, original screen recordings and receipts.
Repository assets and the host SDL3 runtime are required; this is not an installer.
Use Full lighting for the intended atmosphere. Static is also packaged.

Recover three physical gifts from a deserted furniture store: red from the lower
office aisle, stars from the lower lounge, green from the actual upstairs office.
Carry and set them onto the broad counter by the warm exit, then use the door.
The ramp, tall enclosed shaft, upper room and openable door are physical geometry.
The installed mannequin has a black albedo preset; it uses the existing rig,
animations and real fitted PNG unchanged. No pending Home/shared/duck/closet or
corrected cloud mannequin delivery, or rejected Spoonerman, was integrated.

F reveals and holds the creature using the renderer's actual flashlight
illumination, including live occlusion. Darkness hides it immediately; an 850 ms
release grace precedes movement. It walks at 0.9 m/s through the shared planner,
can open the upper door, remembers a seen position for six seconds, and patrols
both floors. No teleport action is used. A defeat requires real collision-aware
sight, less than 1.05 m planar distance, less than 0.6 m height difference, one
continuous second outside the beam, and leaving the entrance refuge. Eight
initial seconds permit orientation. The refuge is explained in the opening and
F1 hints. These deliberately forgiving rules make carrying and looking away
tense without unavoidable random kills.

The short opening is dismissible. F1 gives optional current-progress guidance,
including lost-gift reset instructions. There is no persistent tutorial HUD.
Explicit win/death panels freeze gameplay. Enter resets player, creature, goals,
closures, gifts/held state, doors, lights, audio and flashlight; Esc returns to
the menu. The EXIT sign also performs an ordinary in-world restart when unheld.

<a id="editor-0120-beam-three-substantive-passes"></a>
### Three substantive passes

Every pass was actually played. Original failures remain in the evidence folder;
an attempted capture named `win` does not establish success. `acceptance.json`
records the actual outcome. These are new scenes/scripts and played refinements,
not three renamed copies of the same result.

| Pass | Creation/refinement and observed corrections | Played evidence |
| --- | --- | --- |
| 1: complete loop | Native floor/wall/furniture/upper floor/ramp/door/fixture/trigger/physics authoring, full Luau progression, black mannequin reveal, explicit outcomes/reset. Real attempts exposed out-of-reach pickups, blocked upstairs approach, turn-before-carry-settles, a too-small return table and stale AI reset transforms. Moved the shelf, widened the receiving surface, raised the return band, corrected authored routes and fixed reset synchronization. | `Beam-pass1e.placesmap`, `passes/pass1-source/`; `pass1-counter-win/acceptance.json` passes the complete controls route. `pass1-death-corrected/acceptance.json` passes two deaths/respawns with the corrected runtime. Earlier failed receipts are retained. |
| 2: atmosphere and fairness | Warm entrance landmark, cold search aisles, oak/off-white upper office and oriented counter. Increased release grace and contact dwell; added a quiet existing spatial impact cue for nearby dark movement. Fixed modal audio/input/flashlight lifecycle. Full bake confirmed useful darkness and clear flashlight interaction. | `Beam-pass2.placesmap`, `passes/pass2-source/`; `pass2-win/acceptance.json` and `pass2-death/acceptance.json` pass win and two respawns. Native Full entry/reveal images were inspected. |
| 3: native polish and delivery | Fitted the right fixture away from the shaft wall, closed a small upper wall joint, added lost-gift recovery guidance, reviewed Safety/navigation, saved/reopened/exported and recovered exact sources through the editor. Replayed the actual final baked export and measured uncapped performance separately. | Final package and `passes/pass3-source/`; final campaign receipts, source-recovery hashes and performance receipt below. Genuine gameplay video includes beam behavior, objective carrying/return, win, death and restart. |

<a id="editor-0120-beam-native-acceptance-and-controls-provenance"></a>
### Native acceptance and controls provenance

`tools/authoring/beam_acceptance.py` imports each tested archive into an isolated
state directory and launches the **direct executable**. The route uses ordinary
held W/A/S/D, E/F/Return/F1/jump/crouch events and camera-only aim helpers through
the actual handlers. It never changes player/item/AI poses, navigation goals,
objective flags or invokes gameplay targets directly. `beam_audit.py` checks
captured physical state and actual outcomes. Native input recipes and traces are
retained. This is transparent automated native verification, not a claim that a
human manually authored or played those movements.

Final archive SHA-256:
`12ac18250014116e7ce3f7bb27af17ccead9cc6ebacc11eef0631590d420f332`.
Final accepted player SHA-256:
`ffd1f80fba72fc0a2d5e69d00e931a4e19bb095bf9a1b7affc41ae6ee2828761`.
The full edge campaign/video used the preceding immutable `payload-final` build
(`3e5797a3…`); the final rebuild adds explicit trace fields and strengthens the
small reset regression, with no gameplay/source/asset changes. It replays the
same exported package for immediate reset, win and menu/reentry acceptance.

The final complete winning route physically traverses both floors, opens the
upstairs door with E, carries the green gift down the ramp, rests all three
unheld physical gifts on the counter, and uses the exit for explicit win feedback.
F1 after the first return reports 1 of 3. The store loop stops at completion. The fresh-runtime reset replay additionally
checks the exact `run_restarted` event: AI clock/speed are zero and AI,
renderer-bound frame, the component transform read by `api.actor`, and beam
anchors agree immediately at the authored start before movement. The first
real F reacquisition after each respawn reveals that body; the tiny regression
also invokes actual scoped Luau and checks all five reset-body sampling anchors.
Beam testing measures about 8.5 m versus 2.4 m attenuation, a dimmer 17-degree
edge, exclusion at 35 degrees, immediate disappearance when switched off,
delayed movement release, reacquisition, and real partition occlusion. This
small enclosed map does not offer a straight 24 m maximum-range sightline.

The final campaign separately checks stars-first ordering, pickup invalidating a
return, set-down reentry, dropping elsewhere, missing-objective exit refusal,
ordinary restart sign, death twice/respawn twice, death holding a gift, authored
AI traversal around furniture and onto the upper floor/opening its door, and
live perimeter/upper-floor push/jump/crouch attempts. Every sample checks script,
audio and body-recovery diagnostics. Exact successes and any limitations are in
`beam-handoff.json`; the campaign is explicit developer tooling, not CI or a
proof of every possible input/physics arrangement.

`checks/immediate-reset-agreement.json` retains both zero-time reset snapshots.
The native `accepted-death/acceptance.json` checks them and real F reacquisition;
the actual Luau host read is additionally exercised by the small reset test.

Native editor save/reopen/package export/source recovery is recorded in phases
44–48. `checks/source-recovery.json` confirms all three declared source files
recovered byte-for-byte, including metadata and Luau. Standalone import and play
use the exported baked package, not editable JSON. Normal project `verify
--require-current` still has Closing Time's already reported project-index
limitation; matching source/dependency admission, exact recovery and native play
are the concrete checks here.

<a id="editor-0120-beam-engine-changes-and-validation"></a>
### Engine changes and validation

All gameplay is ordinary map-local Luau. Reusable host additions are readonly
player/actor observations (raw illumination, hold state and collision sight),
bounded navigation through existing AI, bounded dismissible guidance and run
outcomes, and optional `hint` dispatch. `light_response.reveal_only` hides
rendering independently of release grace. New exports declare
`scene-actions-v1`, so older hosts refuse the expanded contract explicitly.
The old `luau-v1` capability and original maps remain readable by this runtime.
Existing execution, memory, action, reference and staged rollback bounds remain.

Three small engine regression tests protect distinct contracts: sparse/oversize
guidance bounds and atomic outcome rollback/reset cancellation; immediate beam
visibility versus movement grace and the native-found stale transform reset;
and collision-aware script sight plus stable nearby navigation goals. They use
tiny metadata worlds, no graphics/assets/full-map bake, and are discovered under
the owning `src/engine/tests/`. All 137 Rust tests pass in under a second after
compilation. Format, strict all-target/all-feature Clippy and locked all-feature
release build pass. Release Luau infinite-loop, memory and error refusals pass
without baking a map. Asset validation reports 594 entries, 371 placeable,
zero errors/warnings; existing detailed texture warnings are retained.

The initial broad Python run hit the sandbox's unavailable Metal adapter in the
existing graphical smoke class. The explicit CPU run uses its supported
`PLACES_SKIP_SMOKE=1`; Beam's direct native Metal campaign is separate. This is
an environment limitation, not a successful run of that graphical test class.

The geometry checker reports zero errors and three retained warnings: two
0.00031 m² generated perimeter-trim joint slivers and the exit leak heuristic
which omits closed doors. Ten missing-wall findings at deliberate floor-strip,
ramp and real door connections have narrow, reasoned intent rectangles. Errors
were never suppressed. Native Safety retains floor-contact model reviews and
its explicit triangle-pair budget warning; its ramp-floor edge samples and exit
door sample need contextual review. No blanket placement ignores were added.

Performance uses physical **1920×1080** (960×540 logical), High filtering,
Full lighting, bloom/flashlight on, reflections off, 1200 warmup plus 6000 measured
frames. The entrance view aims at the mannequin: 28 props, one skinned actor,
three gift bodies, two doors and four fixtures (42 visible batches/28,527 vertices).
Actual presentation switches from initial Fifo to **Immediate**, swap interval is
zero, and the Mac display reports **120.00 Hz**. Recording and frame limiters are
off; GPU pass timing is measured.

| Run | Average FPS | Median FPS | Interpretation |
| --- | ---: | ---: | --- |
| Preceding immutable runtime | 603.08 | 762.78 | Clearly above display refresh; uncapped observation. |
| Fresh final runtime | 121.45 | 120.86 | Refresh plateau despite Immediate; **inconclusive throughput**. |
| Fresh final quiet repeat | 304.75 | 590.35 | Alternating uncapped and refresh-paced intervals; **inconclusive stable throughput**. |

No gameplay change separates these runs. The host's variable presentation pacing
prevents a reliable gain comparison or stable uncapped claim for the final build.
All actual means, frame CSVs, near-refresh/fast counts, presentation, settings and
GPU distributions remain in `checks/performance-campaign.json` and the three
native directories. The compact evidence and final receipt include all three runs and the later
pacing limitation. No minimum FPS or before/after gain is claimed.

<a id="editor-0120-beam-actionable-discovered-gaps-for-the-queued-implementation-task"></a>
### Actionable discovered gaps for the queued implementation task

Shared gaps are deduplicated against the Closing Time report. Ordinary Luau
authoring is intentional; missing source/component/link controls are editor gaps.

| Reproduction / impact | Workaround here | Requested correction and acceptance |
| --- | --- | --- |
| Add a door with its initial footprint inside furniture; Add refuses but the old object remains selected. Subsequent numeric fields can modify an unrelated wall. Phases 13/13b exposed this. | Exact source/checkpoint comparison; restore wall 4, frame an empty location and fit the door before changing fields (13d). | Failed Add should clearly preserve/identify selection or disable follow-on creation edits; add a tiny regression and native refusal/recovery sequence. |
| A valid final ramp/header pose can have invalid intermediate numeric edits. Raising the ramp before fitting its footprint is refused; a header edited through the door is refused. | Dimensions/position first, then height/offset (23b/35b). | Atomic multi-field transform/preview transaction with clear collision reason; preserve validation and undo. |
| Stable IDs, typed state and prop/door script links lack inspector controls; exact item plus unheld and enter plus exit cannot be composed in one trigger sheet. | Documented JSON metadata, filters and bindings. | Implement the shared Closing controls for IDs/state/multiple events/composable filters. Verify held removal/reentry and out-of-order progression in both demos. |
| Detailed nav body, light-response thresholds/reveal/hold, attached audio and physics reset group are not exposed by the coarse behavior/pickup UI. | Explicit source components on the GUI-placed mannequin and gifts. | Expose bounded existing component fields with capability/reference checks, authored-pose preview and clean reset. Do not replace script behavior with map-specific engine branches. |
| Prop Y remains relative to the first room, while selecting an upper floor suggests local storey ownership. Cutting a room removes both floor and ceiling; a ramp needs a separate same-plane tall carrier. | Native upper props use explicit 3 m offsets; separate enclosed shaft and landing rooms. | Clarify/store room ownership and coordinate basis, show cuts/clearance, and offer a safe connected-storey workflow. Retain incompatible-overlap refusal. |
| Holding E prioritizes item drop over door use; a player cannot close/open an ordinary door while carrying. | Open the upper door before collecting green, and leave it open on return. | Define a contextual held-item/door interaction that preserves deliberate set-down and obstruction safety. Replay carrying through both directions with no forced drop/clipping. |
| Pickup/return relies on conservative rotated-body overlap; a low floor-level receiver can briefly count a dropped item touching its envelope. | Raised exact/unheld band to 0.6–1.4 m and a wide real support counter. | Offer support/sleep/centre-aware receipt policy and visible receiver fitting; preserve enter/exit and held transitions. Test a rotated small box, not a whole map. |
| Automatic-handler `api.reset` may discard its dispatch report inside `EntityWorld::tick`; interaction reset works and is tested. This is a code-inspection finding, not a reproduced Beam failure. | Beam resets through ordinary interaction/outcome paths. | Create a tiny start/update-reset repro; propagate player/item/audio reset requests safely if confirmed, without callback recursion or stale commands. |
| The scene needed general messages/outcomes, renderer-backed actor queries and script navigation. | **Implemented here** with bounds/capability admission. | Later safety task should audit readonly/scoped callbacks, sparse/metatable/oversize text, action/reference/finite bounds, reset lifetimes and release refusals; replay both modes. |
| Native death/reset left AI component transforms at the catch position although agent/render state reset. | **Fixed here**: synchronize transforms and clear sight; small regression plus repeated native respawn. | Keep the reset regression and replay both modes after combined changes. |
| World audio/input/flashlight could persist through result/help/reset. | **Fixed here**: common gameplay gate, staged modal input clearing, authored audio reconciliation, flashlight reset. | Later integration should replay hints while carrying, outcomes, menu/reentry and ordinary reset with actual mixer evidence. |
| Installed horror audio lacks a suitable soft mannequin step, beam acquisition cue, distinct gift confirmation and restrained success sting. | Existing spatial `impact_soft`, low HVAC, switch click and door closure; no invented asset IDs. | Supply normalized quiet step/return/success assets with chooser preview and shared lifecycle; no loud random scare required. |
| The current mannequin has inherited hand/rig/animation limitations; black tint is a catalog preset of the unchanged installed model/PNG. | Existing asset only; no cloud revision integrated. | Integrate only the separately corrected verified delivery in the next asset stage; keep all fitted PNG/UV/rig/clip contracts and replay reveal/carry occlusion. |
| Safety sees floor-contact coplanarity, misses doors in leak/drop heuristics, and exhausts triangle-pair review. Full bake reports no probe in one tiny floor strip, although the shared field covers the scene. | Reviewed native contacts, preserved warnings and physical play; no raised furniture or blanket ignores. | Improve door/storey-aware edge checks and candidate prioritization; expose incomplete coverage/probe sampling accurately. Preserve strict actual duplicates. |
| Full project verification/resource-root/prepared reuse issues match Closing Time. Direct SDL windows are missing from desktop app inventory. | Explicit asset environment root, exact source recovery, native input driver and original OS recordings. | Implement the already specified shared fixes and environment integration without changing ordinary package/import limits. |
| Small bitmap buttons/text and modal edge strips remain visible at Retina scaling. | Existing font/layout; concise readable panels, no UI redesign during map work. | Apply the separately queued readability pass, including the approved five-dot crosshair and per-map/session behavior. |

Next work remains ordered: accepted asset/corrected mannequin integration (push
combined local work there), editor readability/crosshair, implement concrete
discovered gaps from both demos followed by Luau/import safety and replay of both,
then the separately authorized connected feature-test map/game with honest
feature coverage/manual gaps. That map exercises supported features and exposes
editor gaps; it does not authorize arbitrary missing engine features. The entire remaining queue is now paused for Connor's playtest and explicit
resume. No work on those later stages was started here.

<a id="editor-0120-beam-evidence-delivery"></a>
### Evidence delivery

Local archive paths, confirmed Library identities, final campaign/performance
results and cleanup receipt are recorded in
`local/verification/0.12.0-beam/beam-handoff.json`. The compact video uses genuine
native screen recordings cropped to the game content, real post-mixer audio and
simple chronological cuts, without synthetic gameplay reconstruction. Source-only
archive and baked distribution are separate. Original recordings remain local.

Library creation and a separate metadata listing confirm all four identities and
byte sizes; returned version attributes were applied and read back locally.

| Deliverable | Confirmed Library ID | Bytes |
| --- | --- | ---: |
| [Beam-native-gameplay-final.mp4](https://chatgpt.com/api/library/files/libfile_bcc9fe4f74788191956ae1862857ccfa/download) | `libfile_bcc9fe4f74788191956ae1862857ccfa` | 2,698,460 |
| [Beam-screenshots-and-evidence.zip](https://chatgpt.com/api/library/files/libfile_cbf70ec98b9c81918d735ee7a7a8f7b8/download) | `libfile_cbf70ec98b9c81918d735ee7a7a8f7b8` | 2,864,760 |
| [Beam-editable-source-only.zip](https://chatgpt.com/api/library/files/libfile_aa4b29d81cc481919dd07d1102eda3eb/download) | `libfile_aa4b29d81cc481919dd07d1102eda3eb` | 12,805 |
| [It-Only-Exists-in-the-Beam.placesmap](https://chatgpt.com/api/library/files/libfile_fae92ac49f6081918edc971c559cb6e2/download) | `libfile_fae92ac49f6081918edc971c559cb6e2` | 6,212,264 |

<a id="editor-0120-closing-time"></a>
## Closing Time — native editor authoring and playable mode

Work is confined to `0.12.0-staging`. The editable project is
`projects/closing-time/`; the level ID is `closing_time`. It uses the currently
integrated Arcade, party, bowling and shared assets. No pending delivery,
Spooner-Man replacement, mannequin work, main merge or release is part of this task.

<a id="editor-0120-closing-time-authored-experience"></a>
### Authored experience

A deserted venue has one short closing shift: return the burgundy ball to its
rack, the star gift to the other gifts, and the cat plush to the lobby lost-property
box; shut down arcade, party and bowling circuits; then lock the front door.
Returns remain outstanding while held or removed. Circuits can be shut down in
any order and repeated presses are idempotent. The closed entrance is the final
interaction and shows a concise good-night/completion world label. Another press
starts a fresh shift. The exit sign also offers a restart during a shift.

The scene has four adjoining floor/material areas, two abbreviated bowling lanes,
a complete pair of pin decks, gutters, arcade cabinets/pinballs, party furniture
and a warm lobby/aisle light that remains after the working circuits go dark.
Installed HVAC emitters progressively stop. Full lighting is the intended
presentation: the existing fixed-prop circuit contract holds authored illumination
in Static. That limitation is an implementation input below, not a claim that
Static provides the same shutdown atmosphere.

<a id="editor-0120-closing-time-genuine-editor-work-and-source-only-operations"></a>
### Genuine editor work and source-only operations

Desktop automation could not address the direct, unbundled SDL editor window
(`cua.getApp` returned `Invalid app`). No `.app` wrapper was created. The existing
`places-editor --verify-script` facility clicked **visible** controls and sent
normal text/key/wheel input through the editor's ordinary event handlers. Hidden
controls cannot be invoked. `screencapture` recorded actual desktop pixels during
these operations. The delivered edit is labeled scripted native-editor authoring,
with normal-speed continuous excerpts, simple cuts and one explicitly labeled native checkpoint still. Cropping removes desktop
chrome; captions identify the operation and method. No replacement GUI or staged
GUI reconstruction is used. Failed attempts and original recordings are retained. Working recipes were regenerated during authoring; `recorded-inputs/provenance.json` explicitly identifies reconstructed early input recipes. Native video and checkpoints are original. Later recordings retain their exact input before launch.

Recorded native operations include floor creation/resize/materials, exterior and
partition walls, built-in model selection and placement, duplication, numeric
fitting, fixture family/power/range/colour/switchability, sounds, trigger bounds,
closed door creation, movable prop physics, Luau source creation/edit/check/save,
Safety inspection and surface mounting. `tools/authoring/` contains explicit
opt-in recording/input/evidence tools; these do not run in Cargo tests or CI.

The exact raw editor baseline and post-editor source diff are retained at
`local/verification/0.12.0-closing-time/editor-authored-project/` and
`manual-workarounds.json`. Source-only work is limited to:

| Operation | Why source editing was needed | Workaround |
| --- | --- | --- |
| Stable project/map names and IDs; spawn; default wall/floor/ceiling; presentation/fog | No existing-level ID rename, spawn or global defaults/presentation sheet | Initial metadata/source fields before geometry authoring; no generated artwork |
| Descriptive runtime identities, declared bool state and conditional E bindings on switches/door | Script linking is exposed only for triggers; component/state/condition authoring is absent | Ordinary `interactable`, `state`, `run_script`, `set_prompt` bindings in level JSON |
| Exact item **and** unheld return filter; enter and exit handlers | Filter chooser cannot compose exact identity with held policy; sheet edits the first event binding | Existing strict item filter plus two ordinary volume bindings |
| Fixed-prop light components, shared circuits and compatible Static-hold policy | No prop-light/circuit membership inspector | Existing `light`, `switch_group`, fixture `hold_in_static`; no mode code in engine |
| One shared physics reset group and fitted rack standable height | No reset-group text or per-axis collision-size inspector | Existing `reset_group`; rack support height follows the installed ball-support placement |
| Completion label capability and conditional visibility on the GUI-placed lock plate | The hand-only HUD does not draw authored prompt strings; label visibility needs an interactable capability even when the plate is not aimable | Disabled interactable capability plus existing world-label actions, hidden before restart |
| Narrow intentional open-plan/closed-door checker annotations | Architectural perimeter heuristic does not infer adjoining floor-material areas or the separate leaf | Check-specific `geometry_intent` rectangles, with reasons; no confirmed defect is suppressed |

Luau gameplay authoring is intentional, ordinary map scripting. Its source was
entered, checked and saved in the native Scripts sheet. It uses the shared
light/sound/state/door/physics/reset actions. Restart resets the complete item group
before the shared world reset, restoring item poses/ownership and fresh VM state.

<a id="editor-0120-closing-time-reusable-fixes-made-within-this-demo"></a>
### Reusable fixes made within this demo

The existing mounting command placed fixture backs 1 mm from their carrier,
inside the geometry checker's 1.5 mm coplanarity tolerance. An ordinary mount
therefore produced confirmed duplicate-surface findings. Mount clearance is now
2 mm. One small owning-editor regression checks a mounted panel against the real
geometry checker and also proves a genuinely coincident panel is still detected.
It uses two tiny CPU mesh checks, no GLBs, lighting solve or shipped map bake.

Native acceptance gained bounded `view` and `view_at` actions that change yaw/pitch only. It
preserves player position, body/stance, motion and item state. This permits a full
controls-driven playthrough without the diagnostic player-position action. The
existing benchmark opt-in, admission and action-count/time bounds remain active.
No Closing Time routine was added to the engine.

<a id="editor-0120-closing-time-verification-evidence"></a>
### Verification evidence

The authoritative local receipt is
`local/verification/0.12.0-closing-time/closing-time-handoff.json`.
The final editor-exported package SHA-256 is
`134c3812a573b8bf4baada22631513e028b967e576ddc18461138f1f7aab8b16`. Initial preview evidence is explicitly labeled Static and
pre-final; it is not final playthrough or throughput acceptance. Native inspection
caught and corrected a wall hiding the pin decks and copied dividers inheriting
the editor's deliberate sideways duplicate offset. Both corrections use the editor.

Final native acceptance uses `Closing-Time-Editor.placesmap`, exported and
recovered through the recorded Project UI. All three declared source files
(metadata, level JSON and Luau) recover byte-for-byte. Native launches independently
import that package into isolated state and use direct executables. Package
validation reports intact dependencies and no warnings. Ordinary
`verify --require-current` computes the matching fingerprint but falsely assumes
a root build-input record in complete project archives; this is documented below,
not counted as a passing check.

- **Natural playthrough:** only ordinary held movement/interaction/flashlight
  input plus camera orientation; no player/item pose changes or direct action
  activation. Returns, circuit shutdown and the entrance reach a locked completion
  with the visible good-night label; all three bodies sleep, none are held.
- **Order/repetition/reset:** return-first and bowling-first sequences work; a
  repeated shutdown is idempotent. Removed and held returns invalidate completion.
  Restart while holding and after winning restores exact item poses, ownership,
  circuits and all four ambience loops. The completion label clears.
- **Collision/escape:** real movement/jump/crouch attempts against all perimeter
  directions, entrance and corner remain enclosed. Cabinets/table block walking;
  a 20 m/s ball stays inside. No recovered or suspended body in any final case.
- **Lighting/audio:** all three circuits actually switch off in Full, leaving
  lobby/aisle fixtures. Mixer loops progress 4 → 3 → 1 → 0 and restore on reset.
  Actual native post-mixer captures have no clipping, dropped voices or errors;
  the final natural second is silent after the lock cue. Audit caught incorrect
  sorted sound-picker indices; recorded UI pass 31 selected the actual catalog
  `switch_click` and `door_close` cues before final export.

On Apple M2 Pro/Metal the fixed lobby workload averages **354.03 FPS**, with a
106.66 FPS 1% low, over 1,200 measured frames after 120 warmup frames. Actual
presentation changes from initial surface Fifo to **Immediate**; swap interval is
0, V-sync and frame limiting are disabled. The drawable is **1920×1080**
(960×540 logical), High quality, Full three-page lightmaps, bloom on, reflections
off: 85 props, three movable bodies, one door, seven fixtures, three circuits,
four spatial loops, no AI, 93,354 visible vertices and 112 draw calls. Other local
baking/encoding jobs had stopped. The current display is 120 Hz; mean and median
(441.57 FPS) exceed refresh, establishing this run is uncapped. This is one
absolute scene measurement, not a performance improvement comparison.

Formatting, strict all-target/all-feature Clippy, locked all-feature release
build, asset/PNG/GLB checks and release Luau error-boundary probes pass. All 134
Rust tests pass in 0.27 s; the existing 368 Python tests pass (three skips).
No shipped-map bake is added to ordinary tests or CI. CI for implementation
`36790aef3817e6f7f453f9a93af7aca411c4626e` is
[run 39096038991](https://github.com/csd113/Places/actions/runs/39096038991);
it was already pushed/started under the earlier instruction. The later explicit
user instruction removed push/CI gates for both map tasks: waiting stopped,
cancellation was requested, and the final evidence/handoff commit remains local.
Do not push again or wait for this run; asset integration after Beam will push
the combined work and run applicable CI.

Runnable direct editor/player/compiler copies remain in `payload-final/` outside
`target`; `try-editor.sh` and `try-player.sh` use isolated manual state. No cargo
clean was needed. The Library delivery bundle contains the compiled package,
editable source, native screenshots, 2m 54s authoring movie, input/provenance
records, manual-edit journal and compact acceptance evidence. Installed staging
assets/player are required; this is not an installation or cross-platform release.

<a id="editor-0120-closing-time-discovered-gaps-for-the-later-combined-implementationsafety-task"></a>
### Discovered gaps for the later combined implementation/safety task

These are concrete inputs for the authorized later task, after Beam's three
passes, accepted asset integration and the editor readability pass. They are not
implemented here unless explicitly marked fixed. Replay both finished modes after
addressing them, then perform Luau/map-import safety checks.

| Reproduction / impact | Current workaround | Proposed correction and acceptance |
| --- | --- | --- |
| Select a wall switch, open Scripts, choose Link: only a selected trigger can link. There is no bool-state or binding-condition editor. Closing requires conditional final-door/restart behavior. | Source JSON bindings/state | Reusable component/event/condition authoring and script export linking for props and doors. Preserve IDs, validate references, undo/save/reopen/export exact behavior. |
| Trigger Filter can choose an exact physical item or unheld items, but cannot combine both. Only the first event binding is configured. A carried or subsequently removed item must invalidate its return. | Exact+unheld filter and enter/exit JSON bindings | Composable identity/held filter controls and a multi-event binding sheet. Native checks must cover held/unheld transitions, removal, fast crossings and reset. |
| Fixture sheet lacks circuit names; prop sheet lacks light controllers and attached-source controls. A cabinet bank and its ceiling fixtures need one switch. | Shared circuit/light-component JSON; matching hold policy | Expose existing circuit/controller settings with compatible-state checks and real source budgets; preserve unrelated maps and source extensions. |
| Select Static lighting and switch the fixed cabinet/fixture circuit: the existing authored-state hold keeps it lit. Closing's full atmosphere currently requires Full. | Use Full in the supplied native workflow | Support bounded prepared off states consistently for the needed fixed-prop/multi-circuit use, or enforce an explicit map presentation capability. Verify both modes and quality changes without a runtime bake or misleading dark faces over lit rooms. |
| Script `api.notice` is diagnostic output. Authored `set_prompt` strings are not drawn by the hand-only HUD. A valid `toggle_label` target without an interactable component is admitted but silently unavailable at runtime. | Disabled interactable capability on a GUI-placed entrance plate; conditional existing world-label actions | Provide bounded optional completion feedback and validate label-target capability at admission. Preserve minimal interface, reference checks and staged-effect rollback. |
| Physics UI exposes on/pickup/mass but not reset-group identity or a fitted fixed compound collider. The ball rack's decorative upper bound is above its actual ball support. | Shared-group JSON and fitted standable collision height | Expose group identity and existing bounded fixed-shape support, with preview and reset validation; verify real rack/basket/shelf placement without broad invisible boxes. |
| Installed audio has one HVAC ambience and generic clicks/door closures, without arcade standby, bowling motor shutdown or a distinct lock. | Separate spatial HVAC emitters; existing switch/door cue | Supply appropriate normalized installed loop/shutdown/lock cues and preview them in the sound chooser. Retain preloaded shared audio lifecycle and real post-mixer evidence. |
| `check-project --asset-root <valid root>` from the relocated direct payload still fails authored-audio admission unless `PLACES_ASSET_ROOT` is also set. | Explicit environment root in native tooling | Thread the explicit resolved resource root through semantic/audio validation; test a tiny emitter project from an unrelated working directory. No full bake needed. |
| Run ordinary `verify --require-current` against a complete project archive: its computed fingerprint matches, but it assumes a root `build-inputs.json` and falsely reports stale. | Successful project validation/dependency checks, matching production fingerprints and exact editor recovery of all declared source bytes, plus standalone package play | Make verification project-aware and resolve shared prepared records through the project index; cover a tiny source-only project archive without a shipped-map bake. |
| Export a project after a semantic-only source edit: the exporter compiles into a fresh temporary location, so prepared-record reuse cannot see the previous package and repeats the full lighting bake (about six minutes here). | Freeze semantics before final Full export; use Static previews during fitting | Seed reusable prepared records only after exact source/asset/signature validation, retaining atomic publication and source-change rejection. |
| Safety on this 85-prop scene retains seven floor-contact coplanarity reviews and reports the model-pair candidate budget. The entrance drop heuristic does not include the closed leaf. | Review actual native contacts/escape attempts; preserve narrow geometry intent | Distinguish intentional floor contacts and doors, prioritize nearby candidate pairs and report incomplete coverage honestly. Do not float scenery or weaken real duplicate checks. |
| Direct SDL windows are absent from the desktop app inventory. Native editor verification can operate them, but normal desktop automation cannot select them. | Existing native driver plus real OS video recording | Treat this as an environment/automation integration gap, separate from editor functionality. Retain direct-binary support. |
| At 1440×816, the fixed low-resolution bitmap UI and four-row scene list make larger authored scenes slow to inspect and small labels difficult to read. | Visible scene scrolling and numeric entry; full native captures retained | The separately requested readability pass should address sharp text, button sizing/contrast and physical/logical display scaling; add practical object search/selection while keeping the workspace uncluttered. |
| Mounting a fixture produced duplicate-surface errors even after Mount surface. | **Fixed here:** 2 mm mount clearance | Keep the new tiny cross-check and final native mount/check acceptance. Do not weaken coplanarity validation or clear intentional ignores globally. |

Optional, unrelated expansion remains outside this task: traffic reservations,
new AI encounters, new campaigns, broad asset authoring or cinematic systems.
This mode intentionally has no NPC/AI requirement.

<a id="editor-0120-closing-time-confirmed-library-delivery"></a>
### Confirmed Library delivery

Library creation succeeded for all three files and a subsequent metadata lookup
confirmed their identities and byte sizes. Local identity/version attributes were
applied with the Library transfer helper. Prepared-transfer reservations were not
finalized: that transfer client was unavailable, so the supported host-upload
create operation transferred and saved the actual local files instead.

| Deliverable | Confirmed `library_file_id` | Bytes |
| --- | --- | ---: |
| [Playable package + editable source + native acceptance](https://chatgpt.com/api/library/files/libfile_e056f800e2cc8191b5aab188f675e407/download) | `libfile_e056f800e2cc8191b5aab188f675e407` | 33,755,403 |
| [Native screenshots + edit decisions + mixer listening copy](https://chatgpt.com/api/library/files/libfile_03016e239500819187b0a9ddc9f82fc3/download) | `libfile_03016e239500819187b0a9ddc9f82fc3` | 6,021,640 |
| [Genuine native authoring video, 2m 54s](https://chatgpt.com/api/library/files/libfile_16d61ee9dc048191882f60e51a88d24c/download) | `libfile_16d61ee9dc048191882f60e51a88d24c` | 13,716,215 |

Closing Time ends this task. Beam, asset integration, readability and the combined
discovered-gap/safety pass remain queued; the separate mannequin worker was not
disturbed.

<a id="editor-0120-gaps-safety"></a>
## Closing Time / Beam implementation and safety pass

Scope: `0.12.0-staging`, starting at `f6090911`. Accepted gameplay, game Luau,
R2 mannequin, Spoonerman and unrelated assets are preserved. Closing Time alone
adds `requires_full_lighting: true`. No main merge, release or later queue stage
belongs here. Final receipts and runnable delivery live in
`local/gaps-safety-0.12.0/`, outside `target`.

<a id="editor-0120-gaps-safety-recorded-gap-checklist"></a>
### Recorded gap checklist

The checklist deduplicates the actual [Closing Time](release-0.12.0.md#editor-0120-closing-time)
and [Beam](release-0.12.0.md#editor-0120-beam) handoffs; ordinary Luau gameplay remains intentional.

| Recorded gap | Implemented behavior |
| --- | --- |
| Missing instance/state/event/condition controls; trigger-only script links | Focused Capabilities hub, typed state, exact selected event/actions/conditions, prop/door/trigger script links; structured-reference rename before scripting, with undo/save/reopen validation. |
| Exact item and unheld cannot compose; only first event editable | Independent identity/held policy and multi-event editing preserve sibling bindings. Optional centre/settled-centre receipt uses bounded physical paths and actual sleep. `item-receipts-v1` prevents silent older-host fallback. |
| Missing circuits, attached sources, detailed AI/beam/audio/reset settings | Separate paged forms expose existing capabilities and validators. Fitted installed fixed shapes retain their real compound; player standable dimensions stay independent. |
| Invalid intermediate transforms and failed Add editing the old selection | Complete-pose draft/validation/preview commits once; refused Add clears selection. Find filters the scene; Y uses its explicit authored basis. |
| Carrying forces drop before door use | E uses an exactly aimed, reachable, unobstructed unlocked door while carrying; looking away or aiming at a locked door retains safe drop and existing door collision safeguards. |
| Automatic start/update/hint reset loses shared lifecycle | Propagate dispatch accounting/reset, stop later callbacks/events and perform the common player/world/item reset. |
| Static holds Closing's independent fixed-prop circuits | Explicit Full requirement, actual prepared atlas admission and `full-lighting-v1`; lesser-mode requests are refused. No runtime bake or invented Static combinations. |
| Silent unavailable labels and missing sound preview | Label targets require actual interactable capability, including disabled controls; bounded existing message/outcome feedback remains. Listen previews one preloaded cue quietly for at most three seconds. |
| Resource-root audio admission; project verify; repeated semantic-only bake | Configure the explicit root before semantic admission; select project record views for provenance; seed exports only from verified prepared records/signatures and retain atomic publication. |
| Floor-contact noise, door/storey and pair-budget diagnostics | Separate supported opaque bases from thin/alpha/duplicate surfaces, spatially prioritize pairs, include authored door colliders and preserve storey ownership; incomplete coverage is explicit. |

Controls use small owning modules and paged sections rather than adding every
advanced property to placement. Raw unknown fields survive. Readability/crosshair
assets and the accepted game's minimal HUD remain unchanged.

<a id="editor-0120-gaps-safety-safety-boundaries"></a>
### Safety boundaries

Luau retains its closed host surface, frozen libraries, no OS/network/module
loader/FFI, 2 MiB per VM and existing source/module/action/reference limits.
Every host invocation now has a 256-call native-query/action budget. Execution
deadlines begin after host setup/compilation, while host setup still counts
against frame accounting. Failed calls publish no staged effects and disable
the module until a fresh run.

An actual sparse `table.move` probe exposed native work beyond cooperative VM
interrupts. Its wrapper preserves ordinary overlapping/negative/optional-target
semantics, accepts real integral 32-bit indices, and caps work at 8192 elements.
The retained before receipt measures a whole compiler process, not isolated
library latency. Release probes include the count-based refusal.

Archive admission checks central-directory framing, encrypted/control/duplicate
entries, existing path/count/aggregate/decompression limits and hash ownership.
Recovery validates one bounded owned snapshot; temporary directories are reserved
atomically and publication never replaces another folder. Source/output identity
is checked after compression at publication. Special files/FIFOs are refused.
Installed dependencies stream through 8 KiB buffers under the actual PNG/GLB
encoded limits, stay inside the canonical asset root, and use regular opened
handles. Repeated portable dependency identities are refused before hashing.
Independent focused review checked these boundaries and the reset integration.

These are in-process cooperative safeguards, not a process sandbox or hard
real-time/RSS guarantee. Native-library calls/decompression can run between
interrupt/cancellation checks. A huge otherwise valid installed resource set
still costs bounded file hashing; malicious concurrent filesystem mutation is
not a promised local authoring sandbox. Saturated event queues can lose a wholly
outside-to-outside transient crossing; final exits are retained. Neither finished
game approaches that saturation.

<a id="editor-0120-gaps-safety-verification-and-delivery"></a>
### Verification and delivery

Required strict Clippy, locked all-feature production build, formatting and
owning-component regressions are retained in `checks/`. The small tests protect
specific capability, transaction, native-cost, reset, receipt, door and geometry
failures; no routine test or CI check bakes a shipped map. The source Python suite
passes 369 checks with 16 supported skips on this local CPU run; separate native
acceptance covers the actual macOS/Metal player/editor.

Final native outcomes, exact-head CI, payload hashes, Library identity, source
custody, cleanup and preserved-binary smoke receipts are recorded in `handoff.json`.
The report is paired with that machine-readable receipt rather than embedding a
self-referential commit hash. Use its runnable commands and final package paths.

Native component/event/save/reopen acceptance passes 36 assertions; additional
pose-preview/one-undo/refused-placement/audio acceptance passes 13. Actual SDL
focus is recorded at every checkpoint. The original Beam win route exposed a
carried-item deposit regression when the locked exit behind the counter took
interaction priority. The target rule and existing small regression now preserve
set-down for locked doors. The failed native attempt is retained alongside the
final rerun rather than counted as acceptance.

The final native player passes eight campaigns (53 focused captures, 202 audit
checks, plus the explicit post-win reset checks): both ordinary win routes;
Closing's alternate return/switch order, held removal and restart; Beam's two
death/reset cycles, return reentry/removal, hints/sign reset and beam/partition
occlusion; carried-door opening/traversal/set-down; and Closing's actual Full
retention, quality changes, circuit state and reset. A closer held-door approach
also retained its physical obstruction guard. These are correctness runs; no
uncapped-throughput conclusion is drawn from their capture timing.

The final delivery compiler and both final packages pass all 44 package-safety
checks and all 11 hostile Luau initializer refusals. A name-only export of an
isolated Closing copy hits the verified prepared-world cache, preserving all
prepared record references and lighting fingerprint while updating package
provenance. The delivered Closing package passes project-aware
`verify --require-current`. Beam's accepted package remains byte-identical.

Read-only native Safety scans preserve both sources. Closing has 29 entries
(two notes, 27 review candidates); Beam has ten (two notes, eight review
candidates, including sampled leak/drop/sliver findings). These remain contextual
review candidates under the documented sampled-geometry limits, not a claim that
either map is proven closed. No automatic correction or ignore was applied.

<a id="editor-0120-gaps-safety-explicit-remaining-work"></a>
### Explicit remaining work

Arbitrary mixed-circuit Static preparation requires a renderer/prepared-record
feature. Connected-storey topology/room ownership, project/map ID migration,
arbitrary collider construction/part overlays, additional sound production and
polish, raw-SDL app inventory integration, and later editor/distribution stages
remain separate. Scripted ID/state rename deliberately refuses opaque Luau
rewrites. Settled-centre receipt does not identify a particular supporting shelf.
Safety remains sampled, uses authored door poses/static models, and does not
prove mesh closure or improve probe-grid sampling. No Windows/Linux native
certification or new FPS comparison is claimed. Broad issues remain open.

<a id="editor-0120-navigation"></a>
## Layered navigation and native editor inspection

Implemented on `0.12.0-staging` after Stage 7 head `588a77fe`. This pass is
navigation only: no later game-mode authoring, asset integration, main merge,
release, version bump or app packaging.

<a id="editor-0120-navigation-behavior-and-architecture"></a>
### Behavior and architecture

The prior audit was substantially correct: automatic 0.20 m sampling, one Y per
XZ cell, bounded A*, and separate runtime AI/perception/movement. The decisive
limitation was losing overlapping floors. PNV2 retains vertically ordered spans
in each XZ column and stores eight explicit per-body directional links. This
keeps automatic authoring and reusable bounded query scratch without introducing
a polygon-mesh toolchain. Region labels, endpoint projection, diagonal traversal
and shortcut smoothing retain floor identity. Shortcut attempts have a shared
128-attempt budget; exhaustion preserves the certified unsmoothed chain.

Clearance considers body radius/height, ceilings and upper-floor undersides.
Continuous slopes respect slope limits independently of the small integration
step; discrete steps and drops use the step limit. Continuous segment probes and
exact swept discs prevent thin solids between sample centres disappearing.
Step sweeps cover the complete vertical body envelope, including short bodies
whose old and new height bands do not overlap. Static, live-item, door and
ceiling checks share this conservative rule.
Adjacent climbable landing edges are distinguished from a low floor directly
over the body. Runtime support stays near current feet and reports actual
movement rather than requested travel. Movement also checks the baked class
links, avoiding a separate permissive steering path.

Doors remain live portals, assigned to the correct height. Capable agents request
unlocked doors; locked and obstructed doors refuse passage. A stuck opening leaf
invalidates the waiting route after a bounded delay. Bounded angular probes check
moving leaves against AI bodies and current physical bounds. Physics colliders
supply a small live AABB overlay, shared by planning and movement, without
rebaking global navigation. Actor body probes and local shoulder passing avoid
ordinary opposing-actor collisions. Repathing rotates across actors with eight
queries per tick, 8,192 A* expansions per runtime query and per-agent cooldowns;
unreachable/blocked destinations remain retryable when the world clears.

`Navigation >` is an optional native editor panel: explicit current-document bake
without lighting/package export; class, actor and spawn-point selection; body and
door-capability labels; floor slices; coverage and exact-span route endpoints.
Feedback distinguishes invalid, unreachable, complete and budget-limited routes.
Edits, undo, level changes and recovery hide stale results. Authored door state
is inspectable; live props and crowds are checked in Play. Coverage rendering is
capped at 12,000 displayed cells with counts; queries retain the whole graph.

<a id="editor-0120-navigation-compatibility"></a>
### Compatibility

Only the navigation record changes to PNV2/version 2. The package container,
source/project formats and lighting records keep their existing versions. Every
navigation reader/writer and the compiler identity use the new version. PNV1
playback is safely refused with a recompilation instruction. Complete Stage 7
archives still recover exact editable sources for rebuilding. Counts, offsets,
strict height ordering, class masks/regions, reciprocal adjacency and traversal
bounds are validated before use. Count and 128 MiB byte budgets are checked
before large allocations.

Stage 7 export/import and production Luau safety paths remain in place. This is
not the later dedicated Luau/map-import review of the two finished modes.

<a id="editor-0120-navigation-verification"></a>
### Verification

Retained evidence is under `local/verification/0.12.0-navigation/`, outside
`target`. `navigation-handoff.json` records final binary/source/package hashes,
the pushed commit, exact CI run and cleanup receipt. `payload/` contains direct
player/editor/compiler binaries; `try-editor.sh`, `try-player.sh` and
`replay-native.sh` retain isolated runnable workflows. No `.app` was made.

The final locked release build, formatting, debug/release strict Clippy and
package validation/current-source verification pass. The source Python suite
passes 368 cases with three optional smoke skips; asset validation reports 593
entries and zero errors. Final production Luau checks pass three bounded
refusals without baking. Stage 7 compatibility passes 19 checks: exact recovery
of the old archive, new export, validation, exact recovery, project check and
player import/listing. Stage 7 and native acceptance are explicit developer
checks outside ordinary CI; ordinary Rust tests never bake a shipped map.

Final native player receipts are `native-player-r4/receipt.json`, with 106 actual
framebuffer captures, live actor/physics snapshots and direct clean exits:

| Scenario | Verified native result |
| --- | --- |
| Stacked furnished rooms and ramp | Lower actor travelled a sampled 30.48 m through the side ramp/bridge to `(3.04, 3.00, 4.02)`; upper actor stayed on its storey |
| Body clearance | Radius 0.16 m actor crossed the 0.8 m aperture; radius 0.48 m actor refused it; trials start clear of other bodies and run sequentially |
| Live door | Locked leaf held the actor; an unlock-only action let AI open/pass; close/lock interrupted return; unlock let AI reopen and recover |
| Moving physics crate | Actual dynamic body blocked both directions; moving it away twice resumed the same actor without resetting its pose |
| Opposing actors | Both reached their goals with lateral avoidance; minimum sampled centre separation was 0.695 m for combined radii 0.60 m; the small Rust fixture also checks every tick |
| Unreachable island | Actor stayed on its island and settled; 13 queries over 9.71 simulated seconds, rather than per-frame retries |

Final native editor receipts are `native/editor/`, checked by
`check_editor_inspection.py`: current-source bake, all bodies and authored actor
selection, stacked route, slices/all-floor coverage, invalid goal, unreachable
island/large passage/locked door, edit invalidation, undo and explicit rebake.
Actual 1440×816 and 960×544 captures were inspected; route feedback and Clear
route remain visible without overlapping ordinary footer controls.

Native findings fixed actual 3D/planar arrival disagreement and long doorway
steering probes. Earlier failed runs are retained. Harness corrections removed
initially overlapping clearance bodies and simultaneous doorway contention,
and aligned the ramp assertion with its one-second position sampling.

The final timing case averaged **572.16 FPS** over 1,200 measured frames after
120 warm-up frames: actual **Immediate**, V-sync off and no frame limiter,
**1600×900** drawable (800×450 logical), Apple M2 Pro/Metal. The display receipt
records 120 Hz; throughput substantially exceeds refresh, excluding that cap.
The compact scene has nine goal-driven actors, four baked body classes including
the reference, one dynamic body and blocked/unreachable goals; High textures,
Static (`off` variant) lighting, bloom on, reflections off, no navigation dumps,
capture or video recording. Fixed 1/60 s simulation covers 22 simulated seconds;
at completion no actor was still moving and 122 queries had run. Mean update
cost was 1.19 ms, with about 19.66 draws and 2,507 visible triangles per frame.
This is an absolute compact-workload measurement, not an improvement comparison.
An earlier refresh-adjacent run remains explicitly inconclusive in its receipt.

Eleven added small regressions have distinct purposes:

| Owning component | Protected failure and unique value | Cost |
| --- | --- | --- |
| Compiler | Both stacked surfaces survive, a real ramp connects them, serial/parallel output agrees | At most 1,200 tiny sampled nodes; no assets |
| Compiler | A 2 mm wall between clear cell centres must break links | One small two-room-width strip |
| Compiler | A low overlapping slab remains overhead rather than a landing exception | Two small overlapping surfaces |
| Engine query/codec | Close-storey smoothing keeps endpoint identity, blocked upper endpoints cannot fall through, PNV2 round trip and obsolete/forged record refusal | Four nodes |
| Engine movement | Correct storey and real partial displacement against a live thin solid; close floors retain support and reject oversized bodies | Small floor samplers; no bake |
| Engine AI | Live blocker removal resumes movement through ordinary bounded retries | 35 nodes |
| Engine AI | Two actors pass without overlap; a twelve-actor query burst remains capped and fair | 49 nodes, at most 160 fixed ticks |
| Engine door/AI | Fast leaf motion cannot tunnel through a mid-swing body; blocked portal becomes usable after clearing | One leaf, three nodes |
| Engine movement | 3D and planar waypoint arrival cannot disagree and strand a ramp walker | One sloped sampler and short movement |
| Engine AI | An actor inside a blocked closing portal can request reopening after unlock, respecting lock/cooldown | 21 nodes |
| Engine movement/query | A short body cannot lift through a suspended static, live-item or door solid between clear endpoints; legal unobstructed steps still work and closed openable doors are requested | One floor sampler, two nodes |

`cargo test --workspace` passes all **133 tests** (0.26 seconds of test bodies),
including the 122 inherited cases. No full shipped map is baked in ordinary tests
or CI. The moving-solid case found and fixed an actual empty-spatial-index sweep
fallback that silently skipped supplied collision boxes. Required strict Clippy
passes with all workspace targets/features and `-D warnings`.

<a id="editor-0120-navigation-remaining-limits"></a>
### Remaining limits

- A layered 0.20 m sample grid can conservatively miss small passages. No polygon
  mesh, manual link editor, jumping, climbing or swimming AI is added.
- Very large detours can exhaust the 8,192-expansion runtime query budget and
  remain incomplete; editor inspection allows 65,536 expansions. Searches do
  not persist their frontier between ticks.
- Dynamic physics uses conservative current AABBs; rotated and hollow objects
  can exclude extra space. Actors do not push/manipulate props to solve a route.
- Full vertical step envelopes can conservatively refuse a tight passage that
  would require more detailed timing between horizontal travel and lifting.
- Local passing handles ordinary encounters where space exists. Tight crowds
  and passages without passing space may wait; there is no traffic reservation
  planner. A solid moved into an actor can trap it until the obstruction clears;
  forced depenetration is not implemented.
- Editor routes use authored static/door state, not a running world simulation.
- Native platform acceptance is macOS/Metal only. Performance numbers apply to
  the recorded compact workload and are not a comparison with the old build.

<a id="editor-0120-navigation-primary-reference-comparison"></a>
### Primary reference comparison

The design separates topology, route queries and local collision probes, informed
by Valve's [AI navigator](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_navigator.cpp),
[pathfinder](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_pathfinder.cpp),
[local navigator](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_localnavigator.cpp),
[move probes](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_moveprobe.cpp),
[network manager](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_networkmanager.cpp)
and [nav generation](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/nav_generate.cpp).
This is functional inspiration, not Source feature parity or a copied network format.

<a id="editor-0120-next-handoff"></a>
## Historical queue and runnable checkpoint custody

The navigation → Closing Time → Beam → accepted assets → editor readability →
combined gap/safety review → Relay Annex order is completed in the feature records
in this report. The temporary playtest pause and local-only publication restriction
ended before the accepted asset integration and prerelease. The later tiny live
menu moved to 0.13.0; broad #18/#21 and Windows/Linux #17 acceptance remain open.
Game modes #25/#26 were outside that queue. The newer Spoonerman was rejected.

The intermediate direct payloads remain useful reproducible historical checkpoints:

| Checkpoint | Preserved payload / launchers | Source/package evidence |
| --- | --- | --- |
| Navigation | `local/verification/0.12.0-navigation/payload/`, `try-editor.sh`, `try-player.sh`, `replay-native.sh layers` | `navigation-handoff.json`; clearance, door, obstacle, meeting, unreachable and performance cases |
| Closing Time | `local/verification/0.12.0-closing-time/payload-final/`, `try-editor.sh`, `try-player.sh` | `Closing-Time-Editor.placesmap`, `closing-time-handoff.json`; three confirmed Library items in its feature record |
| Beam | `local/verification/0.12.0-beam/payload-reset-verified/`, `try-player.sh` | `It-Only-Exists-in-the-Beam.placesmap`, `beam-handoff.json`; original recordings and source snapshots |
| Project export | `local/verification/0.12.0-task7/` | Original malformed-archive fixtures, refusal logs, payload and handoff |

Those development payloads require the matching repository assets and host SDL3;
they are not clean-install distributions. Full lighting is intended for the two
modes. PNV2/record 2 navigation deliberately refuses PNV1 and requires recompiling
editable sources; project archives still recover their exact source. Timing
replays require observed presentation and display refresh rather than an assumed
refresh setting. Small, justified tests and custody outside target remain the
repository policy; the canonical guides describe current workflows.

<a id="editor-0120-relay-annex"></a>
## Relay Annex — feature-test campaign, 0.12.0

Separate campaign on `0.12.0-staging`, based on accepted shadow-only
head `0f87416c`. Closing Time and It Only Exists in the Beam are preserved.
This is a commissioning shift, not the later environment-demo hub.

The three visits are Snow Intake, The Four Floors, and Discharge. Staff Ives,
Mira and Sol give short text conversations and practical work. The player returns
a supply from an icy yard, traverses four physically connected floors at
0/3.2/6.4/9.6 m, completes an impact return and power check, finds a real
flashlight shadow, and retrieves a floating duck before leaving. Forward airlocks
create fresh local state and clear held equipment. Progress is local to each visit;
whole-campaign persistent variables/inventory remain unsupported.

<a id="editor-0120-relay-annex-supported-feature-coverage"></a>
### Supported feature coverage

The inventory used the implemented level definitions, editor Add/property/menu
controls, scripting API and installed catalog, rather than proposed APIs. The
campaign uses supported room/storey geometry, ramps/stairs, doors, typed trigger
volumes, dynamic item bodies/material response, Full lighting/circuits/flashlight,
text guidance, audio emitters, AI/animation/spawn, water/fog, project transitions,
source recovery and native thumbnails. It does not promise every combination.

| Supported combination | Playable use / evidence |
| --- | --- |
| Winter exterior, sky/weather, snow and ice | Intake yard, existing trees/drifts/lantern; carried box retrieved from ice with real sliding/braking. |
| Four connected storeys | Tower floors 0/3.2/6.4/9.6 m; 16 real steps, two opposite ramps, landings and edge rails; every height walked. |
| NPC text and hands-on progression | Ives supply return, Mira impact/power/shadow work, Sol duck rescue and signoff; contextual F1 advice. |
| Doors, locks, triggers and map load | Actual opening leaves and threshold crossings Intake → Tower → Discharge; no injected Load in complete route. |
| Pickup, set down, throw, receipt filters | Settled exact supply/duck centres; floor-two sphere thrown onto target; no recovered-body workaround. |
| Physics/material behavior | Ice friction, authored sphere restitution, carrying/throwing across upper-floor collision, buoyant duck in pool. |
| Lights, Full circuit and flashlight | Floor-three wall switch restores observation; beam finds hidden fourth-floor witness. |
| Shadow-only entity and animation | Invisible mesh remains an actual flashlight caster; supported AI walk cue advances in place. Legacy upper-floor walking route is recorded as a gap. |
| Audio and scripted entity behavior | Existing ambience, switch, bell, impact and footsteps; relief spawns once, arms-up pose; native mixer output has no errors. |
| Water and regional fog | Swimming, carrying duck underwater, shallow east ramp exit, dry return and fog/reflection presentation. |
| Transition state and inventory | Same declared false stamp asserted at each map start; prior stamp, held parcel and flashlight do not persist. Fresh visits are the supported contract. |
| Graphics/settings and icons | Native Low/High changes retain required Full lighting and active circuit; three imported native thumbnails render. Non-entry names use stable IDs. |
| Ending, reset and safe recovery | Win, Enter resets final visit; whole-campaign board returns to Intake; real recovery-well walk and deliberate labelled off-map placement recover safely. |
| Package/source lifecycle | Complete three-map package validates, requires-current verifies, ten editable files recover exactly and verified-cache re-export is byte-identical. |

<a id="editor-0120-relay-annex-authoring-provenance"></a>
### Authoring provenance

The actual direct native editor created the main yard, four main tower floors
(three `Add upper floor` operations), tower shaft, three principal shells, NPC and
challenge placements, Winter dressing, intake door/trigger, basin and its ramp,
and nine additional tower furnishings. Thirteen initial/final passes plus three
furnishing passes have ordinary-handler input recipes, focused-window document
checkpoints and real framebuffer captures. A short desktop recording shows the
final native Winter drift creation. This is **scripted visible-control input**,
not a claim of manual mouse authoring. Failed recipes are retained.

The explicit source passes consolidated native working projects into the final
three-map manifest; assigned stable IDs and fitted collision sizes; added wall
openings, two airlock vestibules, three shaft flights/landings/rails, per-storey
placement heights, ceiling/material policy, Winter sky/weather, water, light and
audio definitions, detailed physics, receipt filters, reset volumes, timer/spawn
animation bindings, and Luau conversations/objectives/gates. Many of these have
existing native controls; using an external source pass for efficiency is not
itself evidence that a feature is missing. Native script editing, state/event
forms, light/audio configuration and project membership already exist.

<a id="editor-0120-relay-annex-concrete-gaps-and-next-stage-reproductions"></a>
### Concrete gaps and next-stage reproductions

| ID | Expected editor feature / actual finding | Workaround and reproduction |
| --- | --- | --- |
| R1 | Create a ramp at a valid destination even when the current camera target is invalid. Add validates its default pose before a full transform can be entered. | Tower: after four stacked floors, Add Ramp at the main room target refuses `spans rooms with different floors`, clears selection. Retained `07-tower-shaft-attempt` then shows no Field control. Source-authored three side flights; no editor rewrite. |
| R2 | Staircase authoring and connected-storey landing/rail construction. Geometry offers Room, Wall, Ramp, Recess; floor tools copy/split but do not build connected stairs. | Tower: native upper floors and shaft, source 16-step first flight, two ramps and three landing rooms. Enter all four storeys in Play; no implicit floor cuts or global regions used. |
| R3 | Per-storey overrides and explicit carrier ownership. Floor patches/regions are global over overlapping rooms. | Tower: an ice target patch affected every floor and caused unplanned slide during traversal. Replaced with a dry contrast patch; room materials distinguish floors. Request a storey selector/preview and diagnostic before assuming local ownership. |
| R4 | Height-aware legacy route controls and preview. A route on the witness with authored `y:9.6` rejects its first 2D move as a >.30 m step. | Tower: retained checked route failure; use the supported AI `idle_clip: "walk"` override in place, with an explicit beam-response body. AI owns its cue; the startup timer alone is overwritten by ordinary idling. Navigation AI and legacy routes are different contracts. Do not claim an upper-floor walking route passed. |
| R5 | Physical contact/clearance diagnostics for dormant items, and a useful warning when pickup succeeds but the held item cannot leave contact. | Tower: sphere at exact `y:3.2` is picked up yet its pose remains unchanged; drop/throw is refused. A 5 mm placement clearance also fails. A two-room CPU regression proves the lower ceiling's 12 cm upward backing overlaps the upper floor; the narrow runtime correction clips that redundant flat-ceiling backing against real coplanar floor coverage. No format change or broad editor work. |
| R6 | Direct water creation/configuration and water/flight vertical slice aids. Basin editor text explicitly calls water a separate feature; there is no Water Add entry. | Discharge: native basin/ramp, source `water[]`, buoyancy parameters and exact unheld/settled return box. Editor currently preserves these fields. A continuous replay exposed that the original rectangle omitted the submerged ramp; a separate validated shallow rectangle now covers that ramp. Inspect the vertical slice and walk continuously out of the water. |
| R7 | Outdoor room/sky/weather and directional-illuminator controls. Map capabilities expose spawn/default materials/presentation/fog/Full requirement, not this complete exterior setup. | Intake: native snow floor and props, source open ceiling, existing Winter sky, snow and moon. Reopen Inspect versus authored Environment/Play. |
| R8 | Clear physical-item aim/receipt feedback during authoring. Exact identity and settled-centre filters exist, but the test square is source geometry with no game-visible receipt preview. | Intake/Discharge: contrast floor pad plus bell and NPC/F1 advice. Future editor could preview actual item centre, sleep state and receiver band in Play. |
| R9 | A graphics cache hit for a non-entry map must inspect that map's manifest. The native Low request falsely reported a changed package. | Fixed the one cache-hit call to use existing `inspect_level(path,id)`. The manifest digest check remains intact. Native quality replay covers actual retained state/circuit/shadow behavior; existing tiny two-map archive regressions already protect selected-manifest isolation. |
| R10 | Connected-storey geometry review should distinguish shared ceiling/floor planes and shaft portals from defects. | Tower's read-only checker reports zero errors and 218 warnings: 201 opposite-face pairs on intended shared structural planes, 14 intentionally open connection runs, three single-height flood candidates inside the multi-height shaft. Retained without blanket ignores; native four-floor/edge checks are separate evidence, not an exhaustive closure proof. |
| R11 | Friendly non-entry map titles in the imported menu without loading inactive semantics. | The three native thumbnails display correctly, but non-entry rows use stable IDs (`relay_tower`, `relay_discharge`). Keep entry-based campaign launch; propose explicit per-map display metadata for the later editor/distribution work. |
| R12 | Message editing should preview panel width or wrap authored prose. | All maps: native captures showed long lines ellipsized, including the goal. Source prose was shortened to at most 46 characters per line; no typography/runtime rewrite. Reopen intro/NPC/F1 and compare the fixed-width native panel. |

Ordinary Luau objective logic and NPC conversation writing are intentional authoring,
not missing engine APIs. Project add/switch/transition/thumbnail controls exist;
this campaign's manifest consolidation was an automation choice. Existing Full
lighting, shadow-only toggle, physics details, event and script forms were not
removed or broadened.

Missing or useful sound work: distinct receipt/clearance success, locked-airlock
refusal, staff acknowledgment/voice, floor/wayfinding announcements, swim-entry and
exit splashes, large door pressure/equalization, and safe-respawn cue. Existing bell,
switch, door, impact, footsteps and ambience are reused honestly; no new sound was
synthesized. Useful future engine work includes explicit campaign-state/inventory
transfer policy, upper-storey route ownership, authored item-contact diagnostics,
visible receipt feedback, animation/prop grasp and door synchronization, fixed-light
dynamic shadows, and better wet/ice interaction feedback. These are backlog items,
not an exhaustive promise or authorization for this stage.

<a id="editor-0120-relay-annex-essential-regression-value"></a>
### Essential regression value

One new CPU regression uses two four-metre rooms, one sphere and sixty fixed
steps. It fails before the correction because the held sphere cannot lift, and
passes after. It also checks actual throw/release and collision on the uncovered
ceiling and upper floor. It uniquely protects the shared-storey backing bug in
about 0.01 seconds, without assets, GPU or any map bake. Existing tests are retained.

<a id="editor-0120-relay-annex-validation-and-delivery"></a>
### Validation and delivery

Strict workspace/all-target/all-feature Clippy, release Clippy, formatting and the
locked all-feature workspace release build pass. All 167 Rust tests pass; all 369
Python tooling tests pass with three supported skips. This adds one small CPU
regression; no campaign bake was added to routine CI. Asset validation reports
zero warnings. Package adversarial checks pass all 30 cases, including bounded
archive rejection, source recovery, no overwrite and no partial publication.
The final source/package identity verifies current, and the recovered ten source
files plus verified-cache re-export are exact.

The final 60-assertion native audit passes. A single continuous run reaches the
ending with zero unintended resets, three actual map requests and all four
walked floor heights. Direct native chapter checks cover all three staff conversations, pickup/drop,
the floor-two actual throw, four heights, switch/beam/posed hidden caster, physical
airlocks, both fresh-state transitions, swimming/carrying/dry receipt, relief spawn,
win, Enter reset, whole-campaign restart, F1, quality rebuilds, edge rails and
recovery. Diagnostic placement is limited to the labelled quality/edge/void
checks; the complete route uses camera aims and ordinary keys/mouse only. Actual
SDL and native focus are checked before interpreting results. Both accepted
Closing Time and Beam natural win regressions pass with the delivery executable;
Beam also exercises Enter reset. Their six original source hashes are unchanged.

The replay recipes and failed attempts are retained. In particular, early FIFO
framebuffer video added enough overhead to invalidate two timed routes and its
fixed-rate output sped up time; those clips are not accepted gameplay evidence.
Subsequent continuous replay exposed the missing ramp water rectangle and a
near-edge duck drop. The authored shallow water fix and a deeper ordinary drop
approach were verified before final acceptance. No diagnostic teleport, forced
objective completion or direct Load substitutes for the continuous route.

Unrecorded throughput samples use High/Full, native Metal on Apple M2 Pro,
1280×720 physical drawable (640×360 logical, 2× backing), 1,200 warmup and 1,200
measured frames per pinned scene. V-sync and all limiters are off. Startup logs
first show Fifo, then confirm the applied Immediate presentation mode. The display
is 120 Hz; actual average throughput exceeds refresh and the timing receipts show
sub-refresh frame intervals, so these samples are not refresh-rate plateaus.
Average FPS is **183.0 Intake, 211.4 Tower, 159.9 final Discharge**. These are static
scene samples, not a traversal average or a before/after improvement claim. The
final Discharge sample includes the corrected ramp water. No video, framebuffer
readback or audio capture runs during throughput measurement; asynchronous GPU
timestamps remain enabled. Movie playback cadence is separate from this evidence.

Delivery is local under `local/relay-annex-0.12.0/`: `play.sh` launches the imported
complete campaign; `editor.sh` opens editable source. The package includes all
three prepared maps, scripts and native thumbnails. Preserved direct macOS
executables live in `payload-delivery/`, outside `target`. After confirming no
active target users, one `cargo clean` removed 8.8 GiB. Preserved compiler
validation, focused native editor reopen with byte-identical sources, and focused
player/F1 smoke all pass after cleanup. Installed SDL3 and the
checkout asset catalog remain required. Source and evidence are packaged with the
Library media. The short reel is a real-time WindowServer capture of the native
game window, cropped to remove its title bar, normalized to 720p, cut to real-time
highlights and muxed with the original engine audio. Its static native ending
receives a labelled-in-provenance 1.5-second editorial hold. The full run PCM
capture has a 60-second cap; Discharge clips therefore use the complete accepted
chapter recording and its original audio. The separate six-second editor clip shows a
representative native Winter dressing operation through scripted visible controls.

Exact commits, exact-head GitHub Actions status, Library identities, media timing,
individual checks, final native assertions and post-clean smoke receipts are in
`local/relay-annex-0.12.0/handoff.json`. No main merge, release, branch deletion,
environment-demo rebuild or next-stage editor implementation is part of delivery.
Native evidence is macOS/Metal only. Campaign persistence, upper-storey legacy
routes, the 218 candidate tower geometry warnings, missing cues and R1–R12 remain
explicit next-stage limitations; the campaign is a bounded feature sample.

<a id="editor-0120-shadow-only"></a>
## Shadow-only entities: 0.12.0

This stage adds reusable `shadow_only` on placed entities and spawn templates.
Default false preserves ordinary entities. Hidden meshes retain the exact posed
vertex/index buffers and material coverage in the handheld flashlight depth pass;
all colour, reflection and emission submissions omit them. Static transport,
placeholder fallback and projected contact shadows cannot reveal a hidden body.
No shader transparency trick, billboard, new artwork or format revision is used.
Packages declare and validate `shadow-only-v1` so older hosts refuse the semantics.

The editor exposes the property separately from collision and behavior, with
undo/save/reopen support for placements and shared spawn templates. Inspect shows
an authorable model; Environment preview and Play hide it. Luau can inspect
`api.actor(id).shadow_only`, and `api.spawn` preserves template policy from the
first rendered frame. The existing animation/action lifecycle remains the sole
controller. Collision, AI, interaction, labels, audio and attached lights remain
explicitly separate capabilities.

Supported models use the existing skinned or rigid animated character lane and
its combined 128-character limit. Missing, unsupported or exhausted posed
resources fail closed with a bounded diagnostic; no shadow is promised in that
case. Float/physics/span/material-override/fixed-light-owner combinations are
refused. These entities cast real **handheld flashlight** shadows only, with the
existing cone, 24 m range, opaque occlusion and wall/floor/object receivers.
They cast no baked fixture/global or approximate contact shadow. Independent
fade/suspension still affects coverage; deliberately authored glow/labels/audio
can expose an actor's location and should be omitted when unwanted.

<a id="editor-0120-shadow-only-native-evidence"></a>
### Native evidence

The separate editable `projects/shadow-only-review/` gallery has four placed
hidden mannequins, a scripted hidden spawn and an ordinary visible control.
Actual native editor facilities authored the room, six walls, placements,
fixture and placed property. An explicit advanced source pass added loop routes,
startup/spawn animation actions, interaction/collision, mirror opening and the
clear range lane. Native reopen/toggle/undo/save preserves every final source
byte, including advanced fields. Failed setup attempts remain in the evidence;
they are not counted as acceptance.

The direct macOS/Metal player passes 103 focused evidence assertions: all hidden
actors stay colour-invisible and opaque shadow casters, animation revisions and
silhouette pixels change, ordinary E interaction and independent solid collision
work, Static/Full and Low lighting/quality changes preserve policy, third-person
adds only the player's ordinary visual, reset restores the world and a single
fresh spawn, and reload restores the policy. Every final player and editor
checkpoint records actual SDL input focus. The mirror visibly reflects the
ordinary control; hidden models remain absent.

Matched hidden-present/removed scenes are byte-identical in flashlight-off,
blocked-beam, >24 m and mirror captures (zero changed pixels). The same view with
the beam on differs in 11,902 shadow pixels. These are actual rendered images;
no scene image was synthesized. The final native video is 18 seconds, 1280×720,
540 frames at 30 fps with explicit BT.709 conversion. Its consecutive source-frame
receipt and visual review remain alongside the original captures.

Final review caught and corrected an animation ownership edge: a visibility-only
frame now carries no pose override. A separate tiny native fixture confirms
placed hidden/visible mannequins both use automatic walking with matching
advancing revisions, while uncued spawned hidden/visible mannequins retain their
held pose. This fixture is explicit developer tooling, outside routine CI.

The accepted Beam source and package remain unchanged. Because the correction
touches the shared pose handoff, both its ordinary-win/reset and beam-response
routes were replayed with the final player, separate from this feature's gallery.
No later feature-test campaign, merge, release or branch deletion belongs here.

<a id="editor-0120-shadow-only-scaling-and-checks"></a>
### Scaling and checks

Two reverse-order runs use one ordinary control plus 1/16/64 looping hidden
mannequins, 120 warmup and 1200 measured frames each. The Apple M2 Pro/Metal
surface actually switches from startup Fifo to **Immediate**, with a 1280×720
physical drawable on the 120 Hz display. V-sync and every frame limiter are off;
no recording/readback pacing runs during measurement. The render camera is
pinned from the first ready frame. The 1/16-actor runs exceed the 120 Hz cadence;
the 64-actor runs plateau near 120 Hz despite Immediate. Uncapped throughput at
64 therefore remains **inconclusive**: only CPU/GPU costs are reported, with no
average-FPS or improvement claim. Its CPU update/pose work is 7.41–7.50 ms and
mean present time is 0.02 ms; the raw intervals and earlier camera-setup trials
remain retained.


| Hidden actors | Frame cadence median, two runs | GPU pass-union mean |
| --- | --- | --- |
| 1 | 1.56–1.62 ms | 1.32–1.42 ms |
| 16 | 2.96–2.97 ms | 1.75–1.79 ms |
| 64 | 8.47–8.58 ms | 1.92–1.94 ms |

Strict all-target/all-feature Clippy, release Clippy, locked all-feature release
build, formatting, all 166 small Rust regressions and the 369 Python source
checks (3 supported local skips) pass. Four focused regressions cover entity/reset
and first-spawn handoff, missing-model fallback leakage, older-host capability
admission, and editor collision-independent undo/reopen. No routine test or CI
step bakes the gallery. Asset validation has zero warnings. Independent focused
renderer review found no blocking pass-routing or first-spawn leak.

Preserved direct player/editor/compiler executables, package, editable source,
inputs, measured traces and machine-readable receipts live outside `target` in
`local/shadow-only-0.12.0/`. Runnable commands are in the gallery README. Final
commit/CI, Library identity, cleanup and preserved-binary smoke outcomes are
recorded in that directory's `handoff.json`. Native certification is macOS only;
Windows/Linux and later queue work remain separate.

<a id="editor-0120-stage7-handoff"></a>
## Pre-export safety design and checkpoint

The stage 6 → 7 handoff required complete project membership, exact Luau/thumbnail
snapshots, bounded archive validation, cancellable atomic publication and safe new
folder recovery. Those requirements were implemented in the stage 7 record below;
the later combined safety pass used both finished modes and malformed fixtures.
The historical queue restriction did not establish completed broad distribution.

Release foreign-unwind evidence remains at
`local/verification/0.12.0-task6/native/runtime-errors-final/` and
`checks/rust-gate-final.log`. The design boundary remains substantive: VM deadlines
are cooperative; bounded native calls can briefly exceed them; parser/compiler
temporary allocations are outside the VM allocator; and the sandbox runs in the
same process. Tiny refusal/reset/rollback fixtures do not certify every finished
mode or every multi-level archive. The final safety record retains the actual
production-binary malformed path, size, decompression, image/source and publication
checks. No claim of process isolation follows from passing those checks.

<a id="editor-0120-task3"></a>
## Basic editor: 0.12.0 task3

This completes the authorized basic-editor stage on `0.12.0-staging`, following
project-contract stage1 `af94e3f` and installation/path stage2 `169748c`.
No main merge, release, branch deletion or later editor stage is included.

`places-editor` is a separate SDL/wgpu application using the shared Places engine,
renderer, asset catalog, geometry and project/path contracts. See
[controls and launch options](../EDITOR.md). It creates/opens projects, selects
real 3D geometry, edits floor/wall footprints and built-in passive props, and
provides position/dimension/yaw/scale entry, snapping, duplication/deletion,
per-level undo/redo, protected save, structured build progress and direct play.

<a id="editor-0120-task3-implementation-boundaries"></a>
### Implementation boundaries

- `src/editor/document/`: authoritative raw JSON, typed validation, independent
  per-level history/savepoints, confined atomic saves and external-change checks.
  Unknown fields survive editing; ambiguous duplicate keys or numeric data that
  cannot be preserved exactly are refused. The manifest is never reserialized.
- `src/editor/viewport/`: camera, picking, selection/grid overlay and coalesced
  background preview preparation. Shared render code supplies an unlit ceiling
  cutaway preview; original scene lighting and ceilings remain in the document.
- `src/editor/{ui,input,actions}.rs`: controls, modal dirty-state protection and
  application actions, using the existing engine font/renderer infrastructure.
- `src/editor/process/` and `src/map_compiler/events.rs`: bounded subprocess
  capture, protocol-v1 NDJSON progress/results, cooperative cancellation with
  kill/reap fallback, and isolated per-package/level player profiles. Save/build
  and project switches cannot race an active build; only one child job runs.
- `src/editor/verification.rs`: explicit bounded developer scripts through the
  normal native event handlers, framebuffer captures and document checkpoints.
  This has no effect during ordinary editing and does not bake maps in tests.

The player now resolves an exact requested level ID before a display-name
fallback, so a different map with the same display name cannot intercept play.

<a id="editor-0120-task3-verification"></a>
### Verification

All production checks passed locally: formatting; strict workspace/all-target/
all-feature Clippy in debug and release; locked release workspace build; all
80 Rust tests; 368 Python tests (16 existing skips); asset validation (593 assets,
370 placeable, eight themes, zero warnings). Six compact editor tests cover raw
data/history/save failure and subprocess protocol/result/ownership regressions.
They perform no render, asset discovery or full-map bake. Existing production
tests remain intact. Local `RUSTC_WRAPPER=` avoids the host's sandboxed sccache;
no repository build settings were relaxed.

Native acceptance used the direct release executables from `/tmp`, with no
`PLACES_ASSET_ROOT` override and no change of working directory. Copies of all
three verified tools and a repository-assets symlink are retained outside
`target/`, alongside one reusable six-metre project and all evidence under
`local/verification/0.12.0-task3/`. `try-editor.sh` opens that project. The payload
requires this checkout's assets and the host SDL3 installation.

The editor rendered on Apple M2 Pro / Metal at physical 1440×816, Fifo. Its
launched game rendered windowed at physical 1920×1088. These are functional
checks, with no FPS/throughput comparison. Desktop automation could not address
the unbundled raw executable, so explicit native verification scripts drove
ordinary SDL event handlers and captured the actual GPU framebuffer. No packaged
application or OS security workaround was used.

| Native exercise | Evidence/result |
| --- | --- |
| New project, floor, four walls, built-in chair | `native/edit/`, correct catalog collision size/solid metadata |
| Pick, snapped XYZ/yaw/uniform scale, undo/redo | `edit-assertions.json`, exact scene restoration and visible selected bounds |
| Orbit/fly, release, frame | Camera checkpoints show movement and no release drift |
| Unsaved close → Cancel, then save/close/reopen | Raw document equals saved source and reopened document; clean savepoint |
| Unknown nested fields through edit/save/build/play | Original unknown data retained exactly in source/checkpoints |
| Save/build with visible pass progress → play → editor | `workflow-assertions.json`; selected ID loaded, actual room/chair visible |
| Wall/floor collision and jump | `collision-assertions.json`, player stops at Z=0.55, eye Y=1.60 on floor and reaches 2.4999 during jump without crossing wall |
| Cancel changed-scene bake, then rebuild | Cancelled result, previous package SHA unchanged, later success publishes changed package |
| Real compiler output failure, retry, repeat play | Task-owned output directory temporarily read-only; failure retained package SHA; restored permissions, retry and second play returned successfully |
| Repeated open/build/play | Three editor launches and two player sessions completed without a freeze; owned children reaped |

Screenshots and the short demo are retained outside `target/`. The demo combines
actual captured editor checkpoints with native game footage; it is an edited
demonstration rather than a continuous desktop recording. The chat copy is below
9 MB. Visual inspection prompted a steeper starting camera and a quieter grid;
the final screenshot shows the room interior, selected prop and readable controls.

<a id="editor-0120-task3-remaining-scope"></a>
### Remaining scope

Windows/Linux native acceptance and final distribution signoff remain deferred.
The preview is static, with authored light/gameplay visible through Play. Rooms
and walls remain axis-aligned; unsupported destructive transformations are
refused. Rich light/trigger/fog/door authoring, advanced floors/pools/ramps,
diagnostics/auto-fixes, Luau, custom asset importers, complete multi-level bundle
export and a bundled menu scene are later stages. Bundled-level ID conflicts use
the existing import rejection; new projects generate distinct IDs.

Broad editor issue #18 and installation/export issue #21 remain open because
their complete requested scope extends beyond this stage.

<a id="editor-0120-task4"></a>
## Geometry and safety tools: 0.12.0 task4

Stage 4 extends stage 3 (`816ef4e`) on `0.12.0-staging`. It adds geometry
creation/editing and bounded safety review to the existing Rust editor, shared
engine and project source contract. No trigger/Luau/menu/distribution stage,
asset remake/adoption, main merge or release is included.

<a id="editor-0120-task4-authoring-and-interface"></a>
### Authoring and interface

`+ Geometry` consolidates floor, wall, ramp and recess creation. The selection
inspector exposes only meaningful scalar fields: a signed ramp rise and
floor-relative offset, one uniform prop scale, or a region offset without a
fictional thickness. Ramp headings identify the ascending axis. Floor `Shape`
provides a snapped world-space rectangular opening and an upper-floor operation.
Detailed explanations precede the explicit mutation. Existing controls were
regrouped rather than supplemented with a permanent feature panel. Build cancel
and Stop Play share one transient slot; Safety occupies the review step in the
file/history/build/review/view workflow.

Openings split an ordinary room into up to four disjoint raw-record copies,
removing that room's floor **and ceiling** inside the chosen rectangle. They do
not invent a floor-portal schema. Upper floors copy an ordinary footprint one
room height above; the lower ceiling remains. Gable transformations, occupied
upper space, ambiguous stacked walking modifiers and unsupported square-ramp
quarter turns are explicitly refused. All accepted geometry operations preserve
unknown data and use the existing atomic validated document/history/save path.

A native pool fixture exposed a preexisting renderer/collision disagreement:
a negative ramp could walk below an unchanged flat carrier floor. The shared
floor grid now cuts at ramp edges and omits carrier cells beneath the slope;
region skirts defer to actual ramp side/end profiles. Higher neighboring deck
faces point into the recessed space, and removed carrier planes no longer make
invisible source-lighting lids. Geometry revision **2** invalidates prepared
geometry and compiler fingerprints. Source/package layouts are unchanged.

<a id="editor-0120-task4-diagnostics-and-corrections"></a>
### Diagnostics and corrections

The on-demand worker prepares a clone through the compiler's ordinary source
preparation, then reuses its geometry checker. Its optional JSON `surfaces`
field retains measured triangle pairs without parsing human diagnostic text;
suppressed warnings cannot attach their triangles to a previous finding.
Additional bounded samples use the real player radius, step, crouched height
and jump apex to flag exposed ramp sides and floor edges. Basin-depth and global
multi-storey-region warnings explain the actual source semantics. They distinguish
a measured geometry problem from a possible jump/drop path or collision setting.

Static models are loaded from the installed GLBs and transformed through the
engine's placement matrix. Bounds reject candidate pairs; parallel-plane
separation and clipped positive triangle overlap supply actual coplanarity
measurement. Opposing single-sided contact is excluded. Opaque conflicts and
cutout/blended material/depth uncertainties have distinct labels. Catalog-envelope
layering estimates retain their heuristic wording. Intentional decals use their
separate depth-biased pass. Non-coplanar artistic intersections are not
reinterpreted as z-fighting or repaired.

Bounds and actual measured triangles locate findings. View in viewport selects
and frames the location; reopening review retains its cursor. Scope/budget notes
preserve the camera. The only automatic correction removes a raw-identical
simple wall with recognized scalar fields; metadata, openings, per-face data
and every prop disqualify deletion. A snapshot-checked candidate renders without
changing source/history. A compact review bar leaves camera navigation available.
Cancel/Close/Esc restores the original view. Apply creates one undo step and
refreshes source findings; undo/redo after a review invalidates and refreshes
findings. No broad fill, prop motion, depth offset or bulk repair is offered.

Analysis is deliberately bounded: 128 rooms, 512 props, 4096 geometry objects,
a 600k estimated-vertex ceiling and a 32m room-cell/collider-work estimate;
model storage is capped at 200k triangles and broad-phase work at 2m pair visits.
Missing models and exhausted budgets are visible incomplete-review findings.
The ordinary engine/source limits are not reduced. Closing review hides the
panel; the bounded job may finish in the background.

<a id="editor-0120-task4-verification-and-evidence"></a>
### Verification and evidence

Production checks: formatting and strict workspace/all-target/all-feature Clippy;
locked release workspace/all-feature build; **91 Rust tests**. Eleven added small
regressions cover document transforms/raw round trips, compound opening history,
failed/stale correction safety, actual terrain picking, jumpable boundaries,
static triangle discrimination, audit budgets, shared ramp mesh/collision/visibility,
and diagnostic surface/suppression identity and partially supported ramp ownership. They run without a map bake, renderer
or asset discovery. The full test body ran in under 0.2 seconds after compilation.

The existing Python source-contract suite passed 368 tests (three optional skips).

Asset validation passed: 593 assets, 370 placeable, eight themes, zero catalog
warnings. Read-only texture and prop checks passed; the texture writer reports
96 existing preferred-size/manifest warnings. No PNG, GLB or catalog was changed.

Native direct-binary acceptance is retained under
`local/verification/0.12.0-task4/`, outside `target/`. It uses ordinary SDL editor
handlers and actual GPU framebuffer captures, plus direct compiler/import/player
execution from `/tmp`. It never runs a packaged `.app`. The final handoff JSON
contains executable hashes, assertion paths and the exact commit/CI identity.

- `native/final/editor-create/`: New, ramp/recess creation, numeric snapping,
  transform undo/redo, save and native captures.
- `native/final/editor-opening/`: exact reopen, unknown-field retention, invalid-cut
  refusal without mutation, custom cut, undo/redo and 960×544 layout/review.
- `native/final/editor-safety/`: correction preview/cancel/apply/undo/redo, actual
  candidate viewport object count, refreshed findings, compile and direct play.
- `native/player-final/`: tiny ramp-side escape, recessed basin/ramp exit,
  multi-floor shaft/opening and intentional prop/surface cases; package
  validation/currentness, player coordinates and actual framebuffer evidence.
- `native/final/editor-assertions.json` and `native/player-final/native-summary.json`: acceptance
  results. `try-editor.sh` opens the retained project using the copied payload.

Final direct-binary runs passed all editor assertions and 62 player assertions.
The correction preview kept eight source walls while drawing seven; Cancel
restored the original mesh. Apply changed only one wall (8→7), reducing 22 review
entries to four (both counts include the scope note). Undo restored 22 and redo
restored four. Props, intentional decals and opaque unknown data remained exact.
The repaired Full/Static package validated and verified current, then imported
and played through the editor before returning to the saved document.

Native ramp contact stopped at X≈5.45 with collision enabled; the elevated eye
was already above the 2 m boundary. Jumping cleared that deliberately low wall
and reached beyond X=6 before ordinary void recovery. This is demonstrated
geometry/containment risk, not evidence of an engine response defect disguised
with an added wall. The pool rim stopped ordinary climbing, its descending ramp
provided an exit, and the split upper floor supported an eye Y≈5.2 before a real
fall through the opening to Y≈1.6. Review captures show the corresponding ramp
warning and measured model triangles; intentional table/chair contact remains
manual review.

The editor rendered on Apple M2 Pro / Metal at 1440×816, Fifo, and at the minimum
960×544. The standalone player checks used actual physical 1920×1280 and observed
Immediate presentation after disabling V-sync. The editor-launched player reported
physical 3024×1676. These are functional checks; no FPS/throughput comparison or
average-FPS claim is made. `cargo clean` then removed 8742 files / 7.8 GiB after a process check found no
other target users. The copied editor reopened the exact saved pool source and
rendered successfully from `/tmp` with target absent and unchanged binary hashes.
The retained payload requires this checkout's installed
assets and host SDL3; it is not clean-install or cross-platform signoff.

<a id="editor-0120-task4-limits-and-next-stage-handoff"></a>
### Limits and next-stage handoff

The review is not exhaustive watertightness, no-clip, route-reachability or rendered
z-fighting detection. Edge samples do not simulate every trajectory, ceiling,
water or ladder interaction; static authored poses omit motion and alpha-pixel
coverage. Model comparisons cover distinct instances and architecture; internal
self-overlap within one GLB remains asset-tool work. Camera/render-state depth failures need visual investigation and are
not silently “fixed” with geometry. Source-light visibility has no analytic ramp
slope plane; Full transport uses the actual emitted mesh. Top-surface picking
covers fully supported ramps/regions; their side faces and protruding unsupported
ramp footprints can require the scene list. Existing valid overlapping ramps
retain last-authored support precedence and can still render overlapping slopes. Partially supported ramps retain their
rendered top via an overlapping carrier and receive a corner-support warning.

Connect floors using explicit shaft footprints, matching cuts, landings and
carrier floors. Global ramps/regions are not attached to a chosen storey. The
editor preserves other authored collections; it does not yet offer rich gameplay,
water/light/fog/door/trigger controls. Windows/Linux acceptance and distribution
remain later gates. Broad enhancement #18 stays open.

Stage 5 should retain the document's raw-authoritative contract, per-level
history, bounded workers and shared engine/compiler boundaries. Before **every**
new button/control, reassess the entire relevant layout and workflow; move,
regroup, consolidate or redesign existing controls when clearer. Keep the viewport
central, details contextual, labels predictable and core actions discoverable.
Do not layer new permanent panels or append controls into spare space. Include
actual normal/minimum-window visual review with functionality checks.

Stage 5 also owns the new mounting request: placement must use the actual
mounting anchor/bounds and surface normal, not just origin grid snapping.
Ceiling fixtures should sit flush to real ceilings with correct orientation;
wall fixtures attach to walls and floor props remain grounded. Preserve a small
intentional mounting clearance without intersection/depth flicker or visible
floating, account for actual model transforms, and support manual adjustment and
undo. Distinguish the mounting face from the luminous aperture. Validate actual
light models in both the native editor and compiled play. This is handed off;
no new mounting UI or placement behavior is added in stage 4.

Stage 5 must also preserve deliberate embedded/rotated scenery, such as a desk
on its side halfway through the floor. Surface snapping must be optional or
bypassable, with manual adjustment and undo; intentional solid intersections
alone must not force correction or prevent export. Verify the fixed, physics-off
desk through save/reopen/compile/play without snapping it out. Distinguish solid
intersection from actual coplanarity and collision/void defects. The present
`PropDef` contract has yaw only (plus uniform scale/floor-relative Y); arbitrary
pitch/roll and the full sideways-desk acceptance are therefore explicit stage 5
work, not a claimed stage 4 capability. Do not introduce physics work for this.

Persistent placement-specific warning dismissal is **not implemented in stage 4**.
The user's late addition is explicitly deferred to stage 5, where placement
identity/transforms and contextual placement controls are being extended. Existing
coordinate-scoped `geometry_intent` annotations are not a substitute. Stage 5
must provide `Ignore for this placement`, persisted by stable instance identity
and issue-specific affected-geometry/transform context. Significant changes must
invalidate or re-evaluate it; other desks and unrelated warnings on the same
item remain visible. Include clear-ignore/review-ignored state, undo and raw-source
preservation. Loader/source validation for malformed or unsafe data stays active.

Every individual or future bulk/fix-all correction must honor that persisted
intent. Never change a protected placement to repair its ignored issue; unrelated
corrections that alter it require explicit review. Test unchanged ignored desk
placement under individual and bulk corrections, then verify clear-ignore restores
eligibility. Also test save/reopen persistence, distinct desk instances, unrelated
same-item findings, duplicate identities and meaningful geometry/placement edits.
Use contextual review UI, not additional permanent controls. Stage 4's sole
correction removes an identical simple wall; it has no prop-moving or bulk repair
path. The full dismissal/protection acceptance remains incomplete until stage 5.

Asset integration waits until all authorized editor stages 4–7 are complete.
Cloud Blender work may continue independently; this stage does not touch any
remakes. The newer Spoonerman candidate was rejected and must not be integrated.
Menu/distribution stages 8–9 remain unauthorized. After later accepted model
integration, rerun the tiny model/surface/native fixtures because measured
coplanarity evidence is bound to the installed assets. Preserve this stage's
direct binaries/evidence outside target before cleaning build output.

<a id="editor-0120-task5"></a>
## Stage 5: gameplay and environment authoring

Implemented on `0.12.0-staging`, starting at `69363d0`. Stage 4 CI
[39074519428](https://github.com/csd113/Places/actions/runs/39074519428) completed
successfully. No shipped map, model, texture, sound or catalog asset changed.
Stages 6–7 remain the next sequential work. Model integration, the rejected newer
Spoonerman, menus/distribution and the future Closing Time mode are outside this
change.

<a id="editor-0120-task5-authoring-workflow"></a>
### Authoring workflow

The existing central viewport and compact scene/transform panel remain. One
grouped creation sheet adds built-in lights, ambient/positional sound emitters,
entities, doors, triggers, box fog, ground fog and spawn points. Selection-specific
configuration sheets expose capabilities without adding permanent panels.
Normal 1440×816 and minimum 960×544 native layouts were visually reviewed.
The trigger action list and light/sound/door/spawn sheets were inspected as part
of real native editing, rather than only after implementation.

Lights configure power, range, colour, face emission, switching and mounting.
Sound emitters configure installed cues, gain, ambient/positional policy,
looping and play-on-load. Props toggle physics per placement, with pickup, mass,
solidity and activation groups. Entity/spawn templates offer existing shared
idle/wander/follow/flee behaviours. Doors default closed; two-way opening chooses
the side away from the occupant and retains it during interrupted motion.

Triggers resize through the ordinary position/dimension inspector and display
translucent authoring-only bounds. Filters select the player, general or held/
unheld physical items, exact items and exact entity identities. Ordered actions
play a configured sound, open a door, spawn at a movable point or activate a
physics group. Spawn points select reusable built-in templates and bounded
lifetimes; copies receive independent one-active-spawn groups. The engine owns
all behaviour. Existing additional bindings and unsupported raw fields survive
ordinary editing.

Native testing caught and corrected configure-before-actions failure: an empty
trigger now stores validated `editor_trigger_policy` metadata until its first
action materializes a valid runtime binding. Removing the last action restores
the draft. Runtime empty-action validation remains strict. New decorative
entities are passive by default so physics can be enabled without an unrelated
interaction component preventing the edit.

<a id="editor-0120-task5-placement-and-protected-intent"></a>
### Placement and protected intent

Props support yaw, pitch and roll through the shared placement matrix, renderer,
prepared geometry, picking, conservative collision/navigation envelopes,
attached point offsets and full rigid-body orientation. Physics remains frozen
on load until interaction or activation. Negative floor-relative Y and solid
intersections remain authorable; no transform-time correction pulls scenery out.
Geometry revision **3** requires recompiling prior prepared output.

Explicit mounting measures actual model base/top/back bounds, aligns floor props
to continuous floor/ramp normals and wall fixtures to the outward wall normal,
and uses a 1 mm clearance. Ceiling mounting accounts for the housing back rather
than the luminous aperture. Snap is optional; manual adjustment and undo remain.
Native mounted values in the 3.2 m acceptance room were 3.199 m for the Office
panel, 3.196 m for the Pool disc (whose local top is +0.003 m), and X=0.251 m for
the wall model against the X=0.25 m interior face, yaw 90°.

`Ignore for this placement` binds one warning to stable raw instance identities,
the measured issue and affected source context. Save/reopen, review/clear and
undo work; meaningful transform/support/geometry changes reactivate the warning.
Other findings on the same placement remain independent. Duplicate/malformed
identities and malformed metadata are rejected, and runtime/source semantic
validation stays active.

Individual preview/apply and the shared batch correction gateway skip ignored
issues and protect their placement/support context. An unrelated correction
that would affect protected context is refused for explicit review. The native
case kept the ignored rotated desks byte-for-byte while removing an unrelated
duplicate wall; clear/undo restored the same active decision. Focused batch
regressions prove protected preservation and clear-ignore eligibility. There is
no new bulk-fix UI.

The fog round-trip regression also exposed the default JSON parser rounding the
last digit of the editor's own saved floats. Enabling `serde_json`'s precise
`float_roundtrip` parser fixes rereading generated coordinates/colours/bounds;
duplicate-key and unsupported-precision safeguards remain enabled.

<a id="editor-0120-task5-shared-runtime-and-bounds"></a>
### Shared runtime and bounds

`engine::entities` retains occupancy, events, actions, audio and spawn lifecycle.
`sound_emitters[]` supplies sound-only entities with absolute world XYZ and a
per-emitter spatial override. `TriggerFilterDef::Entity` watches a placed identity
or explicit authored spawn name; generation-safe membership handles replacement.
Physical actors retain the swept solver path/AABB; other actors use live base
points. `dispatch_actions_from` separates the action owner from occupant origin
for reusable door/action dispatch. Door action counts now propagate through
queued events and sequences as well as the actual state change.

A load-time balanced AABB tree queries swept actors. Previously occupied volumes
remain candidates for exits, disappearance and queue retries. Item bitmasks avoid
all-item scans per volume; exact identities resolve once per watched identity.
Bindings are indexed by owner and only active cooldowns tick. Source caps remain
4000 volumes, 128 physical items and 16 bindings per owner. Existing event/chain
budgets remain. `trigger_query_stats()` exposes exact diagnostic counts.

Regional and separate ground fog share the existing bounded 16-volume uniform
array and quality prefixes 2/8/16. Eye-to-surface rays clip against all six box
faces and integrate four fixed profile samples; walls beyond a volume receive
its haze and lower storeys do not inherit upper-floor fog. Ground layers fade
upward independently. The strongest regional optical depth wins, plus global
atmosphere. No extra rendering pass or per-frame CPU map scan was introduced.
The editor toggles neutral inspection versus authored vertex lighting/atmosphere;
Play uses the compiled Full lighting and complete ceilings.

<a id="editor-0120-task5-validation-and-native-evidence"></a>
### Validation and native evidence

All final gates passed:

- `cargo fmt --all --check` and `git diff --check`.
- Strict workspace/all-target/all-feature Clippy.
- Locked release workspace/all-feature build of production executables.
- `cargo test --workspace`: **109 passed**, zero failures; fixture bodies total
  0.20 s, with no map bake, GPU or full shipped-map dependency.
- Python source suite passed: **368 tests**, three optional skips.
- Asset catalog: 593 assets, 370 placeable, eight themes, zero warnings.
  Read-only texture and prop checks passed; texture checking retains 96 existing
  preferred-size/manifest warnings.
- Full combined WGSL parsed and passed Naga semantic validation, followed by
  actual Metal rendering in the native acceptance.

Native evidence and the copied direct executables are under
`local/verification/0.12.0-task5/`, outside `target/`. They require this checkout's
installed assets and host SDL3; they are not clean-install distribution signoff.
No `.app` was launched.

- `native/editor-acceptance/`: successful 196-step native editor sequence,
  creation/configuration, physics on/off, pending trigger policy and three action
  kinds, resizing, both fog controls, preview, full rotation undo/redo, exact
  mounts, save and framebuffer captures.
- `native/editor-reopen-play/`: exact saved reopen, ordinary Save+Build+Play,
  Full/Static package, direct import/player execution and unchanged return.
  Package validation passed and `verify --require-current` reported identical
  fingerprints. The Full player retained the desk at yaw 15°, pitch 20°, roll
  90° and Y≈−0.2 m without physics or placement correction.
- `native/ignore-acceptance/`: persisted specific ignore, unrelated same-item
  warning still visible, clear/undo, preview/apply of another wall correction and
  saved exact protected desk records.
- `native/gameplay-final/`: opposite door swings, player-trigger sound/door/spawn/
  physics activation, exact entity/item-filtered sound actions, frozen-to-active
  item transition, ambient/positional voices and captured nonzero mixer PCM.
- `native/bathroom-final/`: the existing Arcade restroom entry leaf and local
  source geometry, closed→+90°→closed→−90°→closed. The shipped Arcade source was
  not redesigned or modified.
- `native/ground-fog/`: matched on/off low-eye captures show a distinct ground
  layer; 497,851 pixels changed by more than five channel values. Box fog beyond
  its volume is visible in the gameplay captures.
- `native/assertions.json`: **41/41 passed**. Delivery PNGs and launch script are
  listed in `delivery-manifest.json` and `try-editor.sh`.

After confirming no active Cargo/Rust/player processes, one successful
`cargo clean` removed 12,406 files / 11.8 GiB. The preserved editor then reopened
the exact saved source with `target/` absent, a clean document and unchanged
payload hashes (`native/post-clean-assertions.json`). The host's sccache wrapper
needed the same `RUSTC_WRAPPER=` override used for build gates; the initial
cleanup attempt stopped before removing anything.

Earlier diagnostic runs are retained with their logs, including the workflow
bugs above and two corrected scene-list/modal assumptions in the acceptance
script. The named final evidence is the passing rerun.

<a id="editor-0120-task5-performance-evidence-and-limits"></a>
### Performance evidence and limits

Matched sequential native direct-binary runs used Apple M2 Pro / Metal,
**observed Immediate**, physical **1920×1280**, V-sync off and no frame limiter.
Each discarded 600 warmup frames and measured 6000 frames. No compilation or
other game window ran concurrently. Cadence exceeded display refresh; presentation
averaged 0.019–0.037 ms, with the stress cost in render/GPU backpressure. These are
single-run observations, not universal performance claims. Only average FPS is
reported:

| Case | Stage 4 | Stage 5 |
| --- | ---: | ---: |
| Six-metre room, 16 triggers, one fog region | 809.93 | 661.84 |
| Same room, 16 fully overlapping fog regions | 557.31 | 102.28 |

The correct sightline-integrated fog adds measurable GPU cost versus the old
surface-only approximation: −18.3% in the representative case and −81.6% in the
deliberate maximum-overlap stress. This is a known cost, not a claimed speedup.
Avoid unnecessary overlapping fog; quality prefixes bound work. Worst-case fog
is 16×4 profile samples per fragment and remains expensive in heavier scenes.

The explicit no-bake CPU tool measured 4000 trigger boxes: empty sparse query
0.665 µs average (zero sampled volumes), 128 sparse items 19.055 µs (128 volumes/
128 sweeps), and the deliberately pathological all-overlap case 38.065 ms
(4000 volumes/512,000 sweeps). Relevant active sets still cost work; the source
ceiling is not a promise of 60 FPS for every all-overlap construction. Raw
results, source and counters are retained beside `performance/summary.json`.

Mounting refuses gabled/open fixture ceilings, incompatible fixture families and
footprints crossing discontinuous support, without moving the object. Fixtures
retain yaw-only mounting; fitted spans/floats retain their own upright poses.
Tilted finite-area prop lights retain horizontal footprints. Full transport uses
actual prop triangles; Static/legacy player envelopes remain conservative.
Unknown-size legacy props and unsupported geometry can invalidate ignores more
broadly than necessary. Existing bounded safety-review limitations still apply.
Native Windows/Linux and distribution acceptance are not claimed.

<a id="editor-0120-task5-stage-6-handoff"></a>
### Stage 6 handoff

Continue with Luau, then stage 7 export/import. Reuse `EntityWorld`,
`ActionDef`, `dispatch_actions_from`, exact identity filters, generation-safe
audio/spawn lifecycle and the bounded trigger/event machinery. Keep runtime
logic outside editor UI. Preserve raw document history, stable placement intent,
atomic save and guarded correction gateways. Script execution must not bypass
source/asset validation or silently alter protected placement intent.

The final commit, payload hashes, launch paths, CI identity and cleanup evidence
are recorded in `local/verification/0.12.0-task5/stage5-handoff.json`. Model
integration remains deferred until all stages 4–7 finish. The later Closing Time
mode is context for reusable authoring only; it has not been built here.

<a id="editor-0120-task6"></a>
## Stage 6: bounded Luau and native editor integration

Implemented on `0.12.0-staging`, starting from stage 5 commit `34903df`.
[Stage 5 CI](https://github.com/csd113/Places/actions/runs/39077053443) completed
successfully. No maintained map, model, texture, sound or catalog asset changed.
Main, release creation and branch deletion remain outside this work.

<a id="editor-0120-task6-result-and-architecture"></a>
### Result and architecture

Authors can create/open Luau modules in the native editor, edit multiline source,
save/reopen it, check errors, link exported handlers to ordinary trigger events,
and build/import/play the result. Scripts use the shared engine action dispatcher
for doors, switchable lights, configured sound emitters, physics groups, spawns
and declared entity state. There is no game-specific engine routine or duplicate
physics/audio/trigger implementation. The future Closing Time mode was not built.

`engine::scripting` uses statically built `mlua` 0.12.2 / Luau 0.740 without JIT.
Native hostile-script acceptance exposed that the earlier `panic = "abort"`
release profile aborted when Luau unwound an ordinary budget error through Rust.
The production profile now uses `panic = "unwind"`, incompatible abort builds
fail compilation, and the Rust build gate runs three tiny refusal cases through
the actual release compiler. This checks the FFI boundary that Cargo’s unwind
unit-test harness cannot distinguish; it never bakes a map. See the upstream
[Rust FFI unwinding contract](https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding).

Plain module exports provide optional first-playing-tick `start`, playing-frame
`update`, and explicitly linked named handlers. Binding event kind/key/actor,
subject, delta and available occupant origin cross as read-only values. Two-way
doors keep the shared away-facing origin behavior. Typed Rust host entry points
validate exact boolean/string/value types, existing entities/capabilities,
authored groups/points and declared same-type state slots.

Each loaded map owns its VMs. Reset/replay recreates closures/globals, diagnostics
and disabled status; changing maps drops the previous runtime. Scoped API
callbacks expire after every invocation. Host actions are staged in order and
published only after a successful call. A failed module is disabled for that run;
other modules continue. Existing event depth, occupancy, spawn, render and audio
limits remain authoritative. Maps without modules do not clone events for scripts.

Project resolution admits only confined `scripts/*.luau` references. Selected
project/manifest-level/raw-level references are snapshotted into semantics and
exact source bytes participate in build identity. Edits to Luau source files reuse
prepared geometry/lighting and refresh the package semantics. The compiler checks bounded
initialization and named links before baking. The player opens no script paths.
`luau-v1` is a required package capability, preventing older clients from silently
omitting automatic script behavior. Authoring inline snapshots are refused.

<a id="editor-0120-task6-safety-and-authoring-behavior"></a>
### Safety and authoring behavior

The host exposes no filesystem, network, OS, FFI, arbitrary native calls, module
loading, dynamic source/bytecode loading, debug library, coroutines or protected
calls. Luau freezes builtin tables/metatables; export tables cannot have a
metatable and automatic lookup uses raw reads. Pattern/substring search functions
that can spend expensive native work outside VM interrupts are omitted.

Limits are 16 modules/map, 64 KiB/source, 512 KiB total source and 2 MiB VM heap
per module. Initialization has a 10 ms / 20,000-interrupt budget; handlers have
2 ms / 20,000 interrupts, 32 staged actions and four notices. One frame admits
64 invocations, 128 published script actions and 8 ms cumulative invocation time.
Diagnostics retain at most 32 messages of 512 printable characters per run.
These are cooperative wall-time checks, not a hard realtime scheduler. Bounded
native library calls and compiler temporary allocations have the limits described
in [SCRIPTING.md](../SCRIPTING.md); no stronger process-isolation claim is made.

The toolbar uses the existing transient build-control space for Scripts. A focused
source sheet groups edit/save/check/link/diagnostics; trigger Actions includes its
handler entry point and removal. Export choices are paged. Text buffers have
bounded independent undo/redo and join existing unsaved/conflict/save guards.
Check leaves invalid drafts editable and saveable. Source files publish atomically;
a multi-file save can leave a successfully saved subset after an I/O failure.
The manifest and unrelated raw metadata stay intact.

Stage 5 mounting snap, arbitrary prop rotations, persistent per-placement ignores
and correction protection remain in the existing document/runtime systems. Their
existing regressions remain enabled in the full test gate. No auto-fix gateway or
placement-intent rule was changed.

<a id="editor-0120-task6-validation-and-evidence"></a>
### Validation and evidence

The final working implementation passes formatting, strict all-target/all-feature
Clippy, the locked all-feature production build and **118 Rust tests**. Nine added
owner-component regressions use tiny metadata/file fixtures: valid event/action
wiring, sandbox/resource failures and atomic refusal, stale API/reset/reload,
automatic lifecycle, frame budgets, handler/source contracts, script edit/save/
reopen/conflicts and script-byte build invalidation. No test bakes a shipped map.
The retained Python source suite passes **368 tests** (three optional/native skips).
The catalog check reports 593 assets, 370 placeable, eight themes and zero warnings;
prop checks pass. Texture checks retain 96 pre-existing warnings.

Native macOS/Metal acceptance uses the direct editor/compiler/player executables,
never an `.app` wrapper. A six-metre room with built-in assets is the explicit
fixture. Actual source and handler sheets were inspected at 1440×816 and 960×544.
Native event handlers perform create/edit/undo/redo/save/reopen/check/link/build/
import/play and return to editing. The script opens the existing door, switches
a prepared light layer off, starts ambience, activates physics and spawns through
the existing runtime. Reset restores the door/light and the next trigger reports
`entered 1`; repeated map loads do the same with fresh closures/diagnostics.

Independent runtime modules intentionally exhaust execution, memory and action
budgets or supply an invalid boolean. Their staged door changes are discarded,
bounded errors remain visible and the working module continues. A native syntax
error and failed build retain source buffers and the last package hash; correction
and the next build/play recover. The 55 native assertions also verify that oversized select-all clipboard paste
leaves the original source and clean savepoint intact. The hashes identify the
actual completed cases rather than treating screenshots as exhaustive proof.

Evidence is retained under `local/verification/0.12.0-task6/`:

- `checks/`: Rust, Python and asset/build logs.
- `native/editor-edit/`, `native/editor-link-play/`: source editing, handler linking
  and normal/minimum interface captures/checkpoints.
- `native/editor-errors/`: syntax, failed-build preservation, correction and runtime
  diagnostics through the native editor.
- `native/gameplay-final/`, `native/runtime-errors-final/`: direct-player captures,
  trace/audio/physics state and repeated load/reset outcomes.
- `native/editor-final-reopen/`: fresh source reopen, bounded clipboard-paste
  refusal, copy/paste, save and Check.
- `native/assertions.json`: executable acceptance assertions, produced by
  `assert_native.py`; `payload-manifest.json` pins preserved direct executables.
- `stage6-handoff.json`: exact final commit, remote/CI identities, native assertion
  count, performance evidence, cleanup and any remaining limitations.
- `try-editor.sh`: opens the retained authoring project with direct payload tools
  and explicit repository assets. The host still supplies SDL3.

<a id="editor-0120-task6-remaining-scope"></a>
### Remaining scope

The current output carries selected-map script snapshots. It is not complete
project export/import: multi-level membership, shared/level metadata, thumbnails
and a clean one-package distribution roundtrip remain stage 7. The source editor
has no LSP/typechecker, syntax colouring or arbitrary selection; other entity/
timer/sequence event links remain source-authored. There is no live hot reload,
custom asset importer or game-mode demo. Native Windows/Linux and clean installed
multi-level distribution are not claimed. The broader editor issue remains open.

The approved later queue adds a dedicated Luau/map-import safety review after
stage 7, model integration and editor-authored Closing Time. The handoff records
known budget/compiler/isolation limits and current hostile-fixture evidence for
that review.

Follow the [stage 7 handoff](release-0.12.0.md#editor-0120-stage7-handoff). Model integration
remains deferred until all editor stages finish; the rejected newer Spoonerman
and backlog modes #25/#26 are unchanged.

<a id="editor-0120-task7"></a>
## Stage 7: complete project export, recovery and distribution round trips

Implemented on `0.12.0-staging` after stage 6 `bfe36887538bd1a2da02625488b2b929efe0da6d`.
No maintained map, model, texture, sound, catalog or lighting revision changed.
No main merge, release, branch deletion or `.app` launch is part of this stage.

<a id="editor-0120-task7-result"></a>
### Result

The editor exports a project containing multiple authored JSON levels, project
metadata, Luau and custom thumbnails into one `.placesmap`. It can recover exact
editable files into a new folder and reopen them. Build+Play uses the complete
project; Save+Build retains the selected-map compiler path. The player imports
one file and loads the selected level without eagerly reading other levels’
semantics, scripts, geometry or lightmaps.

`engine::package::project` owns the shared bounded project index and recovery
contract. Each level retains the ordinary manifest and prepared records;
content-identical records are stored once and selected through a virtual entry
table in the existing reader. Models, textures and sounds remain installed
built-ins, with the existing dependency hashes. No custom runtime asset importer
or second compiler GUI was introduced. All source/package/lighting versions stay
at the established reset version 1; `project-transitions-v1` explicitly gates the
new transition action.

The recovery payload contains exact bytes for the manifest, declared levels,
referenced scripts and thumbnails. Unknown fields, stable IDs, warning ignores,
trigger drafts, authored settings and module order survive because recovery
never serializes a typed level back over its source. Prepared semantics remain
separate and may omit editor-only fields. An ordinary compiled single-map package
cannot recover an authoring project and receives an explicit refusal.

<a id="editor-0120-task7-workflow-and-protections"></a>
### Workflow and protections

`Project >` groups metadata, adding/selecting levels, explicit entry selection,
thumbnail attachment, permitted destinations, complete export, recovery and
check/migration commands. Scene transforms remain contextual. Level selection
clears the previous object selection/scroll and frames the new scene. Project
recovery is also available directly from an empty editor. Normal 1440×816
and minimum 960×544 layouts were inspected. Thumbnail attachment validates an
existing 16:9 RGB8/RGBA8 PNG (maximum 1280×720 / 4 MiB), copies its original bytes
by hash and links it; no crop or resize occurs. The player’s thumbnail card UI
is still the separate #6 work.

Export freezes validated source bytes in owned staging, then calls the existing
compiler sequentially for every level, reusing its
structured progress, cancellation, resource preparation and atomic writer.
Progress includes the selected source/level label without repurposing the
variant field. Export rechecks source snapshots and destination identity before
publication. A failed or cancelled operation retains the previous distribution.
Recovery validates in owned staging and publishes with an atomic no-replace
rename; existing files, symlinks and even empty directories are refused.

Validation covers bounded counts/metadata, duplicate and case-conflicting IDs
and paths, unsupported schemas, confined roles, source symlink components,
archive symlink/directory/traversal entries, compressed/decompressed totals,
per-record limits and hashes, UTF-8/Luau/PNG contents, source/runtime script and
transition agreement and all project references. Player import checks every
incoming level ID, not just the entry. Updating the same project can change its
entry level. Shared engine validation now owns the editor’s preserved placement
intent and trigger-draft contracts; automatic correction behavior is unchanged.

A level declares allowed destinations in `transitions`. Ordinary bindings and
sequence steps use `change_level`; Luau uses `api.change_level`. The player
resolves only within the current physical package. Navigation loads the authored
destination spawn with fresh world/VM state; first request wins and reset cancels
pending navigation. `api.reset()` uses the existing reset dispatcher. There is
no persistent campaign-variable system.

`check-project` and `migrate-project` validate supported version-1 data. They
report that no conversion is needed and leave files untouched. Unsupported data
is refused with the expected contract; no historical conversion exists or is
claimed. See [project contract](../PROJECT_FORMAT.md) and [editor controls](../EDITOR.md).

<a id="editor-0120-task7-verification"></a>
### Verification

The Rust suite passes **122 tests**, including four added small regressions for
exact source recovery/shared identities, lazy selected loading versus full import
validation, cancellation/unsafe-index refusal, and declared atomic script
transitions/reset. They use tiny metadata/ZIP fixtures and no map bake or GPU.
Strict all-target/all-feature Clippy, formatting and the locked all-feature
production build pass. The production Luau error-boundary tool passes all three
bounded refusals; `panic = "unwind"` and the compile-time abort guard remain.
The source Python suite passes **368 tests** (three optional/native skips).
The 593-asset catalog check has zero warnings; prop checks pass. Texture checks
retain their 96 existing warnings.

The release compiler/player pass **97 explicit roundtrip/refusal assertions**:
exact source recovery and re-exported compiled-record identity, membership,
metadata, script/thumbnail bytes, repeated imports, project/secondary-ID conflicts,
unsupported/malformed/oversized indices, unsafe paths, symlink and duplicate ZIP
entries, decompression limits, corrupt hashes, invalid images/UTF-8/script syntax,
existing-folder refusal, failed exports, external source edits during a frozen build,
unsupported source migration and structured export/recovery cancellation.
Existing installed packages and editable sources retain their hashes.

Native checks pass **58 state/path assertions**, with framebuffer captures.
Direct-binary editor checks exercise create/edit/save/reopen, adding a
second level, metadata, entry selection and thumbnail attachment, Check/Migration,
export/recovery, script syntax diagnostics/correction, cancelled and failed exports,
and ordinary handler/trigger controls for forward/return transitions. The tiny
rich fixture derives from the retained stage 6 room; fixture setup is documented
in `prepare_native.py`. Transition triggers and links are authored through the
actual editor, not through a manual level-file workaround. Explicit developer
scripts drive ordinary native event handlers and retain framebuffer captures and
state checkpoints. Recovery from an empty editor also opens the recovered sources.

A copied player plus actual built-in assets under
`/tmp/places-stage7-clean-install/` runs from `/tmp`, with no asset-root override
and fresh isolated user storage outside the checkout. It starts with no installed
maps, imports one project twice, discovers both levels and plays it. Actual W-key
movement crosses the script trigger: the door opens, the light switches off and
`entered 1` is recorded. Reset restores authored state and the next crossing
again reports `entered 1`. The script-linked transition loads the second level;
its ordinary return trigger loads the entry with a closed door and fresh VM.
A further restart uses the same installed state without reimporting and repeats
the full sequence successfully. The read trace verifies the entry → second → entry sequence and physical resource
paths. Screenshots are inspected evidence, not a claim of exhaustive visual QA.
No throughput/FPS comparison was performed.

Evidence and reproducible developer tooling are retained outside `target` under
`local/verification/0.12.0-task7/`: `checks/`, `native/`, `project/`, `recovered-ui/`,
`campaign.placesmap`, `payload/`, and the launchers. Final commit, CI, payload
hashes and cleanup results are recorded in `stage7-handoff.json`.

<a id="editor-0120-task7-rust-test-count-audit"></a>
### Rust test-count audit

The final 0.11.0 commit `d5942839` already had **67** essential Rust tests; the
cleanup retired the old expensive suite and a focused suite was restored before
0.12.0. This baseline is corroborated by the release report and the source
`#[test]` declarations at that exact commit. These are totals, not new-test counts.

| Checkpoint | Total Rust tests | Net added in editor stage |
| --- | ---: | ---: |
| 0.11.0 baseline `d5942839` | 67 | — |
| Stage 1 `af94e3f1` | 71 | 4 |
| Stage 2 `169748c4` | 74 | 3 |
| Stage 3 `816ef4e5` | 80 | 6 |
| Stage 4 `31ec68a8` (`69363d03` handoff unchanged) | 91 | 11 |
| Stage 5 `34903df8` | 109 | 18 |
| Stage 6 `bfe36887` | 118 | 9 |
| Stage 7 | 122 | 4 |

Thus stages 1–7 add 55 net tests, and this stage adds four. Source counts were
checked against each checkpoint and the stage reports; this is not a new
individual value review of all inherited tests. Stage 7’s exact additions are:

- `engine/tests/project_package.rs::project_recovers_exact_sources_and_preserves_shared_record_identity`:
  explicit entry selection, exact raw recovery and shared prepared-record identity.
- `...::selected_level_does_not_read_other_level_payloads_but_import_checks_them`:
  an intentionally corrupt inactive record cannot break selected loading, but full
  validation/recovery must reject it before publication.
- `...::project_index_and_cancellation_refuse_unsafe_or_partial_publication`:
  cancellation, unsupported index versions and unsafe virtual paths publish no
  project; invalid image bytes are refused.
- `engine/tests/scripting.rs::script_transitions_are_declared_atomic_and_cleared_by_reset`:
  a declared request reaches the existing dispatcher, an invalid later request
  discards staged navigation atomically, and reset clears pending navigation.

The archive tests use a few KiB of ZIP/JSON and a shared placeholder prepared
record, with short-lived file operations. They never bake/decode a full world or
resolve game assets. The script case adds three bounded handler calls to the
existing tiny metadata world. All four live under the owning `src/engine/tests/`
folder and are wired through `engine/tests/mod.rs`; the archived fixture test
module is explicitly registered there. The entire 122-test body remains under
one second after compilation on this host; exact duration is in the retained log.

`tools/verify-luau-release.py` performs three **production executable** refusal
checks; the native and roundtrip Python files perform explicit developer
acceptance. They are scripts, not additional Rust tests or automatic full-map
CI requirements. The retained 368-test Python source suite is another separate
count. No test-suite rewrite or deletion was performed in this stage.

<a id="editor-0120-task7-limits-and-next-task"></a>
### Limits and next task

Native acceptance covers macOS/Metal. Host SDL remains required; Windows/Linux
runtime acceptance, installers/signing and broad distribution stage 9 remain
outstanding. Thumbnail cards, custom runtime assets, menu scenes and persistent
campaign state are not implemented. Export rebuilds temporary ordinary packages;
it does not add a whole-campaign incremental build cache. VM deadlines remain
cooperative and in-process, with the stage 6 allocation/native-call limits.
Concurrent malicious filesystem mutation is not a promised local authoring sandbox.

Issues #18/#21 remain open for their broader acceptance criteria. The next
separately authorized task is the navigation pass, followed by editor-authored
Closing Time and three passes on It Only Exists in the Beam using currently
integrated assets. Integrate the accepted Home/shared, rubber duck and janitor-closet
assets after both maps are made, then run the dedicated Luau/map-import safety
review of both finished modes.
Stop before roadmap stages 8/9. See the [current handoff](release-0.12.0.md#editor-0120-next-handoff).

<a id="editor-legibility-crosshair-0120"></a>
## Editor legibility and five-dot crosshair — 0.12.0

This queue stage stays on `0.12.0-staging`, starting at `965a4ac9`. No main
merge, release, later queue stage or game-mode source change is included.

The native before captures confirmed two concrete causes: editor glyphs came
from the game's 8×8 nearest-sampled HUD, compressed to 3.5×4.5 reference pixels,
and its SDL window did not request the native Retina drawable. The editor now
uses a separate committed 512×448 antialiased Roboto Mono medium atlas with
clamped linear coverage, 4×7 reference glyphs and native high-density output.
Source font/OFL license and an explicit offline rasterizer are retained under
`tools/fonts/`; runtime only loads the PNG. The game's bitmap font/hand sheet,
world texture settings and shaders remain unchanged.

The slate/mint identity, left scene/transform column, central viewport, grouped
project sheet and build/view toolbar remain. Labels are vertically centered,
muted text has more contrast, toolbar widths fit the larger font, long values
use ellipses, and multiline source/diagnostic spacing and caret positions match
the new glyphs. Tab/Shift-Tab traverses visible enabled controls; Enter activates
the mint-outlined focus. Numeric/script text input keeps its editing keys.

The game draws five small white dots at 40% alpha with a faint 28%-alpha dark rim: centre plus four cardinal
dots, white radius .45, outer radius .62 and spacing 2.5 in the 480×272 reference space. The centre
accounts for the actual drawable and odd letterbox remainders. This is simple
UI geometry sampling the existing white cell, with no new runtime texture
construction or target/physics change.

Top-level Boolean `crosshair` defaults to true on existing maps. Project →
Map crosshair toggles the selected map through ordinary undo/save history.
`/crosshair on|off` overrides that map default for the current visit; help is
registered in `/help` and `/crosshair help`. Real map loads/reloads clear the
override. Respawn/world reset and graphics-only rebuilds retain it. Menus,
loading, console, guidance/death/win panels and third person hide the reticle.
Holding retains it; the interaction hand remains independently tied to the
precise eligible nearby aim target and is hidden while carrying. No new
in-game tutorial prose was added.

<a id="editor-legibility-crosshair-0120-acceptance-and-custody"></a>
### Acceptance and custody

Evidence root: `local/editor-crosshair-0.12.0/`. Direct native binaries only;
repository assets and host SDL3 are required. Native execution is macOS/Metal,
not Windows/Linux certification. Native SDL window raise and focus receipts
verify the actual editor/player window; the previously denied Terminal UI
binding was not retried or bypassed.

- Matched native before/after ordinary window sizes: 960×544, 1200×800,
  1440×816. Retina after drawables are exactly twice the logical dimensions.
  Project, inspector, geometry sheet, scripts and visible keyboard focus are
  captured; the viewport retains its existing region.
- Native per-map toggle/save/reopen/selection and export/import recover the
  manifest and both JSON files byte-for-byte, including opposite Boolean
  defaults. Undo/redo is covered by a tiny existing-document fixture regression.
- Native crosshair receipt checks default maps, override on/off, invalid usage,
  console/help, low texture/graphics change, nearby target/pickup, third person,
  pause/resume, respawn, transition and same-map reload, with actual focus.
- Small CPU regressions cover strict command syntax, map override precedence and
  clearing, exact odd-drawable centring, and independent map history/reopening.
  They use no GPU, asset load or map bake. Ordinary CI still bakes no maps.
- Mannequin R2 bytes, Spooner-Man bytes, Beam black-preset metadata and both mode
  source projects are verified unchanged against the starting head.

Failed setup attempts remain labeled separately: sandbox-native launch had no
available display; an early editor replay redundantly opened an already-open
Project sheet. The corrected supported SDL input route completes the checks.

The editor atlas is embedded from its committed PNG and deliberately stays
outside the world catalog. A compatibility check caught and removed an unnecessary
catalog entry before shipping: the installed catalog remains byte-identical, so
existing game packages retain their dependency identity. Its 8:7/16×8-cell contract
is explicit in the asset specification; world texture budgets are unchanged. No FPS comparison is made: captures, VSync and concurrent
build work are not uncapped throughput evidence. No external non-Retina display
or Windows/Linux native run was available in this stage.

The broad editor/UI issues remain open: their additional platform and workflow
acceptance is beyond this completed legibility/crosshair stage.

Local final gates pass: strict all-target/all-feature Clippy, locked all-feature
release build, formatting, 143 Rust tests (0.28 seconds execution), and 369 Python
checks (98 seconds, three supported skips). Asset/catalog, prop and texture
checks pass with the previous 96 texture advisories. Native editor acceptance
has 19 assertions and initial crosshair acceptance has 27 assertions/19 captures.
The original world catalog remains exact SHA-256 `301c0a0c…`.

Library screenshots: [before, 960×544 logical](https://chatgpt.com/api/library/files/libfile_f20c22026de08191afc8807d8c9a481c/download),
[after, same window with Retina drawable](https://chatgpt.com/api/library/files/libfile_b943bb0a02e4819186120d9106b6872a/download),
[before, 1440×816](https://chatgpt.com/api/library/files/libfile_67ef62cd595481918d63428f6ddc1f1a/download),
[after, 1440×816](https://chatgpt.com/api/library/files/libfile_d4325a475030819198f0a643d2e1b8dd/download),
[keyboard focus](https://chatgpt.com/api/library/files/libfile_41b7d21049688191aecf1a4e44859d90/download),
[five-dot crosshair](https://chatgpt.com/api/library/files/libfile_0566c028ff60819180162b2970a86d0d/download),
[target hand plus crosshair](https://chatgpt.com/api/library/files/libfile_11a26f56847c8191b7031699a2b1a67d/download).

Both original integrated game packages import unchanged with the original
catalog hash. Final-player Beam death/double immediate reset passes all 44
existing assertions; Closing Time's ordinary-input win route passes all ten.
Native Beam death/guidance screens hide the crosshair and the resumed world
restores it. Closing Time's completion remains its existing live world label,
so gameplay retains the reticle. Full asset audit passes: 381 models, 417
embedded PNGs, 581 standalone PNGs, zero errors.

<a id="editor-legibility-crosshair-0120-authorized-refinements-and-menu-images"></a>
### Authorized refinements and menu images

Connor requested a subtler reticle and reliable contrast in darkness and on
light surfaces. White opacity is reduced from .50 to .40. Each dot has a narrow
.62-radius dark rim at .28 alpha around its .45-radius white centre. This is a
fixed contrast-preserving treatment, with no animation, brightness pumping or
frame-to-frame noise. The UI pass currently receives no background texture or
luminance history; adding scene copies/history bindings for dynamic adaptation
would widen this small HUD change. No costly GPU readback or synchronization
is introduced, and no claim of actual luminance adaptation is made.

The additional authorized level-icon work reuses stage 7's exact per-level and
project thumbnail references, content-addressed PNG blobs and source recovery.
The editor offers a native PNG chooser, visible path entry, full 16:9 preview
and removal. Removing an entry image also clears its package fallback; shared
PNG files are retained so other maps are safe. No new image metadata or package
format is introduced. The established RGB/RGBA8, 16:9, maximum 1280×720, 4 MiB
encoded/8 MiB decoder contract is reused; decoding is independent of the world's
1024-edge texture limit. APNG, bad dimensions/paths, missing or corrupt bytes
are refused.

The map list places a small landscape image before each name, with its existing
selection/keyboard/pointer layout retained. The selected map's image wins; only
the entry map can inherit the project/package image. Other maps get the neutral
P fallback. Bounded visible-row CPU/GPU caches upload PNGs only when the visible
images change. Menu image reading opens just the archive index and selected
PNG blob; it never loads meshes, materials, lighting, scripts or world geometry.
Corrupt images produce a named warning and the fallback without crashing
browsing. The small archive regression deliberately corrupts world semantics
and still reads the correct thumbnails, proving that separation.

Native refinement evidence: [dark room](https://chatgpt.com/api/library/files/libfile_23e1373977cc81918c98e322e3d46f66/download),
[bright snow with flashlight](https://chatgpt.com/api/library/files/libfile_f9c9f9ab2ed88191a352718149e2556b/download),
[editor image preview](https://chatgpt.com/api/library/files/libfile_7ed76396303c81918e70214a87c2888c/download),
[menu images](https://chatgpt.com/api/library/files/libfile_32634be2dcb08191891e8a4c928f7f66/download),
[corrupt-image fallback](https://chatgpt.com/api/library/files/libfile_ad0df6d5e6688191b1becf7b151c4561/download).
These refined reticle images supersede the earlier .50-alpha crosshair stills.
The native editor replay selected PNGs through visible Paste path controls; the
new SDL OS file picker was not automated through the unavailable raw-binary UI
binding. Both selection routes share the same validated attachment operation.

Final expanded source gates pass: 143 Rust regressions (0.28 seconds), strict
all-target/all-feature Clippy, locked all-feature release, formatting and 369
Python checks (98 seconds, three supported skips). Preserved final binaries
repeat all 19 crosshair captures, icon/corruption/contrast checks, the ordinary
editor windows/focus, Beam's 44 death/reset assertions and Closing Time's ten
win-route assertions. Native OS file-picker automation remains the narrow
unexecuted UI path described above; validated visible path selection passes.

Final review adds a focused loading guard: visible map PNGs are suppressed by the
same `loading_map` flag that draws the loading modal, so they cannot overdraw it.
Native menu/transition replays pass; the existing game capture queue defers its
loading-named still until the new world, so no modal still is claimed. The
initial CI run on `bd2fd97f` passed every new test but hit an existing Luau
reset/scoped-state failure (zero instead of two after a closure call).
Local full/focused checks pass; hosted scheduling against existing deadlines is
a possible cause, not an established diagnosis. Runtime budgets and later Luau safety scope are
unchanged. A follow-up CI receipt covers the loading guard.

<a id="editor-legibility-crosshair-0120-final-delivery-receipt"></a>
### Final delivery receipt

Implementation commits `bd2fd97f275141735f86ec010ce245b1aa8ea42e` and
`26f5c9dcc0f00981a51f8accaa3f858de59848e1` are pushed to `0.12.0-staging`.
[CI 39112950839](https://github.com/csd113/Places/actions/runs/39112950839)
passed on the latter exact source commit: formatting, target/feature checks,
debug/release Clippy, all Rust regressions, production executables, authored
assets/generators, Python contracts and patch whitespace. The existing Luau
regression that failed in the initial run also passed; no Luau change was made.

Direct binaries, packages, source recovery, scripts, screenshots, bounded-check
logs and the asset inventory are preserved under
`local/editor-crosshair-0.12.0/`, outside `target`. No process was using
`target` at final cleanup; `cargo clean` removed 5,814 files / 5.0 GiB.
The exact preserved payload then validated the package, recovered all five
source/image files byte-for-byte, opened both recovered editor image previews,
and passed native player default-off, console-on, menu-icon, real-focus and
clean-shutdown smoke checks. `target` remains absent.

Final SHA-256: player `06837908ad862fb27cfaa979ff2b1597761fd52987dced313027090eadc024de`,
editor `923967f453eac3e8658e5588c0db0e51b477f1e04327585348cb79c32d96d1e1`,
compiler `379c33d20a49bf844cbc254a2e7b3096aae80fca10c0c870a507bc12650b3ac8`.
Launch the editor from the repository with
`./local/editor-crosshair-0.12.0/run-editor.sh`; it uses these direct binaries
and isolated state, with the repository assets and installed SDL3.

No outstanding execution blocker remains. Native acceptance is macOS/Metal
only; Windows/Linux readability remains unverified. Contrast uses the stable
rim described above, and the OS file-picker path remains unautomated.
Broad issues remain open, no main merge/release occurred, and later queue
stages were not started.

<a id="environment-0120-compatible-campaigns"></a>
## Accepted campaign compatibility copies

The collection adds ten catalogue decals, changing the raw catalogue identity.
The accepted Relay Annex, Beam and Closing Time packages correctly remain stale
against that expanded catalogue. Their bytes and all three accepted project
source trees were preserved; no catalogue check or provenance pin was weakened.
Fresh compatibility exports target only
`local/environment-collection-0.12.0/compatible/`, sequentially. Initial exports
used two workers; the one authorized Closing infrastructure retry used four.

The preserved compiler is SHA-256
`b6d3aecaf75ebc49f3a8132a4bb0a457d81a3eb52753c6dc3187b7bcfce0894c`.
The parent subsequently produced a newer compiler. Its strict currentness check
correctly reports **compiler executable changed** for these exports. The parent
explicitly chose to retain the accepted campaigns' exact older compiler
provenance and avoid another full campaign bake. The new collection alone must
pass currentness against the final compiler. Runtime replays belong to the native
QA receipts, separately from compiler currentness. Recorded ordinary win/reset
campaigns passed on player `95bf2631…` for Relay Annex and Beam, and player
`8070dc5e…` for Closing Time. These exact earlier player identities must not be
described as newest-delivery smokes; later native acceptance is tracked in
`release-0.12.0.md#environment-collection-native-qa`.

| Copy | Result | Package SHA-256 |
| --- | --- | --- |
| `Relay-Annex-Compatible.placesmap` | 24,434,167 bytes; validate, all three embedded levels current under the preserved compiler, ten source files recovered byte-exact | `50912ac052ea79a1f7a0f25f2115d96fe40856632ca9424077e6c2d46cad2c60` |
| `Beam-Compatible.placesmap` | 6,314,015 bytes; validate, selected level current under the preserved compiler, three source files recovered byte-exact | `93ecf5e64996111a0480fbf5838de33164175551bb94f48c7628cc220f14027c` |
| `Closing-Time-Compatible.placesmap` | 34,116,781 bytes; validate, selected level current under the preserved compiler, three source files recovered byte-exact | `0712c4b188a43c500d498dc61549d87e34b23a6e069fd8da026b8de778eac0a4` |

Closing Time authors five switchable fixtures; its Full export builds base and
separate switched-light contributions. The command
service disconnected and lost its session; after reconnection, approved read-only
process inventories showed no compiler running. The log stopped at 18:38,
stdout remained empty and no final archive existed. Its 64 KiB `.partial`
directory contains only the three staged editable sources, with no reusable
prepared lighting. The exact termination signal is unavailable. After these
read-only checks proved that no compiler or published output remained, the parent
authorized exactly one four-worker retry with the same immutable compiler. Its
one-shot supervisor records PID, command, exit 0 and final package hash in
`closing-retry/finished.json`. This retry completed in 724.8 seconds and survived
one further progress-read transport loss. It performed no automatic retry.
The original 64 KiB source staging was removed only after exact membership and
byte checks against the accepted project; no compiled lighting was discarded.

`compatible/receipt.json` indexes the completed validation/recovery operations,
full hashes, source custody and both Closing attempts. All five embedded levels
are current under the preserved compiler, and all sixteen declared editable
files recover byte-exact. All accepted
source and historical package hashes remain equal to the initial custody record.
`closing-export-interruption.json` retains the resolved interruption and original
partial path.
The initial Relay recovery command omitted the relocated compiler's asset root
and safely refused; its corrected explicit-root rerun passed, with both attempts
retained.

The later janitor cardboard water-profile correction belongs to runtime-only
`assets/gameplay.json`. Compiler source inspection and all five completed
embedded campaign manifests confirm that sidecar is not a prepared dependency;
the raw catalogue and model/texture/sky dependencies remain authoritative.
`gameplay-sidecar-dependency-check.json` records this evidence, including all 56
dependencies of the final Closing manifest. The runtime-only janitor profile
correction caused no asset-identity rejection. No provenance field was patched.

No accepted source, gameplay script, historical package, distribution entry or
native player was changed or launched by this compatibility task. All editor
windows from the earlier import/title-spacing verification are closed.

<a id="environment-0120-import-audit"></a>
## Environment collection: legacy import audit

The eight distributed environment sources were imported through the new real
editor operation `Document::import_level`, also available as **Project > Import
JSON map**. The previous UI recovered source-bearing `.placesmap` projects only;
it had no standalone JSON import operation. Imports now validate bounded source,
unique keys, representable numbers, runtime semantics, project membership and
placement-intent metadata, then publish complete original bytes without replacing
an existing destination. Original stable IDs, unknown fields and formatting are
retained. Script dependencies require owning-project recovery; undeclared
destinations and duplicate IDs are refused without guessing replacements.

Evidence is under `local/environment-collection-0.12.0/import-audit/`.
`import-inventory.json` indexes original/migrated hashes, editable imported
projects, diagnostic files, save/reopen equality and installed-asset validation.
All eight migrated baseline hashes equal the original source hashes. No baseline
was automatically corrected, manually edited or given a new ignore. Existing
`geometry_intent` annotations remain exact: 288 records across the originals.
None of the eight originals contains an `editor_placement_intent` ignore.
The existing coordinate-based geometry annotations and the isolated placement
ignore exercise below are different evidence.

| Legacy source | Safety entries | Offline errors / warnings | Preserved intent records |
| --- | ---: | ---: | ---: |
| Pool Showcase | 56 | 3 / 1 | 0 |
| Home Showcase | 17 | 0 / 1 | 0 |
| Office: After Hours | 12 | 0 / 1 | 1 |
| Lantern Hollow | 1 (budget refusal) | 1 incomplete audit / 0 | 54 |
| Winter | 1 (budget refusal) | 0 / 2 | 45 |
| Beach | 1 (budget refusal) | 1 incomplete audit / 0 | 162 |
| Frutiger Aero | 1 (budget refusal) | 0 / 0 | 26 |
| Cosmic Lanes & Arcade | 50 | 0 / 34 | 0 |

The original Pool has three measured duplicate-surface overlaps at the coping
junction near `(8.875, 0.035, 7.625)`: about 0.0013, 0.0078 and 0.0484 m².
These involve threshold geometry, outside the conservative wall-removal autofix.
They were handed to the Pool redesign owner for an explicit geometry edit.
Home has ramp-side/drop and wall-cabinet collision review candidates. Office has
two CRT collision candidates and a lower-storey leak candidate. Arcade's offline
warnings are 34 repeated prop-placement candidates; they do not justify automatic
prop deletion because behavior and composition may differ.

Lantern and Beach exhaust the offline coplanar candidate budget. Those explicit
`geometry-check-incomplete` errors describe incomplete coverage, not confirmed
mesh defects. Interactive Safety also refuses maps over its object/work budgets.
Pool/Office model comparisons retain explicit pair-budget notes. No budget was
raised and no safe/closed-map conclusion follows from these bounded checks.

The originals contain no exact duplicate simple wall records, the only currently
conservative automatic correction. The original-map correction count is therefore
zero, rather than a claim that automatic correction repaired their findings.
A separate isolated **actual imported Pool** copy received two deliberately
duplicated simple walls. Actual Safety
preview left source unchanged; apply and undo restored exact source. One duplicate
was explicitly reviewed/ignored, saved and imported into another fresh project.
Exactly one active per-placement decision survived. Direct correction of the protected
wall refused, the batch candidate gate skipped it, and the independent duplicate
was removed while the protected pair remained. No correction candidate remained
afterward. `ignore-probe-receipt.json` records this complete exercise. The original
and migrated baselines were untouched.

All eight imported projects also pass the preserved direct `places-compile
check-project` operation with explicit repository assets. The first automated
`--events` attempt closed stdin and correctly cancelled with status 130; those
logs remain labelled separately. Plain CLI reruns all exit 0. No lightmap bake is
part of this import audit, Rust tests or ordinary CI.

Two small editor-owned regressions protect byte/intent preservation and rejection
of duplicate IDs, unresolved scripts/destinations, ambiguous JSON and existing
unowned files. They perform local tiny-source writes, without assets, GPU or a
map bake. At the import implementation checkpoint, both passed in 0.03 seconds;
the then-current workspace suite passed all 175 tests in 0.39 seconds of execution,
and strict all-target/all-feature Clippy passed. These are earlier checkpoint
results, not the final collection head's gate results. The parent stage owns the final release
build, native UI/gameplay validation, export/recovery roundtrip and exact-head CI.

An additional read-only review of the redesigned Office is retained under
`final-office-confirmed/`: 12 baseline entries become 11; the leak and both CRT-collision
warnings disappear. Model-coplanarity candidates increase from six to eight;
one model-pair budget
note, support-contact information and scope information remain. The offline
checker reports zero errors and warnings. No source correction or ignore was
applied by this follow-up. This Safety scan identifies source `68609855…`;
the later return-sign polish identifies final Office source `e1aa9307…` in
`office-migration.json`. Native composition review remains with the stage.

<a id="environment-0120-import-audit-deliberate-collection-edits-and-measured-changes"></a>
### Deliberate collection edits and measured changes

The byte-exact baselines are preserved separately from subsequent source
authoring. The Home/Pool and Office authoring helpers operate on those imports;
their geometry redesigns are manual authoring decisions, not editor autofix or
native placement-edit claims. `home-pool-authoring.json`, `office-migration.json`
and `destination-edits.json` record the changes:

* Home is a complete seven-room redesign using the approved domestic assets and
  R2 carry quest. Its chair's standable height is corrected to the actual seat
  surface so the requested cushion does not overlap a full-height collision box.
* Pool retains the original rooms and adds a second basin room and a separate
  janitor closet. The old threshold ends at X8.753, leaving a 2 mm joint before
  the perpendicular coping at X8.755; this deliberate trim resolves the three
  measured duplicate surfaces. No threshold-shape autofix is available. The
  reusable janitor cardboard profile also receives an explicit 0.024 m³ water
  displacement after native body admission exposed the invalid inferred value.
* Office moves the basement flight to the west shaft, raises the vacated east
  floor, connects both landings, regenerates supported rails and fits lighting.
  Four desk objects receive explicit dormant physics; the obsolete fixed mug
  interaction is removed after validation rejected that conflict. One narrow
  new `geometry_intent` annotation describes the physically railed west landing
  opening. The original bridge annotation remains. Final collection sources
  contain 289 geometry annotations and zero per-placement ignores.
* Every destination receives a physical return door, nameplate and explicit visit
  behavior. The other five migrations retain their layouts while fitting
  physical profiles and existing-asset themed interactions. These authored
  additions are not automatic import corrections.

| Check and evidence | Baseline → collection result | Remaining review |
| --- | --- | --- |
| Pool production geometry; `readonly-pool-audit-water-fixed/` | 3 errors / 1 warning → 0 / 0 | Safety still has 56 entries, including 31 exit candidates, 22 model-plane candidates and an incomplete model audit. |
| Office production geometry; `office-migrated-geometry.json` | 0 / 1 → 0 / 0 | Three narrow missing-wall findings are covered by its two explicit opening annotations. Earlier Safety comparison is 12 → 11, with model-plane candidates 6 → 8. |
| Home production geometry; `readonly-home-audit-fixed-seat/` | 0 / 1 → 0 / 2 after complete redesign | The two shared-boundary warnings remain. Safety entries increase 17 → 31, including 21 model-plane candidates, five nonsolid-prop reviews and an incomplete model audit. |
| Arcade production prop check; `source-geometry/arcade-checker-improvement.json` | 34 repeated-prop warnings → 0 | All 34 were distinct balloon component owners. The checker eligibility fix preserves the source placements; no deletion or ignore produces this result. |

Both latest Home and Pool scans report zero movable contact/unchecked findings,
preserve their source hashes and reach the unchanged 2,000,000-step model budget.
Their current source hashes match their read-only receipts. Safety totals include
scope, support and conservative review entries; they are not defect totals.

Across the nine collection maps, the saved production geometry scans, with the
later Arcade checker result replacing its earlier scan, contain two explicit
incomplete-audit errors (Lantern and Beach) and 17 review warnings: hub 13,
Home two and Winter two. No confirmed duplicate-surface error remains. Four large
legacy maps still exceed interactive Safety's budget. These are conservative
coverage limits, not automatic clearance or proof that every room is sealed.
`import-audit/final-supporting-evidence.json` records the exact evidence inputs,
source hashes, manual/automatic distinction and remaining limits.

<a id="environment-0120-import-audit-native-import-and-document-roundtrip"></a>
### Native import and document roundtrip

`local/environment-collection-0.12.0/native-editor-import/receipt.json` records
15 passing checks and 15 checkpoints with actual SDL input focus. The direct
immutable release editor (`3e77420e…`) created a fresh project through `--new`,
clicked the visible **Project > Import JSON map** control, typed the absolute old
Home path, applied and saved. A separate native process reopened the imported
map and inspected its Project controls. Original/imported source hashes remain
`3f5caff7…`; semantics are exact. An isolated complete collection copy then
exercised transform preview, apply, undo, save and a separate native reopen.
All twelve project-file hashes returned exactly to their initial values and the
production project remained unchanged. No build/export/play was invoked.

The real Project screenshot exposed a left-column layout overlap between
**Selected map title** and **Selected / Next**. The smallest correction changes
the six left rows from 17 to 15 logical pixels apart. Buttons remain 13 pixels
high, text remains 7 pixels high, the gaps between rows are 2 pixels and the last
row ends at 155 before the next control starts at 159. The before screenshot is
retained. The fresh immutable release editor (`d87e9c66…`) verifies separate
title/next-map controls, opens the title form and switches maps with actual
focused input. `title-spacing-receipt.json` records all three focused checkpoints
and unchanged source hashes. A first input recipe redundantly requested the
already-open Project panel; its refusal is retained alongside the corrected,
successful recipe. All native processes are closed; no windows remain.

<a id="environment-collection-0120-retirement"></a>
## Quiet Collection distribution handoff — 0.12.0 staging

The maintained environment distribution is now one `environments` entry in
`levels/lineup.txt`, sourced from `projects/environments/places.project.json`.
Its hub and eight destinations reuse the completed, playable package; this
handoff performs no full rebake or map/source edits. Collection implementation
and supporting runtime/editor work were already committed and pushed in
`d2ec9971` when this handoff began; the checkout was clean on `0.12.0-staging`.

<a id="environment-collection-0120-retirement-exact-retirement-inventory"></a>
### Exact retirement inventory

For each ID below, remove exactly `levels/<id>.json` from maintained standalone
sources and `assets/levels/<id>.placesmap` from normal package discovery:

| Retired standalone ID | Maintained collection member |
| --- | --- |
| `pool_showcase` | `pool` |
| `home_showcase` | `home` |
| `lantern_hollow` | `lantern_hollow` |
| `winter` | `winter` |
| `beach_demo` | `beach` |
| `frutiger_aero_demo` | `aero` |
| `retro_arcade_demo` | `arcade` |
| `office_demo` | `office` |

These are eight source entries and eight ignored compiled outputs. The old
packages total 479,914,576 bytes. Their replacement is the existing nine-member
`assets/levels/environments.placesmap`, 449,857,977 bytes, SHA-256
`9e78fbe3f6a2cf00f19f410da76271f1acb6c6b14d91b2e5ba0dd8682e532ee8`.
This is a distribution comparison, not a like-for-like content/codec benchmark.
Compiled maps remain ignored by Git and are not part of the source commit.

Before retirement, all sixteen files were copied and hash-verified under
`local/environment-collection-0.12.0/retirement/preserved-standalone/`, retaining
their original relative paths. `retirement/plan.json` records every size/hash;
`retirement/retirement-switch.json` records the applied switch. Tracked originals
also remain recoverable from Git history. The preflight initially refused old
package schemas; exact reviewed hashes and ZIP manifest identities were checked
before retirement instead of rebuilding unsupported old packages. Failed-attempt
logs remain alongside the successful receipt.

The collection's maps, scripts, thumbnail, shared assets and exact embedded
editable source bytes are preserved. Closing Time, It Only Exists in the Beam
(`projects/beam/`) and Relay Annex remain separate projects; their sources and
existing compatible packages are retained. Other projects, Places Demo/Model Zoo
historical JSON, import baselines, unique files and recovery/native evidence were
not retirement candidates. Hash comparison confirms all 32 protected collection,
campaign and historical source/support files are unchanged (the collection README
was intentionally updated). No user-installed maps were deleted.

<a id="environment-collection-0120-retirement-source-ownership-and-exports"></a>
### Source ownership and exports

The six historical environment builders now require explicit scratch `--out`
paths outside `levels/`, `projects/` and `docs/archive/`. Their reconstruction
logic remains available; generated output cannot overwrite maintained collection
sources. CI and desktop source checks use the collection manifest and Luau link
checks instead of requiring retired generator-owned JSON. No full-map bake was
added to CI. Root `levels/*.json` stays ignored to prevent accidental reintroduction.

Runtime packaging now includes `assets/janitor_closet/`, required by Pool.
External-map export validates discovery against every declared package member,
rather than assuming one playable ID per file. Both flat and macOS bundle
exports retain every janitor resource and the exact accepted collection archive.
Legacy native camera manifests keep their historical IDs and poses; recover old
inputs into scratch storage for a matched historical replay. Current collection
routes use `tools/authoring/environment_acceptance.py`.

<a id="environment-collection-0120-retirement-launch-the-existing-delivery"></a>
### Launch the existing delivery

From the repository root, after this local distribution switch:

```sh
PLACES_ASSET_ROOT="$PWD/assets" PLACES_LEVEL=hub \
  ./local/environment-collection-0.12.0/payload-collection-final/places
```

That preserved player has SHA-256
`b11ac1208dec89915f470adb17488c439835b2d12f0bd9152051425f54b74dff`.
The preserved compiler has SHA-256
`dc27c98d314848149523c4298321af3172a3144e4dfc44fe539f8ebdba7ead63`.
Both remain outside `target/`, with the accepted archive, editor and other
recovery epochs. This task did not run `cargo clean`; the current target is retained.
The earlier cleanup and its post-clean native smoke remain recorded separately.

For another installation, supply a matching player/resources and import the
existing package with `places --import /absolute/path/environments.placesmap`,
then choose **Places / The Quiet Collection** in Explore. `PLACES_LEVEL=hub`
selects the hub directly. A complete project export is needed only after editing
sources. See [installation](../INSTALLATION.md) and the
[collection README](../../projects/environments/README.md).

<a id="environment-collection-0120-retirement-focused-validation"></a>
### Focused validation

No map was rebaked. Local command logs and receipts live under
`local/environment-collection-0.12.0/retirement/`.

- `cargo fmt --all --check` — passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` — passed.
- `cargo build --locked --release --workspace --all-features` — passed.
- `cargo test --workspace --all-features` — passed (191 Rust tests).
- `places-compile check-project projects/environments --asset-root assets` — passed without source mutation.
- `python3 tools/assets/validate.py` — passed: 620 assets, zero warnings.
- `python3 tools/levels/compile.py --check --compiler <preserved-final-compiler>` — passed: all nine sources current, Static/Full records present, no lighting fallback, complete archive valid.
- `python3 -m unittest tests.built_maps` — passed: exact editable bytes, record hashes and every prepared member.
- `PLACES_WEATHER_PACKAGE_DIR="$PWD/assets/levels" python3 -m unittest tests.test_weather.PackagedWeatherTests` — passed.
- Focused collection/export/inventory/validator checks — 27 tests passed, one explicit native-export skip; geometry repair checks also passed.
- `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'` — passed: 368 tests, three expected skips (96.598 seconds).
- `sh tools/package.sh --with-compiler --maps assets/levels local/environment-collection-0.12.0/retirement/runtime-export` — passed; flat and app payload hashes/resources checked.
- Canonical asset-root Full hub→Home→hub native replay and `environment_audit.py` — passed: 75 checks, all six captures focused, immutable inputs and no bounce. Hub return pixels inspected.
- Existing final Full native evidence remains applicable to unchanged map/package bytes: thirteen cases, 1,131 checks, including all eight hub/return routes, Home quest/reset/reentry, Office route and Pool rooms/closet/duck.
- All six historical builders — protected-source refusal, scratch reconstruction and deterministic `--check` passed.
- Flat/app executable-relative `--list-levels` — passed: nine members each with isolated user storage.
- `git diff --check` — passed.

One legacy Python assertion expected generated JSON's exact Unicode escaping.
Pool's accepted editor source uses an escaped em dash. The serializer regression
now checks values, recursive object key order and newline behavior across the
same sources; it no longer claims arbitrary editor formatting is canonical.
Exact source byte recovery remains verified independently by project/package
and native editor contracts. Accepted map bytes were not reformatted.

<a id="environment-collection-0120-retirement-remaining-work-and-limits"></a>
### Remaining work and limits

Sound polish and the dedicated main-menu background are deferred to **0.13.0**;
no work on that version was started. A broader themed-interaction campaign,
uncapped FPS/memory comparisons, finished video/Library media, native installers,
signing and Windows/Linux native acceptance remain incomplete. Native acceptance
here is macOS/Metal correctness evidence, not throughput evidence. Known geometry
audit limits for Lantern Hollow/Beach and review findings for Hub/Home/Winter
remain described in the [collection report](release-0.12.0.md#environment-collection-0120).
No blanket diagnostic suppression was added.

The branch handoff commits and pushes only reviewed retirement/distribution work
on `0.12.0-staging`. It does not wait for hosted CI, merge, publish a release or
start a new version. Historical stage scope/deferral fields remain in local
receipts; this handoff supersedes their old commit/push/retirement deferrals.

<a id="environment-collection-0120-retirement-files-changed-in-this-handoff"></a>
### Files changed in this handoff

- `.github/workflows/rust.yml`
- `.gitignore`
- `CHANGELOG.md`
- `README.md`
- `docs/INSTALLATION.md`
- `docs/README.md`
- `docs/reports/release-0.12.0.md#environment-collection-0120-retirement`
- `docs/reports/release-0.12.0.md#environment-collection-0120`
- `docs/reports/release-0.12.0.md#environment-collection-native-qa`
- `levels/README.md`
- `levels/lineup.txt`
- `projects/environments/README.md`
- `tests/fixtures/native/model-lighting/README.md`
- `tests/test_curated_compile.py`
- `tests/test_geometry_repair.py`
- `tests/test_package.py`
- `tests/test_packaging.py`
- `tools/export-maps.py`
- `tools/levels/build_beach.py`
- `tools/levels/build_frutiger_aero.py`
- `tools/levels/build_lantern_hollow.py`
- `tools/levels/build_office.py`
- `tools/levels/build_retro_arcade.py`
- `tools/levels/build_winter.py`
- `tools/package.sh`
- `tools/verify.sh`

The eight deleted `levels/<id>.json` files are listed individually in the retirement inventory above. Ignored package installation/removal and raw recovery receipts remain local.

<a id="environment-collection-0120"></a>
## Places / The Quiet Collection — 0.12.0 staging

Status: the final nine-map Full package is exported and verified current, with
all twelve editable source snapshots recovered byte-exactly. All thirteen essential final
native cases passed (1,131 checks), followed by a normal Hub startup after Cargo
cleanup. The tested launch command is in
`local/environment-collection-0.12.0/normal-hub-post-clean/command.txt`. The collection
and supporting tooling are committed in `d2ec9971`. The subsequent
[distribution handoff](release-0.12.0.md#environment-collection-0120-retirement) retires the
standalone demos and records current validation, launch commands and deferred work.
The package is 449,857,977 bytes (449.857977 MB / 429.017999 MiB). The accepted
compile/bake/export took 5,770.880 seconds; supervisor-observed wall was
5,771.838 seconds. Detailed per-map size/phase timing and cache decisions are in
`local/environment-collection-0.12.0/export-metrics-final.json`.

One editable project, `projects/environments`, contains a central hub and eight
environments. The hub has eight physical doors with destination materials,
recognizable props and readable nameplates. Each destination has a physical
return door. A fresh aimed E press changes maps; arrivals face away from the
return leaf and do not trigger automatic bounce loops. Each visit starts fresh
objectives, scripts, doors and physics; held objects stay in their environment.

| Previous distributed map | Collection destination | Change |
| --- | --- | --- |
| `home_showcase` | Home / Quiet Rooms | Complete seven-room domestic redesign; R2 cushion quest |
| `pool_showcase` | Pool / Still Water | Second pool hall and basin; separate furnished janitor closet |
| `office_demo` | Office / After Hours | Basement ramp moved to west shaft, reached through middle level |
| `retro_arcade_demo` | Cosmic Lanes & Arcade | Dormant physical tabletop items, lane and object resets |
| `lantern_hollow` | Lantern Hollow | Reviewed import, physical props, trail light interaction |
| `winter` | Winter / Snow & Shelter | Reviewed import, existing weather controls and lodge route |
| `beach_demo` | Beach / Beyond the Shore | Reviewed import, physical crates and crab interaction |
| `frutiger_aero_demo` | Frutiger Aero | Reviewed import, kiosk ambience interaction |

All eight originals were imported through the actual editor `Document` operation,
which is also exposed as Project > Import JSON. Baselines preserve exact source
bytes, stable identities and placement metadata. A narrow imported Pool fixture
exercised preview/apply/undo, saved/reimported placement ignores and both direct
and batch autofix refusal for a protected duplicate. Production maps received no
blanket ignores. The checker now recognizes Arcade's legitimate animated balloon
placements instead of reporting 34 false duplicate-prop warnings. Pool coping
joints required small deliberate geometry edits: wall-only autofix cannot safely
infer their intended shape. See [import evidence](release-0.12.0.md#environment-0120-import-audit).

The mannequin receives the original held cushion atomically, attaches its actual
render resource to R2's animated `hand.R` node, thanks the player and walks to an
opaque service corridor. Wrong objects remain held; repeat dialogue cannot hand
over twice. Reset restores the original item identity, dormant pose, NPC,
interaction bounds and quest state. Group operations refuse NPC-owned items.
Retirement requires one actual opaque, uncut wall to cover the eight projected
corners of the entire current posed character and carried-prop bounds, with a
safety margin. Both meshes are hidden before retirement is confirmed. This
conservative proof can keep an actor waiting where several occluders jointly hide
it; it never relies on a torso point, timer or looking away. The Home corridor is
authored to provide the required complete occluder.

Narrow native preflights passed with direct executables and focus checks: Home
wrong/repeat (62 checks), success/hidden retirement/reset/fresh request (81), two
actual handoffs around a reset (34), following and partial-corner visibility
(135 checks and 15 focused captures), Pool rooms/closet/duck (67), Pool annex
lights/towel (45), Office ordinary route (91), jump/railing safety (57), physical AI
ramp movement in an explicitly widened-home-bounds diagnostic (50), and shipped
AI home-bounds refusal (11). These small Static fixtures are collision/runtime
evidence; they do not replace final Full package visual, transition or performance
acceptance. [Native evidence](release-0.12.0.md#environment-collection-native-qa) and
[Office evidence](release-0.12.0.md#environment-collection-office) retain failed input recipes and
their corrected paths without disguising them as source fixes.

Native failures led to reusable fixes for NPC item/visibility reset, moved
interaction bounds, and admission of more than sixteen distinct dynamic meshes.
The renderer now supports the existing 256-object contract while preserving the
previous aggregate geometry bound. Essential CPU regressions cover ownership,
whole-assembly occlusion, real reset/movement/ray behavior and mesh admission.
The a9 preflight epoch passed formatting, strict workspace/all-target/all-feature
Clippy, locked production release build and all 181 Rust tests. Those receipts
precede the subsequently discovered capacity fix and do not validate its changes.
Asset validation
passes for 620 assets with zero warnings; texture validation retains 96 existing
advisories across 133 textures. Full-map baking remains explicit developer tooling
and is never part of Cargo tests or CI.

Capacity auditing found the eight previous standalone ZIPs total 479,914,576
compressed bytes but 1,466,020,347 decoded record bytes. The previous project
exporter incorrectly applied the ordinary 1 GiB aggregate bound to the entire
collection and retained completed maps' record vectors in memory. Its superseded
a9 run completed five maps before the audit paused it; an observed RSS point was
1,436,208 KiB. A macOS process sample reported a 5.0 GB peak physical footprint,
which is a different measure from RSS. Cooperative cancellation did not finish
within an extended grace period; the owned child was terminated, no package was
published, and its twelve exact source-snapshot files were checked against the
project before removal. Small receipts and logs remain preserved. Complete projects now have a separate bounded
2 GiB allowance admitted only after index/namespace validation, while every member
and shared-prop working budget retains 1 GiB and all typed/source limits remain.
Export streams exact records through owned disk spools with 64 KiB copy buffers,
deduplicates identical bytes and verifies size/hash on both copies. Six tiny
CPU regressions cover metadata budgets, transport, streaming integrity,
cancellation, atomic cleanup and deduplication without a map bake; all six pass,
alongside sixteen existing project/package-loading checks. Final formatting, strict all-target/all-feature Clippy, locked all-feature
release build and all 191 Rust tests now pass with unchanged Rust inputs across
the gates. The final player/compiler are preserved in `payload-collection-final`;
previous epochs remain intact. Final raw/compressed breakdowns are in `export-metrics-final.json`;
the exact compiler child peak RSS was 3,995,697,152 bytes. The copy buffer is not a
total-process memory bound.

The size investigation identifies props and lightmaps as 49.4% and 47.7% of the
previous eight ZIPs respectively. Exact whole-record deduplication finds no
remaining duplicates. A measured, bit-exact split of position, colour and UV
planes across prepared prop variants saves 53,448,181 compressed bytes across
Lantern, Winter and Arcade (11.1% of the old eight-map download), before small
replacement-recipe overhead. The corresponding raw reduction is 186,509,752
bytes. This lossless wire-format improvement is implemented with a
backward-compatible reader; original reconstructed record hashes, vertex float
bits, UVs and independent lighting settings remain unchanged. Three small codec regressions and one capability-admission regression pass,
including exact current PMP1 bytes, both recipe revisions, all accepted strides,
real chunk boundaries and malformed payload refusal. Older players reject v2 by
a named capability before fetching data; reused v1 records keep their original
requirement. These are baseline measurements, not the final collection size. Higher DEFLATE levels and another
lightmap tile codec offer much smaller measured download gains and do not justify
additional format changes here. A different compression library remains an
option with explicit compatibility and security costs, rather than an assumed
solution. Runtime decoded mesh and GPU atlas costs do not fall merely because the
download compresses better.

Runtime loads the selected member on demand from a file-backed ZIP. It keeps one
active simulated/rendered world, with a recent decoded-variant cache capped at
three entries / 192 MiB and separate optional CPU/GPU texture retention budgets
of 256 entries / 256 MiB each. Those limits do not cap the active world, incoming
world, total process RAM or GPU memory. During a door transition the old world
pauses while the destination is prepared/uploaded; commit replaces its simulation
and renderer resources. Last-owner release, shared assets, in-flight GPU work and
allocator/OS caching determine when memory falls. See `project.rs:127`,
`loading.rs:522`, `materials/image.rs:330`, `wgpu/texture.rs:968`, and
`game/app/mod.rs:3394`; the inactive-map corruption regression proves normal
selected loading does not read the other member's payloads.

Relay Annex, Beam, Closing Time, the approved R2 model, Spoonerman and historical
JSON sources remain preserved. Current-catalog compatible campaign packages were
exported and verified against their preserved compiler epoch, with byte-exact
source recovery and natural campaign/reset replays. Their exact binary epochs are
recorded rather than conflated with final collection evidence. See
[campaign compatibility](release-0.12.0.md#environment-0120-compatible-campaigns).

Known automated geometry limits remain visible: Lantern Hollow and Beach exceed
the exhaustive room-check budget, Winter retains two room-leak candidates, and
intentional shared boundaries/openings remain diagnostic candidates in the hub
and Home. Native routes supplement these limits. No blanket suppression or per-placement
ignore was added; Office has one explicit annotation for its intentional railed
landing opening (289 geometry-intent records versus 288 in the imported
baselines).

Automatic approval review denied an optional native menu-policy disable/reopen
check as deferred menu-stage work, and denied a redundant nine-map Static bake.
Neither action executed or was retried indirectly. The enabled policy has native
evidence; separate narrow Home, Pool and Office collision fixtures were approved.
The exact actions, targets and review reasons are in `approval-blocks.json`.

The canonical local evidence root is
`local/environment-collection-0.12.0/`; `handoff.json` records source/package/binary
identities, inventories, acceptance paths, limitations and final delivery state.

<a id="environment-collection-native-qa"></a>
## Environment collection native QA

Explicit developer tooling: `tools/authoring/environment_acceptance.py` imports an existing package into fresh isolated state and launches a supplied direct executable. `environment_audit.py` checks actual SDL/native focus, runtime/renderer map agreement, physical state, script/mixer errors, named objects, AI movement and whole carried-assembly retirement. These runs are outside CI and never bake maps.

<a id="environment-collection-native-qa-verified-preflight-evidence"></a>
### Verified preflight evidence

Evidence lives under `local/environment-collection-0.12.0/` and records binary/package hashes. These narrow Static fixtures are collision/gameplay debugging, not final collection transition, Full lighting or performance acceptance.

| Case | Receipt | Result |
| --- | --- | --- |
| Hub composition only | `native/hub-visual-01/acceptance.json` | 64 checks; ten focused views, all eight signs/material bays reviewed. Temporary toggle doors, no transition claim. |
| Preserved Relay Annex campaign | `native/relay-natural-current-01/` | Continuous ordinary win; 19 generic and seven campaign checks. |
| Preserved Relay water/reset | `native/relay-water-reset-current-01/` | 81 generic and four campaign checks; held duck/swim/dry ramp/win/reset/restart. |
| Preserved Closing Time | `native/closing-natural-reset-final-01/` | 19 generic, thirteen campaign and five reset checks on final 8070dc5e player. Ordinary win and second E restore four ambience loops, original dormant returns and fresh shift. |
| Preserved Beam | `native/beam-natural-current-01/` | 46 generic checks, accepted natural win and real Return reset. |
| Final editor source history/reopen | `native-source-roundtrip-final/acceptance.json` | Nine checks; preview leaves source intact, Apply changes room, Undo restores it; Save/separate-process reopen preserve all thirteen project files byte-exact. Six focused checkpoints, no compilation. |
| Home rooms | `native/home-static-rooms-final-01/acceptance.json` | 43 checks; actual living, kitchen, dining and return approach positions; composition reviewed. |
| Home wrong/repeat | `native/home-static-wrong-repeat-fixed-01/acceptance.json` | 62 checks on fdb2; original held mug remains player-owned through repeated refusals, drop and reset. |
| Home retirement/reset | `native/home-static-retired-reset-fixed-01/acceptance.json` | 81 checks on fdb2; whole-hidden retirement, ordinary reset, original dormant cushion and visible NPC restored, fresh request. |
| Home corrected pickup/handoff | `native/home-static-success-02/acceptance.json` | 61 checks; original cushion key transfers, player hands clear, actual prop/model retire only hidden. Earlier ab55 player. |
| Home reset and second handoff | `native/home-static-reset-repeat-fixed-03/acceptance.json` | 34 checks on fdb2be87; two real handoff events, one original-key dormant cushion restored, visible NPC and successful second fetch/talk. |
| Home following/corner visibility | `native/home-static-follow-fixed-04/acceptance.json` | 135 checks on player fdb2be87; fifteen focused captures. Actual carry_walk/carry_idle pose, following, look-away, visible partial arm/prop, full occlusion, retirement and turn-back. Carrying and partial-corner PNGs visually reviewed. |
| Pool rooms and closet | `native/pool-static-rooms-fixed-04/acceptance.json` | 67 checks on fdb2; both rooms, exact closet/rack/sink position, duck pickup/drop, basin feetY−1.2 and physical return. |
| Pool changing annex | `native/pool-static-annex-fixed-01/acceptance.json` | 45 checks on fdb2; towel pickup/drop, live service-light change and exact restoration, physical return. |
| Office mug/reset/vending | `native/office-static-theme-fixed-02/acceptance.json` | 64 checks on fdb2; ordinary mug pickup/drop, desk reset restores original dormant mug, vending steady/flicker/dark/steady. Generic switches require empty hands by existing runtime policy. |
| Office ordinary three-level route | `native/office-static-natural-final-01/acceptance.json` | 91 checks; eight focused captures and actual positions/heights through upper, middle and moved west basement ramp. All five basement actors move using production planning while remaining on their authored floor. |
| Office isolated AI ramp | `native/office-static-ai-ramp-final-01/acceptance.json` | 50 checks; unchanged actual geometry with only diagnostic AI home bounds widened. Normal planner/mover feet Y0→0.922→3.331→3.6, then crosses upper portal into the middle connector. |
| Office shipped AI bounds | `native/office-static-bounds-guard-final-01/acceptance.json` | Eleven checks; correct refusal of a goal outside authored basement bounds. |
| Office ramp side/jump/contact | `native/office-static-ramp-edges-final-01/acceptance.json` | 57 checks; explicitly diagnostic initial poses, normal forward/jump input. Upper bypass blocked; both ramp rails retain X bounds and feet Y1.8; no void reset. Contact PNGs reviewed in ambient/flashlight from both sides. |

Carry corner evidence uses the actual posed character and actual prop bounds. In `home-static-follow-fixed-04/corner-evidence.json`, the later partial screenshot has both bound-centre rays blocked by the real opaque bend wall, while pixels still show the protruding arm/hand and carried cushion edge. Both meshes remain visible until the entire assembly is occluded. The earlier follow02 campaign itself has two premature endpoint timing assertions and is retained as a failed preflight; corrected final fdb2 follow04 passes the complete campaign.

<a id="environment-collection-native-qa-findings-and-corrections"></a>
### Findings and corrections

* Normal walking uses actual configured 3m/s. An initial follow recipe incorrectly lengthened held movement to simulate a slower speed, overshooting into a wall. Corrected recipes walk between waypoints, then wait off the NPC path. Camera `aim_entity` is a correct live one-shot; aiming before a long wait does not continue tracking a moving actor. Endpoint captures now aim after the wait.
* Real Home reset restored visible actor pose and the original dormant cushion, but its moved non-routed interaction bounds stayed at the departure location. The second pickup worked, yet aimed E could not talk to the actor. `home-static-reset-repeat-final-01/02` retain the failure; the runtime owner identified the missing reset-boundary synchronization. The corrected fdb2 player passes all 34 checks in `home-static-reset-repeat-fixed-03`, including the second actual handoff.
* Pool native startup originally refused `janitor:perforated_supply_crate` as a physical dynamic renderer entry within its resource budget. `native/pool-static-rooms-final-01/native.log` is the exact failure; no inputs/captures occurred. The bounded dynamic-mesh cache fix now admits the unchanged scene. Ordinary route refinement goes around the intentionally stored mop and outward-open closet leaf; no destructive map workaround was applied.
* A single-map Home package retaining a hub transition was correctly rejected by strict import. The parent produced a normal one-map project with only that unavailable transition declaration/binding omitted; production source remained unchanged. The accepted narrow fixture preserves scripts/capabilities and uses normal import guards.

<a id="environment-collection-native-qa-explicit-limitations-and-blocked-action"></a>
### Explicit limitations and blocked action

The canceled a9 export and its fdb2/a9 preflights remain historical evidence; they
do not establish final Full acceptance. The later export completed successfully
with compiler `dc27c98d` and player `b11ac120`. Its unchanged package passed all
thirteen essential native cases (1,131 checks): all eight hub/destination/return
routes, Office's ordinary route, Home success/reset, following/turn-back,
completed-quest reentry and Pool rooms/closet/duck. A normal Hub startup also
passed after the earlier Cargo cleanup. Exact identities and case receipts are
in `essential-native-progress.json` and `normal-hub-post-clean/receipt.json` under
the local evidence root. See the [distribution handoff](release-0.12.0.md#environment-collection-0120-retirement)
for the retirement and focused revalidation. Broader themed interaction coverage,
an uncapped performance campaign and finished video/Library media remain
incomplete. No throughput or total-memory improvement is claimed by these checks.

The optional native editor entry-policy disable/reopen command was denied before execution by automatic approval review: “The action tests and changes the collection’s menu entry-only policy, which falls under the explicitly deferred later menu/distribution stage; the isolated copy does not override that scope restriction. Do not bypass this rejection through a workaround or indirect execution.” The enabled isolated project is preserved; no retry/indirect mutation was attempted. The prior enable succeeded with four focused checkpoints; unrelated source history/reopen completed separately with no policy controls.

<a id="environment-collection-office"></a>
## Office collection migration

Office remains the same three-storey building: entry offices at Y7.2, displaced
middle corridors at Y3.6 and the basement at Y0. No additional upper storey is
introduced. The approved R2 mannequin and four cockroaches retain their original
basement AI, bounds, refuges, clips and lighting response.

<a id="environment-collection-office-route-correction"></a>
### Route correction

The old east shaft put both flights on one south landing. Players could descend
the upper flight and immediately turn onto the basement flight without entering
the middle rooms. The basement flight now occupies a separate enclosed west shaft
(X−8…−4). Its 3 m wide ramp runs Z4…14, descending from Y3.6 to Y0. Short north
and south connector corridors meet the middle and basement, respectively.

The vacated east shaft floor is raised to Y3.6 and its former basement opening is
closed. The upper flight retains its world position. A player who jumps over its
rail can therefore reach only the middle-level shaft floor, rather than bypassing
the middle and falling directly to the basement. The west ramp avoids the upper
vestibule footprint, respecting the engine's single-carrier-plane ramp contract.

Every shaft railing is regenerated through the existing file-backed module
linker using actual carrier/slope support. Its 44 posts have 396 sampled footprint
contacts, with bases embedded 6–53.52 mm into support. The two routes retain
2.86 m between rail centrelines and about 2.74 m between their inside faces,
more than the player's 0.6 m diameter. Fixed
fixtures illuminate the moved slope and both landings; the existing eastern
lower wall light moves above the raised floor. Standing clearance is at least
2.8 m at the west ramp's high landing and increases down the flight.

<a id="environment-collection-office-interaction-and-physics"></a>
### Interaction and physics

The paired drinks/snack power interaction retains its steady, flickering and dark
states, shared switch group, authored emission overrides and independent basement
lamp failures. The two CRTs, mug and books now explicitly use their reusable
`assets/gameplay.json` physical profiles and the `office_desk_items` reset group.
They start dormant at their authored desk poses. A compact vestibule wall switch
restores those four items through `reset_physics_group`; no furniture is turned
into a loose object. The mug's old fixed-placement E/tap binding is retired in
favor of actual pickup; its allowed audio emitter remains. Movable props cannot
own a fixed-placement interactable or event bindings, and the first production
validation explicitly caught that conflict before the source was accepted.

`office_return_hub` is a real closed door in the upper vestibule's west wall. Its
hinge is world `[−4,7.2,9.7]`, closed rotation 90°, width 1.4 m, and centre
`[−4,8.375,9]`. The authored entry spawn remains `(−2,9)`, facing east (90°), away
from the return door. The collection controller owns its transition binding.
The 4:1 return plate is 1.2 × 0.3 m, centred at world Y9.81. Its Y9.66–9.96
extent clears the actual casing top at Y9.62 and the Y10 ceiling by 4 cm on each
side. This avoids hiding the plate's lower border behind the proud door frame.

<a id="environment-collection-office-reproduction-and-evidence"></a>
### Reproduction and evidence

The migration starts from the actual editor-saved import, supplied explicitly to
`tools/levels/migrate_office_collection.py --source IMPORTED --out
projects/environments/levels/office.json --report RECEIPT`. The helper refuses the
standalone source path and never overwrites it. This deliberate geometry redesign
is separate from automatic import correction; it preserves unrelated source
fields, per-placement review metadata, intentional sunken furniture and existing
AI. No new artwork or blanket geometry ignores are introduced.
The helper is the initial migration recipe, not a generator for the final project;
reproduce into an isolated output rather than overwriting later hub wiring.

The original standalone source's production geometry scan reports zero errors and
one bounded `room-leak` candidate at `[24.875,0,14.125]`, beside the old shared
landing; two existing narrow `missing-wall` suppressions remain. The actual
editor import preserved the original source bytes and passed save/reopen and
asset validation, with 12 review entries and no eligible automatic corrections.
The final migrated source passes the strict production geometry scan with **zero
errors and zero warnings**. One additional narrow `missing-wall` annotation
identifies the west landing's intentional opening onto the physically railed
ramp; it does not suppress leaks, geometry defects or model findings. The first
trim revision produced six slivers, corrected by joining the former doorway's
boards and clipping at the actual perpendicular basement partitions.

Local receipts are `local/environment-collection-0.12.0/office-migration.json`,
`office-migrated-geometry.json`, and the import's
`import-audit/imports/office_demo/receipt.json`. The collection's combined handoff
records the later build/package validation, focused native routes and photographs.

A read-only final actual Document/Safety review in `import-audit/final-office/`
reduces the 12 baseline entries to 11. Both CRT collision reviews and the old
shaft leak candidate are resolved. Eight contextual model-plane candidates
remain: four tiny west railing corner contacts (0.00013 m² each), two existing
filing-plinth contacts, and CRT/desk plus mug/desk support contacts. The module
linker deliberately embeds rails into support sides. These candidates remain
visible for native review; no placement ignore or automatic move is applied.
The model triangle pair budget remains incomplete, alongside the scope note and
12 measured floor-contact notes. This bounded review is not a complete model
intersection or visibility proof.

Normal-input route waypoints, with feet heights:

| Section | Waypoints (X,Z) | Feet Y |
| --- | --- | --- |
| Top entry to upper flight | (−2,9), (20.3,9), (20.3,6), (22.5,2), (32.5,2) | 7.2 |
| Upper descent | (32.5,4), (32.5,14), (32.5,15.8), (22.5,16) | 7.2→3.6 |
| Middle through-route | (21,11), (21,7), (7.8,7), (7.8,9.8), (3.7,9.8), (3.7,1.5), (−6,1.5) | 3.6 |
| Basement descent and entry | (−6,4), (−6,14), (−6,15.9), (2,16) | 3.6→0 |

<a id="environment-collection-office-early-native-static-evidence"></a>
### Early native Static evidence

The isolated one-map projects in `local/environment-collection-0.12.0/office-static-fixture/`
use the frozen production Office source, with only its unavailable hub-return
binding and transition declaration omitted. The separate AI-ramp project also
widens only the mannequin's `ai.home_bounds`; its geometry and authored actor pose
remain unchanged. Both passed normal project checks, package validation and
current-source verification. Compiler `9bb1abca…` built Static only with two
workers; native player `8070dc5e…` ran the following focused-window checks.

| Native receipt under `local/environment-collection-0.12.0/native/` | Result | What it establishes |
| --- | --- | --- |
| `office-static-natural-final-01/acceptance.json` | 91 checks passed | Ordinary held movement and camera input reached the upper east landing, crossed the middle rooms, descended the moved west flight and entered the basement. All five shipped agents moved and retained Y0. No player recovery occurred. |
| `office-static-ramp-edges-final-01/acceptance.json` | 57 checks passed | Explicit initial diagnostic player poses followed by normal movement/jump input retained the correct side and height at the tested ramp edges, with zero recovery. |
| `office-static-bounds-guard-final-01/acceptance.json` | 11 checks passed | The shipped mannequin's original basement home bounds refused the west-shaft goal. This is a bounds check, not evidence of a ramp climb. |
| `office-static-ai-ramp-final-01/acceptance.json` | 50 checks passed | In the bounds-only variant, production navigation physically moved the mannequin from the basement to the ramp, up to Y3.6 and through the upper portal into the middle connector. No actor/player pose action moved it there. |

The ordinary route reached approximately `(32.468,3.6,15.771)` at the east
landing, `(3.672,3.6,1.409)` at the middle west approach,
`(−6.038,0,15.830)` below the moved ramp and `(1.971,0,15.930)` inside the
basement. Coordinates here use feet Y. The bounds-only actor progressed through
Y0.922 and Y3.331, then reached `(−5.956,3.6,2.579)` on the upper landing and
`(−1.952,3.6,2.105)` in the connector, with no recorded stuck time. The shipped
mannequin remains intentionally basement-only; it does not gain this diagnostic
cross-storey behavior.

The side/jump probes stopped at X34.300 on the upper flight's eastern approach
with feet Y3.6. The west ramp's two probes stopped at X−4.928 and X−7.072 with
feet Y1.8. These bounded contacts establish the tested cases, not every possible
jump. Ambient, flashlight and opposite-side Static close-ups show the sampled
posts meeting their support without an obvious hover gap. Still images do not
establish absence of temporal shimmer; all model-contact candidates and the
incomplete pair-budget warning remain reviewable.

Final **a9 Full collection** validation, ordinary route replay, hub→Office→hub,
lighting/composition review and uncapped performance remain pending. The Static
fixtures intentionally cannot verify hub return and provide no Full visual or
performance acceptance. The recorded runs also do not exercise the new desk-item
reset switch or the preserved vending cycle; those interactions must not be
reported as newly native-verified. The source route is reversible, but these
Static receipts establish the downward ordinary traversal only. Final acceptance
is owned by the combined collection pass before standalone package retirement.

<a id="macos-distribution-0120"></a>
## Essential macOS distribution and first run — October 18, 2026

The `0.12.0-staging` player and shipped tools can run directly outside the
checkout, with resources located from the executable and writable data located
through the shared user-path policy. This builds on commit `877c7bef` and the
[Quiet Collection handoff](release-0.12.0.md#environment-collection-0120-retirement).
No shipped map, asset, gameplay format, import budget or package integrity check
was changed. No full map was baked.

<a id="macos-distribution-0120-changes"></a>
### Changes

- Keep the existing executable-relative resolver authoritative. A copied player
  cannot fall back to its compile-time checkout or the shell's working directory.
  Surface missing installation resources and first-run storage failures in a
  native startup notice as well as the terminal error; headless CLI errors remain
  terminal-only.
- Create and report `saves/` beside `levels/`, `import/`, `projects/` and
  `diagnostics/` in macOS Application Support. Settings remain `settings.json`;
  cache remains in Library/Caches. Preflight non-directory ancestors, inaccessible
  paths and dangling symlinks across all roots before creating any folders.
  Existing settings, maps, inbox contents and
  save-folder files survive initialization and relaunch.
- Add `--with-editor`, which includes the sibling compiler/player and editor
  instructions. Both flat and bundle layouts resolve tools without PATH lookup.
- Bundle the host SDL3 library, its exact license and runtime identity receipt.
  Relocate all executable load commands, verify their remaining dependencies are
  Apple system libraries, and ad-hoc sign/verify each modified binary and library
  plus the completed app. Unsupported external linkage stops publication.
- Record the highest linked minimum macOS version in `MACOS_RUNTIME.json` and
  `LSMinimumSystemVersion`. This host's SDL3 requires **macOS 27.0**; the accepted
  payload is **arm64, macOS 27.0+**. No older OS compatibility is claimed.
- Restore both previous exports if either publication rename fails. Keep
  unrelated export-root siblings intact. Isolate config/cache as well as data
  during optional map validation, even when caller overrides are obstructed.

The new regression coverage protects concrete distribution failures: a newer
SDL deployment target, hidden non-system dependencies and paths with spaces;
failed second-product publication; save-file custody; and a config-file
obstruction/dangling symlink that previously allowed partial first-run folder creation. The
existing tiny fixtures were extended where possible. No test builds a shipped
map or requires native graphics; the added Python linkage checks use canned
`otool` output and cost milliseconds.

<a id="macos-distribution-0120-native-acceptance"></a>
### Native acceptance

Host: macOS 27.0.1 (26A434), Apple M2 Pro, arm64, Metal. Direct exported binaries
were launched with `subprocess`, never temporary wrapper apps or `open -a`.
The source checkout was not the working directory. The test root was
`/private/tmp/Places distribution 0.12.0 x96b49qd`, including installation,
project, launch and user paths containing spaces and Unicode.

The 27-check explicit installation suite passed against a map-free export and a
current, tiny editor project containing a built-in chair. Further direct checks
passed with the actual collection distribution:

| Case | Evidence/result |
| --- | --- |
| Clean native first launch | New isolated HOME, no prior path CLI initialization; created Application Support and Cache folders plus settings |
| Relaunch and different working directory | Same default-resolved profile, existing files retained, no state written beside executable or into launch directory |
| Flat/bundle resource resolution | `--paths` selects the appropriate installed assets; direct bundle and flat players share default storage |
| Empty installation | Explore displays import instructions and remains usable |
| Compatible map import | Inbox and CLI import, repeat import, native Full play and restart discovery passed |
| Invalid imports | Corrupt package and conflicting ID/name rejected; previous installed bytes preserved |
| Resource/storage failures | Missing catalog/model, invalid overrides, assets-as-storage, obstructed and unwritable roots return named errors; restoration allows subsequent launch |
| Read-only installation | Discovery, native play and first-run writable storage passed without modifying installed resources |
| Actual collection menu | Hub background, Explore, Settings and Exit readable; Explore shows one Quiet Collection entry with thumbnail |
| Collection access | Nine bundled members discovered and Hub played; complete collection also imported into a map-free installation, rediscovered as nine installed members and played from another directory |
| Settings persistence | Ordinary keyboard navigation changed master volume from 100% to 95%; restart displayed 95% and retained exact settings bytes |
| Save data location | Existing sentinel files under `saves/` remained byte-identical across initialization and native relaunch |
| SDL relocation | `DYLD_PRINT_LIBRARIES=1` confirmed the copied local SDL3 loaded, with no Homebrew library in the loader log |
| Shipped tools | Direct flat and bundle editors opened a copied tiny project, saved/built through sibling compiler, exported/imported/played through sibling player and returned to editing; source bytes retained |

Native captures were visually inspected, including empty Explore, collection
menu/Explore, changed and reloaded Audio settings, and the editor after play.
Player captures were 3024×1676 physical pixels and used Fifo presentation. These
are correctness checks; no FPS comparison or uncapped throughput claim is made.

The collection archive remains exactly **449,857,977 bytes**, SHA-256
`9e78fbe3f6a2cf00f19f410da76271f1acb6c6b14d91b2e5ba0dd8682e532ee8`.
The earlier handoff's accepted environment routes remain applicable to these
unchanged map bytes. This task adds distribution/first-run acceptance rather
than repeating the full environment campaign.

<a id="macos-distribution-0120-validation-and-preserved-deliverables"></a>
### Validation and preserved deliverables

- `cargo fmt --all --check` — passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` — passed; includes the repository-required `-D warnings` gate.
- `cargo build --locked --release --workspace --all-features` — passed; final build 3m 11s (initial pass 3m 15s).
- `cargo test --workspace --all-features` — passed, 192 Rust tests.
- `python3 -m unittest tests.test_packaging tests.test_macos_bundle` — passed, eight tests with one expected native-export skip; explicit real exports were run separately.
- `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'` — passed, 370 tests with three expected skips.
- `python3 tools/assets/validate.py` — passed, 620 assets, zero warnings.
- `sh tools/package.sh --with-editor --maps assets/levels <output>` — real map-free and collection exports passed; final export also checked obstructed caller config/cache overrides were isolated.
- `python3 tools/verify-installation.py --installation <map-free-flat> --project <current-tiny-project> --package <current-tiny-package> --out <fresh-evidence>` — all 27 checks passed.
- `git diff --check` — passed.

Source release player SHA-256:
`b0483d3555f0085a07eb327256effc2f1a5b245e95beb18ce8af2ead6d4b5157`.
Compiler: `dc27c98d314848149523c4298321af3172a3144e4dfc44fe539f8ebdba7ead63`.
Editor: `2c8ae9eed32e10ac078cb26b3dbca43d357d5e195a757b1a63f08b98283dfd00`.
Relocation/signing changes export hashes; final payload identities are recorded
separately in the local delivery receipt. Source release binaries are untouched.

After the final dangling-symlink fix, all Rust gates were rerun and the exports
were rebuilt. The refreshed direct binaries passed clean menu startup, bundle
relaunch with settings/save-file preservation, no-mutation dangling-symlink
rejection, compiler validation and editor build/play/return. The permanent copy
then rediscovered all nine members, loaded its own SDL3 after relocation and
passed strict bundle signature verification. The receipt also verifies matching
minimum-OS metadata and isolation of obstructed caller config/cache overrides.

Runnable exports are preserved outside `target/` under
`local/macos-distribution-0.12.0/final-deliverables/{Places,Places.app}`. Pre-change
executables remain under `preserved-before/`. Logs, native captures, fixture
sources and receipts are retained under the same task root, including copied
`native-evidence/`; the temporary external root is additional evidence, not the
only runnable copy. **No `cargo clean` was needed or run.**

Launch and import commands and complete distribution contents are documented in
[INSTALLATION.md](../INSTALLATION.md). The original stage-1 fixture's old
metadata-only Luau placeholder is no longer valid; native acceptance used the
later tiny editor-created project. The missing-assets test retains the newly
bundled SDL so it reaches the application's resource check instead of dyld.

<a id="macos-distribution-0120-limits-and-scope"></a>
### Limits and scope

`saves/` reserves stable persistent storage; there is still no gameplay save/load
format or command. Player pose, quest progress and moved objects remain
session-local. Settings, installed maps and saved editor projects persist.
Testing used isolated HOME/default paths and explicit test profiles; the real
user's existing profile was not changed.

The crate/bundle version remains the existing staging value `0.11.0`; this work
does not perform the version bump or a release. The export is host-architecture,
ad-hoc signed, and not Developer ID signed/notarized or a Gatekeeper acceptance
claim. Building a payload for older macOS requires a suitable SDL3 runtime.
Windows/Linux native validation, installers, custom asset importing, the new
dedicated menu background and broader sound polish are outside this task; menu
and sound polish remain deferred to 0.13.0. No dependencies were added.

<a id="macos-distribution-0120-files-changed"></a>
### Files changed

- `CHANGELOG.md`
- `docs/INSTALLATION.md`
- `docs/reports/release-0.12.0.md#macos-distribution-0120`
- `src/engine/paths.rs`
- `src/engine/tests/paths.rs`
- `src/game/app/mod.rs`
- `tests/test_macos_bundle.py`
- `tests/test_packaging.py`
- `tools/bundle-macos.py`
- `tools/export-maps.py`
- `tools/package.sh`
- `tools/verify-installation.py`

Commit/push is scoped to these files on `0.12.0-staging`. No hosted CI wait,
merge or release is part of this handoff.
