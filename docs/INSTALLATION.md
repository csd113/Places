# Installation and paths (0.12.0 foundations)

Ship the executable **and** its external `assets/` payload. Maps are optional:
a complete installation starts with zero maps and opens Explore with import
instructions. Missing required resources are a startup error, distinct from an
empty map list. The player never compiles a map at startup.

## Installed resources

```text
Places/
  places                     places.exe on Windows
  places-compile             optional authoring tool
  assets/
    catalog.json, gameplay.json
    shared/, environments/, entities/, diagnostic/, audio/
    levels/                  optional compiled .placesmap files
  docs/INSTALLATION.md
  THIRD_PARTY_LICENSES.txt
```

Mac bundles place executables under `Contents/MacOS/` and this resource payload
under `Contents/Resources/`. Unix prefix layouts may use `bin/places` alongside
`share/places/assets/`. These paths are discovered from the executable, never
from the process working directory. No writes are required in the installation.

`sh tools/package.sh [--with-compiler] [--maps <directory>] <export-parent>`
stages the flat layout and Mac bundle from existing release binaries. It copies
licenses, sound credits and runtime resources, excludes source artwork/masters,
and includes no maps unless explicitly requested. `--maps` validates existing
packages against the staged assets; it does not bake maps. This is a host payload
exporter, not a signed installer, SDL bundler or cross-platform release builder.
Signing, native installers and Windows/Linux acceptance remain stage 9.

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
(default authoring workspace) and `diagnostics/`. Settings are `settings.json`
under the settings directory. Projects may live elsewhere: a project root is an
explicit authoring folder, never an inference from the resource or data root.

On first player launch, required directories are created and checked for write
access. Existing files are preserved. A non-directory, inaccessible or unwritable
location fails with its path and recovery override. Writable roots and their
derived directory symlinks cannot point inside the immutable asset tree. Local
storage is trusted; these checks are not a sandbox against concurrent filesystem
mutation by another process. There is no automatic move/delete of legacy state
beside a previous executable; copy wanted maps/settings deliberately.

## Overrides and precedence

`engine::paths` is the single path policy for game, compiler and future editor.

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

For a compact compile/import/play check, use the retained stage1 project and
packages under `local/verification/0.12.0-task1/project/` (local evidence, not
shipped source). Import via `places --import <output.placesmap>`, discover via
`--list-levels`, then run the direct executable with `PLACES_LEVEL=<id>`.
For repeatable native acceptance after exporting a map-free `--with-compiler`
payload, run `python3 tools/verify-installation.py --installation <flat-payload>
--project <tiny-project> --package <tiny-package-with-chair> --out <evidence-dir>`.
It copies the payload before testing, isolates all state, records native captures
and checks read-only storage/dependency failures. `cases/` must not already exist.
This replaces obsolete smoke tests that required a full Arcade bake and wrote
state beside the executable. Ordinary unit tests never launch this tool.

The editor, custom asset importer, Luau execution, project bundle export and menu
level remain later stages. Old real-map packages need explicit revision-one
rebakes; this path stage does not bake the demo lineup.

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
