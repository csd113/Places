# Developer orientation

Places keeps authored content, developer controls and generated output separate.
Use these ownership boundaries, then select the source, package or native workflow below.

| Directory | Contents |
| --- | --- |
| `src/` | Rust player, offline compiler and engine modules |
| `assets/shared/` | Shared surface sheets, fitted decals, static props and UI atlas |
| `assets/environments/<theme>/` | Environment textures, decals, props and source masters |
| `assets/entities/<id>/` | Rigged GLBs and standalone texture sheets |
| `assets/references/` | Immutable concept art and authoring prompts; excluded from exports |
| `assets/catalog.json` | Logical asset/material registry; paths are relative to `assets/` |
| `levels/` | Optional maintained source maps and their explicit compiler `lineup.txt` allowlist |
| `tests/fixtures/levels/` | Focused movement, lighting, geometry, entity and capacity controls |
| `tests/fixtures/native/` | Camera, controller and reference-image inputs |
| `tools/{levels,textures,props,entities}/` | Offline authoring generators and checks |
| `tools/bench/` | Explicit compiler, loading and native diagnostic campaigns |
| `docs/` | Current contracts, architecture and verification; `reports/` holds dated findings |
| `docs/images/` | Curated original documentation images |

Read [map authoring](MAP_AUTHORING_GUIDE.md) before changing a map and
[asset contracts](ASSET_SPECIFICATION.md) before changing artwork or models.
[Environment profiles](assets/ENVIRONMENTS.md) and [entity profiles](assets/ENTITIES.md)
consolidate kit-specific instructions formerly spread across 24 asset READMEs.
Prop sheets stay beside their GLBs; logical `core:` and theme IDs are unchanged.

## Build from source

Install the [desktop prerequisites](../README.md#desktop-prerequisites). The
repository toolchain is pinned in `rust-toolchain.toml`.

```sh
cargo build --locked --release
./target/release/places --list-levels
./target/release/places
```

No map source or compiled package is required to build, test, package or launch.
The player opens a usable menu with zero maps; Settings and Quit remain available.
Explore rescans external packages on opening. Drop a `.placesmap` into the writable
`levels/` folder, or into `import/` and choose Import levels (which rescans both).
`PLACES_LEVEL` optionally selects an installed ID or display name; an unknown
selection returns to the menu with a warning. Otherwise the first discovered map
in deterministic order can supply the menu background. No map is embedded.

To author the optional curated content, run `sh tools/build-maps.sh --workers 8`.
That explicit command compiles exactly `levels/lineup.txt` through `places-compile`,
then verifies provenance, archive validity and Static/Full variants.
Packages go to ignored `assets/levels/`. Compiler capture/bake work is offline
and may take several minutes per large map. Use a native GPU for normal captures.

After changing just one source, use an explicit output path:

```sh
./target/release/places-compile build levels/winter.json \
  --out assets/levels/winter.placesmap --asset-root assets --workers 8
python3 tools/levels/compile.py --check
```

Run a generator's `--check` before editing its owned output. For generated maps,
edit the generator and regenerate the JSON; the [map index](../levels/README.md)
lists each owner. [Offline tooling](OFFLINE_TOOLING.md) documents the asset pipeline.
Textures are committed PNG inputs; the player never generates texture artwork.

## Maintained maps and regression controls

The playable sources are `pool_showcase`,
`home_showcase`, `lantern_hollow`, `winter`, `beach_demo`, `frutiger_aero_demo`
and `retro_arcade_demo`, plus `office_demo`.
Home and Pool are compact standalone showcases; Retro Arcade connects bowling,
arcade, birthday and restroom spaces. Places Demo and Model Zoo are preserved
byte-for-byte as historical JSON in `docs/archive/levels/`; they are outside
active discovery, default build lists and bake campaigns.

The catalog has eight environments: Office, Pool, Home, Outdoor, Winter, Beach,
Frutiger Aero and Arcade. The dedicated `office_demo` now connects three storeys
and bounded basement encounters; its [implementation report](reports/office-demo.md)
records the authored behavior and native acceptance.

Obsolete local dev maps, historical compiler/renderer review snapshots and their
public discovery paths were retired. Eleven superseded Hero-stage source maps
and the old asset-polish showroom were replaced, where needed, by small
`material_transport` and `architecture_before`/`architecture_after` fixtures.
Movement Test moved into the [regression fixture index](../tests/fixtures/levels/README.md).
Geometry, water/ice, collision, entity and capacity assertions remain supported.
Severe Winter controls derive from the single maintained Winter source and an
explicit weather configuration rather than maintaining a second village.

Native diagnostic tools consume explicitly compiled fixtures. For example:

```sh
mkdir -p target/verification/maps
./target/release/places-compile build tests/fixtures/levels/movement_test.json \
  --out target/verification/maps/tests__fixtures__levels__movement_test.placesmap \
  --asset-root assets --workers 8
python3 tools/bench/capture_movement_test.py --case step_ \
  --out target/verification/movement-steps
```

The fixture index and `tools/bench/` help name the additional inputs for complete
campaigns. Diagnostic packages do not belong in the curated export directory.

## Verification and export

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'
python3 tools/assets/validate.py
```

Rust tests use bounded deterministic fixtures and require no existing `.placesmap`
files. They do not compile or bake full shipped maps. Test additions must justify
unique behavior coverage and runtime/resource cost; prefer the smallest fixture
that reproduces a concrete regression. The test profile preserves debug assertions
and overflow checks.

Automatic GitHub checks run source contracts and separate, timed Rust format,
check, debug/release Clippy and fixture-test stages. They do not compile shipped
maps, capture their reflection probes or run native game campaigns. Run those
explicitly on the desktop:
`python3 tools/levels/compile.py --map retro_arcade_demo --workers 8` compiles and
validates one authored map. `sh tools/verify.sh` is the full desktop gate, including
the compiled lineup, package tests and native smoke checks. See
[verification](VERIFICATION.md) for the explicit diagnostic gates.

Export the map-free player with `sh tools/package.sh OUTPUT_DIRECTORY`. It requires
only the release player and runtime assets. To include external maps explicitly,
use `sh tools/package.sh --maps PACKAGE_DIRECTORY OUTPUT_DIRECTORY`. That directory
may contain any supported `.placesmap` files; no curated names are required.
Optional map inclusion uses the release compiler to validate records and the player
to check discovery/catalog/dependency compatibility before publishing the export.
It never bakes maps or reads `levels/lineup.txt`.

Both flat and macOS bundle exports include empty writable map folders. Runtime
exports exclude concept references and source masters. Destination checks reject
the repository root, its ancestors, home-root destinations and product symlinks.
Cargo source packages exclude optional `levels/` and generated `assets/levels/`.
See [map independence verification](reports/map-independence.md) for the empty
installation and external-map acceptance evidence.

## Evidence and generated output

Keep current contracts in the guides and finalized conclusions in the
[documentation index](README.md). Raw receipts and captures belong in ignored
verification directories. The curated original images, native reference fixtures,
immutable concepts and source masters retain their distinct roles.

Compiled packages, caches and local recovery collections are generated or
machine-specific output, not fresh-checkout prerequisites. No history or LFS
purge is part of documentation cleanup. Preserve requested runnable deliverables
outside `target`, stop owned jobs and clean build output only when it is unowned.

At the October 13 documentation/tooling handoff, only the Office package was
current after shared engine changes; the seven other installed packages were
unchanged and stale. They were not refreshed for this cleanup. Verify or rebuild
selected maps explicitly before native/package acceptance; a previous all-map
campaign does not establish present currency.
