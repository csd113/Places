# Essential macOS distribution and first run — October 18, 2026

The `0.12.0-staging` player and shipped tools can run directly outside the
checkout, with resources located from the executable and writable data located
through the shared user-path policy. This builds on commit `877c7bef` and the
[Quiet Collection handoff](environment-collection-0.12.0-retirement.md).
No shipped map, asset, gameplay format, import budget or package integrity check
was changed. No full map was baked.

## Changes

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

## Native acceptance

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

## Validation and preserved deliverables

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

## Limits and scope

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

## Files changed

- `CHANGELOG.md`
- `docs/INSTALLATION.md`
- `docs/reports/macos-distribution-0.12.0.md`
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
