# Diagnostic assets

Development and validation content that exists to test the renderer, not to
furnish a level. Diagnostic assets are `asset_class: diagnostic` and keep that
class rather than being labelled Office or Pool just to fit the theme system.

Currently catalogued here:

| asset | type | resource |
| --- | --- | --- |
| `core:decal_test_01` | decal | generated (internal validation marking) |

The diagnostic **levels** themselves live with the rest of the shipped levels
(`../levels/lighting_diagnostic.json`, `../levels/rendering_diagnostic.json`)
because levels are discovered from the level directories, not the asset catalog.
Future diagnostic models, textures or fixtures belong in this directory.

Goal 1 (RGB lighting) and Goal 2 (stable overlays/decals) diagnostic content
still loads and resolves unchanged.
