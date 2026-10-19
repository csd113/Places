# Quiet Collection distribution handoff — 0.12.0 staging

The maintained environment distribution is now one `environments` entry in
`levels/lineup.txt`, sourced from `projects/environments/places.project.json`.
Its hub and eight destinations reuse the completed, playable package; this
handoff performs no full rebake or map/source edits. Collection implementation
and supporting runtime/editor work were already committed and pushed in
`d2ec9971` when this handoff began; the checkout was clean on `0.12.0-staging`.

## Exact retirement inventory

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

## Source ownership and exports

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

## Launch the existing delivery

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

## Focused validation

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

## Remaining work and limits

Sound polish and the dedicated main-menu background are deferred to **0.13.0**;
no work on that version was started. A broader themed-interaction campaign,
uncapped FPS/memory comparisons, finished video/Library media, native installers,
signing and Windows/Linux native acceptance remain incomplete. Native acceptance
here is macOS/Metal correctness evidence, not throughput evidence. Known geometry
audit limits for Lantern Hollow/Beach and review findings for Hub/Home/Winter
remain described in the [collection report](environment-collection-0.12.0.md).
No blanket diagnostic suppression was added.

The branch handoff commits and pushes only reviewed retirement/distribution work
on `0.12.0-staging`. It does not wait for hosted CI, merge, publish a release or
start a new version. Historical stage scope/deferral fields remain in local
receipts; this handoff supersedes their old commit/push/retirement deferrals.

## Files changed in this handoff

- `.github/workflows/rust.yml`
- `.gitignore`
- `CHANGELOG.md`
- `README.md`
- `docs/INSTALLATION.md`
- `docs/README.md`
- `docs/reports/environment-collection-0.12.0-retirement.md`
- `docs/reports/environment-collection-0.12.0.md`
- `docs/reports/environment-collection-native-qa.md`
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
