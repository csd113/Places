# Installation and paths (0.13.0 staging)

Ship the executable **and** its external `assets/` payload. Maps are optional:
a complete installation starts with zero maps and opens Explore with import
instructions. Missing required resources are a startup error, distinct from an
empty map list. The player never compiles a map at startup.

## Installed resources

```text
Places/
  places                     places.exe on Windows
  places-compile             optional authoring tool
  places-editor              optional basic authoring application
  lib/libSDL3.0.dylib         macOS runtime, included by the exporter
  SDL3-LICENSE.txt            license for the exact bundled SDL runtime
  MACOS_RUNTIME.json         runtime identity and ad-hoc signing receipt
  assets/
    catalog.json, gameplay.json
    shared/, environments/, entities/, diagnostic/, janitor_closet/, audio/
    levels/                  optional compiled .placesmap files
  docs/INSTALLATION.md
  THIRD_PARTY_LICENSES.txt
```

Mac bundles place executables under `Contents/MacOS/` and this resource payload
under `Contents/Resources/`. Unix prefix layouts may use `bin/places` alongside
`share/places/assets/`. These paths are discovered from the executable, never
from the process working directory. No writes are required in the installation.

`sh tools/package.sh [--with-compiler | --with-editor] [--maps <directory>] <export-parent>`
stages the flat layout and Mac bundle from existing release binaries. It copies
licenses, sound credits and runtime resources, excludes source artwork/masters,
and includes no maps unless explicitly requested. `--maps` validates existing
packages against the staged assets; it does not bake maps. `--with-editor` includes
the compiler, player and editor instructions. On macOS the exporter copies the
linked SDL3 runtime and its license, rewrites player/tool load commands to relative
paths and ad-hoc signs the completed payload. Unexpected non-system dependencies
stop the export before publication. Apple system frameworks remain OS-provided.
The runtime license is found beside the installed SDL prefix; a nonstandard SDL
installation can supply `PLACES_SDL_LICENSE=/absolute/path/LICENSE.txt`.
This is a host-architecture export. Developer ID signing, notarization, native
installers and Linux native acceptance remain deferred. Windows portable-player
acceptance is documented below and does not establish clean-OS installer coverage.
Ad-hoc signing
does not grant Gatekeeper trust to downloaded software.
The effective minimum macOS version is the highest deployment target declared
by the executables and bundled SDL3; it is recorded in `MACOS_RUNTIME.json` and
the bundle's `LSMinimumSystemVersion`. The October 18 native acceptance payload
is **arm64, macOS 27.0 or newer** because its host SDL3 requires 27.0. An older
OS needs a compatible SDL3 build before exporting; Rust's lower target alone
does not make the payload compatible.
See the [macOS distribution acceptance report](reports/release-0.12.0.md#macos-distribution-0120)
for the tested scope and remaining limits.

The [0.12.0 release report](reports/release-0.12.0.md) preserves the earlier
distribution evidence. The [0.13.0 staging report](reports/release-0.13.0.md) records
the current candidate, repaired gameplay paths and remaining limitations. Native
Linux, clean-OS installers and downloaded Gatekeeper trust remain open;
successful installation is not exhaustive gameplay acceptance.

## Windows portable game and authoring tools

Extract the entire `Places-0.13.0-Windows.zip`, keep its `Places` folder together,
and open `Places/places.exe`. Open `Places/places-editor.exe` for the editor;
`Places/places-compile.exe` is the offline compiler and offers its native GUI
with the `gui` argument. The x64 player requires Windows and a Direct3D 12 GPU.
Rust, Python and the repository are not runtime dependencies.

The payload includes all three EXEs, `SDL3.dll`, `dxcompiler.dll`, `dxil.dll`,
`msvcp140.dll`, `vcruntime140.dll`, `vcruntime140_1.dll`, runtime assets and seven
complete editable project packages: **The Backrooms**, **The Quiet Collection**
(Hub plus Home, Pool, Office, Arcade, Lantern Hollow, Winter, Beach and Aero),
**It Only Exists in the Beam** (four chapters), **Closing Time**, **One Guest Too
Many**, **The Shape of an Exit**, and **After Hours - Rooms Beyond**. Relay Annex
source remains in the repository and is excluded from release downloads.
Development gyms, labs and review fixtures are excluded too.

The release inventory is explicit in `tools/release-projects.json`;
`levels/lineup.txt` describes the smaller curated developer build. Complete release
exports retain their normal Static and Full variants. Explore lists project
entries, and authored transitions reach their internal members.

On Windows, use the native exporter after building matching binaries and preparing
the selected map directory. `--with-editor` includes both editor and compiler:

```powershell
python tools/package-windows.py --with-editor --maps C:\Exports\ReleaseMaps `
  --runtime-dir C:\Exports\Runtime --sdl-license C:\Dev\SDL3\LICENSE.txt `
  C:\Exports\Candidate
```

The runtime directory supplies the six x64 DLLs above and a `licenses/` folder
with their Windows SDK/Visual C++ notices (or pass `--runtime-notices <directory>`).
The exporter preserves those notices and the exact SDL license alongside the
payload. It validates PE
architecture and every selected package against the staged assets, then checks
the staged player's complete discovered member inventory. It never bakes maps.
Use a new output root: an existing `Places` product is refused. `WINDOWS_RUNTIME.json`
records actual input identities and source-worktree status; `PAYLOAD_SHA256.json`
covers the payload files. `--build-receipt` can preserve the exact gated build/source
receipt. The Unix shell exporter is not the Windows packaging entry point.

Writable data uses `%LOCALAPPDATA%\Places`; no writes belong in the extracted
payload. Each project package includes recoverable authoring sources: use compiler
`import-project <package> --out <new-project-folder>` and open the new folder in
the editor. Keep the entire installation beside all three EXEs when moving it.
The [0.13.0 report](reports/release-0.13.0.md) records exact native scope, hashes
and remaining gates. This portable candidate is separate from a native installer
or a public release.

## macOS launch

Build and export from the checkout, retaining the existing verified collection:

```sh
cargo build --locked --release --workspace --all-features
sh tools/package.sh --with-editor --maps assets/levels "$HOME/Places Exports/0.13.0"
```

Keep the entire `Places` directory together when moving it. Launch the direct
binary from Terminal; neither `cd` into the installation nor a resource override
is needed. Quote paths containing spaces:

```sh
"$HOME/Places Exports/0.13.0/Places/places"
"$HOME/Places Exports/0.13.0/Places/places" --paths
"$HOME/Places Exports/0.13.0/Places/places" --import "$HOME/Downloads/example.placesmap"
"$HOME/Places Exports/0.13.0/Places/places-editor" "/absolute/path/my project"
```

The equivalent bundle binaries live at `Places.app/Contents/MacOS/`; its SDL3
runtime lives in `Contents/Frameworks/`. Native acceptance uses these direct
binaries. The two layouts contain the same resources and share the default user
profile. The exporter never changes the source binaries in `target/release`.

## Quiet Collection

The maintained environment distribution is `environments.placesmap`: Central
Hub plus eight destinations, with only the hub exposed in Explore. Each
destination has a physical return door. The former eight standalone environment
packages are retired; [the retirement inventory](reports/release-0.12.0.md#environment-collection-0120-retirement)
records their exact IDs. Closing Time and It Only Exists in the Beam remain separate optional project
packages. Relay Annex is preserved as source and excluded from 0.13.0 downloads.

Import an existing collection package using the matching player and resources:

```sh
PLACES_ASSET_ROOT="$PWD/assets" ./target/release/places --import /absolute/path/environments.placesmap
PLACES_ASSET_ROOT="$PWD/assets" PLACES_LEVEL=hub ./target/release/places
```

For an installation export, use `sh tools/package.sh --maps assets/levels
<export-parent>` after the existing verified collection is installed in that
directory. This validates and copies packages without baking. Include the
`janitor_closet/` resources used by Pool; the exporter now carries them alongside
the other catalog resources. Keep user-imported maps and recovery copies intact;
the repository retirement does not delete anyone's installed user maps.

Models are external catalog-selected `.glb` files, **not embedded in the player**.
The supported GLB profile stores its PNG material images inside the GLB itself;
it does not load image URIs from the launch directory. Surface, fixture, decal,
normal and emissive PNGs are external catalog resources. `gameplay.json` is the
optional shared physical-profile sidecar (included by the exporter). Audio has its own
external catalog, WAV files and credits. WGSL shaders, completed-model review
metadata and a few font/diagnostic/fallback PNGs are compiled into the executable.
Those small fallbacks do not replace the required installed library.

A `.placesmap` carries prepared world records, semantics, lighting/captures and
dependency identities. Shared model/image/sound resources still come from the
installation. Its dependency checks reject missing or changed files before play.
Custom texture/model/sound importing is reserved, not implemented.

## Writable storage and first launch

| Platform | Data | Settings | Cache |
| --- | --- | --- | --- |
| macOS | `~/Library/Application Support/Places` | data directory | `~/Library/Caches/Places` |
| Windows | `%LOCALAPPDATA%\Places` | data directory | data directory's `cache/` |
| Linux/Unix | `${XDG_DATA_HOME:-~/.local/share}/places` | `${XDG_CONFIG_HOME:-~/.config}/places` | `${XDG_CACHE_HOME:-~/.cache}/places` |

Data owns `levels/` (installed maps), `import/` (drop-in inbox), `projects/`
(default authoring workspace), `saves/` (collection completion receipts) and
`diagnostics/`. Settings are `settings.json`
under the settings directory. Projects may live elsewhere: a project root is an
explicit authoring folder, never an inference from the resource or data root.

The player persists settings, installed packages and saved editor projects.
Quiet Collection favor completions also persist as small atomic JSON receipts
under `saves/`, keyed to the installed package path. Rebuilding a package at the
same path preserves those receipts. Player position and moved physics objects
reset on a new visit; this is not a general world-state save/load system.
First-run and relaunch preserve existing saves.
The 0.13.0 player uses a dedicated bundled live-menu scene, independent of
optional gameplay maps. A map-free install offers the live menu, empty Explore
with import guidance, Settings and Credits. The exporter always includes the
menu scene and its dependencies.

On first player launch, required directories are created and checked for write
access. Existing files are preserved. A non-directory, inaccessible or unwritable
location fails with its path and recovery override. Writable roots and their
derived directory symlinks cannot point inside the immutable asset tree. Local
storage is trusted; these checks are not a sandbox against concurrent filesystem
mutation by another process. There is no automatic move/delete of legacy state
beside a previous executable; copy wanted maps/settings deliberately.

## Overrides and precedence

`engine::paths` is the single path policy for game, compiler and editor.

* Assets: explicit compiler `--asset-root` / `PathConfig::resolve_assets(Some(..))`
  > `PLACES_ASSET_ROOT` > executable-relative layout > actual Cargo checkout.
  The canonical argument is the directory containing `catalog.json`; the legacy
  package root containing `assets/catalog.json` is also accepted by both tools.
  An explicit invalid override or an existing broken installation errors instead
  of silently selecting another tree. The launch directory is never searched.
* A binary actually below the build checkout's `target/` can use that checkout's
  assets, including release developer builds. A copied binary cannot use its
  compile-time checkout path. Pin `PLACES_ASSET_ROOT` for other developer layouts.
* `PLACES_STATE_ROOT` overrides data and, by default, settings and `cache/` below
  that root. This preserves the existing isolated/portable-state convention.
* `PLACES_CONFIG_ROOT` and `PLACES_CACHE_ROOT` override their respective locations
  even when `PLACES_STATE_ROOT` is present. With no overrides use the table above.
  There is no implicit writable-portable mode beside the executable.

Relative explicit overrides resolve against the launch directory once; absolute
paths are recommended for launches from several directories. Spaces and Unicode
are preserved. Empty Places overrides fail; missing platform homes fail instead
of falling back to the launch or install directory. XDG values must be absolute;
empty XDG values use the platform default. Process resource/storage selections
are cached independently. Call `paths::configure_assets` before resource use;
changing an already selected tree is rejected. No environment mutation is needed.

## Import and diagnose

Run `places --paths` to print the resolved directories as JSON and initialize
user folders. `places --list-levels` prints installed playable IDs and full paths.
Both commands validate required resources and report configuration errors.

In Explore choose **Open Import Folder**, copy a compiled `.placesmap` there,
then choose **Import levels**. The inbox source remains available. Imports are
staged, bounded, validated and atomically published to user `levels/`; failed
validation leaves existing maps intact. The legacy `levels/import/` inbox and
direct user `levels/` discovery remain supported. Raw JSON/ZIP authoring sources
give the compiler command instead of becoming playable entries.

`places --import /absolute/path/map.placesmap` uses the same import path without
opening a window; then launch normally or select an installed ID with the
existing `PLACES_LEVEL` developer override. Imported maps persist across restart
and changes of working directory. Same-file/repeated imports are safe. Replacing
a package with the same exact ID/name updates that map. A different ID at the
same filename, a duplicate ID at another filename, nonportable filenames and
ASCII case collisions are rejected. Bundled map IDs retain discovery precedence;
an import cannot silently hide behind a bundled map. Unicode filename equivalence
follows the native filesystem; Windows/Linux native path acceptance is still due.

## Stage 3 integration boundary

Use `paths::assets()`, `paths::user()` and `paths::initialize_user()`; do not add
another resolver. `UserPaths::projects()` is a default folder, not a project
manifest. Keep `ProjectFolder::open` and confined `ProjectPath` source resolution.
Save validated authoring documents atomically, then invoke `places-compile build
<project> --level <id> --out <explicit-output.placesmap>`; installed compiler
discovery works without `--asset-root`. `BuildRequest.asset_root` should come from
the same resolver. The default output still uses `project::output_path`.

For a compact compile/import/play check, use a current tiny editor project with
a built-in chair dependency. Historical stage1 fixtures contain a metadata-only
Luau placeholder that predates script execution and is no longer a valid module.
Import via `places --import <output.placesmap>`, discover via
`--list-levels`, then run the direct executable with `PLACES_LEVEL=<id>`.
For repeatable native acceptance after exporting a map-free `--with-compiler`
payload, run `python3 tools/verify-installation.py --installation <flat-payload>
--project <tiny-project> --package <tiny-package-with-chair> --out <evidence-dir>`.
It copies the payload before testing, isolates all state, records native captures
and checks read-only storage/dependency failures. `cases/` must not already exist.
This replaces obsolete smoke tests that required a full Arcade bake and wrote
state beside the executable. Ordinary unit tests never launch this tool.

The [native editor](EDITOR.md) provides source editing and save/build/play.
Its compiler/player paths can be overridden independently; editor play uses an
isolated profile below the shared user cache. Project-local Luau, complete project
export/recovery and per-map menu artwork are implemented.
Custom runtime asset importing remains deferred. Historical packages with old
schema or compiler identities require an explicit export from maintained sources;
the final 0.13.0 candidate uses explicitly verified current-source exports.

## Stage 2 validation (2026-10-17)

The change passed 74 compact Rust regressions, strict debug/release all-target
all-feature Clippy, the locked production build, 368 Python tests (16 optional
skips), packaging safety checks and the 593-entry asset check. No shipped map
was baked. Three new Rust tests cover platform defaults/overrides, executable
layouts and first-run preservation/confinement; existing import fixtures cover
conflicting IDs/names and failed publication without native resources.

On macOS 27.0.1 arm64 / Apple M2 Pro / Metal, 27 explicit installation checks
and a separate direct bundle-path check passed using a copied payload and the
tiny stage1 project/package. Five captures cover empty Explore, inbox import,
imported play, read-only resources and a fresh user root launched from a
read-only install directory. Native play used Full lighting; screenshots were
3024x1676 with Fifo presentation. These are correctness checks, not performance
evidence. Settings/imports survived restart and malformed incoming packages;
missing models/dependencies and unwritable storage produced named errors.

Raw logs, isolated projects/packages, copied executables and screenshots remain
outside `target/` in `local/verification/0.12.0-task2/`. The sandbox denied the
default Mac user-folder probe and initially exposed no displays. Approved direct
desktop launches passed with isolated writable state; writing to the real default
user folder was not retried. Windows/Linux policies are covered by pure path
tests, not native acceptance. Host SDL remains an external system dependency.
Issue #21 stays open for editor/export integration and complete installation /
cross-platform acceptance in later stages.
