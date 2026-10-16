# Asset library

[Catalog](../../assets/catalog.json) owns logical asset identity and file locations. Read the
[asset specification](../ASSET_SPECIFICATION.md) before authoring or changing
imagery, and the [map authoring guide](../MAP_AUTHORING_GUIDE.md) before
placing assets or changing a map.

## Where content lives

Paths below are repository-relative.

| Location | Content |
| --- | --- |
| `assets/shared/` | Theme-independent props, source PNGs, surfaces, decals and engine image resources |
| `assets/environments/<theme>/` | Office, Pool, Home, Outdoor, Winter, Beach, Frutiger Aero and Retro Arcade kits |
| `assets/entities/<id>/model/` | Skinned character and creature GLBs |
| `assets/entities/<id>/textures/` | Standalone entity PNGs and retained masters |
| `assets/diagnostic/` | Renderer test artwork, including intentional NPOT/alpha probes |
| `assets/references/<theme-or-entities>/` | Original concept sheets and historical authoring prompts |
| `assets/levels/*.placesmap` | Ignored playable compiler outputs |
| [`levels/`](../../levels) | Maintained map JSON sources and their ownership guide |
| [`tests/fixtures/levels/`](../../tests/fixtures/levels) | Regression map sources |

Within each environment, `props/` keeps GLBs beside their native PNGs and
masters. Surface sheets use `textures/{walls,floors,ceilings,doors,water,sky}`;
fixture faces use `textures/lights/`; fitted markings use `decals/`. Shared
normals and effects use `shared/textures/normals/` and `effects/`. Home's
`props/art/` retains fitted CRT screen artwork. Reference art is preserved at
its original dimensions and bytes; it is not runtime texture input.

`references/neon_bowling_arcade/` preserves the bowling and carpet concept
boards and the complete master specification. The Retro Arcade kit is built
from those references and showcased by `retro_arcade_demo`.

[Environment profiles](ENVIRONMENTS.md) describe kit-specific
atlas layouts, mounting and construction helpers.
[Entity profiles](ENTITIES.md) describe rigs, clips, placement
and provenance. [Offline tooling](../OFFLINE_TOOLING.md#asset-authoring)
explains source ownership, rebuilds and inspection.

## Catalog contract

Levels reference IDs such as `core:desk`, `home:wall_switch` and `beach:crab`.
Physical folder names do not rename those IDs or the `core`, `environment`,
`entity` and `diagnostic` classes. Shared domestic props remain environment
assets without a theme; core engine resources retain class `core`. A theme
organizes content and never limits placement, including themed entities.

A file entry names one resource in `model`, relative to `assets/`: GLB for
props/entities, PNG for textures/decals/fixture faces. A material definition
names a logical `texture` plus its tiling, tint and response controls. Material
IDs and texture IDs are separate. File paths must stay below the asset root,
and two catalog entries cannot claim the same file.

Models embed their runtime PNGs. Standalone sheets, larger masters and
intentional family duplicates remain committed as authoring sources. Completed
exports can contain later approved artwork than the retained constructor sheets;
check the catalog's `authoring` record and `assets/completed-models.json` before
exporting. Keep separately supplied Blender/GIMP masters with their delivery
records. A texture change requires an intentional model export and review;
moving files requires repairing catalog paths, embedded file references,
authoring loaders and tests. Preserve aspect, UV layout, orientation, alpha
and tiling throughout either operation.

The [cabinet artwork prompts](../../assets/environments/arcade/props/illustrations/CABINET_ARTWORK_PROMPTS.md)
remain beside their original inputs. Prompt records and concept boards preserve
provenance; the catalog and completed-export identities select current assets.

## Validate from the repository root

```sh
python3 tools/assets/validate.py
python3 tools/textures/build.py --check
python3 tools/props/build.py --check
python3 tools/assets/audit.py --workers 8 --out target/asset-audit/inventory.json
python3 tools/entities/validate_entities.py --workers 8
python3 tools/entities/check_clip_boundaries.py
```

These checks cover different contracts; PNG header validation does not prove
seamlessness or model source/embedded parity. The audit reports duplicate
bytes/pixels for review and does not authorize deleting family sources.
Run the required Rust and native checks in
[desktop verification](../VERIFICATION.md) before completing a code or
asset change. Compile maintained sources into `assets/levels/` before launch;
the player does not load raw JSON or generate authored texture imagery.
