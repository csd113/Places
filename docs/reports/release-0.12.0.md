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
  [collection handoff](environment-collection-0.12.0-retirement.md).
- The separate campaigns remain independent. Shadow-only entities, layered
  navigation, readable editor text and the five-dot crosshair are included.
- Installed resources resolve from the executable; writable data/settings/cache
  use the documented user paths. macOS exports include relocated SDL3, licenses,
  sibling editor/compiler binaries, ad-hoc signatures and minimum-OS metadata.
  See [installation](../INSTALLATION.md) and the
  [distribution acceptance report](macos-distribution-0.12.0.md).

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
[campaign compatibility report](environment-0.12.0-compatible-campaigns.md) and
[native collection report](environment-collection-native-qa.md).

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
